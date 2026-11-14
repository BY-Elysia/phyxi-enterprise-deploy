//! Password login for management only. No inference request depends on these sessions.
use crate::App;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

struct Session {
    csrf: String,
    expires: u64,
}
#[derive(Default)]
struct Memory {
    sessions: HashMap<String, Session>,
    attempts: VecDeque<u64>,
}
pub struct Admin {
    hash: String,
    memory: Mutex<Memory>,
    hashing: Arc<Semaphore>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn cookie_name() -> &'static str {
    if insecure_local() {
        "phyxi_session"
    } else {
        "__Host-phyxi_session"
    }
}
fn insecure_local() -> bool {
    std::env::var("PHYXI_LOCAL_HTTP_LOGIN").as_deref() == Ok("1")
}
fn sid(h: &HeaderMap) -> Option<&str> {
    h.get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| s.trim().strip_prefix(&format!("{}=", cookie_name())))
}
fn source(h: &HeaderMap) -> Option<&str> {
    if std::env::var("PHYXI_TRUST_GATEWAY").as_deref() == Ok("1") {
        h.get("x-phyxi-client-ip").and_then(|v| v.to_str().ok())
    } else {
        None
    }
}
fn error(code: StatusCode, message: &str) -> Response {
    (code, Json(json!({"error":message}))).into_response()
}

