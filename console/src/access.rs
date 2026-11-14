//! Independent workspace credential authority. No Docker access or inference bodies.
use crate::store::Store;
use axum::{
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[derive(Serialize, Deserialize, Clone)]
struct Credential {
    gateway: String,
    enabled: bool,
    hash: Option<String>,
    revision: u64,
    #[serde(default)]
    expires: Option<u64>,
}
struct Service {
    store: Store,
    token: String,
    gate: Mutex<()>,
}
fn failure(code: StatusCode, message: &str) -> Response {
    (code, Json(json!({"error":message}))).into_response()
}
fn hash(key: &str) -> String {
    format!("{:x}", Sha256::digest(key.as_bytes()))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
}
fn records(s: &Service) -> Result<BTreeMap<String, Credential>, String> {
    let v = s.store.setting("credentials")?;
    if v.is_null() {
        Ok(BTreeMap::new())
    } else {
        serde_json::from_value(v).map_err(|e| e.to_string())
    }
}
fn public(c: &Credential) -> Value {
    json!({"gateway":c.gateway,"enabled":c.enabled,"has_key":c.hash.is_some(),"revision":c.revision})
}
async fn admin(State(s): State<Arc<Service>>, r: Request, next: Next) -> Response {
    if r.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        != Some(&format!("Bearer {}", s.token))
    {
        return failure(StatusCode::UNAUTHORIZED, "内部管理认证失败");
    }
    next.run(r).await
}
async fn read(State(s): State<Arc<Service>>, Path(id): Path<String>) -> Response {
    let _guard = s.gate.lock().unwrap();
    match records(&s) {
        Ok(r) => match r.get(&id) {
            Some(c) => Json(public(c)).into_response(),
            None => failure(StatusCode::NOT_FOUND, "尚未初始化工作区凭据"),
        },
        Err(_) => failure(StatusCode::SERVICE_UNAVAILABLE, "凭据存储不可用"),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    operation: String,
    gateway: Option<String>,
}
async fn change(
    State(s): State<Arc<Service>>,
    Path(id): Path<String>,
    Json(body): Json<Change>,
) -> Response {
    if !valid_id(&id) {
        return failure(StatusCode::BAD_REQUEST, "工作区标识无效");
    }
    let _guard = s.gate.lock().unwrap();
    let Ok(mut rows) = records(&s) else {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "凭据存储不可用");
    };
    if body.operation == "apply" {
        let Some(gateway) = body.gateway.filter(|g| valid_id(g)) else {
            return failure(StatusCode::BAD_REQUEST, "网关标识无效");
        };
        if !rows.contains_key(&id) && rows.len() >= 100 {
            return failure(StatusCode::CONFLICT, "工作区数量达到上限");
        }
        let c = rows.entry(id.clone()).or_insert(Credential {
            gateway: gateway.clone(),
            enabled: true,
            hash: None,
            revision: 0,
            expires: None,
        });
        if c.gateway != gateway {
            c.hash = None;
            c.gateway = gateway;
        }
    }
    let Some(c) = rows.get_mut(&id) else {
        return failure(StatusCode::NOT_FOUND, "请先应用工作区接入配置");
    };
    let mut plaintext = None;
    match body.operation.as_str() {
        "apply" => (),
        "rotate" => {
            if !c.enabled {
                return failure(StatusCode::CONFLICT, "工作区已停用");
            }
            let key = format!(
                "px_{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            c.hash = Some(hash(&key));
            c.expires = if id.starts_with("system-") {
                Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        + 120,
                )
            } else {
                None
            };
            plaintext = Some(key);
        }
        "revoke" => c.hash = None,
        "disable" => {
            c.enabled = false;
            c.hash = None;
        }
        "enable" => c.enabled = true,
        _ => return failure(StatusCode::BAD_REQUEST, "不支持的凭据操作"),
    }
    c.revision += 1;
    let state = public(c);
    if s.store
        .set_setting("credentials", &serde_json::to_value(&rows).unwrap())
        .is_err()
    {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "凭据写入失败，操作未确认");
    }
    Json(json!({"state":state,"key":plaintext})).into_response()
}
async fn check(State(s): State<Arc<Service>>, h: HeaderMap) -> Response {
    let Some(key) = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return failure(StatusCode::UNAUTHORIZED, "缺少工作区密钥");
    };
    if key.len() > 128 || !key.starts_with("px_") {
        return failure(StatusCode::UNAUTHORIZED, "密钥无效");
    }
    let scope = h
        .get("x-phyxi-gateway")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let _guard = s.gate.lock().unwrap();
    let Ok(rows) = records(&s) else {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "鉴权服务不可用");
    };
    let digest = hash(key);
    let found = rows.iter().find(|(_, c)| {
        c.enabled
            && c.gateway == scope
            && c.hash.as_deref() == Some(digest.as_str())
            && c.expires.is_none_or(|until| {
                until
                    > std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
            })
    });
    let Some((id, _)) = found else {
        return failure(
            StatusCode::UNAUTHORIZED,
            "密钥已撤销、工作区已停用或入口不匹配",
        );
    };
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert("x-phyxi-workspace", id.parse().unwrap());
    response
}
pub async fn serve() -> Result<(), String> {
    let data = PathBuf::from(std::env::var("PHYXI_ACCESS_DATA").unwrap_or(".phyxi-access".into()));
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    use fs2::FileExt;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("access.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "此凭据目录已有鉴权服务运行")?;
    let secret = std::env::var("PHYXI_ACCESS_ADMIN_FILE").map_err(|_| "需要内部管理密钥文件")?;
    let token = std::fs::read_to_string(secret)
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    if token.len() < 32 {
        return Err("内部管理密钥过短".into());
    }
    let state = Arc::new(Service {
        store: Store::open(&data.join("access.db"))?,
        token,
        gate: Mutex::new(()),
    });
    let api = Router::new()
        .route("/workspaces/{id}", get(read).post(change))
        .route_layer(middleware::from_fn_with_state(state.clone(), admin));
    let app = Router::new()
        .route("/health", get(|| async { StatusCode::NO_CONTENT }))
        .route("/check", get(check))
        .nest("/admin", api)
        .layer(axum::extract::DefaultBodyLimit::max(4096))
        .with_state(state);
    let bind = std::env::var("PHYXI_ACCESS_BIND").unwrap_or("127.0.0.1:8766".into());
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| e.to_string())?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string())
}

