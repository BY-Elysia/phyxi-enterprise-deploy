//! Local-only transport between the unprivileged web console and the execution owner.
//! The peer exposes typed product routes, never shell commands or the Docker API.
use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use hyper_util::rt::TokioIo;
use serde_json::json;
use std::{path::PathBuf, time::Duration};

pub fn enabled() -> bool {
    std::env::var("PHYXI_ROLE").as_deref() == Ok("executor")
}
pub fn proxy() -> bool {
    std::env::var("PHYXI_ROLE").as_deref() == Ok("console")
}

pub async fn forward(request: Request) -> Response {
    match tokio::time::timeout(Duration::from_secs(45), exchange(request)).await {
        Ok(Ok(response)) => response,
        _ => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":"宿主机执行器暂不可用；控制台仍可登录。请检查 phyxi-executor 服务，已运行容器不会因此停止。"}))).into_response(),
    }
}
async fn exchange(request: Request) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
    let socket = std::env::var("PHYXI_EXECUTOR_SOCKET")?;
    let token = std::fs::read_to_string(std::env::var("PHYXI_EXECUTOR_TOKEN_FILE")?)?;
    let original = request
        .extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|v| v.0.clone())
        .ok_or("missing original URI")?;
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 220000).await?;
    let mut builder = Request::builder()
        .method(parts.method)
        .uri(original.path_and_query().ok_or("missing path")?.as_str())
        .header(header::HOST, "localhost")
        .header(header::AUTHORIZATION, format!("Bearer {}", token.trim()));
    if let Some(v) = parts.headers.get(header::CONTENT_TYPE) {
        builder = builder.header(header::CONTENT_TYPE, v);
    }
    if std::env::var("PHYXI_TRUST_GATEWAY").as_deref() == Ok("1") {
        if let Some(v) = parts.headers.get("x-phyxi-client-ip") {
            builder = builder.header("x-phyxi-client-ip", v);
        }
    }
    let request = builder.body(Body::from(body))?;
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let response = sender.send_request(request).await?;
    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let bytes = to_bytes(Body::new(response.into_body()), 4 * 1024 * 1024).await?;
    let mut response = (status, bytes).into_response();
    if let Some(v) = content_type {
        response.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    Ok(response)
}

pub async fn serve(app: crate::App) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let socket = PathBuf::from(std::env::var("PHYXI_EXECUTOR_SOCKET")?);
    // The directory is created by systemd, not by an HTTP request.
    if socket.exists() {
        if tokio::net::UnixStream::connect(&socket).await.is_ok() {
            return Err("执行器 socket 已占用".into());
        }
        std::fs::remove_file(&socket)?;
    }
    let listener = tokio::net::UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o660))?;
    let token_path = socket.with_file_name("executor.token");
    std::fs::write(&token_path, &app.token)?;
    std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o640))?;
    // Never expose login/static pages or bypass API authentication on this socket.
    let routes = crate::router(app).layer(axum::middleware::from_fn(
        |request: Request, next: axum::middleware::Next| async move {
            if !request.uri().path().starts_with("/api/")
                || request.uri().path().starts_with("/api/auth/")
            {
                return StatusCode::NOT_FOUND.into_response();
            }
            next.run(request).await
        },
    ));
    println!("PhyXi executor listening on {}", socket.display());
    axum::serve(listener, routes)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