impl Admin {
    pub fn load(data: &Path) -> Result<Option<Arc<Self>>, String> {
        let path = data.join("admin.hash");
        if !path.exists() {
            return Ok(None);
        }
        let hash = fs::read_to_string(path).map_err(|e| e.to_string())?;
        PasswordHash::new(hash.trim()).map_err(|_| "管理员密码文件损坏")?;
        Ok(Some(Arc::new(Self {
            hash: hash.trim().into(),
            memory: Mutex::new(Memory::default()),
            hashing: Arc::new(Semaphore::new(1)),
        })))
    }
    pub fn authorize(&self, h: &HeaderMap, write: bool) -> bool {
        let Ok(mut m) = self.memory.lock() else {
            return false;
        };
        m.sessions.retain(|_, s| s.expires > now());
        let Some(s) = sid(h).and_then(|id| m.sessions.get(id)) else {
            return false;
        };
        !write || h.get("x-csrf-token").and_then(|v| v.to_str().ok()) == Some(s.csrf.as_str())
    }
}
pub fn set_password(data: &Path, password: &str) -> Result<(), String> {
    let minimum = if insecure_local() { 8 } else { 12 };
    if !(minimum..=256).contains(&password.len()) {
        return Err(format!("管理员密码需要 {minimum}–256 字节"));
    }
    let salt =
        SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).map_err(|e| e.to_string())?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();
    let path = data.join("admin.hash.new");
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    f.write_all(hash.as_bytes()).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    fs::rename(path, data.join("admin.hash")).map_err(|e| e.to_string())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    #[serde(default = "admin_username")]
    username: String,
    password: String,
}
fn admin_username() -> String {
    "admin".into()
}
fn origin_allowed(h: &HeaderMap) -> bool {
    if h.get("sec-fetch-site").is_some_and(|v| v == "cross-site") {
        return false;
    }
    let Some(origin) = h.get("origin") else {
        return true;
    };
    let Some(host) = h.get("host").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    origin.to_str().is_ok_and(|s| {
        s == format!("https://{host}")
            || (insecure_local()
                && (host.starts_with("127.0.0.1:") || host.starts_with("localhost:"))
                && s == format!("http://{host}"))
    })
}
pub async fn info(State(app): State<App>, headers: HeaderMap) -> Response {
    let Some(admin) = app.admin else {
        return Json(json!({"mode":"token"})).into_response();
    };
    let m = admin.memory.lock().unwrap();
    let csrf = sid(&headers)
        .and_then(|id| m.sessions.get(id))
        .filter(|s| s.expires > now())
        .map(|s| s.csrf.clone());
    Json(json!({"mode":"password","authenticated":csrf.is_some(),"csrf":csrf})).into_response()
}
pub async fn login(
    State(app): State<App>,
    headers: HeaderMap,
    Json(body): Json<Login>,
) -> Response {
    let Some(admin) = app.admin else {
        return error(StatusCode::CONFLICT, "请先通过服务器本机初始化管理员密码");
    };
    if !origin_allowed(&headers) {
        return error(StatusCode::FORBIDDEN, "登录来源不允许");
    }
    if body.password.len() > 256 {
        return error(StatusCode::UNAUTHORIZED, "账号或密码错误");
    }
    {
        let mut m = admin.memory.lock().unwrap();
        while m
            .attempts
            .front()
            .is_some_and(|t| now().saturating_sub(*t) >= 60)
        {
            m.attempts.pop_front();
        }
        if m.attempts.len() >= 5 {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "登录尝试过多，请一分钟后再试",
            );
        }
        m.attempts.push_back(now());
    }
    let Ok(permit) = admin.hashing.clone().try_acquire_owned() else {
        return error(StatusCode::TOO_MANY_REQUESTS, "正在验证登录，请稍后重试");
    };
    let hash = admin.hash.clone();
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        body.username == "admin"
            && PasswordHash::new(&hash).is_ok_and(|h| {
                Argon2::default()
                    .verify_password(body.password.as_bytes(), &h)
                    .is_ok()
            })
    })
    .await
    .unwrap_or(false);
    if !valid {
        let _ = app
            .store
            .audit_source("admin", "login", 401, source(&headers));
        return error(StatusCode::UNAUTHORIZED, "账号或密码错误");
    }
    let id = random();
    let csrf = random();
    let mut m = admin.memory.lock().unwrap();
    m.sessions.retain(|_, s| s.expires > now());
    if m.sessions.len() >= 32 {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "管理员会话数量已达上限，请退出旧会话",
        );
    }
    m.sessions.insert(
        id.clone(),
        Session {
            csrf: csrf.clone(),
            expires: now() + 8 * 3600,
        },
    );
    let _ = app
        .store
        .audit_source("admin", "login", 200, source(&headers));
    let mut response = Json(json!({"authenticated":true,"csrf":csrf})).into_response();
    let secure = if insecure_local() { "" } else { "; Secure" };
    response.headers_mut().insert(
        "set-cookie",
        format!(
            "{}={id}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800{secure}",
            cookie_name()
        )
        .parse()
        .unwrap(),
    );
    response
}
pub async fn logout(State(app): State<App>, headers: HeaderMap) -> Response {
    let Some(admin) = app.admin else {
        return error(StatusCode::CONFLICT, "当前不是密码登录模式");
    };
    if !origin_allowed(&headers) || !admin.authorize(&headers, true) {
        return error(StatusCode::FORBIDDEN, "会话或 CSRF 校验失败");
    }
    if let Some(id) = sid(&headers) {
        admin.memory.lock().unwrap().sessions.remove(id);
    }
    let _ = app
        .store
        .audit_source("admin", "logout", 200, source(&headers));
    let mut response = Json(json!({"logged_out":true})).into_response();
    let secure = if insecure_local() { "" } else { "; Secure" };
    response.headers_mut().insert(
        "set-cookie",
        format!(
            "{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}",
            cookie_name()
        )
        .parse()
        .unwrap(),
    );
    response
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sessions_require_csrf_and_expire() {
        let a = Admin {
            hash: String::new(),
            memory: Mutex::new(Memory::default()),
            hashing: Arc::new(Semaphore::new(1)),
        };
        let id = random();
        a.memory.lock().unwrap().sessions.insert(
            id.clone(),
            Session {
                csrf: "csrf".into(),
                expires: now() + 10,
            },
        );
        let mut h = HeaderMap::new();
        h.insert("cookie", format!("{}={id}", cookie_name()).parse().unwrap());
        assert!(a.authorize(&h, false));
        assert!(!a.authorize(&h, true));
        h.insert("x-csrf-token", "csrf".parse().unwrap());
        assert!(a.authorize(&h, true));
        a.memory
            .lock()
            .unwrap()
            .sessions
            .get_mut(&id)
            .unwrap()
            .expires = 0;
        assert!(!a.authorize(&h, false));
    }
}
