//! Configuration CRUD and execution-time composition, separate from the runtime owner.
use super::*;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scope {
    pub(crate) job_id: Option<String>,
    kind: Option<String>,
    id: Option<String>,
}
pub(crate) async fn catalog_get(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(
        serde_json::to_value(catalog::Catalog::load(&a.store)?).map_err(|e| e.to_string())?,
    ))
}
pub(crate) async fn catalog_put(
    State(a): State<App>,
    Path((kind, id)): Path<(String, String)>,
    Json(mut record): Json<catalog::Record>,
) -> Result<Json<Value>> {
    if kind == "workspaces" && id.starts_with("system-") {
        return Err("system- 前缀用于系统内部工作区".to_string().into());
    }
    if kind == "models" {
        let mut model: catalog::Model =
            serde_json::from_value(record.data.clone()).map_err(|e| e.to_string())?;
        if let Some(source) = model.source {
            let source = artifacts::resolve(source).await?;
            model.model_path = artifacts::destination(&a.runtime.data, &source)
                .display()
                .to_string();
            model.source = Some(source);
        }
        record.data = serde_json::to_value(model).map_err(|e| e.to_string())?;
    }
    let _catalog = a.catalog_gate.lock().await;
    let mut cat = catalog::Catalog::load(&a.store)?;
    cat.put(&kind, &id, record)?;
    cat.save(&a.store)?;
    Ok(Json(json!({"saved":true,"record":cat.get(&kind,&id)?})))
}
pub(crate) async fn catalog_delete(
    State(a): State<App>,
    Path((kind, id)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let _permit = a
        .gate
        .clone()
        .try_write_owned()
        .map_err(|_| Error(StatusCode::CONFLICT, "任务执行中，请稍后删除配置".into()))?;
    if kind == "workspaces" && access::configured() {
        return Err("工作区请停用并撤销密钥，保留审计记录；不直接删除"
            .to_string()
            .into());
    }
    if kind == "deployments" {
        let status = a.runtime.status().await?;
        if let Some(job) = status["job"].as_str() {
            if a.store.setting(&format!("composition:{job}"))?["id"] == id {
                return Err("请先停止该部署实例".to_string().into());
            }
        }
    }
    if kind == "gateways"
        && a.gateway.running().await?
        && a.store.setting("active_gateway")?["id"] == id
    {
        return Err("请先停止该网关".to_string().into());
    }
    let _catalog = a.catalog_gate.lock().await;
    let mut cat = catalog::Catalog::load(&a.store)?;
    cat.delete(&kind, &id)?;
    cat.save(&a.store)?;
    Ok(Json(json!({"deleted":true})))
}
pub(crate) async fn deployment_preview(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    Ok(Json(
        json!({"config":prepared_config(&a,&cat,&id).await?,"composition":cat.snapshot("deployments",&id)?}),
    ))
}
pub(crate) async fn workspace_export(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    let w: catalog::Workspace = cat.data("workspaces", &id)?;
    let g: catalog::Gateway = cat.data("gateways", &w.gateway)?;
    let d: catalog::Deployment = cat.data("deployments", &g.deployment)?;
    let mut m: catalog::Model = cat.data("models", &d.model)?;
    if !d.model_name.is_empty() {
        m.model_name = d.model_name;
    }
    w.generation.validate()?;
    let request = w.generation.request(&m.model_name);
    let base_url = format!("https://{}:{}/v1", g.config.domain, g.config.port);
    let opencode_v2 = json!({"$schema":"https://opencode.ai/config.json","model":format!("phyxi/{}",m.model_name),"providers":{"phyxi":{"name":"PhyXi","package":"@opencode/ai/providers/openai-compatible","settings":{"baseURL":base_url,"apiKey":format!("{{env:{}}}",w.credential_env)},"models":{(m.model_name.clone()):{"name":m.model_name,"body":w.generation}}}}});
    // Export a client template, never the server token or a private key.
    Ok(Json(
        json!({"format":"phyxi-client-config-v1","generation":w.generation,"request_template":request,"opencode_v2":opencode_v2,"opencode_legacy_note":"opencode 字段保留旧版连接模板，不携带生成预设；请使用 opencode_v2 或 request_template。","workspace":cat.get("workspaces",&id)?.name,"base_url":format!("https://{}:{}/v1",g.config.domain,g.config.port),"model":m.model_name,"credential_env":w.credential_env,"ca_file":g.config.ca_file,"note":"客户端需自行设置凭据环境变量并安装信任证书；这是配置模板，不代表服务已经发布。","opencode":{"provider":{"phyxi":{"npm":"@ai-sdk/openai-compatible","name":"PhyXi","options":{"baseURL":format!("https://{}:{}/v1",g.config.domain,g.config.port),"apiKey":format!("{{env:{}}}",w.credential_env)},"models":{(m.model_name.clone()):{"name":m.model_name}}}}},"composition":cat.snapshot("workspaces",&id)?}),
    ))
}
pub(crate) async fn image_status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    let i: catalog::Image = cat.data("images", &id)?;
    catalog::image_valid(&i.reference)?;
    let effective = crate::image_sources::load(&a.runtime.data)?.resolve(&i.reference)?;
    match a.runtime.local_image(&i.reference).await {
        Ok(v) => Ok(Json(
            json!({"available":true,"id":v["Id"],"architecture":v["Architecture"],"size_bytes":v["Size"],"reference":i.reference,"effective_reference":effective}),
        )),
        Err(e) => Ok(Json(
            json!({"available":false,"detail":e,"effective_reference":effective}),
        )),
    }
}
pub(crate) async fn resolve_action(
    a: &App,
    action: &str,
    scope: &Scope,
) -> Result<(Config, gateway::GatewayConfig, Value)> {
    let (Some(kind), Some(id)) = (&scope.kind, &scope.id) else {
        if scope.kind.is_some() || scope.id.is_some() {
            return Err("需要同时指定配置类型和 ID".to_string().into());
        }
        return Ok((
            a.store.config()?,
            tasks::gateway_config(&a.store)?,
            Value::Null,
        ));
    };
    let cat = catalog::Catalog::load(&a.store)?;
    let mut snapshot = cat.snapshot(kind, id)?;
    let mut g = gateway::GatewayConfig::default();
    let c = match (kind.as_str(), action) {
        ("models", "download") => {
            let m: catalog::Model = cat.data(kind, id)?;
            let source = m.source.ok_or("该模型没有配置下载来源".to_string())?;
            source.validate()?;
            Config {
                model_name: m.model_name,
                model_path: artifacts::destination(&a.runtime.data, &source)
                    .display()
                    .to_string(),
                source: Some(source),
                ..Config::default()
            }
        }
        ("images", "pull_image") => {
            let i: catalog::Image = cat.data(kind, id)?;
            catalog::image_valid(&i.reference)?;
            Config {
                image: i.reference,
                ..Config::default()
            }
        }
        ("deployments", "start" | "redeploy" | "verify") => {
            let c = prepared_config(a, &cat, id).await?;
            if std::env::var("PHYXI_MODEL_NETWORK").is_ok() && c.engine == config::Engine::LlamaCpu
            {
                return Err("CPU 后端仅供原生本地测试；企业容器模式请选择 GPU 推理引擎"
                    .to_string()
                    .into());
            }
            if action == "redeploy" {
                assert_deployment(a, id).await?;
            }
            if action == "verify" {
                assert_deployment(a, id).await?;
                if a.gateway.running().await? {
                    return Err("请通过正在运行的网关执行验收，或先停止网关"
                        .to_string()
                        .into());
                }
            }
            if ["start", "redeploy"].contains(&action)
                && c.port
                    == std::env::var("PHYXI_PORT")
                        .unwrap_or("8765".into())
                        .parse::<u16>()
                        .unwrap_or(8765)
            {
                return Err("模型端口与控制台冲突".to_string().into());
            }
            if crate::services::enabled() && [8443, 8766, 18765].contains(&c.port) {
                return Err("模型端口与企业接入组件保留端口冲突".to_string().into());
            }
            c
        }
        ("deployments", "stop") => {
            assert_deployment(a, id).await?;
            let st = a.runtime.status().await?;
            a.store
                .job_config(st["job"].as_str().ok_or("缺少部署快照".to_string())?)?
        }
        (
            "gateways",
            "management_start" | "management_stop" | "certificate" | "gateway_start"
            | "gateway_apply" | "gateway_stop" | "verify" | "publish",
        ) => {
            let profile: catalog::Gateway = cat.data(kind, id)?;
            g = profile.config;
            if matches!(action, "gateway_start" | "gateway_apply" | "certificate") {
                g.validate()?;
            }
            if access::configured() {
                g.workspace_scope = id.clone();
            }
            if action == "management_start" || action == "management_stop" {
                Config::default()
            } else if action == "certificate" {
                if a.gateway.running().await? {
                    return Err("请先停止网关，再更换证书".to_string().into());
                }
                Config::default()
            } else if action == "gateway_stop" {
                if a.store.setting("active_gateway")?["id"] != *id {
                    return Err("所选网关不是当前运行网关".to_string().into());
                }
                Config::default()
            } else {
                assert_deployment(a, &profile.deployment).await?;
                let active = tasks::active_config(&a.runtime, &a.store).await?;
                let status = a.runtime.status().await?;
                if let Some(job) = status["job"].as_str() {
                    snapshot["dependencies"]["deployment"] =
                        a.store.setting(&format!("composition:{job}"))?;
                }
                if g.port == active.port
                    || g.port
                        == std::env::var("PHYXI_PORT")
                            .unwrap_or("8765".into())
                            .parse::<u16>()
                            .unwrap_or(8765)
                {
                    return Err("网关端口与模型或控制台冲突".to_string().into());
                }
                if action != "gateway_start" && a.store.setting("active_gateway")?["id"] != *id {
                    return Err("所选网关未运行".to_string().into());
                }
                active
            }
        }
        _ => return Err("该配置不支持此操作".to_string().into()),
    };
    Ok((c, g, snapshot))
}
async fn assert_deployment(a: &App, id: &str) -> Result<()> {
    let st = a.runtime.status().await?;
    let job = st["job"]
        .as_str()
        .ok_or("没有属于此部署的实例".to_string())?;
    if a.store.setting(&format!("composition:{job}"))?["id"] != id {
        return Err("当前实例属于其他部署，不能操作".to_string().into());
    }
    Ok(())
}

