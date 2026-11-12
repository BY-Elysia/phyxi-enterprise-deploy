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
    let m: catalog::Model = cat.data("models", &d.model)?;
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
        ("deployments", "start" | "verify") => {
            let c = prepared_config(a, &cat, id).await?;
            if action == "verify" {
                assert_deployment(a, id).await?;
                if a.gateway.running().await? {
                    return Err("请通过正在运行的网关执行验收，或先停止网关"
                        .to_string()
                        .into());
                }
            }
            if action == "start"
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
        ("gateways", "certificate" | "gateway_start" | "gateway_stop" | "verify" | "publish") => {
            let profile: catalog::Gateway = cat.data(kind, id)?;
            g = profile.config;
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
