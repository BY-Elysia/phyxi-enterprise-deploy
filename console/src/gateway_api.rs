#[cfg(test)]
use crate::gateway;
use crate::{access, catalog, services, tasks, App, Result};
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};

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
            "新配置尚未生效：点击「使新配置生效（无需重启）」。网关继续运行，新请求使用新配置，已有连接允许完成；模型服务不受影响。",
        )
    } else {
        (
            "restart",
            "需要重启网关：先停止，再点击「启动网关」。期间客户端访问中断，模型服务不受影响。",
        )
    };
    let (kind, title, certificate_message) = match certificate {
        "missing" => ("certificate_missing", "需要配置证书", "尚未配置证书。可导入 Let’s Encrypt 证书，或使用已有 CA。"),
        "identity_mismatch" => ("certificate_reissue", "需要重新签发证书", "当前证书未覆盖保存的访问域名 / IP。请按新地址生成 CSR 并签发，或导入已覆盖该地址的证书。"),
        "invalid" => ("certificate_invalid", "需要修复证书", "证书链、有效期或私钥校验未通过。请在证书管理查看原因，重新签发或导入匹配的有效证书。"),
        "invalid_config" => ("invalid_config", "需要修正入口配置", "入口配置不符合当前证书模式，请修改后再启用。"),
        _ => (application, match application { "reload" => "新配置尚未生效", "restart" => "需要重启网关", "start" => "启动时生效", _ => "配置已生效" },
            if g.config.tls_mode == "enterprise" { "现有证书覆盖当前地址且校验通过，无需重新签发证书。" } else { "本地测试证书在启动时自动准备，无需向 CA 申请。" }),
    };
    json!({"kind":kind,"title":title,"application":application,"application_message":application_message,
        "certificate_message":certificate_message,"changes":changes})
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
