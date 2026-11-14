mod access;
mod admin;
mod artifacts;
mod backup;
mod catalog;
mod catalog_api;
use catalog_api::{
    catalog_delete, catalog_get, catalog_put, deployment_preview, image_status, resolve_action,
    workspace_export, Scope,
};
mod config;
mod environment;
mod gateway;
mod gateway_container;
mod host_metrics;
mod presets;
mod process;
mod runtime;
mod store;
mod tasks;
mod telemetry;
mod verification;

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
    gateway: gateway::Gateway,
    token: String,
    admin: Option<Arc<admin::Admin>>,
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
    let write = !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    );
    let allowed = if let Some(admin) = &a.admin {
        admin.authorize(request.headers(), write)
    } else {
        request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {}", a.token))
    };
    if !allowed {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"请登录控制台；写操作需有效会话和 CSRF 校验"})),
        )
            .into_response();
    }
    let source = if std::env::var("PHYXI_TRUST_GATEWAY").as_deref() == Ok("1") {
        request
            .headers()
            .get("x-phyxi-client-ip")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    } else {
        None
    };
    let action = format!("{} {}", request.method(), request.uri().path());
    let mut response = next.run(request).await;
    if write {
        let _ = a.store.audit_source(
            "admin",
            &action,
            response.status().as_u16(),
            source.as_deref(),
        );
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
fn router(app: App) -> Router {
    let api = Router::new()
        .route(
            "/deployments/{id}/comparison",
            get(catalog_api::deployment_comparison),
        )
        .route(
            "/workspaces/{id}/requests",
            get(catalog_api::workspace_requests),
        )
        .route(
            "/workspaces/{id}/credential",
            get(catalog_api::credential_status).post(catalog_api::credential_change),
        )
        .route(
            "/gateways/{id}/certificate",
            post(catalog_api::certificate_upload)
                .layer(axum::extract::DefaultBodyLimit::max(220000)),
        )
        .route(
            "/audit",
            get(|State(a): State<App>| async move {
                a.store.audit_rows().map(Json).map_err(Error::from)
            }),
        )
        .route("/presets", get(|| async { Json(presets::list()) }))
        .route("/models/{id}/status", get(catalog_api::model_status))
        .route("/catalog", get(catalog_get))
        .route(
            "/catalog/{kind}/{id}",
            axum::routing::put(catalog_put).delete(catalog_delete),
        )
        .route("/deployments/{id}/preview", get(deployment_preview))
        .route("/workspaces/{id}/export", get(workspace_export))
        .route("/images/{id}/status", get(image_status))
        .route("/config", get(config_get).put(config_save))
        .route("/gpus", get(gpus))
        .route("/environment", get(environment_get))
        .route("/gateway", get(gateway_get).put(gateway_save))
        .route("/reports", get(reports))
        .route("/telemetry", get(telemetry_get))
        .route("/metrics", get(metrics))
        .route("/model-metrics", get(model_metrics))
        .route("/status", get(status))
        .route("/logs", get(logs))
        .route("/jobs", get(jobs))
        .route("/jobs/{id}/events", get(events))
        .route("/actions/{action}", post(action))
        .route_layer(middleware::from_fn_with_state(app.clone(), auth));
    Router::new()
        .route("/api/auth/info", get(admin::info))
        .route("/api/auth/login", post(admin::login))
        .route("/api/auth/logout", post(admin::logout))
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
        .layer(middleware::from_fn(|request:Request,next:Next| async move {
            let mut response=next.run(request).await;
            for (name,value) in [("cache-control","no-store"),("x-content-type-options","nosniff"),("referrer-policy","no-referrer"),("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")] {response.headers_mut().insert(axum::http::HeaderName::from_static(name),value.parse().unwrap());}
            response
        }))
        .with_state(app)
}
async fn model_metrics(State(a): State<App>) -> Result<Json<Value>> {
    let c = tasks::active_config(&a.runtime, &a.store).await?;
    let (status, text) =
        verification::request(&a.runtime.data, &c, None, "/metrics", None, true).await?;
    if status != 200 {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "当前引擎未提供 metrics，请检查引擎启动选项".into(),
        ));
    }
    Ok(Json(json!({"text":text})))
}
async fn telemetry_get(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(a.store.setting("telemetry")?))
}
async fn metrics(State(a): State<App>) -> Result<String> {
    Ok(telemetry::prometheus(&a.store)?)
}
async fn environment_get(State(a): State<App>) -> Json<Value> {
    Json(environment::inspect(&a.runtime).await)
}
async fn gateway_get(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(
        json!({"config":tasks::gateway_config(&a.store)?,"running":a.gateway.running().await?}),
    ))
}
async fn gateway_save(
    State(a): State<App>,
    Json(c): Json<gateway::GatewayConfig>,
) -> Result<Json<Value>> {
    c.validate()?;
    a.store.set_setting(
        "gateway_config",
        &serde_json::to_value(c).map_err(|e| e.to_string())?,
    )?;
    Ok(Json(json!({"saved":true})))
}
async fn reports(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(
        json!({"verification":a.store.setting("last_verification")?,"releases":a.store.setting("releases")?}),
    ))
}
async fn config_get(State(a): State<App>) -> Result<Json<Config>> {
    Ok(Json(a.store.config()?))
}
async fn config_save(State(a): State<App>, Json(mut c): Json<Config>) -> Result<Json<Value>> {
    if let Some(source) = &c.source {
        source.validate()?;
        c.model_path = artifacts::destination(&a.runtime.data, source)
            .display()
            .to_string();
    }
    c.validate()?;
    a.store.save(&c)?;
    Ok(Json(json!({"saved":true})))
}
async fn gpus(State(a): State<App>) -> Result<Json<Vec<runtime::Gpu>>> {
    Ok(Json(a.runtime.gpus().await?))
}
async fn status(State(a): State<App>) -> Result<Json<Value>> {
    let mut status = a.runtime.status().await?;
    if let Some(job) = status["job"].as_str() {
        status["composition"] = a.store.setting(&format!("composition:{job}"))?;
    }
    Ok(Json(status))
}
async fn logs(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(json!({"text":a.runtime.logs().await?})))
}
async fn jobs(State(a): State<App>) -> Result<Json<Value>> {
    let mut rows = Vec::new();
    for job in a.store.jobs()? {
        let mut row = serde_json::to_value(&job).map_err(|e| e.to_string())?;
        row["composition"] = a.store.setting(&format!("composition:{}", job.id))?;
        rows.push(row);
    }
    Ok(Json(json!(rows)))
}
#[derive(Deserialize)]
struct Cursor {
    #[serde(default)]
    after: i64,
    #[serde(default)]
    tail: bool,
}
async fn events(
    State(a): State<App>,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<Vec<store::Event>>> {
    Ok(Json(if cursor.tail {
        a.store.recent_events(&id)?
    } else {
        a.store.events(&id, cursor.after)?
    }))
}
async fn action(
    State(a): State<App>,
    Path(action): Path<String>,
    Query(scope): Query<Scope>,
) -> Result<(StatusCode, Json<Value>)> {
    if action == "cancel" {
        if a.gate.available_permits() != 0 {
            return Err(Error(StatusCode::CONFLICT, "没有运行中的任务".into()));
        }
        a.cancel.store(true, Ordering::SeqCst);
        return Ok((StatusCode::ACCEPTED, Json(json!({"cancelling":true}))));
    }
    if !tasks::ACTIONS.contains(&action.as_str()) {
        return Err(Error(StatusCode::NOT_FOUND, "未知操作".into()));
    }
    let permit = a.gate.clone().try_acquire_owned().map_err(|_| {
        Error(
            StatusCode::CONFLICT,
            "已有任务运行，请等待或申请取消".into(),
        )
    })?;
    let (c, gateway_config, context) = resolve_action(&a, &action, &scope).await?;
    if action == "start" {
        c.validate()?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    a.store
        .set_setting(&format!("composition:{id}"), &context)?;
    a.store.create(&id, &action, &c)?;
    a.cancel.store(false, Ordering::SeqCst);
    let response = json!({"id":id});
    tokio::spawn(async move {
        let _permit = permit;
        let result = tasks::run(
            &a.runtime,
            &a.gateway,
            &action,
            (&c, &gateway_config),
            &id,
            a.store.clone(),
            a.cancel.clone(),
        )
        .await;
        let (status, detail) = match result {
            Ok(()) => {
                if action == "gateway_start" || action == "gateway_apply" {
                    let _ = a.store.set_setting("active_gateway", &context);
                }
                if action == "gateway_stop" {
                    let _ = a.store.set_setting("active_gateway", &Value::Null);
                }
                ("succeeded", "执行完成".into())
            }
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
pub(crate) fn private_file(path: &std::path::Path) -> std::io::Result<String> {
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
#[tokio::main(worker_threads = 2)]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--access") {
        return access::serve().await.map_err(Into::into);
    }
    if std::env::args().nth(1).as_deref() == Some("--prepare-host") {
        return environment::prepare_host().await.map_err(Into::into);
    }
    let data = PathBuf::from(std::env::var("PHYXI_STATE_DIR").unwrap_or(".phyxi-console".into()));
    if std::env::args().nth(1).as_deref() == Some("--restore") {
        use std::io::Read;
        let input = std::env::args().nth(2).ok_or("需要备份文件路径")?;
        if !data.is_absolute() {
            return Err("恢复需要绝对状态目录路径".into());
        }
        let data = fs::canonicalize(data.parent().ok_or("恢复路径无父目录")?)?
            .join(data.file_name().ok_or("恢复路径必须包含目录名")?);
        let mut password = zeroize::Zeroizing::new(String::new());
        std::io::stdin().take(1024).read_to_string(&mut password)?;
        backup::restore(
            std::path::Path::new(&input),
            &data,
            password.trim_end_matches(['\r', '\n']),
        )?;
        println!("恢复完成；会话未恢复，请检查资源和路径后再启动。");
        return Ok(());
    }

    fs::create_dir_all(&data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700))?;
    }
    let data = fs::canonicalize(data)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("server.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "此状态目录已有控制台运行")?;
    if std::env::args().nth(1).as_deref() == Some("--set-admin") {
        use std::io::Read;
        let mut password = String::new();
        std::io::stdin().take(1024).read_to_string(&mut password)?;
        admin::set_password(&data, password.trim_end_matches(['\r', '\n']))?;
        println!("管理员密码已设置；重启控制台后旧会话失效。");
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() == Some("--backup") {
        use std::io::Read;
        let output = std::env::args().nth(2).ok_or("需要备份输出路径")?;
        let mut password = zeroize::Zeroizing::new(String::new());
        std::io::stdin().take(1024).read_to_string(&mut password)?;
        backup::create(
            &data,
            std::path::Path::new(&output),
            password.trim_end_matches(['\r', '\n']),
        )?;
        println!("加密备份完成（不含权重与登录会话）。");
        return Ok(());
    }
    let host = std::env::var("PHYXI_DOCKER_HOST").unwrap_or("unix:///var/run/docker.sock".into());
    if !host.starts_with("unix:///") {
        return Err("首版仅支持本地 Unix Docker socket".into());
    }
    let port: u16 = std::env::var("PHYXI_PORT")
        .unwrap_or("8765".into())
        .parse()?;
    let app = App {
        store: Arc::new(Store::open(&data.join("state.db"))?),
        gateway: gateway::Gateway::new(&data),
        runtime: Runtime::new(private_file(&data.join("owner"))?, host, data.clone()),
        token: private_file(&data.join("token"))?,
        admin: admin::Admin::load(&data)?,
        gate: Arc::new(Semaphore::new(1)),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    catalog::migrate(&app.store)?;
    telemetry::start(app.store.clone(), app.runtime.clone());
    let bind: std::net::Ipv4Addr = std::env::var("PHYXI_BIND")
        .unwrap_or("127.0.0.1".into())
        .parse()?;
    if !bind.is_loopback()
        && (app.admin.is_none() || std::env::var("PHYXI_LOCAL_HTTP_LOGIN").as_deref() == Ok("1"))
    {
        return Err("非回环控制台必须启用管理员密码及 Secure 会话，并通过 HTTPS 网关接入".into());
    }
    let listener = tokio::net::TcpListener::bind((bind, port)).await?;
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
            gateway: gateway::Gateway::new(&std::env::temp_dir()),
            runtime: Runtime::new(
                "test".into(),
                "unix:///var/run/docker.sock".into(),
                std::env::temp_dir(),
            ),
            token: "test-token".into(),
            admin: None,
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
