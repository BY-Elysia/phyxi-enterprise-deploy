//! Child handles, never persisted PIDs: only the creating process may signal them.
use crate::{
    config::Config,
    runtime::{execute, Runtime},
    store::Store,
};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::process::{Child, Command};

pub struct Managed {
    pub child: Child,
    pub config: Config,
    pub job: String,
    pub log: PathBuf,
}
impl Managed {
    pub fn status(&mut self) -> Result<Value, String> {
        let exit = self.child.try_wait().map_err(|e| e.to_string())?;
        Ok(
            json!({"state":if exit.is_none(){"running"}else{"exited"},"engine":"llama_cpu","job":self.job,"model":self.config.model_name}),
        )
    }
    pub fn logs(&self) -> Result<String, String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = File::open(&self.log).map_err(|e| e.to_string())?;
        let len = f.metadata().map_err(|e| e.to_string())?.len();
        f.seek(SeekFrom::Start(len.saturating_sub(65536)))
            .map_err(|e| e.to_string())?;
        let mut b = vec![];
        f.read_to_end(&mut b).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }
    pub async fn stop(&mut self) -> Result<(), String> {
        if self.child.try_wait().map_err(|e| e.to_string())?.is_none() {
            self.child.kill().await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
pub async fn start_local(
    r: &Runtime,
    c: &Config,
    id: &str,
    store: Arc<Store>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let binary = std::env::var("PHYXI_LLAMA_SERVER")
        .map_err(|_| "本地 CPU 后端未启用；设置 PHYXI_LLAMA_SERVER 为 llama-server 绝对路径")?;
    if !std::path::Path::new(&binary).is_absolute() {
        return Err("llama-server 必须使用绝对路径".into());
    }
    if !std::path::Path::new(&c.model_path).is_file() {
        return Err("GGUF 文件不存在".into());
    }
    // This developer backend cannot take over an existing Docker model.
    if let Ok(Some(_)) = r.inspect().await {
        return Err("已有 Docker 模型，请先停止".into());
    }
    store.update(id, "running", "启动 CPU 小模型（2 线程、单并发、无 GPU）")?;
    let log = r.data.join("local-model.log");
    let f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&log)
        .map_err(|e| e.to_string())?;
    let child = Command::new(binary)
        .args([
            "--model",
            &c.model_path,
            "--alias",
            &c.model_name,
            "--host",
            "127.0.0.1",
            "--port",
            &c.port.to_string(),
            "--ctx-size",
            &c.max_model_len.to_string(),
            "--parallel",
            "1",
            "--threads",
            "2",
            "--threads-batch",
            "2",
            "--n-gpu-layers",
            "0",
            "--no-webui",
            "--log-disable",
        ])
        .stdin(Stdio::null())
        .stdout(f.try_clone().map_err(|e| e.to_string())?)
        .stderr(f)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    *r.local.lock().await = Some(Managed {
        child,
        config: c.clone(),
        job: id.into(),
        log,
    });
    for _ in 0..60 {
        if cancel.load(Ordering::SeqCst) {
            return Err("任务已取消".into());
        }
        if r.local
            .lock()
            .await
            .as_mut()
            .ok_or("本地模型被停止")?
            .status()?["state"]
            != "running"
        {
            return Err("CPU 模型启动失败，请查看日志".into());
        }
        let mut cmd = Command::new("curl");
        cmd.args([
            "--silent",
            "--show-error",
            "--fail",
            "--noproxy",
            "*",
            "--max-time",
            "2",
            &format!("http://127.0.0.1:{}/health", c.port),
        ]);
        if execute(cmd, None, 4).await.is_ok() {
            store.event(id, "CPU 模型健康；请继续执行接口验收")?;
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err("CPU 模型健康检查超时".into())
}