async fn prepared_config(a: &App, cat: &catalog::Catalog, id: &str) -> Result<Config> {
    let draft = cat.compose_draft(id, &a.runtime.data)?;
    if draft.engine == config::Engine::LlamaCpu {
        return Ok(cat.compose(id, &a.runtime.data)?);
    }
    let mut c = draft;
    if c.engine != config::Engine::LlamaCpu {
        let v = a.runtime.local_image(&c.image).await?;
        c.image = v["Id"].as_str().ok_or("镜像 ID 不存在".to_string())?.into();
    }
    c.validate()?;
    Ok(c)
}
pub(crate) async fn model_status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    let m: catalog::Model = cat.data("models", &id)?;
    let path = m
        .source
        .as_ref()
        .map(|s| artifacts::destination(&a.runtime.data, s))
        .unwrap_or_else(|| std::path::PathBuf::from(m.model_path));
    Ok(Json(artifacts::readiness(&path, m.source.as_ref())))
}

/// Readiness describes this selected gateway, not the global default draft.
pub(crate) async fn gateway_readiness(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    let mut gateway: catalog::Gateway = cat.data("gateways", &id)?;
    let active = a.store.setting("active_gateway")?;
    let running = a.gateway.running().await?;
    let owned = running && active["id"].as_str() == Some(id.as_str());
    let model = a
        .runtime
        .status()
        .await
        .unwrap_or_else(|error| json!({"state":"unknown","error":error}));
    let composition = match model["job"].as_str() {
        Some(job) => a.store.setting(&format!("composition:{job}"))?,
        None => Value::Null,
    };
    let model_ready = model["state"] == "running"
        && composition["id"].as_str() == Some(gateway.deployment.as_str());
    let certificate = if gateway.config.tls_mode == "enterprise" {
        a.gateway.validate_certificates(&gateway.config).await
    } else {
        gateway.config.validate()
    };
    let certificate_reason = if certificate.is_ok() {
        "ready"
    } else if gateway.config.tls_mode != "enterprise" {
        "invalid_config"
    } else if [
        &gateway.config.cert_file,
        &gateway.config.key_file,
        &gateway.config.ca_file,
    ]
    .iter()
    .any(|v| v.is_empty())
    {
        "missing"
    } else if a
        .gateway
        .validate_certificate_material(&gateway.config, false)
        .await
        .is_ok()
    {
        "identity_mismatch"
    } else {
        "invalid"
    };
    let services = services::status(&a.runtime).await?;
    let services_ready = !services::enabled()
        || services["components"]
            .as_array()
            .is_some_and(|c| c.len() == 2 && c.iter().all(|v| v["state"] == "running"));
    let mut changed =
        owned && active["data"] != serde_json::to_value(&gateway).map_err(|e| e.to_string())?;
    if owned && model_ready {
        if access::configured() {
            gateway.config.workspace_scope = id.clone();
        }
        let config = tasks::active_config(&a.runtime, &a.store).await?;
        changed |= a.gateway.matches(&gateway.config, &config).await.is_err();
    }
    let regenerate_test = gateway.config.tls_mode == "self_signed"
        && owned
        && (active["data"]["config"]["domain"] != gateway.config.domain
            || active["data"]["config"]["tls_mode"] != "self_signed");
    let online = crate::gateway_container::name().is_some() && !regenerate_test;
    let mut impact = gateway_change_plan(
        &gateway,
        &active,
        owned,
        changed,
        online,
        certificate_reason,
    );
    if !model_ready && !gateway.deployment.is_empty() {
        impact["model_message"] = json!("当前关联模型尚未确认运行，请先检查并启动该部署；切换模型部署与网关应用配置是两个步骤。");
    }
    Ok(Json(
        json!({"running":owned,"other_running":running && !owned,
        "active_name":active["name"],"model_ready":model_ready,"model_error":model["error"],"deployment":gateway.deployment,
        "certificate_ready":certificate.is_ok(),"certificate_error":certificate.err(),
        "self_signed":gateway.config.tls_mode == "self_signed", "services_ready":services_ready,
        "changed":changed,"online_apply":online,"impact":impact,"certificate_reason":certificate_reason,
        "busy":a.gate.try_write().is_err()}),
    ))
}

