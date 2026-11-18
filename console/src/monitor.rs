//! Read-only, bounded server monitoring. Missing/reset counters produce gaps, never fake zeroes.
use crate::{host_metrics, runtime::execute, store::Store};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::process::Command;
type Counters = BTreeMap<String, (u64, u64)>;
async fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args).env("LC_ALL", "C");
    execute(command, None, 5).await.ok()
}
fn linux_cpu(raw: &str) -> Option<(u64, u64)> {
    let line = raw.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if v.len() < 4 {
        return None;
    }
    Some((
        v.iter().try_fold(0u64, |a, b| a.checked_add(*b))?,
        v[3].checked_add(*v.get(4).unwrap_or(&0))?,
    ))
}
fn cpu_delta(previous: Option<(u64, u64)>, current: Option<(u64, u64)>) -> Option<f64> {
    let (p, c) = previous.zip(current)?;
    let total = c.0.checked_sub(p.0)?;
    let idle = c.1.checked_sub(p.1)?;
    if total == 0 || idle > total {
        return None;
    }
    Some(100.0 * (total - idle) as f64 / total as f64)
}
fn mac_cpu(raw: &str) -> Option<f64> {
    let line = raw.lines().filter(|s| s.starts_with("CPU usage:")).nth(1)?;
    let idle = line
        .split(',')
        .next_back()?
        .trim()
        .split('%')
        .next()?
        .parse::<f64>()
        .ok()?;
    (idle.is_finite() && (0.0..=100.0).contains(&idle)).then_some(100.0 - idle)
}
fn linux_network(raw: &str) -> Option<Counters> {
    let mut out = Counters::new();
    for line in raw.lines().skip(2) {
        let (name, values) = line.split_once(':')?;
        if name.trim() == "lo" {
            continue;
        }
        let v: Vec<_> = values.split_whitespace().collect();
        out.insert(
            name.trim().into(),
            (v.first()?.parse().ok()?, v.get(8)?.parse().ok()?),
        );
    }
    (!out.is_empty()).then_some(out)
}
fn mac_network(raw: &str) -> Option<Counters> {
    let mut out = Counters::new();
    for line in raw.lines().skip(1) {
        let v: Vec<_> = line.split_whitespace().collect();
        if v.len() < 10 || !v[2].starts_with("<Link#") || v[0].starts_with("lo") {
            continue;
        }
        // The optional link address changes the prefix length; counters are fixed at the end.
        let n = v.len();
        out.insert(
            v[0].trim_end_matches('*').into(),
            (v[n - 5].parse().ok()?, v[n - 2].parse().ok()?),
        );
    }
    (!out.is_empty()).then_some(out)
}
fn linux_disk(raw: &str, devices: &[String]) -> Option<Counters> {
    let mut out = Counters::new();
    for line in raw.lines() {
        let v: Vec<_> = line.split_whitespace().collect();
        if v.len() < 10 || !devices.iter().any(|d| d == v[2]) {
            continue;
        }
        out.insert(
            v[2].into(),
            (
                v[5].parse::<u64>().ok()?.checked_mul(512)?,
                v[9].parse::<u64>().ok()?.checked_mul(512)?,
            ),
        );
    }
    (!out.is_empty()).then_some(out)
}
fn mac_disk(raw: &str) -> Option<Counters> {
    let mut out = Counters::new();
    let mut device = String::new();
    for line in raw.lines() {
        if line.contains("<class IOBlockStorageDriver,") {
            device = line.split("id ").nth(1)?.split(',').next()?.into();
        }
        if line.contains("\"Statistics\" =") && !device.is_empty() {
            let value = |key: &str| -> Option<u64> {
                line.split(&format!("\"{key}\"="))
                    .nth(1)?
                    .split([',', '}'])
                    .next()?
                    .parse()
                    .ok()
            };
            out.insert(
                device.clone(),
                (value("Bytes (Read)")?, value("Bytes (Write)")?),
            );
        }
    }
    (!out.is_empty()).then_some(out)
}
fn rates(
    previous: Option<&Counters>,
    current: Option<&Counters>,
    seconds: f64,
) -> Option<(f64, f64)> {
    let (p, c) = previous.zip(current)?;
    if seconds <= 0.0 || seconds > 60.0 || !p.keys().eq(c.keys()) {
        return None;
    }
    let (mut a, mut b) = (0u64, 0u64);
    for (name, (x, y)) in c {
        let old = p.get(name)?;
        a = a.checked_add(x.checked_sub(old.0)?)?;
        b = b.checked_add(y.checked_sub(old.1)?)?;
    }
    Some((a as f64 / seconds, b as f64 / seconds))
}
pub fn start(store: Arc<Store>, data: PathBuf) {
    tokio::spawn(async move {
        let mut previous_cpu = None;
        let (mut previous_disk, mut previous_network) = (None, None);
        let mut previous_time = Instant::now();
        let mut timer = tokio::time::interval(Duration::from_secs(15));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            let (metrics, cpu, disk, network) = if cfg!(target_os = "macos") {
                let (m, c, d, n) = tokio::join!(
                    host_metrics::sample(&data),
                    output("top", &["-l", "2", "-s", "1", "-n", "0"]),
                    output("ioreg", &["-r", "-c", "IOBlockStorageDriver"]),
                    output("netstat", &["-ibn"])
                );
                (
                    m,
                    c.as_deref().and_then(mac_cpu),
                    d.as_deref().and_then(mac_disk),
                    n.as_deref().and_then(mac_network),
                )
            } else {
                let m = host_metrics::sample(&data).await;
                let counters = std::fs::read_to_string("/proc/stat")
                    .ok()
                    .as_deref()
                    .and_then(linux_cpu);
                let cpu = cpu_delta(previous_cpu, counters);
                previous_cpu = counters;
                // Only whole physical devices: exclude partitions, loop and device-mapper duplicates.
                let devices: Vec<String> = std::fs::read_dir("/sys/block")
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .filter(|e| e.path().join("device").exists())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect();
                let d = std::fs::read_to_string("/proc/diskstats")
                    .ok()
                    .and_then(|s| linux_disk(&s, &devices));
                let n = std::fs::read_to_string("/proc/net/dev")
                    .ok()
                    .as_deref()
                    .and_then(linux_network);
                (m, cpu, d, n)
            };
            let seconds = previous_time.elapsed().as_secs_f64();
            previous_time = Instant::now();
            let disk_rate = rates(previous_disk.as_ref(), disk.as_ref(), seconds);
            let network_rate = rates(previous_network.as_ref(), network.as_ref(), seconds);
            previous_disk = disk;
            previous_network = network;
            let used = metrics["memory"]["used_bytes"].as_f64();
            let total = metrics["memory"]["total_bytes"].as_f64();
            let percent = used
                .zip(total)
                .filter(|(_, t)| *t > 0.0)
                .map(|(u, t)| u / t * 100.0);
            let sample = json!({"unix":metrics["sampled_unix"],"cpu_percent":cpu,"memory_percent":percent,"memory_used_bytes":metrics["memory"]["used_bytes"],"memory_total_bytes":metrics["memory"]["total_bytes"],"disk_read_bps":disk_rate.map(|r|r.0),"disk_write_bps":disk_rate.map(|r|r.1),"network_receive_bps":network_rate.map(|r|r.0),"network_send_bps":network_rate.map(|r|r.1)});
            let metadata = json!({"metrics":metrics,"platform":std::env::consts::OS,"scope":if crate::executor::enabled(){"host"}else if std::path::Path::new("/.dockerenv").exists(){"container"}else{"local"},"interval_seconds":15});
            let _ = store.monitor_sample(&sample, &metadata);
        }
    });
}
pub fn history(store: &Store) -> Result<Value, String> {
    store.monitor_history()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deltas_handle_missing_reset_and_device_changes() {
        assert_eq!(cpu_delta(Some((100, 50)), Some((200, 75))), Some(75.0));
        assert_eq!(cpu_delta(Some((200, 75)), Some((100, 50))), None);
        let p = BTreeMap::from([("eth0".into(), (100, 200))]);
        let c = BTreeMap::from([("eth0".into(), (200, 400))]);
        assert_eq!(rates(Some(&p), Some(&c), 10.0), Some((10.0, 20.0)));
        assert_eq!(rates(Some(&c), Some(&p), 10.0), None);
        assert_eq!(rates(None, Some(&c), 10.0), None);
        assert_eq!(rates(Some(&p), Some(&c), 120.0), None);
        assert_eq!(rates(Some(&p), Some(&BTreeMap::new()), 10.0), None);
    }
    #[test]
    fn platform_counters_do_not_double_count() {
        assert_eq!(linux_cpu("cpu 10 0 20 70 5 0 0 0 999 0"), Some((105, 75)));
        let n=mac_network("Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll\nen0 1500 <Link#1> aa:bb 1 0 100 2 0 200 0\nen0 1500 192.168 1.2.3.4 1 - 100 2 - 200 -\nlo0 16000 <Link#2> 1 0 50 2 0 60 0").unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n["en0"], (100, 200));
        assert_eq!(
            linux_disk(
                "8 0 sda 1 0 2 0 1 0 4 0\n8 1 sda1 1 0 2 0 1 0 4 0",
                &["sda".into()]
            )
            .unwrap()["sda"],
            (1024, 2048)
        );
        assert_eq!(
            mac_cpu("CPU usage: 1% user, 1% sys, 98% idle\nCPU usage: 10% user, 5% sys, 85% idle"),
            Some(15.0)
        );
        assert_eq!(mac_disk("+-o IOBlockStorageDriver <class IOBlockStorageDriver, id 0x123, active>\n\"Statistics\" = {\"Bytes (Read)\"=100,\"Bytes (Write)\"=200}").unwrap()["0x123"],(100,200));
    }
}
