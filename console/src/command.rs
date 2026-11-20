//! Subprocess execution and bounded output capture.
use crate::store::Store;
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

async fn read<R: tokio::io::AsyncRead + Unpin>(
    mut r: R,
    log: Option<(Arc<Store>, String)>,
) -> Result<String, String> {
    let mut buf = [0u8; 2048];
    let mut out = String::new();
    loop {
        let n = r.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let s = String::from_utf8_lossy(&buf[..n]);
        if let Some((store, id)) = &log {
            store.event(id, &s)?;
        }
        if out.len() < 65536 {
            out.push_str(&s);
        }
    }
    Ok(out)
}
pub(crate) async fn execute(
    mut cmd: Command,
    log: Option<(Arc<Store>, String)>,
    seconds: u64,
) -> Result<String, String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "无法启动 {}：{e}",
            cmd.as_std().get_program().to_string_lossy()
        )
    })?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let work = async {
        let (status, out, err) =
            tokio::join!(child.wait(), read(stdout, log.clone()), read(stderr, log));
        let status = status.map_err(|e| e.to_string())?;
        let out = out?;
        let err = err?;
        if !status.success() {
            return Err(format!("命令退出 {status}: {err}"));
        }
        Ok(format!("{out}{err}"))
    };
    tokio::time::timeout(Duration::from_secs(seconds), work)
        .await
        .map_err(|_| "命令执行超时".to_string())?
}
