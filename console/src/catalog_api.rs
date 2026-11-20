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
        json!({"records":catalog::Catalog::load(&a.store)?.records}),
    ))
}
#[derive(Deserialize)]
pub(crate) struct SaveOptions {
    #[serde(default)]
    local: bool,
    root: Option<String>,
}
pub(crate) async fn catalog_put(
    State(a): State<App>,
    Path((kind, id)): Path<(String, String)>,
    Query(options): Query<SaveOptions>,
    Json(mut record): Json<catalog::Record>,
) -> Result<Json<Value>> {
    if kind == "workspaces" && id.starts_with("system-") {
        return Err("system- 前缀用于系统内部工作区".to_string().into());
    }
    let mut registered_path = None;
    if kind == "models" {
        let mut model: catalog::Model =
            serde_json::from_value(record.data.clone()).map_err(|e| e.to_string())?;
        if options.local {
            if model.source.is_some() {
                return Err("添加已有权重时无需填写下载来源".to_string().into());
            }
            let data = a.runtime.data.clone();
            let input = model.model_path.clone();
            let root = options.root;
            let checked = tokio::task::spawn_blocking(move || {
                local_models::check_model(&data, &input, root.as_deref())
            })
            .await
            .map_err(|e| e.to_string())??;
            let path = checked.path;
            let basename = if path.is_file() {
                path.file_stem()
            } else {
                path.file_name()
            }
            .and_then(|s| s.to_str())
            .unwrap_or("phyxi-model");
            if record.name.trim().is_empty() {
                record.name = basename.chars().take(40).collect();
            }
            if model.model_name.trim().is_empty() {
                model.model_name = basename
                    .chars()
                    .take(100)
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || "_-.".contains(c) {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
            }
            model.model_path = path.to_string_lossy().into_owned();
            if checked.requires_registration {
                registered_path = Some(model.model_path.clone());
            }
        } else if let Some(source) = model.source {
            let source = artifacts::resolve(source).await?;
            let previous = catalog::Catalog::load(&a.store)?
                .data::<catalog::Model>("models", &id)
                .ok();
            let root = previous
                .as_ref()
                .and_then(|m| {
                    m.source.as_ref().and_then(|s| {
                        download_storage::root_for(std::path::Path::new(&m.model_path), s).ok()
                    })
                })
                .unwrap_or(download_storage::selected(&a)?);
            model.model_path = download_storage::target(&root, &source)
                .display()
                .to_string();
            registered_path = Some(model.model_path.clone());
            model.source = Some(source);
        }
        record.data = serde_json::to_value(model).map_err(|e| e.to_string())?;
    }
    if kind == "deployments" {
        let d: catalog::Deployment =
            serde_json::from_value(record.data.clone()).map_err(|e| e.to_string())?;
        let old = catalog::Catalog::load(&a.store)?
            .data::<catalog::Deployment>("deployments", &id)
            .ok();
        if old.as_ref().is_some_and(|previous| previous.mode != d.mode)
            && deployment_job(&a, &a.runtime.status().await?, &id)?.is_some()
        {
            return Err("请先停止此部署再切换接入方式".to_string().into());
        }
        match d.mode {
            native_inference::Mode::External => {
                native_inference::probe(d.port, &d.model_name).await?;
            }
            native_inference::Mode::Native => {
                native_inference::binary(&d.native_binary)?;
            }
            _ => {}
        }
    }
    if kind == "images" && options.local {
        let mut image: catalog::Image =
            serde_json::from_value(record.data.clone()).map_err(|e| e.to_string())?;
        let checked = a
            .runtime
            .checked_local_image(&image.reference, &image.engine)
            .await?;
        if record.name.trim().is_empty() {
            record.name = checked.reference.chars().take(80).collect();
        }
        image.reference = checked.id;
        record.data = serde_json::to_value(image).map_err(|e| e.to_string())?;
    }
    let _catalog = a.catalog_gate.lock().await;
    let mut cat = catalog::Catalog::load(&a.store)?;
    cat.put(&kind, &id, record)?;
    if let Some(path) = registered_path {
        cat.register_model_path(&id, path);
    }
    cat.save(&a.store)?;
    Ok(Json(json!({"saved":true,"record":cat.get(&kind,&id)?})))
}
pub(crate) async fn local_model_paths(
    State(a): State<App>,
    Query(query): Query<local_models::Browse>,
) -> Result<Json<Value>> {
    let data = a.runtime.data.clone();
    let result = tokio::task::spawn_blocking(move || local_models::browse(&data, query))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub(crate) struct LocalImagesQuery {
    engine: Option<config::Engine>,
}
pub(crate) async fn local_images(
    State(a): State<App>,
    Query(query): Query<LocalImagesQuery>,
) -> Result<Json<Value>> {
    let engine = query.engine.unwrap_or_default();
    Ok(Json(
        json!({"images":a.runtime.discover_local_images(&engine).await?}),
    ))
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
    if kind == "deployments" && deployment_job(&a, &a.runtime.status().await?, &id)?.is_some() {
        return Err("请先停止该部署实例".to_string().into());
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
                model_path: m.model_path,
                source: Some(source),
                ..Config::default()
            }
        }
        ("images", "pull_image") => {
            let i: catalog::Image = cat.data(kind, id)?;
            catalog::image_valid(&i.reference)?;
            if i.reference.starts_with("sha256:") {
                return Err("本机镜像 ID 没有下载地址；请重新检测或添加仓库来源"
                    .to_string()
                    .into());
            }
            Config {
                image: i.reference,
                ..Config::default()
            }
        }
        ("deployments", "start" | "redeploy" | "verify") => {
            let c = prepared_config(a, &cat, id).await?;
            if std::env::var("PHYXI_MODEL_NETWORK").is_ok() && c.engine == config::Engine::LlamaCpu
            {
                return Err("CPU 后端仅供原生本地测试；容器模式请选择 GPU 推理引擎"
                    .to_string()
                    .into());
            }
            if action == "redeploy" {
                require_deployment_job(a, id).await?;
            }
            if action == "verify" {
                require_deployment_job(a, id).await?;
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
                return Err("模型端口与接入组件保留端口冲突".to_string().into());
            }
            c
        }
        ("deployments", "stop") => {
            let job = require_deployment_job(a, id).await?;
            a.store.job_config(&job)?
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
                require_deployment_job(a, &profile.deployment).await?;
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
fn deployment_job(a: &App, state: &Value, id: &str) -> Result<Option<String>> {
    let Some(job) = state["job"].as_str() else {
        return Ok(None);
    };
    let snapshot = a.store.setting(&format!("composition:{job}"))?;
    Ok((snapshot["id"] == id).then(|| job.to_owned()))
}

async fn require_deployment_job(a: &App, id: &str) -> Result<String> {
    deployment_job(a, &a.runtime.status().await?, id)?
        .ok_or_else(|| "没有属于此部署的实例".to_string().into())
}

async fn prepared_config(a: &App, cat: &catalog::Catalog, id: &str) -> Result<Config> {
    let draft = cat.compose_draft(id, &a.runtime.data)?;
    if draft.engine == config::Engine::LlamaCpu {
        return Ok(cat.compose(id, &a.runtime.data)?);
    }
    let mut c = draft;
    if !c.native_binary.is_empty() {
        native_inference::binary(&c.native_binary)?;
    } else if c.engine != config::Engine::LlamaCpu {
        let v = a.runtime.local_image(&c.image).await?;
        c.image = v["Id"].as_str().ok_or("镜像 ID 不存在".to_string())?.into();
    }
    c.validate()?;
    Ok(c)
}
pub(crate) async fn download_preview(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let m: catalog::Model = catalog::Catalog::load(&a.store)?.data("models", &id)?;
    let source = m.source.ok_or("没有下载来源".to_string())?;
    let metadata = artifacts::metadata(&source).await?;
    Ok(Json(
        json!({"path":m.model_path,"available_bytes":download_storage::free(std::path::Path::new(&m.model_path)),"estimated_bytes":artifacts::estimate(&metadata,&source)}),
    ))
}
pub(crate) async fn model_status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    let m: catalog::Model = cat.data("models", &id)?;
    let path = std::path::PathBuf::from(&m.model_path);
    let result = tokio::task::spawn_blocking(move || {
        if executor::enabled() && m.source.is_none() {
            match local_models::status_path(&a.runtime.data, &path, &a.store) {
                Ok(path) => artifacts::readiness(&path, None),
                Err(detail) => json!({"ready":false,"path":path,"detail":detail}),
            }
        } else {
            artifacts::readiness(&path, m.source.as_ref())
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(Json(result))
}

pub(crate) async fn deployment_comparison(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let cat = catalog::Catalog::load(&a.store)?;
    cat.get("deployments", &id)?;
    let status = a.runtime.status().await?;
    let active = match deployment_job(&a, &status, &id)? {
        Some(job) => serde_json::to_value(a.store.job_config(&job)?).map_err(|e| e.to_string())?,
        None => Value::Null,
    };
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
