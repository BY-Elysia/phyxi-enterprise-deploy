//! Optional, console-owned support containers. No other workload may be adopted.
use crate::{
    gateway::Gateway,
    runtime::{execute, Runtime},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::process::Command;
const ACCESS: &str = "phyxi-managed-access";
const GATEWAY: &str = "phyxi-managed-gateway";
const OWNER: &str = "io.phyxi.console.owner";
pub fn enabled() -> bool {
    std::env::var("PHYXI_MANAGED_SERVICES").as_deref() == Ok("1")
}
fn command(r: &Runtime, args: &[String]) -> Command {
    let mut c = Command::new("docker");
    for k in [
        "DOCKER_HOST",
        "DOCKER_CONTEXT",
        "DOCKER_TLS_VERIFY",
        "DOCKER_TLS",
        "DOCKER_API_VERSION",
        "DOCKER_CERT_PATH",
        "DOCKER_CONFIG",
    ] {
        c.env_remove(k);
    }
    c.args(["--host", &r.docker_host]).args(args);
    c
}
async fn docker(r: &Runtime, args: &[String]) -> Result<String, String> {
    execute(command(r, args), None, 30).await
}
async fn inspect(r: &Runtime, name: &str) -> Result<Option<Value>, String> {
    let names = docker(
        r,
        &[
            "container".into(),
            "ls".into(),
            "-a".into(),
            "--format".into(),
            "{{.Names}}".into(),
        ],
    )
    .await?;
    if !names.lines().any(|s| s == name) {
        return Ok(None);
    }
    let rows: Value = serde_json::from_str(&docker(r, &["inspect".into(), name.into()]).await?)
        .map_err(|e| e.to_string())?;
    let v = rows[0].clone();
    let role = if name == ACCESS { "access" } else { "gateway" };
    let mount = if name == ACCESS {
        r.data.join("access")
    } else {
        r.data.join("gateway")
    };
    if v["Config"]["Labels"][OWNER] != r.owner
        || v["Config"]["Labels"]["io.phyxi.role"] != role
        || v["HostConfig"]["NetworkMode"] != "host"
        || !v["Mounts"].as_array().is_some_and(|ms| {
            ms.iter().any(|m| {
                m["Source"].as_str() == mount.to_str()
                    && m["Destination"].as_str() == mount.to_str()
            })
        })
    {
        return Err(format!("拒绝操作不属于当前控制台的容器：{name}"));
    }
    Ok(Some(v))
}
pub async fn status(r: &Runtime) -> Result<Value, String> {
    if !enabled() {
        return Ok(json!({"managed":false}));
    }
    let mut rows = vec![];
    for (name, label) in [(ACCESS, "工作区鉴权服务"), (GATEWAY, "Nginx 企业网关")] {
        let v = match inspect(r, name).await {
            Ok(v) => v,
            Err(error) => {
                rows.push(json!({"name":label,"container":name,"state":"unknown","detail":error}));
                continue;
            }
        };
        rows.push(json!({"name":label,"container":name,"state":v.as_ref().map(|v|v["State"]["Status"].clone()).unwrap_or(json!("absent"))}));
    }
    Ok(
        json!({"managed":true,"components":rows,"gateway_image":crate::image_sources::load(&r.data)?.gateway_image}),
    )
}
pub async fn absent(g: &Gateway) -> Result<bool, String> {
    let data = g.dir.parent().ok_or("状态目录无效")?.to_path_buf();
    let owner = fs::read_to_string(data.join("owner")).map_err(|e| e.to_string())?;
    let r = Runtime::new(
        owner.trim().into(),
        std::env::var("PHYXI_DOCKER_HOST").unwrap_or("unix:///var/run/docker.sock".into()),
        data,
    );
    Ok(inspect(&r, GATEWAY)
        .await?
        .is_none_or(|v| v["State"]["Running"] != true))
}
pub async fn run(
    r: &Runtime,
    g: &Gateway,
    start: bool,
    id: &str,
    store: Arc<Store>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    if !enabled() {
        return Err("请使用单控制台安装模式；现有外部 Compose 组件不会被接管".into());
    }
    // Validate every name before making any mutation.
    let access = inspect(r, ACCESS).await?;
    let gateway = inspect(r, GATEWAY).await?;
    if !start {
        if g.running().await? {
            return Err("请先在企业网关页停止业务入口，再停止接入组件".into());
        }
        for (name, v) in [(GATEWAY, gateway), (ACCESS, access)] {
            if v.is_some() {
                docker(r, &["stop".into(), name.into()]).await?;
            }
        }
        return Ok(());
    }
    let image = std::env::var("PHYXI_CONTROL_IMAGE").unwrap_or("phyxi-console:local".into());
    crate::catalog::image_valid(&image)?;
    // Same installed console image, never download an unrelated authentication implementation.
    docker(r, &["image".into(), "inspect".into(), image.clone()]).await?;
    let gateway_reference = crate::image_sources::load(&r.data)?.gateway_image;
    if gateway.is_none() && r.local_image(&gateway_reference).await.is_err() {
        store.update(id, "running", "下载 Nginx 镜像")?;
        crate::image_sources::pull(r, &gateway_reference, id, store.clone(), cancel.clone())
            .await?;
    }
    let gateway_image = if gateway.is_none() {
        r.local_image(&gateway_reference).await?["Id"]
            .as_str()
            .ok_or("Nginx 镜像缺少 ID")?
            .to_owned()
    } else {
        String::new()
    };
    if cancel.load(Ordering::SeqCst) {
        return Err("任务已取消".into());
    }
    use std::os::unix::fs::PermissionsExt;
    for d in [
        r.data.join("access"),
        r.data.join("service-secrets"),
        g.dir.clone(),
    ] {
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        fs::set_permissions(d, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    crate::private_file(&r.data.join("service-secrets/access.token")).map_err(|e| e.to_string())?;
    let management = g.dir.join("management.conf");
    if !management.exists() {
        fs::write(&management, "").map_err(|e| e.to_string())?;
    }
    let config = g.dir.join("nginx.conf");
    if !config.exists() {
        fs::write(&config, crate::gateway_container::bootstrap(&g.dir)?)
            .map_err(|e| e.to_string())?;
    }
    for (name, existing, role) in [(ACCESS, access, "access"), (GATEWAY, gateway, "gateway")] {
        if cancel.load(Ordering::SeqCst) {
            return Err("任务已取消".into());
        }
        store.update(id, "running", &format!("启动 {role} 组件"))?;
        if existing.is_some() {
            docker(r, &["start".into(), name.into()]).await?;
            continue;
        }
        let mount = r.data.join(role);
        let mut args = vec![
            "run".into(),
            "-d".into(),
            "--name".into(),
            name.into(),
            "--restart".into(),
            "unless-stopped".into(),
            "--network".into(),
            "host".into(),
            "--label".into(),
            format!("{OWNER}={}", r.owner),
            "--label".into(),
            format!("io.phyxi.role={role}"),
            "--log-opt".into(),
            "max-size=10m".into(),
            "--log-opt".into(),
            "max-file=3".into(),
            "-v".into(),
            format!("{}:{}", mount.display(), mount.display()),
        ];
        if role == "access" {
            args.extend([
                "-v".into(),
                format!(
                    "{}:/run/phyxi/access.token:ro",
                    r.data.join("service-secrets/access.token").display()
                ),
                "-e".into(),
                format!("PHYXI_ACCESS_DATA={}", mount.display()),
                "-e".into(),
                "PHYXI_ACCESS_ADMIN_FILE=/run/phyxi/access.token".into(),
                "-e".into(),
                "PHYXI_ACCESS_BIND=127.0.0.1:8766".into(),
                image.clone(),
                "--access".into(),
            ]);
        } else {
            args.extend([
                "--entrypoint".into(),
                "nginx".into(),
                gateway_image.clone(),
                "-p".into(),
                g.dir.display().to_string(),
                "-c".into(),
                config.display().to_string(),
                "-g".into(),
                "daemon off;".into(),
            ]);
        }
        docker(r, &args).await?;
    }
    for _ in 0..30 {
        let mut ready = true;
        for port in [8766, 18765] {
            let mut c = Command::new("curl");
            c.args([
                "--noproxy",
                "*",
                "--fail",
                "--silent",
                "--max-time",
                "1",
                &format!("http://127.0.0.1:{port}/health"),
            ]);
            ready &= execute(c, None, 2).await.is_ok();
        }
        if ready
            && inspect(r, ACCESS)
                .await?
                .is_some_and(|v| v["State"]["Running"] == true)
            && inspect(r, GATEWAY)
                .await?
                .is_some_and(|v| v["State"]["Running"] == true)
        {
            store.event(id, "企业接入组件已就绪；业务入口仍需配置证书并显式启动")?;
            return Ok(());
        }
        if cancel.load(Ordering::SeqCst) {
            return Err("任务已取消".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    Err("接入组件未通过健康检查；请查看组件日志后重试，已创建组件会保留".into())
}

/// Publish the optional administrator HTTPS entry without making it a bootstrap dependency.
pub async fn management(
    r: &Runtime,
    g: &Gateway,
    c: &crate::gateway::GatewayConfig,
    start: bool,
) -> Result<(), String> {
    if !enabled() {
        return Err("此入口仅用于单控制台安装模式".into());
    }
    if inspect(r, GATEWAY)
        .await?
        .is_none_or(|v| v["State"]["Running"] != true)
    {
        return Err("请先部署企业接入组件".into());
    }
    c.validate()?;
    let mut contents = vec![];
    if start {
        if c.tls_mode != "enterprise" {
            return Err("企业管理入口需要上传有效企业证书，初始测试请继续使用 SSH 隧道".into());
        }
        g.validate_certificates(c).await?;
        for (source, dest) in [
            (&c.cert_file, "admin.pem"),
            (&c.key_file, "admin.key"),
            (&c.ca_file, "admin-ca.pem"),
        ] {
            contents.push((
                g.dir.join(dest),
                fs::read(source).map_err(|e| e.to_string())?,
            ));
        }
    }
    let path = g.dir.join("management.conf");
    let generation = uuid::Uuid::new_v4().to_string();
    let text = if start {
        let control_port: u16 = std::env::var("PHYXI_PORT")
            .unwrap_or("8765".into())
            .parse()
            .map_err(|_| "控制台端口无效")?;
        let allow = c
            .allowed_cidrs
            .iter()
            .map(|v| format!("allow {v};"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("server {{ listen {}:8443 ssl; server_name {}; ssl_certificate \"{}/admin.pem\"; ssl_certificate_key \"{}/admin.key\"; ssl_protocols TLSv1.2 TLSv1.3; {allow} deny all; access_log off; client_max_body_size 220k; location = /_phyxi/ready {{ allow 127.0.0.1; deny all; return 200 '{generation}'; }} location / {{ proxy_pass http://127.0.0.1:{control_port}; proxy_set_header Host $http_host; proxy_set_header X-Forwarded-Proto https; proxy_set_header X-Phyxi-Client-IP $remote_addr; proxy_cookie_flags phyxi_session secure httponly samesite=strict; proxy_read_timeout 60s; }} }}",c.bind_ip,c.domain,g.dir.display(),g.dir.display())
    } else {
        String::new()
    };
    contents.push((path, text.into_bytes()));
    let originals = contents
        .iter()
        .map(|(p, _)| {
            Ok((
                p.clone(),
                match fs::read(p) {
                    Ok(v) => Some(v),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e.to_string()),
                },
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let result = async {
        for (p, bytes) in &contents {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(p)
                .map_err(|e| e.to_string())?;
            f.write_all(bytes).map_err(|e| e.to_string())?;
        }
        let config = fs::read_to_string(g.dir.join("nginx.conf")).map_err(|e| e.to_string())?;
        crate::gateway_container::apply(g, &config).await?;
        if start {
            let address = if c.bind_ip == "0.0.0.0" {
                "127.0.0.1"
            } else {
                &c.bind_ip
            };
            for _ in 0..20 {
                let mut cmd = Command::new("curl");
                cmd.args([
                    "--noproxy",
                    "*",
                    "--fail",
                    "--silent",
                    "--max-time",
                    "2",
                    "--cacert",
                ])
                .arg(g.dir.join("admin-ca.pem"))
                .args([
                    "--resolve",
                    &format!("{}:8443:{address}", c.domain),
                    &format!("https://{}:8443/_phyxi/ready", c.domain),
                ]);
                if execute(cmd, None, 3).await.is_ok_and(|s| s == generation) {
                    return Ok(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            return Err("企业管理入口未加载新配置，已尝试恢复旧入口".into());
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        for (p, old) in originals {
            match old {
                Some(b) => fs::write(p, b).map_err(|e| e.to_string())?,
                None => {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
    if let Err(original_error) = &result {
        let config = fs::read_to_string(g.dir.join("nginx.conf")).map_err(|e| e.to_string())?;
        if let Err(error) = crate::gateway_container::apply(g, &config).await {
            return Err(format!("{}；恢复旧入口失败：{error}", original_error));
        }
    }
    result
}

pub async fn logs(r: &Runtime, component: &str) -> Result<String, String> {
    if !enabled() {
        return Err("当前安装不由控制台管理接入组件".into());
    }
    let name = match component {
        "access" => ACCESS,
        "gateway" => GATEWAY,
        _ => return Err("未知接入组件".into()),
    };
    if inspect(r, name).await?.is_none() {
        return Err("该组件尚未部署".into());
    }
    docker(
        r,
        &["logs".into(), "--tail".into(), "100".into(), name.into()],
    )
    .await
}
