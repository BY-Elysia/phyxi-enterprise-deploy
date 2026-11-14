//! Enterprise ingress owns TLS/auth only; it never starts an inference engine.
use crate::{config::Config, runtime::execute};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct GatewayConfig {
    pub domain: String,
    pub workspace_scope: String,
    pub bind_ip: String,
    pub port: u16,
    pub allowed_cidrs: Vec<String>,
    pub tls_mode: String,
    pub cert_file: String,
    pub key_file: String,
    pub ca_file: String,
}
impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            domain: "localhost".into(),
            workspace_scope: String::new(),
            bind_ip: "127.0.0.1".into(),
            port: 18443,
            allowed_cidrs: vec!["127.0.0.1/32".into()],
            tls_mode: "self_signed".into(),
            cert_file: String::new(),
            key_file: String::new(),
            ca_file: String::new(),
        }
    }
}
impl GatewayConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.workspace_scope.len() > 100
            || !self
                .workspace_scope
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err("工作区入口标识无效".into());
        }
        if self.domain.is_empty()
            || self.domain.len() > 253
            || !self
                .domain
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
        {
            return Err("域名格式错误".into());
        }
        self.bind_ip
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| "监听地址必须是 IPv4")?;
        if self.port < 1024 {
            return Err("使用 ≥1024 的非特权网关端口".into());
        }
        if self.allowed_cidrs.is_empty() {
            return Err("至少填写一个来源网段".into());
        }
        for cidr in &self.allowed_cidrs {
            let (ip, n) = cidr.split_once('/').ok_or("来源网段需要 CIDR 格式")?;
            ip.parse::<std::net::Ipv4Addr>()
                .map_err(|_| "来源网段需 IPv4")?;
            if n.parse::<u8>().map_err(|_| "网段掩码错误")? > 32 {
                return Err("网段掩码错误".into());
            }
        }
        if !["self_signed", "enterprise"].contains(&self.tls_mode.as_str()) {
            return Err("未知证书模式".into());
        }
        if self.tls_mode == "self_signed" && self.bind_ip != "127.0.0.1" {
            return Err("自签测试证书只允许回环地址；企业监听请导入 CA 证书".into());
        }
        if self.tls_mode == "enterprise" {
            for p in [&self.cert_file, &self.key_file, &self.ca_file] {
                if !Path::new(p).is_absolute() {
                    return Err("证书、私钥、CA 必须是服务器绝对路径".into());
                }
            }
        }
        Ok(())
    }
}
struct Live {
    child: Child,
    config: GatewayConfig,
    model: Config,
}
#[derive(Clone)]
pub struct Gateway {
    pub dir: PathBuf,
    live: Arc<Mutex<Option<Live>>>,
}
impl Gateway {
    pub fn new(data: &Path) -> Self {
        Self {
            dir: data.join("gateway"),
            live: Arc::new(Mutex::new(None)),
        }
    }
    pub async fn running(&self) -> Result<bool, String> {
        if crate::gateway_container::name().is_some() {
            return crate::gateway_container::running(self).await;
        }
        let mut l = self.live.lock().await;
        if let Some(p) = l.as_mut() {
            return Ok(p.child.try_wait().map_err(|e| e.to_string())?.is_none());
        }
        Ok(false)
    }
    pub async fn matches(&self, c: &GatewayConfig, m: &Config) -> Result<(), String> {
        if crate::gateway_container::name().is_some() {
            return crate::gateway_container::matches(self, c, m).await;
        }
        let mut l = self.live.lock().await;
        let p = l.as_mut().ok_or("网关未启动")?;
        if p.child.try_wait().map_err(|e| e.to_string())?.is_some()
            || &p.config != c
            || &p.model != m
        {
            return Err("网关配置与当前模型不匹配，请重新启动网关".into());
        }
        Ok(())
    }
    pub fn token(&self) -> Result<String, String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        crate::private_file(&self.dir.join("client.token")).map_err(|e| e.to_string())
    }
    pub fn cert(&self) -> PathBuf {
        self.dir.join("server.pem")
    }
    async fn openssl(&self, args: Vec<String>) -> Result<String, String> {
        let mut c = Command::new("openssl");
        c.args(args);
        execute(c, None, 30).await
    }
    pub async fn validate_certificates(&self, c: &GatewayConfig) -> Result<(), String> {
        c.validate()?;
        self.openssl(vec![
            "verify".into(),
            "-CAfile".into(),
            c.ca_file.clone(),
            "-untrusted".into(),
            c.cert_file.clone(),
            "-verify_hostname".into(),
            c.domain.clone(),
            c.cert_file.clone(),
        ])
        .await?;
        let a = self
            .openssl(vec![
                "x509".into(),
                "-in".into(),
                c.cert_file.clone(),
                "-pubkey".into(),
                "-noout".into(),
            ])
            .await?;
        let b = self
            .openssl(vec![
                "pkey".into(),
                "-in".into(),
                c.key_file.clone(),
                "-pubout".into(),
            ])
            .await?;
        if a.trim() != b.trim() {
            return Err("证书与私钥不匹配".into());
        }
        Ok(())
    }
    pub async fn certificates(&self, c: &GatewayConfig) -> Result<(), String> {
        c.validate()?;
        if self.running().await? {
            return Err("请先停止网关再修改证书".into());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        if c.tls_mode == "enterprise" {
            self.validate_certificates(c).await?;
            for (src, dst) in [
                (&c.cert_file, "server.pem"),
                (&c.key_file, "server.key"),
                (&c.ca_file, "ca.pem"),
            ] {
                fs::copy(src, self.dir.join(dst)).map_err(|e| e.to_string())?;
            }
        } else {
            self.openssl(vec![
                "req".into(),
                "-x509".into(),
                "-newkey".into(),
                "rsa:2048".into(),
                "-nodes".into(),
                "-days".into(),
                "7".into(),
                "-subj".into(),
                format!("/CN={}", c.domain),
                "-addext".into(),
                format!("subjectAltName=DNS:{},IP:127.0.0.1", c.domain),
                "-keyout".into(),
                self.dir.join("server.key").display().to_string(),
                "-out".into(),
                self.cert().display().to_string(),
            ])
            .await?;
            fs::copy(self.cert(), self.dir.join("ca.pem")).map_err(|e| e.to_string())?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in ["server.key", "server.pem", "ca.pem"] {
                fs::set_permissions(self.dir.join(name), fs::Permissions::from_mode(0o600))
                    .map_err(|e| e.to_string())?;
            }
        }
        fs::write(
            self.dir.join("certificate-config.json"),
            serde_json::to_vec(c).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        self.token()?;
        Ok(())
    }
    pub fn render(&self, c: &GatewayConfig, m: &Config, token: &str) -> Result<String, String> {
        c.validate()?;
        if c.port == m.port {
            return Err("网关端口和模型端口不能相同".into());
        }
        if !token.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err("无效工作站令牌".into());
        }
        let quote = |p: PathBuf| -> Result<String, String> {
            let s = p.display().to_string();
            if s.contains(['\n', '\r', '$', '"', '\\']) {
                return Err("状态路径含 Nginx 不支持的字符".into());
            }
            Ok(format!("\"{s}\""))
        };
        let loopback_listener = if c.bind_ip != "127.0.0.1" && c.bind_ip != "0.0.0.0" {
            format!("listen 127.0.0.1:{} ssl;", c.port)
        } else {
            String::new()
        };
        let allows = c
            .allowed_cidrs
            .iter()
            .map(|v| format!("allow {v};"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut rendered = format!(
            r#"worker_processes 1;
pid {};
error_log {} warn;
events {{ worker_connections 128; }}
http {{
 access_log off;
 client_body_temp_path {};
 proxy_temp_path {};
 map $http_authorization $client_allowed {{ default 0; "Bearer {token}" 1; }}
 limit_conn_zone $binary_remote_addr zone=clients:1m;
 server {{
  listen {}:{} ssl;
  {loopback_listener}
  server_name {};
  ssl_certificate {};
  ssl_certificate_key {};
  ssl_protocols TLSv1.2 TLSv1.3;
  allow 127.0.0.1;
  {allows}
  deny all;
  client_max_body_size 8m;
  location ~ ^/v1/(models|chat/completions)$ {{
   if ($client_allowed = 0) {{ return 401; }}
   limit_conn clients 4;
   limit_conn_status 429;
   proxy_pass http://127.0.0.1:{};
   proxy_set_header Authorization "";
   proxy_http_version 1.1;
   proxy_set_header Connection "";
   proxy_buffering off;
   proxy_read_timeout 120s;
  }}
  location / {{ return 404; }}
 }}
}}
"#,
            quote(self.dir.join("nginx.pid"))?,
            quote(self.dir.join("error.log"))?,
            quote(self.dir.join("body"))?,
            quote(self.dir.join("proxy"))?,
            c.bind_ip,
            c.port,
            c.domain,
            quote(self.cert())?,
            quote(self.dir.join("server.key"))?,
            m.port
        );
        if std::env::var("PHYXI_MODEL_NETWORK").is_ok() {
            rendered = rendered.replace(
                &format!("proxy_pass http://127.0.0.1:{};", m.port),
                "proxy_pass http://phyxi-console-model:8000;",
            );
        }
        if crate::access::configured() {
            if c.workspace_scope.is_empty() {
                return Err("独立鉴权模式要求使用具名网关对象".into());
            }
            let endpoint = std::env::var("PHYXI_ACCESS_URL").map_err(|_| "鉴权地址缺失")?;
            if !endpoint.starts_with("http://")
                || endpoint[7..].contains(['/', '?', '#', '@', '\n', '\r'])
            {
                return Err("鉴权服务地址无效".into());
            }
            rendered=rendered.replace(&format!(" map $http_authorization $client_allowed {{ default 0; \"Bearer {token}\" 1; }}"),"");
            rendered=rendered.replace("   if ($client_allowed = 0) { return 401; }", "   auth_request /_phyxi_auth;\n   auth_request_set $workspace_id $upstream_http_x_phyxi_workspace;");
            rendered=rendered.replace("  location ~ ^/v1/",&format!("  location = /_phyxi_auth {{ internal; proxy_connect_timeout 1s; proxy_read_timeout 2s; proxy_send_timeout 2s; proxy_next_upstream off; proxy_pass {endpoint}/check; proxy_pass_request_body off; proxy_set_header Content-Length \"\"; proxy_set_header Authorization $http_authorization; proxy_set_header X-Phyxi-Gateway {}; }}\n  location ~ ^/v1/",c.workspace_scope));
            rendered=rendered.replace(" access_log off;",r#" map $uri $audit_route { default other; /v1/models models; /v1/chat/completions chat; }
 log_format phyxi escape=json '{"type":"request","workspace_id":"$workspace_id","request_id":"$request_id","time":"$time_iso8601","route":"$audit_route","status":$status,"duration":$request_time}';
 access_log /dev/stdout phyxi;"#);
        }
        if std::env::var("PHYXI_GATEWAY_BRIDGE").as_deref() == Ok("1") {
            rendered = rendered.replace(
                &format!("listen {}:{} ssl;", c.bind_ip, c.port),
                &format!("listen 0.0.0.0:{} ssl;", c.port),
            );
        }
        Ok(rendered)
    }
    pub async fn start(&self, c: &GatewayConfig, m: &Config) -> Result<(), String> {
        if self.running().await? {
            return Err("网关已运行，请先停止".into());
        }
        let saved: GatewayConfig = serde_json::from_slice(
            &fs::read(self.dir.join("certificate-config.json")).map_err(|_| "请先准备证书")?,
        )
        .map_err(|e| e.to_string())?;
        if saved != *c {
            return Err("证书对应的配置已改变，请重新准备证书".into());
        }
        self.openssl(vec![
            "x509".into(),
            "-in".into(),
            self.cert().display().to_string(),
            "-checkend".into(),
            "0".into(),
            "-noout".into(),
        ])
        .await?;
        let text = self.render(c, m, &self.token()?)?;
        if crate::gateway_container::name().is_some() {
            return crate::gateway_container::start(self, c, m, &text).await;
        }
        let path = self.dir.join("nginx.conf");
        fs::write(&path, text).map_err(|e| e.to_string())?;
        let binary = std::env::var("PHYXI_NGINX").unwrap_or("nginx".into());
        let mut test = Command::new(&binary);
        test.args([
            "-t",
            "-p",
            &self.dir.display().to_string(),
            "-c",
            &path.display().to_string(),
        ]);
        execute(test, None, 15).await?;
        let f = fs::File::create(self.dir.join("process.log")).map_err(|e| e.to_string())?;
        let child = Command::new(binary)
            .args([
                "-p",
                &self.dir.display().to_string(),
                "-c",
                &path.display().to_string(),
                "-g",
                "daemon off; master_process off;",
            ])
            .stdin(Stdio::null())
            .stdout(f.try_clone().map_err(|e| e.to_string())?)
            .stderr(f)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| e.to_string())?;
        *self.live.lock().await = Some(Live {
            child,
            config: c.clone(),
            model: m.clone(),
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if !self.running().await? {
            return Err("网关启动失败，检查 gateway/error.log".into());
        }
        Ok(())
    }
    pub async fn apply_config(&self, c: &GatewayConfig, m: &Config) -> Result<(), String> {
        if crate::gateway_container::name().is_none() {
            return Err("在线应用仅支持独立 Nginx 容器；原生测试请停止后重新启动网关".into());
        }
        if !self.running().await? {
            return Err("请先启动网关；在线应用只用于运行中的网关".into());
        }
        c.validate()?;
        let directory = self.dir.join(format!("tls-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let work = async {
            let sources = if c.tls_mode == "enterprise" {
                vec![
                    PathBuf::from(&c.cert_file),
                    PathBuf::from(&c.key_file),
                    PathBuf::from(&c.ca_file),
                ]
            } else {
                vec![
                    self.cert(),
                    self.dir.join("server.key"),
                    self.dir.join("ca.pem"),
                ]
            };
            for (src, name) in sources.iter().zip(["server.pem", "server.key", "ca.pem"]) {
                fs::copy(src, directory.join(name)).map_err(|e| e.to_string())?;
                fs::set_permissions(directory.join(name), fs::Permissions::from_mode(0o600))
                    .map_err(|e| e.to_string())?;
            }
            let mut candidate = c.clone();
            candidate.tls_mode = "enterprise".into();
            candidate.cert_file = directory.join("server.pem").display().to_string();
            candidate.key_file = directory.join("server.key").display().to_string();
            candidate.ca_file = directory.join("ca.pem").display().to_string();
            self.validate_certificates(&candidate).await?;
            let text = self
                .render(c, m, &self.token()?)?
                .replace(
                    &format!("ssl_certificate \"{}\";", self.cert().display()),
                    &format!("ssl_certificate \"{}\";", candidate.cert_file),
                )
                .replace(
                    &format!(
                        "ssl_certificate_key \"{}\";",
                        self.dir.join("server.key").display()
                    ),
                    &format!("ssl_certificate_key \"{}\";", candidate.key_file),
                );
            crate::gateway_container::start(self, c, m, &text).await?;
            for name in ["server.pem", "server.key", "ca.pem"] {
                fs::copy(directory.join(name), self.dir.join(name)).map_err(|e| e.to_string())?;
                fs::set_permissions(self.dir.join(name), fs::Permissions::from_mode(0o600))
                    .map_err(|e| e.to_string())?;
            }
            fs::write(
                self.dir.join("certificate-config.json"),
                serde_json::to_vec(c).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        }
        .await;
        // Successful configurations reference this immutable directory. Keep it on ambiguous
        // post-reload failures as well; deleting it could break an already active certificate.
        work
    }
    pub async fn stop(&self) -> Result<(), String> {
        if crate::gateway_container::name().is_some() {
            return crate::gateway_container::stop(self).await;
        }
        let mut l = self.live.lock().await;
        if let Some(p) = l.as_mut() {
            if p.child.try_wait().map_err(|e| e.to_string())?.is_none() {
                p.child.kill().await.map_err(|e| e.to_string())?;
            }
        }
        *l = None;
        Ok(())
    }
    pub fn export(&self, c: &GatewayConfig, m: &Config) -> Result<PathBuf, String> {
        let dir = self.dir.join("client-export");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::copy(self.dir.join("ca.pem"), dir.join("company-ca.pem")).map_err(|e| e.to_string())?;
        if crate::access::configured() {
            if dir.join("workstation.token").exists() {
                fs::remove_file(dir.join("workstation.token")).map_err(|e| e.to_string())?;
            }
        } else {
            fs::write(dir.join("workstation.token"), self.token()?).map_err(|e| e.to_string())?;
        }
        fs::write(dir.join("endpoint.json"),serde_json::to_vec_pretty(&serde_json::json!({"base_url":format!("https://{}:{}/v1",c.domain,c.port),"model":m.model_name,"provider":"phyxi-enterprise"})).unwrap()).map_err(|e|e.to_string())?;
        let inputs = if m.verify_images {
            vec!["text", "image"]
        } else {
            vec!["text"]
        };
        let opencode = serde_json::json!({"providers":{"phyxi-enterprise":{"name":"PhyXi enterprise gateway","package":"@opencode/ai/providers/openai-compatible","settings":{"baseURL":format!("https://{}:{}/v1",c.domain,c.port),"apiKey":"{env:PHYXI_GATEWAY_TOKEN}"},"models":{(m.model_name.clone()):{"modelID":m.model_name,"name":m.model_name,"capabilities":{"tools":m.verify_tools,"input":inputs,"output":["text"]}}}}}});
        fs::write(
            dir.join("opencode-2.2.json"),
            serde_json::to_vec_pretty(&opencode).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            if !crate::access::configured() {
                fs::set_permissions(
                    dir.join("workstation.token"),
                    fs::Permissions::from_mode(0o600),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        Ok(dir)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_injection_and_public_test_tls() {
        let c = GatewayConfig {
            domain: "a;return 200".into(),
            ..GatewayConfig::default()
        };
        assert!(c.validate().is_err());
        let c = GatewayConfig {
            bind_ip: "0.0.0.0".into(),
            ..GatewayConfig::default()
        };
        assert!(c.validate().is_err());
    }
    #[test]
    fn private_gateway_routes_only_api() {
        let g = Gateway::new(Path::new("/tmp/phyxi"));
        let c = GatewayConfig::default();
        let m = crate::config::fixture();
        let s = g.render(&c, &m, "abcd-1234").unwrap();
        assert!(s.contains("deny all;"));
        assert!(s.contains("return 401"));
        assert!(s.contains("proxy_buffering off;"));
        assert!(!s.contains("/api/config"));
    }
}
