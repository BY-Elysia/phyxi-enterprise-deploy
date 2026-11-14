//! Optional externally owned Nginx process. Never create/remove arbitrary containers.
use crate::{
    config::Config,
    gateway::{Gateway, GatewayConfig},
    runtime::execute,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use tokio::process::Command;

#[derive(Serialize, Deserialize)]
struct Active {
    generation: String,
    gateway: GatewayConfig,
    model: Config,
}

pub fn name() -> Option<String> {
    std::env::var("PHYXI_GATEWAY_CONTAINER")
        .ok()
        .filter(|s| !s.is_empty())
}

async fn docker(args: &[String]) -> Result<String, String> {
    let host = std::env::var("PHYXI_DOCKER_HOST").unwrap_or("unix:///var/run/docker.sock".into());
    let mut command = Command::new("docker");
    for k in [
        "DOCKER_HOST",
        "DOCKER_CONTEXT",
        "DOCKER_TLS_VERIFY",
        "DOCKER_TLS",
        "DOCKER_API_VERSION",
        "DOCKER_CERT_PATH",
        "DOCKER_CONFIG",
    ] {
        command.env_remove(k);
    }
    command.arg("--host").arg(host).args(args);
    execute(command, None, 15).await
}

pub async fn check(g: &Gateway) -> Result<String, String> {
    let name = name().ok_or("未配置独立网关容器")?;
    if name.starts_with('-')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        return Err("网关容器名无效".into());
    }
    let text = docker(&["inspect".into(), name.clone()]).await?;
    let rows: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "网关容器检查结果无效")?;
    let v = &rows[0];
    let mounted = v["Mounts"].as_array().is_some_and(|rows| {
        rows.iter().any(|m| {
            m["Source"].as_str() == g.dir.to_str() && m["Destination"].as_str() == g.dir.to_str()
        })
    });
    if v["Config"]["Labels"]["io.phyxi.role"] != "gateway" || !mounted {
        return Err("拒绝操作非本状态目录所属的网关容器".into());
    }
    if v["State"]["Running"] != true {
        return Err("独立网关容器未运行，请通过 Compose 恢复".into());
    }
    Ok(name)
}

async fn nginx(g: &Gateway, args: &[&str]) -> Result<(), String> {
    let name = check(g).await?;
    let mut command = vec![
        "exec".into(),
        name,
        "nginx".into(),
        "-p".into(),
        g.dir.display().to_string(),
        "-c".into(),
        g.dir.join("nginx.conf").display().to_string(),
    ];
    command.extend(args.iter().map(|s| s.to_string()));
    docker(&command).await?;
    Ok(())
}

