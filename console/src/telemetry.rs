//! Bounded resource samples, independent from deployment progress. No model memory in control plane.
use crate::{
    runtime::{execute, Runtime},
    store::Store,
};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::process::Command;
pub fn start(store: Arc<Store>, runtime: Runtime) {
    tokio::spawn(async move {
        loop {
            let mut cmd = Command::new("ps");
            cmd.args([
                "-o",
                "rss=",
                "-o",
                "%cpu=",
                "-p",
                &std::process::id().to_string(),
            ]);
            if let Ok(s) = execute(cmd, None, 5).await {
                let values: Vec<_> = s.split_whitespace().collect();
                if values.len() == 2 {
                    let rss = values[0].parse::<u64>().unwrap_or(0);
                    let cpu = values[1].parse::<f64>().unwrap_or(0.0);
                    let gpu = runtime.gpus().await.ok();
                    let mut samples = store
                        .setting("telemetry")
                        .ok()
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    samples.push(json!({"unix":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),"console_rss_kib":rss,"console_cpu_percent":cpu,"gpus":gpu}));
                    if samples.len() > 120 {
                        samples.drain(..samples.len() - 120);
                    }
                    let _ = store.set_setting("telemetry", &json!(samples));
                }
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}
pub fn prometheus(store: &Store) -> Result<String, String> {
    let samples = store.setting("telemetry")?;
    let last = samples.as_array().and_then(|a| a.last());
    let mut out=String::from("# HELP phyxi_console_rss_bytes Resident memory of the control plane\n# TYPE phyxi_console_rss_bytes gauge\n");
    if let Some(v) = last {
        out.push_str(&format!(
            "phyxi_console_rss_bytes {}\nphyxi_console_cpu_percent {}\n",
            v["console_rss_kib"].as_u64().unwrap_or(0) * 1024,
            v["console_cpu_percent"].as_f64().unwrap_or(0.0)
        ));
    }
    for state in ["running", "succeeded", "failed", "cancelled", "interrupted"] {
        let n = store.jobs()?.iter().filter(|j| j.status == state).count();
        out.push_str(&format!("phyxi_recent_jobs{{status=\"{state}\"}} {n}\n"));
    }
    Ok(out)
}