fn gateway_change_plan(
    g: &catalog::Gateway,
    active: &Value,
    running: bool,
    changed: bool,
    online: bool,
    certificate: &str,
) -> Value {
    let current = serde_json::to_value(&g.config).unwrap_or(Value::Null);
    let mut changes = Vec::new();
    if running {
        for (key, label) in [
            ("domain", "访问域名 / IP"),
            ("bind_ip", "监听地址"),
            ("port", "HTTPS 端口"),
            ("allowed_cidrs", "允许来源网段"),
            ("tls_mode", "证书模式"),
        ] {
            if active["data"]["config"][key] != current[key] {
                changes.push(label);
            }
        }
        if ["cert_file", "key_file", "ca_file"]
            .iter()
            .any(|key| active["data"]["config"][key] != current[key])
        {
            changes.push("证书材料");
        }
        if active["data"]["deployment"] != g.deployment {
            changes.push("关联模型部署");
        }
        if changed && changes.is_empty() {
            changes.push("运行配置快照");
        }
    }
    let (application, application_message) = if !running {
        (
            "start",
            "当前入口未启动；准备完成后点击「启动网关」，保存的配置将在启动时生效。",
        )
    } else if !changed {
        ("none", "当前配置已生效，无需重启网关。")
    } else if online {
        (
            "reload",
            "需要应用更改：点击「应用更改」在线加载，无需重启网关。",
        )
    } else {
        (
            "restart",
            "需要重启网关：先停止，再点击「启动网关」。期间客户端访问中断，模型服务不受影响。",
        )
    };
    let (kind, title, certificate_message) = match certificate {
        "missing" => ("certificate_missing", "需要配置证书", "尚未配置企业证书，请先申请并导入，或导入已有证书与私钥。"),
        "identity_mismatch" => ("certificate_reissue", "需要重新签发证书", "当前证书未覆盖保存的访问域名 / IP。请按新地址生成 CSR 并签发，或导入已覆盖该地址的证书。"),
        "invalid" => ("certificate_invalid", "需要修复证书", "证书链、有效期或私钥校验未通过。请在证书管理查看原因，重新签发或导入匹配的有效证书。"),
        "invalid_config" => ("invalid_config", "需要修正入口配置", "入口配置不符合当前证书模式，请修改后再启用。"),
        _ => (application, match application { "reload" => "需要应用更改", "restart" => "需要重启网关", "start" => "启动时生效", _ => "配置已生效" },
            if g.config.tls_mode == "enterprise" { "现有证书覆盖当前地址且校验通过，无需重新签发证书。" } else { "本地测试证书在启动时自动准备，无需向 CA 申请。" }),
    };
    json!({"kind":kind,"title":title,"application":application,"application_message":application_message,
        "certificate_message":certificate_message,"changes":changes})
}

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
    ca: String,
}
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
    for text in [&body.certificate, &private_key, &body.ca] {
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
            ("server.pem", &body.certificate),
            ("server.key", &private_key),
            ("ca.pem", &body.ca),
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CredentialAction {
    operation: String,
}
pub(crate) async fn credential_status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    let workspace: catalog::Workspace = cat.data("workspaces", &id)?;
    if !access::configured() {
        return Ok(Json(json!({"configured":false})));
    }
    let (status, mut data) = access::call(&a.runtime.data, &id, None).await?;
    if status == 404 {
        return Ok(Json(json!({"configured":true,"initialized":false})));
    }
    if status != 200 {
        return Err("无法读取独立鉴权服务".to_string().into());
    }
    data["configured"] = json!(true);
    data["initialized"] = json!(true);
    data["matches_draft"] = json!(data["gateway"] == workspace.gateway);
    Ok(Json(data))
}
pub(crate) async fn credential_change(
    State(a): State<App>,
    Path(id): Path<String>,
    Json(body): Json<CredentialAction>,
) -> Result<Json<Value>> {
    let _permit = a
        .gate
        .clone()
        .try_write_owned()
        .map_err(|_| Error(StatusCode::CONFLICT, "任务执行中".into()))?;
    let cat = catalog::Catalog::load(&a.store)?;
    let workspace: catalog::Workspace = cat.data("workspaces", &id)?;
    cat.get("gateways", &workspace.gateway)?;
    if !["apply", "rotate", "revoke", "disable", "enable"].contains(&body.operation.as_str()) {
        return Err("未知密钥操作".to_string().into());
    }
    if ["rotate", "enable"].contains(&body.operation.as_str()) {
        let (status, current) = access::call(&a.runtime.data, &id, None).await?;
        if status != 200 || current["gateway"] != workspace.gateway {
            return Err(Error(
                StatusCode::CONFLICT,
                "请先应用当前工作区接入配置".into(),
            ));
        }
    }
    let (status, data) = access::call(
        &a.runtime.data,
        &id,
        Some(json!({"operation":body.operation,"gateway":workspace.gateway})),
    )
    .await?;
    if status != 200 {
        return Err(Error(
            StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),
            data["error"].as_str().unwrap_or("密钥操作未确认").into(),
        ));
    }
    Ok(Json(data))
}

