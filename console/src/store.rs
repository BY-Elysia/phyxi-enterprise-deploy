use crate::config::Config;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{path::Path, sync::Mutex};

pub struct Store(Mutex<Connection>);
#[derive(Debug, Serialize)]
pub struct Job {
    pub id: String,
    pub action: String,
    pub status: String,
    pub step: String,
    pub created: String,
    pub config: Config,
}
#[derive(Serialize)]
pub struct Event {
    pub seq: i64,
    pub message: String,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let c = Connection::open(path).map_err(|e| e.to_string())?;
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS audit (seq INTEGER PRIMARY KEY AUTOINCREMENT, actor TEXT NOT NULL, action TEXT NOT NULL, status INTEGER NOT NULL, created TEXT DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE IF NOT EXISTS monitor_samples (unix INTEGER PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS gpu_monitor_samples (unix INTEGER PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS config (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, action TEXT NOT NULL, status TEXT NOT NULL, step TEXT NOT NULL, created TEXT DEFAULT CURRENT_TIMESTAMP, config TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY AUTOINCREMENT, job_id TEXT NOT NULL, message TEXT NOT NULL);
        UPDATE jobs SET status='interrupted', step='控制台重启；请检查运行状态，停止后再部署' WHERE status='running';").map_err(|e|e.to_string())?;
        let has_source: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('audit') WHERE name='source')",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !has_source {
            c.execute("ALTER TABLE audit ADD COLUMN source TEXT", [])
                .map_err(|e| e.to_string())?;
        }
        Ok(Self(Mutex::new(c)))
    }
    pub fn monitor_sample(
        &self,
        sample: &serde_json::Value,
        metadata: &serde_json::Value,
    ) -> Result<(), String> {
        let unix = sample["unix"].as_i64().ok_or("missing sample timestamp")?;
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO monitor_samples(unix,value) VALUES(?1,?2)",
            params![unix, sample.to_string()],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM monitor_samples WHERE unix < ?1 OR unix NOT IN (SELECT unix FROM monitor_samples ORDER BY unix DESC LIMIT 5760)",[unix-86400]).map_err(|e|e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES('monitor_metadata',?1)",
            [metadata.to_string()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn monitor_history(&self) -> Result<serde_json::Value, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut statement=c.prepare("SELECT value FROM monitor_samples WHERE unix >= CAST(strftime('%s','now') AS INTEGER)-86400 ORDER BY unix").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let samples = rows
            .map(|r| {
                r.map_err(|e| e.to_string()).and_then(|s| {
                    serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string())
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let raw = c
            .query_row(
                "SELECT value FROM settings WHERE key='monitor_metadata'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "{}".into());
        let metadata: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"samples":samples,"metadata":metadata}))
    }
    pub fn record_telemetry(&self, samples: &[serde_json::Value], unix: i64) -> Result<(), String> {
        let latest = samples.last().ok_or("missing telemetry sample")?;
        let mut connection = self.0.lock().map_err(|e| e.to_string())?;
        let tx = connection.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES('telemetry',?1)",
            [serde_json::to_string(samples).map_err(|e| e.to_string())?],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT OR REPLACE INTO gpu_monitor_samples(unix,value) VALUES(?1,?2)",
            params![unix, latest["gpus"].to_string()],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM gpu_monitor_samples WHERE unix < ?1",
            [unix - 86400],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn gpu_history(&self, seconds: i64, bucket: i64) -> Result<serde_json::Value, String> {
        let connection = self.0.lock().map_err(|e| e.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT unix,value FROM gpu_monitor_samples WHERE unix IN (
                    SELECT MAX(unix) FROM gpu_monitor_samples
                    WHERE unix BETWEEN CAST(strftime('%s','now') AS INTEGER)-?1
                                   AND CAST(strftime('%s','now') AS INTEGER)
                    GROUP BY unix / ?2
                ) ORDER BY unix",
            )
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params![seconds, bucket], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let samples = rows
            .map(|row| {
                let (unix, raw) = row.map_err(|e| e.to_string())?;
                let gpus: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({"unix":unix,"gpus":gpus}))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(serde_json::json!({"samples":samples,"bucket_seconds":bucket}))
    }
    pub fn audit_source(
        &self,
        actor: &str,
        action: &str,
        status: u16,
        source: Option<&str>,
    ) -> Result<(), String> {
        let source = source
            .and_then(|s| s.parse::<std::net::IpAddr>().ok())
            .map(|ip| ip.to_string());
        let c = self.0.lock().map_err(|e| e.to_string())?;
        c.execute(
            "INSERT INTO audit(actor,action,status,source) VALUES(?1,?2,?3,?4)",
            params![
                actor,
                action.chars().take(256).collect::<String>(),
                status,
                source
            ],
        )
        .map_err(|e| e.to_string())?;
        c.execute("DELETE FROM audit WHERE seq NOT IN (SELECT seq FROM audit ORDER BY seq DESC LIMIT 5000)",[]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn audit_rows(&self) -> Result<serde_json::Value, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut q = c
            .prepare(
                "SELECT actor,action,status,created,source FROM audit ORDER BY seq DESC LIMIT 200",
            )
            .map_err(|e| e.to_string())?;
        let rows=q.query_map([],|r|Ok(serde_json::json!({"actor":r.get::<_,String>(0)?,"action":r.get::<_,String>(1)?,"status":r.get::<_,u16>(2)?,"created":r.get::<_,String>(3)?,"source":r.get::<_,Option<String>>(4)?}))).map_err(|e|e.to_string())?;
        Ok(serde_json::Value::Array(
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        ))
    }
    pub fn setting(&self, key: &str) -> Result<serde_json::Value, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        match c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get::<_, String>(0)
        }) {
            Ok(v) => serde_json::from_str(&v).map_err(|e| e.to_string()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(serde_json::Value::Null),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), String> {
        self.0.lock().map_err(|e|e.to_string())?.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value.to_string()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn has_legacy_config(&self) -> Result<bool, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row("SELECT EXISTS(SELECT 1 FROM config)", [], |r| r.get(0))
            .map_err(|e| e.to_string())
    }
    pub fn recent_events(&self, id: &str) -> Result<Vec<Event>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut statement=c.prepare("SELECT seq,message FROM (SELECT seq,message FROM events WHERE job_id=?1 ORDER BY seq DESC LIMIT 200) ORDER BY seq").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([id], |r| {
                Ok(Event {
                    seq: r.get(0)?,
                    message: r.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    pub fn job_config(&self, id: &str) -> Result<Config, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let s: String = c
            .query_row("SELECT config FROM jobs WHERE id=?1", [id], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }
    pub fn config(&self) -> Result<Config, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let r = c.query_row("SELECT value FROM config WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        });
        match r {
            Ok(v) => serde_json::from_str(&v).map_err(|e| e.to_string()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Config::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self, cfg: &Config) -> Result<(), String> {
        self.0.lock().map_err(|e|e.to_string())?.execute("INSERT INTO config VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value",[serde_json::to_string(cfg).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn create(&self, id: &str, action: &str, cfg: &Config) -> Result<(), String> {
        self.0.lock().map_err(|e|e.to_string())?.execute("INSERT INTO jobs(id,action,status,step,config) VALUES(?1,?2,'running','等待执行',?3)",params![id,action,serde_json::to_string(cfg).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn update(&self, id: &str, status: &str, step: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE jobs SET status=?2,step=?3 WHERE id=?1",
                params![id, status, step],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn event(&self, id: &str, message: &str) -> Result<(), String> {
        // Bounded records, bounded per-record size. UI uses a cursor for incremental reads.
        let message: String = message.chars().take(4096).collect();
        let c = self.0.lock().map_err(|e| e.to_string())?;
        c.execute(
            "INSERT INTO events(job_id,message) VALUES(?1,?2)",
            params![id, message],
        )
        .map_err(|e| e.to_string())?;
        let key = format!("progress:{id}");
        let progress: Option<String> = c
            .query_row("SELECT value FROM settings WHERE key=?1", [&key], |r| {
                r.get(0)
            })
            .ok();
        if let Some(raw) = progress {
            let mut progress: crate::download_progress::Progress =
                serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            progress.feed(&message);
            c.execute(
                "UPDATE settings SET value=?2 WHERE key=?1",
                params![
                    key,
                    serde_json::to_string(&progress).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        c.execute("DELETE FROM events WHERE job_id=?1 AND seq NOT IN (SELECT seq FROM events WHERE job_id=?1 ORDER BY seq DESC LIMIT 2000)",[id]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn jobs(&self) -> Result<Vec<Job>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut s=c.prepare("SELECT id,action,status,step,created,config FROM jobs ORDER BY rowid DESC LIMIT 30").map_err(|e|e.to_string())?;
        let rows = s
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = vec![];
        for row in rows {
            let (id, action, status, step, created, config) = row.map_err(|e| e.to_string())?;
            out.push(Job {
                id,
                action,
                status,
                step,
                created,
                config: serde_json::from_str(&config).map_err(|e| e.to_string())?,
            });
        }
        Ok(out)
    }
    pub fn events(&self, id: &str, after: i64) -> Result<Vec<Event>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut s = c
            .prepare(
                "SELECT seq,message FROM events WHERE job_id=?1 AND seq>?2 ORDER BY seq LIMIT 200",
            )
            .map_err(|e| e.to_string())?;
        let rows = s
            .query_map(params![id, after], |r| {
                Ok(Event {
                    seq: r.get(0)?,
                    message: r.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_history_retains_nulls_and_survives_restart_with_bounded_retention() {
        let path = std::env::temp_dir().join(format!("phyxi-monitor-{}.db", uuid::Uuid::new_v4()));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        {
            let store = Store::open(&path).unwrap();
            store
                .monitor_sample(
                    &serde_json::json!({"unix":now-90000,"cpu_percent":99}),
                    &serde_json::json!({}),
                )
                .unwrap();
            store
                .monitor_sample(
                    &serde_json::json!({"unix":now,"cpu_percent":null}),
                    &serde_json::json!({"scope":"local"}),
                )
                .unwrap();
            assert_eq!(
                store
                    .0
                    .lock()
                    .unwrap()
                    .query_row("SELECT count(*) FROM monitor_samples", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
        {
            let store = Store::open(&path).unwrap();
            let history = store.monitor_history().unwrap();
            assert_eq!(history["samples"].as_array().unwrap().len(), 1);
            assert!(history["samples"][0]["cpu_percent"].is_null());
            assert_eq!(history["metadata"]["scope"], "local");
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn gpu_history_retains_latest_sample_per_bucket_and_ages_out_old_rows() {
        let path = std::env::temp_dir().join(format!("phyxi-gpu-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        let now: i64 = store
            .0
            .lock()
            .unwrap()
            .query_row("SELECT CAST(strftime('%s','now') AS INTEGER)", [], |r| {
                r.get(0)
            })
            .unwrap();
        let bucket_start = now / 60 * 60 - 60;
        for (unix, value) in [
            (now - 90000, serde_json::json!([])),
            (
                bucket_start,
                serde_json::json!([{"utilization_percent":17}]),
            ),
            (
                bucket_start + 1,
                serde_json::json!([{"utilization_percent":0}]),
            ),
            (now, serde_json::Value::Null),
        ] {
            store
                .record_telemetry(&[serde_json::json!({"gpus":value})], unix)
                .unwrap();
        }
        drop(store);
        {
            let store = Store::open(&path).unwrap();
            let history = store.gpu_history(21600, 60).unwrap();
            let samples = history["samples"].as_array().unwrap();
            assert_eq!(samples.len(), 2);
            assert_eq!(samples[0]["gpus"][0]["utilization_percent"], 0);
            assert!(samples[1]["gpus"].is_null());
            let rows: u32 = store
                .0
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM gpu_monitor_samples", [], |r| r.get(0))
                .unwrap();
            assert_eq!(rows, 3);
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn snapshots_and_restart_are_durable() {
        let path = std::env::temp_dir().join(format!("phyxi-{}.db", uuid::Uuid::new_v4()));
        let c = crate::config::fixture();
        {
            let s = Store::open(&path).unwrap();
            s.save(&c).unwrap();
            s.create("one", "start", &c).unwrap();
            s.event("one", "step 1").unwrap();
            let mut changed = c.clone();
            changed.max_model_len = 4096;
            s.save(&changed).unwrap();
            assert_eq!(s.jobs().unwrap()[0].config, c);
        }
        {
            let s = Store::open(&path).unwrap();
            assert_eq!(s.jobs().unwrap()[0].status, "interrupted");
            assert_eq!(s.config().unwrap().max_model_len, 4096);
            let e = s.events("one", 0).unwrap();
            assert_eq!(e.len(), 1);
            assert!(s.events("one", e[0].seq).unwrap().is_empty());
        }
        let _ = std::fs::remove_file(path);
    }
}
