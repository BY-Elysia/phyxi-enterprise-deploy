//! Configuration CRUD and execution-time composition, separate from the runtime owner.
use super::*;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scope {
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
    let _permit = a
        .gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error(StatusCode::CONFLICT, "任务执行中，请稍后修改配置".into()))?;
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
        .try_acquire_owned()
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
    // Export a client template, never the server token or a private key.
    Ok(Json(
        json!({"workspace":cat.get("workspaces",&id)?.name,"base_url":format!("https://{}:{}/v1",g.config.domain,g.config.port),"model":m.model_name,"credential_env":w.credential_env,"ca_file":g.config.ca_file,"note":"客户端需自行设置凭据环境变量并安装信任证书；这是配置模板，不代表服务已经发布。","opencode":{"provider":{"phyxi":{"npm":"@ai-sdk/openai-compatible","name":"PhyXi","options":{"baseURL":format!("https://{}:{}/v1",g.config.domain,g.config.port),"apiKey":format!("{{env:{}}}",w.credential_env)},"models":{(m.model_name.clone()):{"name":m.model_name}}}}},"composition":cat.snapshot("workspaces",&id)?}),
    ))
}
pub(crate) async fn image_status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    let i: catalog::Image = cat.data("images", &id)?;
    catalog::image_valid(&i.reference)?;
    match a.runtime.local_image(&i.reference).await {
        Ok(v) => Ok(Json(
            json!({"available":true,"id":v["Id"],"architecture":v["Architecture"],"size_bytes":v["Size"],"reference":i.reference}),
        )),
        Err(e) => Ok(Json(json!({"available":false,"detail":e}))),
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
            "certificate" | "gateway_start" | "gateway_apply" | "gateway_stop" | "verify"
            | "publish",
        ) => {
            let profile: catalog::Gateway = cat.data(kind, id)?;
            g = profile.config;
            if access::configured() {
                g.workspace_scope = id.clone();
            }
            if action == "certificate" {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CertificateUpload {
    revision: u64,
    certificate: String,
    private_key: String,
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
        .try_acquire_owned()
        .map_err(|_| Error(StatusCode::CONFLICT, "任务执行中".into()))?;
    let mut cat = catalog::Catalog::load(&a.store)?;
    let mut record = cat.get("gateways", &id)?.clone();
    if record.revision != body.revision {
        return Err(Error(
            StatusCode::CONFLICT,
            "配置已更新，请刷新后重新上传".into(),
        ));
    }
    for text in [&body.certificate, &body.private_key, &body.ca] {
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
            ("server.key", &body.private_key),
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
        .try_acquire_owned()
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
