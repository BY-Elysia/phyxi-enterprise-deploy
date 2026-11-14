//! One orchestrator, distinct model, host, gateway and acceptance responsibilities.
use crate::{
    config::Config,
    gateway::{Gateway, GatewayConfig},
    runtime::{execute, Runtime},
    store::Store,
    verification,
};
use serde_json::{json, Value};
use std::sync::{atomic::AtomicBool, Arc};
use tokio::process::Command;
pub const ACTIONS: &[&str] = &[
    "management_start",
    "management_stop",
    "services_start",
    "services_stop",
    "download",
    "pull_image",
    "start",
    "redeploy",
    "stop",
    "prepare",
    "repair_toolkit",
    "verify_gpu",
    "certificate",
    "gateway_start",
    "gateway_apply",
    "gateway_stop",
    "verify",
    "publish",
];
pub fn gateway_config(store: &Store) -> Result<GatewayConfig, String> {
    let v = store.setting("gateway_config")?;
    if v.is_null() {
        Ok(GatewayConfig::default())
    } else {
        serde_json::from_value(v).map_err(|e| e.to_string())
    }
}
pub async fn active_config(runtime: &Runtime, store: &Store) -> Result<Config, String> {
    let status = runtime.status().await?;
    if status["state"] != "running" {
        return Err("没有运行中的模型".into());
    }
    let id = status["job"].as_str().ok_or("缺少模型配置归属")?;
    store.job_config(id)
}
pub async fn run(
    runtime: &Runtime,
    gateway: &Gateway,
    action: &str,
    snapshot: (&Config, &GatewayConfig),
    id: &str,
    store: Arc<Store>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let (c, g) = snapshot;
    store.set_setting(
        &format!("task-context:{id}"),
        &json!({"gateway":g,"model":c}),
    )?;
    store.update(id, "running", action)?;
    match action {
        "management_start" | "management_stop" => {
            crate::services::management(runtime, gateway, g, action == "management_start").await
        }
        "services_start" | "services_stop" => {
            crate::services::run(
                runtime,
                gateway,
                action == "services_start",
                id,
                store,
                cancel,
            )
            .await
        }
        "download" => {
            if runtime
                .status()
                .await
                .ok()
                .is_some_and(|s| s["state"] == "running")
            {
                return Err("请先停止运行模型，再准备权重".into());
            }
            crate::artifacts::download(&runtime.data, c, id, store, cancel).await
        }
        "pull_image" => {
            crate::image_sources::pull(runtime, &c.image, id, store, cancel.clone()).await?;
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("任务已取消".into());
            }
            Ok(())
        }
        "start" => runtime.job(action, c, id, store, cancel).await,
        "stop" => {
            if gateway.running().await? {
                return Err("网关正在提供服务，请先停止网关再停止模型".into());
            }
            runtime.job(action, c, id, store, cancel).await
        }
        "verify_gpu" => crate::host_setup::verify(runtime, id, store).await,
        "prepare" | "repair_toolkit" if crate::executor::enabled() => {
            let mut cmd = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
            cmd.args([
                "--prepare-toolkit",
                if action == "prepare" {
                    "install"
                } else {
                    "repair"
                },
            ]);
            execute(cmd, Some((store, id.into())), 1800).await?;
            Ok(())
        }
        "prepare" | "repair_toolkit" => {
            Err("宿主机安装需要连接受控执行器；不会在控制台容器内安装。".into())
        }
        "redeploy" => {
            let previous = active_config(runtime, &store).await?;
            let mut snapshot = store.setting("active_gateway")?;
            let gateway_config = if gateway.running().await? {
                let profile: crate::catalog::Gateway =
                    serde_json::from_value(snapshot["data"].clone())
                        .map_err(|_| "缺少运行网关快照，拒绝自动停用")?;
                let mut config = profile.config;
                if crate::access::configured() {
                    config.workspace_scope = snapshot["id"].as_str().ok_or("缺少网关归属")?.into();
                }
                gateway.matches(&config, &previous).await?;
                Some(config)
            } else {
                None
            };
            store.set_setting(
                &format!("redeploy_previous:{id}"),
                &json!({"model":previous,"gateway":snapshot}),
            )?;
            if gateway_config.is_some() {
                gateway.stop().await?;
                store.set_setting("active_gateway", &Value::Null)?;
            }
            runtime
                .job("stop", &previous, id, store.clone(), cancel.clone())
                .await?;
            store.event(
                id,
                "旧实例已停止；开始按冻结快照重新部署，失败时不会假称旧实例仍可用",
            )?;
            runtime.job("start", c, id, store.clone(), cancel).await?;
            if let Some(config) = gateway_config {
                gateway.start(&config, c).await?;
                snapshot["dependencies"]["deployment"] =
                    store.setting(&format!("composition:{id}"))?;
                store.set_setting("active_gateway", &snapshot)?;
            }
            Ok(())
        }
        "certificate" => {
            if gateway.running().await? {
                return Err("请先停止网关，再更换证书".into());
            }
            gateway.certificates(g).await?;
            store.event(id, "证书已验证并保存，私钥不会通过页面返回")
        }
        "gateway_start" => {
            let active = active_config(runtime, &store).await?;
            gateway.start(g, &active).await?;
            store.event(id, "网关启动，下一步执行接口验收")
        }
        "gateway_apply" => {
            let active = active_config(runtime, &store).await?;
            gateway.apply_config(g, &active).await?;
            store.event(id, "网关配置与证书已校验并在线应用，已有连接允许完成")
        }
        "gateway_stop" => gateway.stop().await,
        "verify" | "publish" => {
            let active = active_config(runtime, &store).await?;
            if *c != active {
                return Err("草稿配置与运行实例不一致，禁止验收/发布；恢复配置或重新部署".into());
            }
            let via_gateway = gateway.running().await?;
            if action == "publish" && !via_gateway {
                return Err("发布需要启动企业网关，并通过网关重新验收".into());
            }
            if via_gateway {
                gateway.matches(g, &active).await?;
            }
            store.event(
                id,
                "检查模型名、真实生成、SSE；网关模式额外检查匿名请求被拒绝",
            )?;
            let verification = verification::verify(
                &runtime.data,
                &active,
                if via_gateway {
                    Some((gateway, g))
                } else {
                    None
                },
            )
            .await;
            let report = match verification {
                Ok(report) => report,
                Err(error) => {
                    let report = json!({"passed":false,"error":error,"via_gateway":via_gateway,"model_config":active});
                    store.set_setting("last_verification", &report)?;
                    store.set_setting(&format!("verification:{id}"), &report)?;
                    return Err(error);
                }
            };
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("任务已取消".into());
            }
            store.set_setting("last_verification", &report)?;
            store.set_setting(&format!("verification:{id}"), &report)?;
            store.event(id, &report.to_string())?;
            if action == "publish" {
                let export = gateway.export(g, &active)?;
                let mut releases = store
                    .setting("releases")?
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                releases.insert(0,json!({"id":id,"created_unix":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs(),"model":active,"gateway":g,"composition":store.setting(&format!("composition:{id}"))?,"report":report,"export_directory":export}));
                releases.truncate(20);
                store.set_setting("releases", &Value::Array(releases))?;
                store.event(id, "发布记录与工作站材料已保存；令牌仅保存在服务器受控目录")?;
            }
            Ok(())
        }
        _ => Err("未知任务".into()),
    }
}