pub fn configured() -> bool {
    std::env::var("PHYXI_ACCESS_URL").is_ok()
}
pub async fn call(
    data: &std::path::Path,
    id: &str,
    body: Option<Value>,
) -> Result<(u16, Value), String> {
    if !valid_id(id) {
        return Err("工作区标识无效".into());
    }
    let url = std::env::var("PHYXI_ACCESS_URL").map_err(|_| "尚未配置独立鉴权服务")?;
    if !url.starts_with("http://") || url[7..].contains(['/', '?', '#', '@', '\n', '\r']) {
        return Err("鉴权服务地址需为内部 HTTP 主机与端口".into());
    }
    let token = std::fs::read_to_string(
        std::env::var("PHYXI_ACCESS_ADMIN_FILE").map_err(|_| "缺少内部管理密钥路径")?,
    )
    .map_err(|e| e.to_string())?;
    if token.trim().contains(['\n', '\r']) {
        return Err("内部管理密钥格式错误".into());
    }
    let temp = data.join(format!("access-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&temp).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let work = async {
        std::fs::write(
            temp.join("header"),
            format!("Authorization: Bearer {}", token.trim()),
        )
        .map_err(|e| e.to_string())?;
        let mut cmd = tokio::process::Command::new("curl");
        cmd.args([
            "--silent",
            "--show-error",
            "--noproxy",
            "*",
            "--max-time",
            "10",
            "--write-out",
            "\n%{http_code}",
            "--header",
        ])
        .arg(format!("@{}", temp.join("header").display()));
        if let Some(body) = body {
            std::fs::write(temp.join("body"), body.to_string()).map_err(|e| e.to_string())?;
            cmd.args([
                "--header",
                "Content-Type: application/json",
                "--data-binary",
            ])
            .arg(format!("@{}", temp.join("body").display()));
        }
        cmd.arg(format!("{url}/admin/workspaces/{id}"));
        let text = crate::runtime::execute(cmd, None, 12).await?;
        let (body, status) = text.rsplit_once('\n').ok_or("鉴权响应无效")?;
        Ok((
            status.parse().map_err(|_| "鉴权状态码无效")?,
            serde_json::from_str(body).map_err(|_| "鉴权响应格式无效")?,
        ))
    }
    .await;
    let _ = std::fs::remove_dir_all(temp);
    work
}
