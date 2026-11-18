//! Host-installed vLLM and read-only discovery of a local OpenAI-compatible service.
use crate::{
    config::Config,
    process::Managed,
    runtime::{execute, Runtime},
    store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::process::Command;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Container,
    Native,
    External,
}

pub fn binaries() -> Vec<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let mut paths = Vec::new();
    if let Some(configured) = std::env::var_os("PHYXI_VLLM_BIN") {
        paths.extend(std::env::split_paths(&configured));
    }
    for key in ["VIRTUAL_ENV", "CONDA_PREFIX"] {
        if let Some(root) = std::env::var_os(key) {
            paths.push(PathBuf::from(root).join("bin/vllm"))
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path).map(|p| p.join("vllm")));
    }
    let mut paths: Vec<_> = paths
        .into_iter()
        .filter(|p| {
            p.is_absolute()
                && p.file_name().is_some_and(|name| name == "vllm")
                && p.metadata()
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

pub fn binary(path: &str) -> Result<PathBuf, String> {
    binaries().into_iter().find(|p| p == Path::new(path)).ok_or(
        "本机 vLLM 程序不可用；请重新检测环境，或通过 PHYXI_VLLM_BIN 指定已安装的程序".into(),
    )
}

pub async fn models(port: u16) -> Result<Vec<String>, String> {
    if port < 1024 {
        return Err("请输入本机模型服务的非特权端口".into());
    }
    let mut cmd = Command::new("curl");
    cmd.args([
        "--disable",
        "--fail",
        "--silent",
        "--show-error",
        "--noproxy",
        "*",
        "--max-time",
        "4",
        "--max-filesize",
        "65536",
        &format!("http://127.0.0.1:{port}/v1/models"),
    ]);
    let response = execute(cmd, None, 6).await?;
    let value: Value = serde_json::from_str(&response).map_err(|_| "服务没有返回模型清单")?;
    let names: Vec<String> = value["data"]
        .as_array()
        .ok_or("服务未返回 OpenAI 兼容模型清单")?
        .iter()
        .filter_map(|v| v["id"].as_str())
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .take(128)
        .map(String::from)
        .collect();
    if names.is_empty() {
        return Err("未检测到已提供服务的模型".into());
    }
    Ok(names)
}

pub async fn probe(port: u16, expected: &str) -> Result<Value, String> {
    let ids = models(port).await?;
    if !ids.iter().any(|id| id == expected) {
        return Err("服务返回的模型清单不包含所选模型".into());
    }
    Ok(json!({"model":expected,"url":format!("http://127.0.0.1:{port}/v1"),"available":true}))
}

pub fn weight_group() -> Result<Option<u32>, String> {
    if !crate::executor::enabled() {
        return Ok(None);
    }
    std::env::var("PHYXI_WEIGHT_READER_GID")
        .ok()
        .map(|s| s.parse().map_err(|_| "模型读取组配置无效".into()))
        .transpose()
}

struct ModelLink(PathBuf);
impl Drop for ModelLink {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub async fn start(
    r: &Runtime,
    c: &Config,
    id: &str,
    store: Arc<Store>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let binary = binary(&c.native_binary)?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", c.port))
        .await
        .map_err(|_| "模型端口被占用")?;
    let unit = format!("phyxi-vllm-{id}.service");
    let privileged = unsafe { libc::geteuid() } == 0;
    let mut model_path = c.model_path.clone();
    let mut model_link = None;
    let mut cmd = if privileged {
        if !cfg!(target_os = "linux") || !crate::executor::enabled() {
            return Err("以 root 运行原生 vLLM 需要 Linux 宿主机执行器".into());
        }
        // The private mount exposes only the chosen directory, even if its parents are private.
        let link = PathBuf::from(format!("/run/phyxi/model-{id}"));
        std::os::unix::fs::symlink(&c.model_path, &link).map_err(|e| e.to_string())?;
        model_link = Some(ModelLink(link.clone()));
        model_path = format!("/run/phyxi-vllm-{id}/model");
        let mut run = Command::new("/usr/bin/systemd-run");
        if weight_group()?.is_some() {
            run.arg("--property=SupplementaryGroups=phyxi-model-read");
        }
        run.arg(format!(
            "--property=BindReadOnlyPaths={}:{}",
            link.display(),
            model_path
        ));
        run.args([
            "--wait",
            "--pipe",
            "--collect",
            "--quiet",
            "--unit",
            &unit,
            "--property=DynamicUser=yes",
            "--property=User=phyxi-vllm",
            "--property=PrivateTmp=yes",
            "--property=NoNewPrivileges=yes",
            "--property=BindsTo=phyxi-executor.service",
            "--property=After=phyxi-executor.service",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=15",
            &format!("--property=RuntimeDirectory=phyxi-vllm-{id}"),
            &format!("--setenv=HOME=/run/phyxi-vllm-{id}"),
            "--setenv=HF_HUB_OFFLINE=1",
            "--setenv=TRANSFORMERS_OFFLINE=1",
            &format!("--setenv=CUDA_VISIBLE_DEVICES={}", c.gpu_uuids.join(",")),
            "--",
        ])
        .arg(&binary);
        run
    } else {
        let mut cmd = Command::new(binary);
        cmd.env("CUDA_VISIBLE_DEVICES", c.gpu_uuids.join(","))
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1");
        cmd
    };
    cmd.args(crate::vllm::serve_args(c, &model_path, "127.0.0.1", c.port));
    let log = r.data.join("local-model.log");
    let file = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    drop(listener);
    let child = cmd
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(file.try_clone().map_err(|e| e.to_string())?)
        .stderr(file)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    *r.local.lock().await = Some(Managed {
        child,
        job: id.into(),
        config: c.clone(),
        log,
        unit: privileged.then_some(unit),
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1200);
    let error = loop {
        if cancel.load(Ordering::SeqCst) {
            break "启动已取消";
        }
        if tokio::time::Instant::now() >= deadline {
            break "原生 vLLM 启动超时";
        }
        if r.local
            .lock()
            .await
            .as_mut()
            .ok_or("实例已结束")?
            .status()?["state"]
            != "running"
        {
            break "原生 vLLM 已退出，请查看运行日志及程序、权重读取权限";
        }
        if probe(c.port, &c.model_name).await.is_ok() {
            drop(model_link);
            store.event(id, "原生 vLLM 已返回所选模型")?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(750)).await;
    };
    let mut local = r.local.lock().await;
    if let Some(p) = local.as_mut() {
        p.stop().await?;
    }
    *local = None;
    Err(error.into())
}
