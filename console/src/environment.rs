//! Host provisioning is a separate, fixed privileged operation; normal HTTP runs unprivileged.
use crate::runtime::{execute, Runtime};
use serde::Serialize;
use std::{fs, path::Path};
use tokio::process::Command;
#[derive(Serialize)]
pub struct Check {
    pub name: String,
    pub ready: bool,
    pub detail: String,
}
async fn probe(name: &str, program: &str, args: &[&str]) -> Check {
    let mut c = Command::new(program);
    c.args(args);
    match execute(c, None, 10).await {
        Ok(s) => Check {
            name: name.into(),
            ready: true,
            detail: s.chars().take(800).collect(),
        },
        Err(e) => Check {
            name: name.into(),
            ready: false,
            detail: e,
        },
    }
}
pub async fn inspect(r: &Runtime) -> serde_json::Value {
    let nginx = std::env::var("PHYXI_NGINX").unwrap_or("nginx".into());
    let docker_args = [
        "--host",
        &r.docker_host,
        "info",
        "--format",
        "Docker {{.ServerVersion}} | runtimes: {{range $name, $runtime := .Runtimes}}{{$name}} {{end}}",
    ];
    let (system, driver, docker, toolkit, gateway, tls) = tokio::join!(
        probe("系统", "uname", &["-sm"]),
        probe(
            "显卡驱动",
            "nvidia-smi",
            &[
                "--query-gpu=name,driver_version,memory.total",
                "--format=csv,noheader"
            ]
        ),
        probe("容器引擎", "docker", &docker_args),
        probe("NVIDIA Toolkit", "nvidia-ctk", &["--version"]),
        probe("企业网关", &nginx, &["-v"]),
        probe("证书工具", "openssl", &["version"])
    );
    let checks = vec![system, driver, docker, toolkit, gateway, tls];
    serde_json::json!({"checks":checks,"host_setup_enabled":std::env::var("PHYXI_ENABLE_HOST_SETUP").as_deref()==Ok("1"),"plan":plan(),"notes":"安装仅支持 Ubuntu 22.04/24.04 专用服务器；保留已有驱动/CUDA，不格式化磁盘。现有任务运行时拒绝修改容器环境。"})
}
pub fn plan() -> Vec<&'static str> {
    vec![
        "检查 Ubuntu、管理员权限、驱动、GPU/容器无运行任务",
        "准备官方签名 APT 源；保留现有配置",
        "安装缺失的 Docker、NVIDIA Container Toolkit、Nginx、OpenSSL",
        "配置 NVIDIA Docker runtime；必要时重启空闲 Docker",
        "验证 Docker runtime；输出已安装包版本（不启动模型/网关）",
    ]
}
async fn run(program: &str, args: &[&str]) -> Result<String, String> {
    println!("+ {program} {}", args.join(" "));
    let mut c = Command::new(program);
    c.args(args)
        .env("DEBIAN_FRONTEND", "noninteractive")
        .env("NEEDRESTART_MODE", "l");
    execute(c, None, 1200).await
}
async fn exists(program: &str, args: &[&str]) -> bool {
    let mut c = Command::new(program);
    c.args(args);
    execute(c, None, 15).await.is_ok()
}
fn readable(path: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).map_err(|e| e.to_string())
}
fn write_managed(path: &str, body: &str) -> Result<(), String> {
    let p = Path::new(path);
    if p.exists() {
        if fs::read_to_string(p).map_err(|e| e.to_string())? != body {
            return Err(format!("已有不同源配置 {path}，请管理员检查"));
        }
        return Ok(());
    }
    fs::write(p, body).map_err(|e| e.to_string())
}
struct PolicyGuard(bool);
impl Drop for PolicyGuard {
    fn drop(&mut self) {
        if self.0 {
            let _ = fs::remove_file("/usr/sbin/policy-rc.d");
        }
    }
}
pub async fn prepare_host() -> Result<(), String> {
    if std::env::consts::OS != "linux" {
        return Err("环境准备只能在 Ubuntu 服务器执行".into());
    }
    if run("id", &["-u"]).await?.trim() != "0" {
        return Err("环境准备需要管理员运行已安装的受控程序".into());
    }
    let os = fs::read_to_string("/etc/os-release").map_err(|e| e.to_string())?;
    let codename = if os.lines().any(|l| l == "VERSION_ID=\"24.04\"") {
        "noble"
    } else if os.lines().any(|l| l == "VERSION_ID=\"22.04\"") {
        "jammy"
    } else {
        return Err("支持 Ubuntu 22.04/24.04".into());
    };
    if !os.lines().any(|l| l == "ID=ubuntu") {
        return Err("需要 Ubuntu".into());
    }
    run("nvidia-smi", &[]).await?;
    if !run(
        "nvidia-smi",
        &["--query-compute-apps=pid", "--format=csv,noheader"],
    )
    .await?
    .trim()
    .is_empty()
    {
        return Err("存在 GPU 工作负载，拒绝环境变更".into());
    }
    let docker = exists("docker", &["--version"]).await;
    if docker
        && !run(
            "docker",
            &["--host", "unix:///var/run/docker.sock", "ps", "-aq"],
        )
        .await?
        .trim()
        .is_empty()
    {
        return Err("已有容器，拒绝自动修改主机环境".into());
    }
    let _policy = if !Path::new("/usr/sbin/policy-rc.d").exists() {
        use std::os::unix::fs::PermissionsExt;
        fs::write("/usr/sbin/policy-rc.d", "#!/bin/sh\nexit 101\n").map_err(|e| e.to_string())?;
        fs::set_permissions("/usr/sbin/policy-rc.d", fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        PolicyGuard(true)
    } else {
        PolicyGuard(false)
    };
    run("apt-get", &["update"]).await?;
    run(
        "apt-get",
        &[
            "install",
            "-y",
            "--no-install-recommends",
            "-o",
            "Dpkg::Options::=--force-confold",
            "ca-certificates",
            "curl",
            "gnupg",
            "openssl",
            "nginx",
        ],
    )
    .await?;
    fs::create_dir_all("/etc/apt/keyrings").map_err(|e| e.to_string())?;
    if !docker {
        run(
            "curl",
            &[
                "-fsSL",
                "https://download.docker.com/linux/ubuntu/gpg",
                "-o",
                "/etc/apt/keyrings/phyxi-docker.asc",
            ],
        )
        .await?;
        let arch = run("dpkg", &["--print-architecture"]).await?;
        if !["amd64", "arm64"].contains(&arch.trim()) {
            return Err("不支持的架构".into());
        }
        readable("/etc/apt/keyrings/phyxi-docker.asc")?;
        write_managed("/etc/apt/sources.list.d/phyxi-docker.list",&format!("deb [arch={} signed-by=/etc/apt/keyrings/phyxi-docker.asc] https://download.docker.com/linux/ubuntu {codename} stable\n",arch.trim()))?;
        run("apt-get", &["update"]).await?;
        run(
            "apt-get",
            &[
                "install",
                "-y",
                "--no-install-recommends",
                "docker-ce",
                "docker-ce-cli",
                "containerd.io",
                "docker-compose-plugin",
            ],
        )
        .await?;
    }
    if !exists("nvidia-ctk", &["--version"]).await {
        run(
            "curl",
            &[
                "-fsSL",
                "https://nvidia.github.io/libnvidia-container/gpgkey",
                "-o",
                "/etc/apt/keyrings/phyxi-nvidia.asc",
            ],
        )
        .await?;
        readable("/etc/apt/keyrings/phyxi-nvidia.asc")?;
        write_managed("/etc/apt/sources.list.d/phyxi-nvidia.list","deb [signed-by=/etc/apt/keyrings/phyxi-nvidia.asc] https://nvidia.github.io/libnvidia-container/stable/deb/$(ARCH) /\n")?;
        run("apt-get", &["update"]).await?;
        run(
            "apt-get",
            &[
                "install",
                "-y",
                "--no-install-recommends",
                "nvidia-container-toolkit=1.20.1-1",
                "nvidia-container-toolkit-base=1.20.1-1",
                "libnvidia-container-tools=1.20.1-1",
                "libnvidia-container1=1.20.1-1",
            ],
        )
        .await?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [
            "/etc/apt/keyrings/phyxi-docker.asc",
            "/etc/apt/keyrings/phyxi-nvidia.asc",
        ] {
            if Path::new(path).exists() {
                fs::set_permissions(path, fs::Permissions::from_mode(0o644))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    let daemon = Path::new("/etc/docker/daemon.json");
    if daemon.exists() && !Path::new("/etc/docker/daemon.json.phyxi-original").exists() {
        fs::copy(daemon, "/etc/docker/daemon.json.phyxi-original").map_err(|e| e.to_string())?;
    }
    run("nvidia-ctk", &["runtime", "configure", "--runtime=docker"]).await?;
    run("systemctl", &["enable", "--now", "docker"]).await?;
    run("systemctl", &["restart", "docker"]).await?;
    let info = run(
        "docker",
        &[
            "--host",
            "unix:///var/run/docker.sock",
            "info",
            "--format",
            "{{json .Runtimes}}",
        ],
    )
    .await?;
    if !info.contains("nvidia") {
        return Err("Docker 未发现 NVIDIA runtime".into());
    }
    println!(
        "{}",
        run(
            "dpkg-query",
            &[
                "-W",
                "docker-ce",
                "nvidia-container-toolkit",
                "nginx",
                "openssl"
            ]
        )
        .await?
    );
    println!(
        "环境准备完成。新安装 Docker 后，服务账号可能需要重新登录/重启服务才能获取 socket 组权限。"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn refuses_local_host_mutation() {
        if std::env::consts::OS != "linux" {
            assert!(prepare_host().await.is_err());
        }
    }
    #[test]
    fn provisioning_is_separate() {
        assert_eq!(plan().len(), 5);
        assert!(!plan().join(" ").contains("安装驱动"));
    }
}
