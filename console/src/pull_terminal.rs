//! Capture Docker's real terminal progress while keeping argv execution and cancellation ownership.
use crate::{download_progress::Progress, store::Store};
use std::{
    fs::File,
    io::{self, Read},
    os::fd::FromRawFd,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{io::unix::AsyncFd, process::Command};
fn terminal() -> io::Result<(File, File)> {
    let mut master = -1;
    let mut slave = -1;
    let mut size = libc::winsize {
        ws_row: 60,
        ws_col: 180,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // openpty writes two new owned descriptors; no pointers refer to temporary Rust storage.
    if unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(size),
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    // Ownership is transferred exactly once; all subsequent failures close both descriptors.
    let pair = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
    for fd in [master, slave] {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let flags = unsafe { libc::fcntl(master, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pair)
}
async fn collect(file: File, store: Arc<Store>, id: &str) -> Result<String, String> {
    // SAFETY: AsyncFd exclusively owns this File and its descriptor stays open and stable.
    let fd = unsafe { AsyncFd::register(file) }.map_err(|e| e.to_string())?;
    let key = format!("progress:{id}");
    let mut p: Progress =
        serde_json::from_value(store.setting(&key)?).map_err(|e| e.to_string())?;
    let mut last = Instant::now();
    let mut previous = None;
    let mut tail = String::new();
    loop {
        let mut guard = fd.readable().await.map_err(|e| e.to_string())?;
        let mut buf = [0u8; 2048];
        let n = match guard.try_io(|inner| {
            let mut f = inner.get_ref();
            f.read(&mut buf)
        }) {
            Err(_) => continue,
            Ok(Err(e)) if e.raw_os_error() == Some(libc::EIO) => break,
            Ok(value) => value.map_err(|e| e.to_string())?,
        };
        if n == 0 {
            break;
        }
        let part = String::from_utf8_lossy(&buf[..n]);
        p.feed(&part);
        tail.push_str(&part);
        if tail.len() > 16384 {
            tail = tail
                .chars()
                .rev()
                .take(8192)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
        }
        if last.elapsed() >= Duration::from_secs(1) {
            let now = p.network_bytes();
            p.speed_bps = previous
                .map(|old: u64| now.saturating_sub(old) as f64 / last.elapsed().as_secs_f64());
            previous = Some(now);
            last = Instant::now();
            p.updated_unix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            p.measure();
            store.set_setting(&key, &serde_json::to_value(&p).map_err(|e| e.to_string())?)?;
        }
    }
    p.feed("\n");
    p.measure();
    p.speed_bps = None;
    p.updated_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    store.set_setting(&key, &serde_json::to_value(&p).map_err(|e| e.to_string())?)?;
    // Persist one concise final snapshot; avoid storing every terminal redraw as a log entry.
    store.event(
        id,
        &format!(
            "镜像层状态：{}",
            serde_json::to_string(&p.layers).map_err(|e| e.to_string())?
        ),
    )?;
    Ok(tail)
}
pub async fn execute(mut cmd: Command, store: Arc<Store>, id: &str) -> Result<(), String> {
    let (master, slave) = terminal().map_err(|e| format!("无法读取 Docker 下载进度：{e}"))?;
    cmd.stdin(Stdio::null())
        .stdout(slave.try_clone().map_err(|e| e.to_string())?)
        .stderr(slave)
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    // Drop Command's copies of the slave FDs so read reaches EOF when Docker exits.
    drop(cmd);
    let (status, output) = tokio::try_join!(
        async { child.wait().await.map_err(|e| e.to_string()) },
        collect(master, store, id)
    )?;
    if !status.success() {
        return Err(format!("Docker 下载退出 {status}: {output}"));
    }
    Ok(())
}