pub fn bootstrap(dir: &Path) -> Result<String, String> {
    let path = dir.display().to_string();
    if path.contains(['\n', '\r', '$', '"', '\\']) {
        return Err("网关路径包含不支持的字符".into());
    }
    let address = if std::env::var("PHYXI_GATEWAY_BRIDGE").as_deref() == Ok("1") {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let include = if std::env::var("PHYXI_GATEWAY_BRIDGE").as_deref() == Ok("1") {
        format!("include \"{path}/management.conf\";")
    } else {
        String::new()
    };
    Ok(format!("worker_processes 1;\npid \"{path}/nginx.pid\";\nerror_log stderr warn;\nevents {{ worker_connections 128; }}\nhttp {{ {include} access_log off; server {{ listen {address}:18765; location = /health {{ return 200 'ready'; }} location / {{ return 404; }} }} }}\n"))
}

fn atomic(path: &Path, text: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(text).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(temp, path).map_err(|e| e.to_string())
}

pub async fn apply(g: &Gateway, text: &str) -> Result<(), String> {
    check(g).await?;
    let path = g.dir.join("nginx.conf");
    let old = fs::read(&path).map_err(|e| e.to_string())?;
    atomic(&path, text.as_bytes())?;
    // Bind-mounted files on desktop VMs can briefly expose old attributes after rename.
    let mut validation = nginx(g, &["-t"]).await;
    for _ in 0..4 {
        if validation.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        validation = nginx(g, &["-t"]).await;
    }
    if let Err(e) = validation {
        atomic(&path, &old)?;
        return Err(e);
    }
    if let Err(e) = nginx(g, &["-s", "reload"]).await {
        atomic(&path, &old)?;
        return Err(e);
    }
    Ok(())
}

pub async fn running(g: &Gateway) -> Result<bool, String> {
    check(g).await?;
    let path = g.dir.join("active-container.json");
    if !path.exists() {
        return Ok(false);
    }
    let active: Active = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    for _ in 0..10 {
        if observed().await.is_ok_and(|s| s == active.generation) {
            return Ok(true);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err("网关进程存在，但实际配置尚未与快照匹配，请检查网关日志".into())
}

pub async fn matches(g: &Gateway, c: &GatewayConfig, m: &Config) -> Result<(), String> {
    if !running(g).await? {
        return Err("网关未启动".into());
    }
    let active: Active = serde_json::from_slice(
        &fs::read(g.dir.join("active-container.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if active.gateway != *c || active.model != *m {
        return Err("运行网关与当前配置不匹配，请先停止后重新应用".into());
    }
    Ok(())
}

async fn observed() -> Result<String, String> {
    let mut cmd = Command::new("curl");
    cmd.args(["--noproxy", "*", "--fail", "--silent", "--max-time", "2"]);
    cmd.arg(format!(
        "http://{}:18765/health",
        std::env::var("PHYXI_GATEWAY_HOST").unwrap_or("127.0.0.1".into())
    ));
    execute(cmd, None, 3).await
}

pub async fn start(g: &Gateway, c: &GatewayConfig, m: &Config, text: &str) -> Result<(), String> {
    let generation = format!("{:x}", Sha256::digest(text.as_bytes()));
    let address = if std::env::var("PHYXI_GATEWAY_BRIDGE").as_deref() == Ok("1") {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let include = if std::env::var("PHYXI_GATEWAY_BRIDGE").as_deref() == Ok("1") {
        format!("include \"{}/management.conf\";", g.dir.display())
    } else {
        String::new()
    };
    let health=format!("http {{ {include}\n server {{ listen {address}:18765; location = /health {{ return 200 '{generation}'; }} location / {{ return 404; }} }}");
    let candidate = text.replacen("http {", &health, 1);
    let old = fs::read_to_string(g.dir.join("nginx.conf")).map_err(|e| e.to_string())?;
    apply(g, &candidate).await?;
    let mut ready = false;
    for _ in 0..20 {
        if observed().await.is_ok_and(|s| s == generation) {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !ready {
        apply(g, &old).await?;
        return Err("网关配置未就绪，已恢复原配置".into());
    }
    atomic(
        &g.dir.join("active-container.json"),
        &serde_json::to_vec(&Active {
            generation,
            gateway: c.clone(),
            model: m.clone(),
        })
        .map_err(|e| e.to_string())?,
    )
}

pub async fn stop(g: &Gateway) -> Result<(), String> {
    apply(g, &bootstrap(&g.dir)?).await?;
    let mut ready = false;
    for _ in 0..20 {
        if observed().await.is_ok_and(|s| s == "ready") {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if !ready {
        return Err("网关停用配置尚未确认生效，请检查独立网关".into());
    }
    let path = g.dir.join("active-container.json");
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bootstrap_is_loopback_only_and_rejects_injection() {
        let text = bootstrap(Path::new("/srv/phyxi/gateway")).unwrap();
        assert!(text.contains("listen 127.0.0.1:18765"));
        assert!(!text.contains("proxy_pass"));
        assert!(bootstrap(Path::new("/tmp/$bad")).is_err());
    }
}

#[cfg(test)]
mod docker_tests {
    use super::*;
    // Run alone through scripts/test-gateway-container.py; requires a real Docker daemon.
    #[tokio::test]
    #[ignore]
    async fn external_gateway_survives_controller_and_rolls_back_invalid_config() {
        let root = std::path::PathBuf::from(std::env::var("PHYXI_GATEWAY_TEST_DATA").unwrap());
        let gateway = Gateway::new(&root);
        let config = GatewayConfig::default();
        let model = crate::config::fixture();
        assert!(!gateway.running().await.unwrap());
        gateway.certificates(&config).await.unwrap();
        gateway.start(&config, &model).await.unwrap();
        assert!(gateway.running().await.unwrap());
        let generation = observed().await.unwrap();
        let mut anonymous = Command::new("curl");
        anonymous.args([
            "--noproxy",
            "*",
            "--silent",
            "--cacert",
            gateway.dir.join("ca.pem").to_str().unwrap(),
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "https://localhost:18443/v1/models",
        ]);
        assert_eq!(execute(anonymous, None, 10).await.unwrap(), "401");
        drop(gateway);
        let recovered = Gateway::new(&root);
        assert!(recovered.running().await.unwrap());
        recovered.matches(&config, &model).await.unwrap();
        let prior = fs::read(recovered.dir.join("nginx.conf")).unwrap();
        assert!(apply(&recovered, "invalid nginx syntax").await.is_err());
        assert_eq!(fs::read(recovered.dir.join("nginx.conf")).unwrap(), prior);
        assert_eq!(observed().await.unwrap(), generation);
        let foreign_root = root.join("foreign");
        let foreign = Gateway::new(&foreign_root);
        assert!(check(&foreign).await.is_err());
        recovered.stop().await.unwrap();
        assert!(!recovered.running().await.unwrap());
    }
}

pub async fn requests(g: &Gateway, workspace: &str) -> Result<serde_json::Value, String> {
    let name = check(g).await?;
    let text = docker(&["logs".into(), "--tail".into(), "200".into(), name]).await?;
    let rows=text.lines().filter_map(|line|serde_json::from_str::<serde_json::Value>(line).ok()).filter(|v|v["type"]=="request"&&v["workspace_id"]==workspace).map(|v|serde_json::json!({"workspace_id":v["workspace_id"],"request_id":v["request_id"],"time":v["time"],"route":v["route"],"status":v["status"],"duration":v["duration"]})).collect::<Vec<_>>();
    Ok(serde_json::Value::Array(rows))
}
