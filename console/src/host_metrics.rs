//! Small, read-only host samples; unknown values stay null instead of becoming zero.
use crate::runtime::execute;
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};
use tokio::process::Command;
async fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut c = Command::new(program);
    c.args(args).env("LC_ALL", "C");
    execute(c, None, 5).await.ok()
}
fn linux_memory(s: &str) -> Option<Value> {
    let values: HashMap<_, _> = s
        .lines()
        .filter_map(|line| {
            let (key, rest) = line.split_once(':')?;
            Some((
                key,
                rest.split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()?
                    .checked_mul(1024)?,
            ))
        })
        .collect();
    let total = *values.get("MemTotal")?;
    let available = *values.get("MemAvailable")?;
    if total == 0 || available > total {
        return None;
    }
    Some(
        json!({"total_bytes":total,"available_bytes":available,"used_bytes":total-available,"estimated":false}),
    )
}
fn mac_memory(s: &str, total: u64) -> Option<Value> {
    let page_size = s
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    let values: HashMap<_, _> = s
        .lines()
        .skip(1)
        .filter_map(|line| {
            let (key, rest) = line.split_once(':')?;
            Some((key, rest.trim().trim_end_matches('.').parse::<u64>().ok()?))
        })
        .collect();
    let pages = values
        .get("Pages free")?
        .checked_add(*values.get("Pages inactive")?)?
        .checked_add(*values.get("Pages speculative")?)?;
    let available = pages.checked_mul(page_size)?;
    if total == 0 || available > total {
        return None;
    }
    Some(
        json!({"total_bytes":total,"available_bytes":available,"used_bytes":total-available,"estimated":true}),
    )
}
fn disk(s: &str) -> Option<Value> {
    let fields: Vec<_> = s.lines().nth(1)?.split_whitespace().collect();
    if fields.len() < 6 {
        return None;
    }
    Some(
        json!({"total_bytes":fields[1].parse::<u64>().ok()?.checked_mul(1024)?,"used_bytes":fields[2].parse::<u64>().ok()?.checked_mul(1024)?,"available_bytes":fields[3].parse::<u64>().ok()?.checked_mul(1024)?,"used_percent":fields[4].trim_end_matches('%').parse::<u8>().ok()?}),
    )
}
fn load(s: &str) -> Option<Vec<f64>> {
    let suffix = s
        .split("load averages:")
        .nth(1)
        .or_else(|| s.split("load average:").nth(1))?;
    let values: Vec<f64> = suffix
        .replace(',', " ")
        .split_whitespace()
        .take(3)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if values.len() != 3 || values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    Some(values)
}
pub async fn sample(data: &Path) -> Value {
    let data_path = data.display().to_string();
    let disk_args = ["-kP", data_path.as_str()];
    let (disk_raw, load_raw) = tokio::join!(output("df", &disk_args), output("uptime", &[]));
    let memory = if cfg!(target_os = "macos") {
        let (raw, total) = tokio::join!(
            output("vm_stat", &[]),
            output("sysctl", &["-n", "hw.memsize"])
        );
        raw.zip(total)
            .and_then(|(s, n)| mac_memory(&s, n.trim().parse().ok()?))
    } else {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| linux_memory(&s))
    };
    json!({"memory":memory,"disk":disk_raw.as_deref().and_then(disk),"disk_path":data,"load":load_raw.as_deref().and_then(load),"logical_cpus":std::thread::available_parallelism().ok().map(|n|n.get()),"sampled_unix":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_units_and_unavailable_values() {
        let l = linux_memory("MemTotal: 1000 kB\nMemAvailable: 400 kB\n").unwrap();
        assert_eq!(l["used_bytes"], 600 * 1024);
        assert!(linux_memory("MemTotal: 1000 kB").is_none());
        let m=mac_memory("Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 10.\nPages inactive: 20.\nPages speculative: 2.\n",64*16384).unwrap();
        assert_eq!(m["available_bytes"], 32 * 16384);
        assert_eq!(m["estimated"], true);
        assert!(mac_memory("error", 1000).is_none());
    }
    #[test]
    fn disk_units_and_load_platforms() {
        assert_eq!(disk("Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/test 100 30 70 30% /\n").unwrap()["used_bytes"],30*1024);
        assert_eq!(
            load("up 2 days, load average: 0.25, 1.50, 2.00").unwrap(),
            vec![0.25, 1.5, 2.]
        );
        assert_eq!(
            load("up 2 days, load averages: 0.25 1.50 2.00").unwrap(),
            vec![0.25, 1.5, 2.]
        );
        assert!(disk("unavailable").is_none());
        assert!(load("load average: NaN, 1, 2").is_none());
    }
}
