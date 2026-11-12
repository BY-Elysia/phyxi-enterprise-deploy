mod config;
mod runtime;
mod store;

use axum::{
    extract::{Path, Query, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use config::Config;
use fs2::FileExt;
use runtime::Runtime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use store::Store;
use tokio::sync::Semaphore;

#[derive(Clone)]
struct App {
    store: Arc<Store>,
    runtime: Runtime,
    token: String,
    gate: Arc<Semaphore>,
    cancel: Arc<AtomicBool>,
}
struct Error(StatusCode, String);
impl From<String> for Error {
    fn from(s: String) -> Self {
        Self(StatusCode::BAD_REQUEST, s)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
type Result<T> = std::result::Result<T, Error>;
async fn auth(State(a): State<App>, request: Request, next: Next) -> Response {
    if request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        != Some(&format!("Bearer {}", a.token))
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"请填写本机控制台访问令牌"})),
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
fn router(app: App) -> Router {
    let api = Router::new()
        .route("/config", get(config_get).put(config_save))
        .route("/gpus", get(gpus))
        .route("/status", get(status))
        .route("/logs", get(logs))
        .route("/jobs", get(jobs))
        .route("/jobs/{id}/events", get(events))
        .route("/actions/{action}", post(action))
        .route_layer(middleware::from_fn_with_state(app.clone(), auth));
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/style.css"),
                )
            }),
        )
        .nest("/api", api)
        .layer(axum::extract::DefaultBodyLimit::max(16384))
        .with_state(app)
}
async fn config_get(State(a): State<App>) -> Result<Json<Config>> {
    Ok(Json(a.store.config()?))
}
async fn config_save(State(a): State<App>, Json(c): Json<Config>) -> Result<Json<Value>> {
    c.validate()?;
    a.store.save(&c)?;
    Ok(Json(json!({"saved":true})))
}
async fn gpus(State(a): State<App>) -> Result<Json<Vec<runtime::Gpu>>> {
    Ok(Json(a.runtime.gpus().await?))
}
async fn status(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(a.runtime.status().await?))
}
async fn logs(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(json!({"text":a.runtime.logs().await?})))
}
async fn jobs(State(a): State<App>) -> Result<Json<Vec<store::Job>>> {
    Ok(Json(a.store.jobs()?))
}
#[derive(Deserialize)]
struct Cursor {
    #[serde(default)]
    after: i64,
}
async fn events(
    State(a): State<App>,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<Vec<store::Event>>> {
    Ok(Json(a.store.events(&id, cursor.after)?))
}
async fn action(
    State(a): State<App>,
    Path(action): Path<String>,
) -> Result<(StatusCode, Json<Value>)> {
    if action == "cancel" {
        if a.gate.available_permits() != 0 {
            return Err(Error(StatusCode::CONFLICT, "没有运行中的任务".into()));
        }
        a.cancel.store(true, Ordering::SeqCst);
        return Ok((StatusCode::ACCEPTED, Json(json!({"cancelling":true}))));
    }
    if !["start", "stop"].contains(&action.as_str()) {
        return Err(Error(StatusCode::NOT_FOUND, "未知操作".into()));
    }
    let permit = a.gate.clone().try_acquire_owned().map_err(|_| {
        Error(
            StatusCode::CONFLICT,
            "已有任务运行，请等待或申请取消".into(),
        )
    })?;
    let c = a.store.config()?;
    if action == "start" {
        c.validate()?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    a.store.create(&id, &action, &c)?;
    a.cancel.store(false, Ordering::SeqCst);
    let response = json!({"id":id});
    tokio::spawn(async move {
        let _permit = permit;
        let result = a
            .runtime
            .job(&action, &c, &id, a.store.clone(), a.cancel.clone())
            .await;
        let (status, detail) = match result {
            Ok(()) => ("succeeded", "执行完成".into()),
            Err(e) => {
                let state = if e == "任务已取消" {
                    "cancelled"
                } else {
                    "failed"
                };
                (state, e)
            }
        };
        if let Err(e) = a
            .store
            .event(&id, &detail)
            .and_then(|_| a.store.update(&id, status, &detail))
        {
            eprintln!("Task {id} persistence failed: {e}");
        }
    });
    Ok((StatusCode::ACCEPTED, Json(response)))
}
fn private_file(path: &std::path::Path) -> std::io::Result<String> {
    match fs::read_to_string(path) {
        Ok(v) => {
            let v = v.trim().to_string();
            if v.len() < 32 {
                return Err(std::io::Error::other("invalid identity/token file"));
            }
            Ok(v)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let v = uuid::Uuid::new_v4().to_string();
            let mut o = OpenOptions::new();
            o.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                o.mode(0o600);
            }
            let mut f = o.open(path)?;
            f.write_all(v.as_bytes())?;
            f.sync_all()?;
            Ok(v)
        }
        Err(e) => Err(e),
    }
}
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let data = PathBuf::from(std::env::var("PHYXI_STATE_DIR").unwrap_or(".phyxi-console".into()));
    fs::create_dir_all(&data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700))?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("server.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "此状态目录已有控制台运行")?;
    let host = std::env::var("PHYXI_DOCKER_HOST").unwrap_or("unix:///var/run/docker.sock".into());
    if !host.starts_with("unix:///") {
        return Err("首版仅支持本地 Unix Docker socket".into());
    }
    let port: u16 = std::env::var("PHYXI_PORT")
        .unwrap_or("8765".into())
        .parse()?;
    let app = App {
        store: Arc::new(Store::open(&data.join("state.db"))?),
        runtime: Runtime {
            owner: private_file(&data.join("owner"))?,
            docker_host: host,
        },
        token: private_file(&data.join("token"))?,
        gate: Arc::new(Semaphore::new(1)),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!(
        "PhyXi console: http://127.0.0.1:{}\n访问令牌文件: {}",
        listener.local_addr()?.port(),
        data.join("token").display()
    );
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    drop(lock);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    fn app() -> App {
        App {
            store: Arc::new(Store::open(std::path::Path::new(":memory:")).unwrap()),
            runtime: Runtime {
                owner: "test".into(),
                docker_host: "unix:///var/run/docker.sock".into(),
            },
            token: "test-token".into(),
            gate: Arc::new(Semaphore::new(1)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    #[tokio::test]
    async fn requires_auth_and_rejects_invalid_configuration() {
        let r = router(app());
        let q = Request::builder()
            .uri("/api/config")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(
            r.clone().oneshot(q).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let q = Request::builder()
            .uri("/api/config")
            .header("authorization", "Bearer test-token")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(r.clone().oneshot(q).await.unwrap().status(), StatusCode::OK);
        let q = Request::builder()
            .method("PUT")
            .uri("/api/config")
            .header("authorization", "Bearer test-token")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                serde_json::to_string(&Config::default()).unwrap(),
            ))
            .unwrap();
        assert_eq!(
            r.oneshot(q).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn serializes_actions() {
        let a = app();
        let permit = a.gate.clone().acquire_owned().await.unwrap();
        let q = Request::builder()
            .method("POST")
            .uri("/api/actions/stop")
            .header("authorization", "Bearer test-token")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(
            router(a).oneshot(q).await.unwrap().status(),
            StatusCode::CONFLICT
        );
        drop(permit);
    }
}
