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
        let mut l = self.live.lock().await;
        if let Some(p) = l.as_mut() {
            return Ok(p.child.try_wait().map_err(|e| e.to_string())?.is_none());
        }
        Ok(false)
    }
    pub async fn matches(&self, c: &GatewayConfig, m: &Config) -> Result<(), String> {
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
    pub async fn certificates(&self, c: &GatewayConfig) -> Result<(), String> {
        c.validate()?;
        if self.running().await? {
            return Err("请先停止网关再修改证书".into());
        }
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        if c.tls_mode == "enterprise" {
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
        Ok(format!(
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
        ))
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
    pub async fn stop(&self) -> Result<(), String> {
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
        fs::write(dir.join("workstation.token"), self.token()?).map_err(|e| e.to_string())?;
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
            fs::set_permissions(
                dir.join("workstation.token"),
                fs::Permissions::from_mode(0o600),
            )
            .map_err(|e| e.to_string())?;
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
