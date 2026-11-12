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
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS config (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, action TEXT NOT NULL, status TEXT NOT NULL, step TEXT NOT NULL, created TEXT DEFAULT CURRENT_TIMESTAMP, config TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY AUTOINCREMENT, job_id TEXT NOT NULL, message TEXT NOT NULL);
        UPDATE jobs SET status='interrupted', step='控制台重启；请检查运行状态，停止后再部署' WHERE status='running';").map_err(|e|e.to_string())?;
        Ok(Self(Mutex::new(c)))
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
