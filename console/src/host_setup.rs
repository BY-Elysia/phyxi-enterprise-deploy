//! Fixed host maintenance operations. No arbitrary package, URL or shell parameters.
use crate::{
    runtime::{execute, Runtime},
    store::Store,
};
use std::{fs, path::Path, sync::Arc};
use tokio::process::Command;
async fn run(program: &str, args: &[&str]) -> Result<String, String> {
    println!("+ {program} {}", args.join(" "));
    let mut c = Command::new(program);
    c.args(args)
        .env("DEBIAN_FRONTEND", "noninteractive")
        .env("NEEDRESTART_MODE", "l");
    execute(c, None, 1200).await
}
async fn configured() -> Result<bool, String> {
    let text = run(
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
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(value.get("nvidia").is_some_and(|v| v.is_object()))
}
async fn idle() -> Result<(), String> {
    let running = run(
        "docker",
        &[
            "--host",
            "unix:///var/run/docker.sock",
            "ps",
            "--format",
            "{{.Names}}",
        ],
    )
    .await?;
    for name in running.lines().filter(|s| !s.is_empty()) {
        if name != "phyxi-managed-console-1" {
            return Err(format!("容器 {name} 正在运行；为避免影响业务，拒绝安装或重启 Docker。请先由其管理员安排停机。"));
        }
        let raw = run(
            "docker",
            &["--host", "unix:///var/run/docker.sock", "inspect", name],
        )
        .await?;
        let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if v[0]["Config"]["Labels"]["com.docker.compose.project"] != "phyxi-managed"
            || v[0]["Config"]["Labels"]["io.phyxi.role"] != "console"
        {
            return Err("同名控制台容器归属不符，拒绝主机变更".into());
        }
    }
    if !run(
        "nvidia-smi",
        &["--query-compute-apps=pid", "--format=csv,noheader"],
    )
    .await?
    .trim()
    .is_empty()
    {
        return Err("GPU 存在计算进程，拒绝修改宿主机依赖".into());
    }
    Ok(())
}
pub async fn prepare(mode: &str) -> Result<(), String> {
    if !["install", "repair"].contains(&mode) {
        return Err("未知宿主机操作".into());
    }
    if std::env::consts::OS != "linux"
        || Path::new("/.dockerenv").exists()
        || run("id", &["-u"]).await?.trim() != "0"
    {
        return Err("此操作只能由 Linux 宿主机 root 执行器运行，不能在容器中执行".into());
    }
    let os = fs::read_to_string("/etc/os-release").map_err(|e| e.to_string())?;
    if !os.lines().any(|v| v == "ID=ubuntu")
        || !os
            .lines()
            .any(|v| ["VERSION_ID=\"22.04\"", "VERSION_ID=\"24.04\""].contains(&v))
    {
        return Err("自动安装仅支持 Ubuntu 22.04/24.04".into());
    }
    run("nvidia-smi", &["-L"]).await?;
    let installed = run("nvidia-ctk", &["--version"]).await.is_ok();
    if installed && configured().await? && mode == "install" {
        println!("NVIDIA 容器工具已安装且 Docker 已配置；无需安装或重启。");
        return Ok(());
    }
    if mode == "repair" && !installed {
        return Err("请先安装 NVIDIA 容器工具，再修复配置".into());
    }
    idle().await?;
    if !installed {
        // Separate repository file; never overwrite an administrator's differing configuration.
        let list = Path::new("/etc/apt/sources.list.d/phyxi-nvidia.list");
        let source = "deb [signed-by=/etc/apt/keyrings/phyxi-nvidia.asc] https://nvidia.github.io/libnvidia-container/stable/deb/$(ARCH) /\n";
        if list.exists() && fs::read_to_string(list).map_err(|e| e.to_string())? != source {
            return Err("已有不同的 NVIDIA APT 源，请管理员检查".into());
        }
        fs::create_dir_all("/etc/apt/keyrings").map_err(|e| e.to_string())?;
        run(
            "curl",
            &[
                "--proto",
                "=https",
                "--tlsv1.2",
                "-fsSL",
                "--max-time",
                "60",
                "https://nvidia.github.io/libnvidia-container/gpgkey",
                "-o",
                "/etc/apt/keyrings/phyxi-nvidia.asc",
            ],
        )
        .await?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            "/etc/apt/keyrings/phyxi-nvidia.asc",
            fs::Permissions::from_mode(0o644),
        )
        .map_err(|e| e.to_string())?;
        fs::write(list, source).map_err(|e| e.to_string())?;
        run("apt-get", &["update"]).await?;
        idle().await?;
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
    idle().await?;
    let daemon = Path::new("/etc/docker/daemon.json");
    let backup = Path::new("/etc/docker/daemon.json.phyxi-before-toolkit");
    if daemon.exists() && !backup.exists() {
        fs::copy(daemon, backup).map_err(|e| e.to_string())?;
    }
    run("nvidia-ctk", &["runtime", "configure", "--runtime=docker"]).await?;
    println!("将重启 Docker；控制台可能短暂断开，任务由宿主机执行器继续记录。");
    idle().await?;
    run("systemctl", &["restart", "docker"]).await?;
    if !configured().await? {
        return Err("Docker 重启后仍未注册 NVIDIA runtime；保留配置备份供管理员恢复".into());
    }
    println!("NVIDIA 容器支持已配置；请执行 GPU 容器验证。");
    Ok(())
}
pub async fn verify(r: &Runtime, id: &str, store: Arc<Store>) -> Result<(), String> {
    let image = std::env::var("PHYXI_CONTROL_IMAGE").map_err(|_| "未配置验证镜像")?;
    crate::catalog::immutable_image_valid(&image)?;
    r.local_image(&image).await?;
    let name = format!("phyxi-gpu-check-{}", uuid::Uuid::new_v4());
    let mut c = Command::new("docker");
    c.args([
        "--host",
        &r.docker_host,
        "run",
        "--rm",
        "--pull",
        "never",
        "--name",
        &name,
        "--label",
        &format!("io.phyxi.console.owner={}", r.owner),
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--memory",
        "128m",
        "--pids-limit",
        "64",
        "--gpus",
        "all",
        "-e",
        "NVIDIA_DRIVER_CAPABILITIES=utility",
        "--entrypoint",
        "nvidia-smi",
        &image,
        "--query-gpu=uuid,name,driver_version",
        "--format=csv,noheader",
    ]);
    store.event(
        id,
        "使用已安装的固定镜像短时验证 GPU 设备访问；不下载镜像，不加载模型。",
    )?;
    let result = execute(c, Some((store.clone(), id.into())), 45).await;
    // Unique generated name belongs only to this task, including timeout recovery.
    let mut cleanup = Command::new("docker");
    cleanup.args(["--host", &r.docker_host, "rm", "-f", &name]);
    let _ = execute(cleanup, None, 15).await;
    result?;
    store.event(id, "GPU 容器访问验证通过；不代表模型推理性能验收。")
}
