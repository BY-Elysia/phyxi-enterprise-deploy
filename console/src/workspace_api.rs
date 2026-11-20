use crate::{access, catalog, App, Error, Result};
use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

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
        json!({"format":"phyxi-client-config-v1","generation":w.generation,"request_template":request,"opencode_v2":opencode_v2,"opencode_legacy_note":"opencode 字段保留旧版连接模板，不携带生成预设；请使用 opencode_v2 或 request_template。","workspace":cat.get("workspaces",&id)?.name,"base_url":format!("https://{}:{}/v1",g.config.domain,g.config.port),"model":m.model_name,"credential_env":w.credential_env,"ca_file":g.config.ca_file,"note":"客户端需设置凭据环境变量。使用私有 CA 或自签证书时，还需信任对应的 CA 或证书。","opencode":{"provider":{"phyxi":{"npm":"@ai-sdk/openai-compatible","name":"PhyXi","options":{"baseURL":format!("https://{}:{}/v1",g.config.domain,g.config.port),"apiKey":format!("{{env:{}}}",w.credential_env)},"models":{(m.model_name.clone()):{"name":m.model_name}}}}},"composition":cat.snapshot("workspaces",&id)?}),
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
