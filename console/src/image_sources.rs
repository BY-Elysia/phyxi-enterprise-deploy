//! Explicit image source routing. No daemon-wide mirrors or silent fallback.
use crate::{
    runtime::{execute, Runtime},
    store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::process::Command;
pub const NGINX: &str =
    "nginx@sha256:552e7481ca93ffccd046aa658dbbed22caefbc09c66fa7cd247cbb90b8a5c609";
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sources {
    pub mode: String,
    pub mirror_prefix: String,
    pub gateway_image: String,
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            mode: "default".into(),
            mirror_prefix: String::new(),
            gateway_image: NGINX.into(),
        }
    }
}
impl Sources {
    pub fn validate(&self) -> Result<(), String> {
        if !["default", "direct", "enterprise"].contains(&self.mode.as_str()) {
            return Err("未知镜像下载方式".into());
        }
        if self.mode == "enterprise" {
            let prefix = &self.mirror_prefix;
            let host = prefix.split('/').next().unwrap_or_default();
            if prefix.is_empty()
                || prefix.len() > 256
                || prefix.contains(['@', '?', '#'])
                || prefix.contains("..")
                || prefix.ends_with('/')
                || !(host.contains('.') || host.contains(':') || host == "localhost")
            {
                return Err("企业代理需填写仓库主机及可选前缀，例如 registry.company.example/dockerhub；不含协议、密码或末尾斜杠".into());
            }
            crate::catalog::image_valid(prefix)?;
        }
        crate::catalog::image_valid(&self.gateway_image)
    }
    pub fn resolve(&self, reference: &str) -> Result<String, String> {
        self.validate()?;
        crate::catalog::image_valid(reference)?;
        if reference.starts_with("sha256:") || self.mode == "direct" {
            return Ok(reference.into());
        }
        let hub = reference.strip_prefix("docker.io/").unwrap_or(reference);
        let first = hub.split('/').next().unwrap_or_default();
        if hub.contains('/') && (first.contains('.') || first.contains(':') || first == "localhost")
        {
            return Ok(reference.into());
        }
        let repo = if hub.contains('/') {
            hub.to_string()
        } else {
            format!("library/{hub}")
        };
        // Only built-in public namespaces use the public default. Custom/private names stay explicit.
        if ![
            "library/nginx:",
            "library/nginx@",
            "vllm/vllm-openai:",
            "vllm/vllm-openai@",
            "lmsysorg/sglang:",
            "lmsysorg/sglang@",
        ]
        .iter()
        .any(|p| repo.starts_with(p))
        {
            return Ok(reference.into());
        }
        let prefix = if self.mode == "default" {
            "m.daocloud.io/docker.io"
        } else {
            &self.mirror_prefix
        };
        Ok(format!("{prefix}/{repo}"))
    }
}
pub fn load(data: &Path) -> Result<Sources, String> {
    let settings = match fs::read(data.join("image-sources.json")) {
        Ok(raw) => {
            serde_json::from_slice::<Sources>(&raw).map_err(|e| format!("镜像来源配置损坏：{e}"))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Sources::default(),
        Err(e) => return Err(e.to_string()),
    };
    settings.validate()?;
    Ok(settings)
}
pub fn save(data: &Path, settings: &Sources) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    settings.validate()?;
    let path = data.join("image-sources.json.new");
    let mut f = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    fs::rename(path, data.join("image-sources.json")).map_err(|e| e.to_string())
}
pub fn info(data: &Path) -> Result<Value, String> {
    let c = load(data)?;
    Ok(
        json!({"default_gateway_image":NGINX,"effective_gateway_image":c.resolve(&c.gateway_image)?,"config":c,"note":"默认使用 DaoCloud 公共镜像服务，仅路由 Nginx/vLLM/SGLang 预设。自定义完整仓库地址原样使用；失败不静默切换其他公共源。"}),
    )
}
pub async fn pull(
    r: &Runtime,
    reference: &str,
    id: &str,
    store: Arc<Store>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let target = load(&r.data)?.resolve(reference)?;
    store.set_setting(
        &format!("progress:{id}"),
        &json!({"kind":"image","source":target,"phase":"正在连接镜像仓库，等待镜像层信息"}),
    )?;
    store.event(id,&format!("下载镜像：{reference}\n实际来源：{target}\n仅准备镜像，不启动模型；已完成的镜像层由 Docker 缓存复用。"))?;
    let mut c = Command::new("docker");
    c.args(["--host", &r.docker_host, "pull", &target]);
    let result = tokio::select! {
        result = execute(c, Some((store.clone(), id.into())), 4 * 60 * 60) => result,
        _ = async {
            loop {
                if cancel.load(Ordering::SeqCst) { break; }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        } => {
            store.event(id, "镜像下载已取消；已完成的层保留在 Docker 缓存中")?;
            return Err("任务已取消".into());
        },
    };
    if let Err(e) = result {
        store.event(id, &e)?;
        return Err("镜像下载未完成。请在“镜像下载来源”检查网络或改用企业仓库，然后重试；原始错误见任务日志。不会自动切换来源。".into());
    }
    let image = r.local_image(&target).await?;
    store.event(
        id,
        &format!("镜像已准备：{}；按本地不可变 ID 部署。", image["Id"]),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_is_explicit_and_private_names_are_not_leaked() {
        let c = Sources::default();
        assert_eq!(
            c.resolve("nginx:1.28-bookworm").unwrap(),
            "m.daocloud.io/docker.io/library/nginx:1.28-bookworm"
        );
        assert_eq!(
            c.resolve("docker.io/vllm/vllm-openai:v0.11.0").unwrap(),
            "m.daocloud.io/docker.io/vllm/vllm-openai:v0.11.0"
        );
        for reference in [
            "registry.company.example/ai/vllm:v1",
            "private-team/secret:v1",
            "docker.io/private-team/secret:v1",
        ] {
            assert_eq!(c.resolve(reference).unwrap(), reference);
        }
        assert_eq!(
            c.resolve(NGINX).unwrap(),
            format!("m.daocloud.io/docker.io/library/{NGINX}")
        );
        let local = format!("sha256:{}", "a".repeat(64));
        assert_eq!(c.resolve(&local).unwrap(), local);
        let direct = Sources {
            mode: "direct".into(),
            ..c.clone()
        };
        assert_eq!(direct.resolve(NGINX).unwrap(), NGINX);
        let enterprise = Sources {
            mode: "enterprise".into(),
            mirror_prefix: "registry.company.example/dockerhub".into(),
            ..c
        };
        assert_eq!(
            enterprise.resolve("nginx:1.28-bookworm").unwrap(),
            "registry.company.example/dockerhub/library/nginx:1.28-bookworm"
        );
        for prefix in [
            "https://registry.example",
            "user:pass@registry.example",
            "registry.example/a/../b",
            "registry.example/",
        ] {
            assert!(Sources {
                mirror_prefix: prefix.into(),
                ..enterprise.clone()
            }
            .validate()
            .is_err());
        }
    }
}
