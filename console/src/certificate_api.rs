use crate::{catalog, App, Error, Result};
use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CsrRequest {
    revision: u64,
    #[serde(default)]
    organization: String,
    #[serde(default)]
    ips: Vec<std::net::IpAddr>,
}
fn certificate_directory(a: &App, id: &str) -> std::result::Result<std::path::PathBuf, String> {
    let id = uuid::Uuid::parse_str(id).map_err(|_| "证书申请标识无效")?;
    Ok(a.runtime.data.join("certificates").join(id.to_string()))
}
pub(crate) async fn csr_create(
    State(a): State<App>,
    Path(id): Path<String>,
    Json(body): Json<CsrRequest>,
) -> Result<Json<Value>> {
    let _permit = a
        .gate
        .clone()
        .try_write_owned()
        .map_err(|_| Error(StatusCode::CONFLICT, "任务执行中".into()))?;
    let _catalog = a.catalog_gate.lock().await;
    let cat = catalog::Catalog::load(&a.store)?;
    let record = cat.get("gateways", &id)?;
    if record.revision != body.revision {
        return Err(Error(
            StatusCode::CONFLICT,
            "配置已更新，请刷新后重新申请".into(),
        ));
    }
    let gateway: catalog::Gateway = cat.data("gateways", &id)?;
    if body.ips.len() > 16
        || body.organization.len() > 128
        || body
            .organization
            .chars()
            .any(|c| c.is_control() || "/\\+=<>;\"".contains(c))
    {
        return Err("组织名称含不支持的字符或 IP 数量超过 16".to_string().into());
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let directory = certificate_directory(&a, &request_id)?;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::create_dir_all(directory.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    let result: std::result::Result<Value, String> = async {
        let domain = &gateway.config.domain;
        let mut san = vec![format!("{}:{domain}", if domain.parse::<std::net::IpAddr>().is_ok() { "IP" } else { "DNS" })];
        san.extend(body.ips.iter().map(|ip| format!("IP:{ip}")));
        let subject = if body.organization.is_empty() { format!("/CN={domain}") }
            else { format!("/O={}/CN={domain}", body.organization) };
        a.gateway.openssl(vec!["req".into(), "-new".into(), "-utf8".into(), "-newkey".into(), "rsa:2048".into(),
            "-nodes".into(), "-sha256".into(), "-keyout".into(), directory.join("server.key").display().to_string(),
            "-out".into(), directory.join("server.csr").display().to_string(), "-subj".into(), subject,
            "-addext".into(), format!("subjectAltName={}", san.join(","))]).await?;
        fs::set_permissions(directory.join("server.key"), fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        let value = json!({"request_id":request_id,"domain":domain,"ips":body.ips,"organization":body.organization});
        a.store.set_setting(&format!("csr:{id}"), &value)?;
        Ok(value)
    }.await;
    match result {
        Ok(v) => Ok(Json(v)),
        Err(e) => {
            let _ = fs::remove_dir_all(directory);
            Err(e.into())
        }
    }
}
pub(crate) async fn csr_download(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let _catalog = a.catalog_gate.lock().await;
    catalog::Catalog::load(&a.store)?.get("gateways", &id)?;
    let mut request = a.store.setting(&format!("csr:{id}"))?;
    if let Some(request_id) = request["request_id"].as_str() {
        let directory = certificate_directory(&a, request_id)?;
        request["csr"] =
            json!(fs::read_to_string(directory.join("server.csr")).map_err(|e| e.to_string())?);
    }
    Ok(Json(request))
}
pub(crate) async fn certificate_status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let _catalog = a.catalog_gate.lock().await;
    let gateway: catalog::Gateway = catalog::Catalog::load(&a.store)?.data("gateways", &id)?;
    if gateway.config.tls_mode != "enterprise" || gateway.config.cert_file.is_empty() {
        return Ok(Json(json!({"configured":false})));
    }
    let details = a
        .gateway
        .openssl(vec![
            "x509".into(),
            "-in".into(),
            gateway.config.cert_file.clone(),
            "-noout".into(),
            "-subject".into(),
            "-issuer".into(),
            "-dates".into(),
            "-fingerprint".into(),
            "-sha256".into(),
        ])
        .await;
    let details = match details {
        Ok(details) => details,
        Err(error) => {
            return Ok(Json(
                json!({"configured":true,"details":"","valid":false,"error":error}),
            ))
        }
    };
    let validation = a.gateway.validate_certificates(&gateway.config).await;
    Ok(Json(
        json!({"configured":true,"details":details,"valid":validation.is_ok(),"error":validation.err()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CertificateUpload {
    revision: u64,
    certificate: String,
    #[serde(default)]
    private_key: String,
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    ca: String,
    #[serde(default)]
    use_lets_encrypt: bool,
}

// Official ISRG trust anchors from https://letsencrypt.org/certificates/.
const LETS_ENCRYPT_ROOTS: &str = include_str!("../certs/lets-encrypt-roots.pem");

pub(crate) async fn certificate_upload(
    State(a): State<App>,
    Path(id): Path<String>,
    Json(body): Json<CertificateUpload>,
) -> Result<Json<Value>> {
    let _permit = a
        .gate
        .clone()
        .try_write_owned()
        .map_err(|_| Error(StatusCode::CONFLICT, "任务执行中".into()))?;
    let _catalog = a.catalog_gate.lock().await;
    let mut cat = catalog::Catalog::load(&a.store)?;
    let mut record = cat.get("gateways", &id)?.clone();
    if record.revision != body.revision {
        return Err(Error(
            StatusCode::CONFLICT,
            "配置已更新，请刷新后重新上传".into(),
        ));
    }
    let private_key = if body.request_id.is_empty() {
        body.private_key.clone()
    } else {
        let request = a.store.setting(&format!("csr:{id}"))?;
        if !body.private_key.is_empty()
            || request["request_id"].as_str() != Some(body.request_id.as_str())
        {
            return Err("证书申请已更新或不属于此网关，请重新选择"
                .to_string()
                .into());
        }
        fs::read_to_string(certificate_directory(&a, &body.request_id)?.join("server.key"))
            .map_err(|e| e.to_string())?
    };
    let ca = if body.use_lets_encrypt {
        if !body.ca.is_empty() || !body.request_id.is_empty() {
            return Err(
                "Let’s Encrypt 导入需要上传证书链和私钥，请勿同时提供根 CA 或 CSR 编号"
                    .to_string()
                    .into(),
            );
        }
        LETS_ENCRYPT_ROOTS
    } else {
        &body.ca
    };
    for text in [body.certificate.as_str(), private_key.as_str(), ca] {
        if text.is_empty() || text.len() > 65536 {
            return Err("证书文件为空或超过 64 KiB".to_string().into());
        }
    }
    let directory = a
        .runtime
        .data
        .join("certificates")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let result: std::result::Result<(), String> = async {
        for (name, text) in [
            ("server.pem", body.certificate.as_str()),
            ("server.key", private_key.as_str()),
            ("ca.pem", ca),
        ] {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(directory.join(name))
                .map_err(|e| e.to_string())?;
            f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        let mut gateway: catalog::Gateway =
            serde_json::from_value(record.data.clone()).map_err(|e| e.to_string())?;
        gateway.config.tls_mode = "enterprise".into();
        gateway.config.cert_file = directory.join("server.pem").display().to_string();
        gateway.config.key_file = directory.join("server.key").display().to_string();
        gateway.config.ca_file = directory.join("ca.pem").display().to_string();
        a.gateway.validate_certificates(&gateway.config).await?;
        record.data = serde_json::to_value(gateway).map_err(|e| e.to_string())?;
        cat.put("gateways", &id, record)?;
        cat.save(&a.store)?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&directory);
        return Err(e.into());
    }
    Ok(Json(
        json!({"saved":true,"applied":false,"record":cat.get("gateways",&id)?}),
    ))
}