pub(crate) async fn workspace_requests(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    catalog::Catalog::load(&a.store)?.get("workspaces", &id)?;
    Ok(Json(
        crate::gateway_container::requests(&a.gateway, &id).await?,
    ))
}

pub(crate) async fn deployment_comparison(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    cat.get("deployments", &id)?;
    let status = a.runtime.status().await?;
    let mut active = Value::Null;
    if let Some(job) = status["job"].as_str() {
        if a.store.setting(&format!("composition:{job}"))?["id"] == id {
            active = serde_json::to_value(a.store.job_config(job)?).map_err(|e| e.to_string())?;
        }
    }
    let mut problem = None;
    let draft = match prepared_config(&a, &cat, &id).await {
        Ok(c) => serde_json::to_value(c).map_err(|e| e.to_string())?,
        Err(e) => {
            problem = Some(e.1);
            cat.compose_draft(&id, &a.runtime.data)
                .ok()
                .and_then(|c| serde_json::to_value(c).ok())
                .unwrap_or(Value::Null)
        }
    };
    let changes = if let (Some(current), Some(next)) = (active.as_object(), draft.as_object()) {
        next.iter()
            .filter(|(k, v)| current.get(*k) != Some(*v))
            .map(|(k, v)| json!({"field":k,"running":current.get(k),"draft":v}))
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    Ok(Json(
        json!({"draft":draft,"running":active,"changes":changes,"preparation_error":problem,"runtime_state":status["state"],"composition":cat.snapshot("deployments",&id)?}),
    ))
}

#[cfg(test)]
mod change_plan_tests {
    use super::*;
    #[test]
    fn change_plan_distinguishes_activation_restart_and_signing() {
        let mut gateway = catalog::Gateway {
            deployment: "model".into(),
            config: gateway::GatewayConfig {
                tls_mode: "enterprise".into(),
                ..Default::default()
            },
        };
        let active = json!({"data":gateway});
        gateway.config.port = 19443;
        let reload = gateway_change_plan(&gateway, &active, true, true, true, "ready");
        assert_eq!(reload["application"], "reload");
        assert_eq!(reload["changes"], json!(["HTTPS 端口"]));
        assert!(reload["certificate_message"]
            .as_str()
            .unwrap()
            .contains("无需重新签发"));
        assert_eq!(
            gateway_change_plan(&gateway, &active, true, true, false, "ready")["kind"],
            "restart"
        );
        assert_eq!(
            gateway_change_plan(&gateway, &active, false, false, true, "ready")["kind"],
            "start"
        );
        assert_eq!(
            gateway_change_plan(&gateway, &active, true, false, true, "ready")["kind"],
            "none"
        );
        gateway.config.domain = "new.example.test".into();
        assert_eq!(
            gateway_change_plan(&gateway, &active, true, true, true, "identity_mismatch")["kind"],
            "certificate_reissue"
        );
        // A SAN-covered name change still only needs applying the configuration.
        assert_eq!(
            gateway_change_plan(&gateway, &active, true, true, true, "ready")["kind"],
            "reload"
        );
        assert_eq!(
            gateway_change_plan(&gateway, &active, true, true, true, "invalid")["kind"],
            "certificate_invalid"
        );
    }
}
