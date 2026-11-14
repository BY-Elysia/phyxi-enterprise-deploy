//! Fixed-revision public HF artifacts; only data files, never remote Python code.
use crate::{config::Config, runtime::execute, store::Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::process::Command;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub repo: String,
    pub revision: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub file: String,
}
impl Source {
    pub fn validate(&self) -> Result<(), String> {
        if !self.endpoint.is_empty() {
            endpoint(&self.endpoint)?;
        }
        let parts: Vec<_> = self.repo.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|s| {
                s.is_empty()
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                    || *s == ".."
            })
        {
            return Err("模型仓库必须是 owner/name".into());
        }
        if self.revision.len() != 40 || !self.revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("模型版本必须是固定的 40 位 commit".into());
        }
        if !self.file.is_empty() && !safe_file(&self.file) {
            return Err("模型文件名不安全或不是允许的数据文件".into());
        }
        Ok(())
    }
}
fn safe_file(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('/')
        && s.split('/').all(|p| {
            !p.is_empty()
                && p != ".."
                && p != "."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_ .".contains(&b))
        })
        && [
            ".json",
            ".safetensors",
            ".model",
            ".txt",
            ".tiktoken",
            ".jinja",
            ".gguf",
        ]
        .iter()
        .any(|e| s.ends_with(e))
}
pub fn destination(data: &Path, s: &Source) -> PathBuf {
    let root = data
        .join("models")
        .join(s.repo.replace('/', "--"))
        .join(&s.revision);
    if s.file.is_empty() {
        root
    } else {
        root.join(&s.file)
    }
}
fn hash(p: &Path) -> Result<String, String> {
    let mut f = fs::File::open(p).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut b = [0u8; 65536];
    loop {
        let n = f.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn endpoint(value: &str) -> Result<&str, String> {
    let base = value.trim_end_matches('/');
    if !["https://huggingface.co", "https://hf-mirror.com"].contains(&base) {
        return Err("PHYXI_HF_ENDPOINT 仅支持 Hugging Face 官方站或 hf-mirror.com".into());
    }
    Ok(base)
}
pub fn source_endpoint(s: &Source) -> Result<String, String> {
    let configured = if s.endpoint.is_empty() {
        std::env::var("PHYXI_HF_ENDPOINT").unwrap_or_else(|_| "https://huggingface.co".into())
    } else {
        s.endpoint.clone()
    };
    Ok(endpoint(&configured)?.into())
}
/// Resolve only metadata; never fetch model blobs while saving a configuration.
pub async fn resolve(mut s: Source) -> Result<Source, String> {
    for base in ["https://huggingface.co/", "https://hf-mirror.com/"] {
        if let Some(path) = s.repo.strip_prefix(base) {
            s.endpoint = base.trim_end_matches('/').into();
            s.repo = path.trim_end_matches('/').into();
            break;
        }
    }
    if s.revision.is_empty() {
        s.revision = "main".into();
    }
    let requested = s.revision.clone();
    s.revision = "a".repeat(40);
    s.validate()?;
    s.revision = requested;
    if s.revision.len() == 40 && s.revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(s);
    }
    if s.revision.len() > 128
        || !s
            .revision
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err("模型版本需填写分支、标签或 commit".into());
    }
    let base = source_endpoint(&s)?;
    let mut cmd = Command::new("curl");
    cmd.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-time",
        "30",
        &format!("{base}/api/models/{}/revision/{}", s.repo, s.revision),
    ]);
    let v: Value =
        serde_json::from_str(&execute(cmd, None, 35).await?).map_err(|e| e.to_string())?;
    s.revision = v["sha"].as_str().ok_or("模型来源未返回固定版本")?.into();
    s.validate()?;
    Ok(s)
}
pub fn readiness(path: &Path, source: Option<&Source>) -> Value {
    let local = if path.extension().is_some_and(|s| s == "gguf") {
        path.metadata().is_ok_and(|m| m.len() > 0)
    } else {
        path.join("config.json").is_file()
            && fs::read_dir(path).ok().is_some_and(|files| {
                files.filter_map(Result::ok).any(|f| {
                    f.path()
                        .extension()
                        .is_some_and(|s| s == "safetensors" || s == "bin")
                        && f.metadata().is_ok_and(|m| m.len() > 0)
                })
            })
    };
    let downloaded = source.is_none_or(|s| {
        let mut root = path.to_path_buf();
        for _ in Path::new(&s.file).components() {
            root.pop();
        }
        fs::read(root.join("phyxi-manifest.json"))
            .ok()
            .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
            .is_some_and(|v| {
                v["source"]["repo"] == s.repo
                    && v["source"]["revision"] == s.revision
                    && v["files"].as_array().is_some_and(|files| {
                        !files.is_empty()
                            && files.iter().all(|f| {
                                f["file"].as_str().is_some_and(|name| {
                                    safe_file(name)
                                        && root.join(name).metadata().is_ok_and(|m| {
                                            m.is_file() && Some(m.len()) == f["bytes"].as_u64()
                                        })
                                })
                            })
                    })
            })
    });
    json!({"ready":local&&downloaded,"path":path,"detail":if local&&downloaded{"本地文件已准备，可进入部署；模型兼容性以启动验收为准"}else{"权重尚未准备完整，请先下载或提供完整本地模型路径"}})
}
pub async fn download(
    data: &Path,
    c: &Config,
    id: &str,
    store: Arc<Store>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let s = c.source.as_ref().ok_or("请先填写模型来源")?;
    s.validate()?;
    let configured = source_endpoint(s)?;
    let base = configured.as_str();
    store.event(
        id,
        &format!("模型下载源：{base}；仍校验固定 revision 与 LFS SHA256"),
    )?;
    if Path::new(&c.model_path) != destination(data, s) {
        return Err("下载路径与来源不匹配".into());
    }
    let mut cmd = Command::new("curl");
    cmd.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        "60",
        &format!(
            "{base}/api/models/{}/revision/{}?blobs=true",
            s.repo, s.revision
        ),
    ]);
    let meta: Value =
        serde_json::from_str(&execute(cmd, None, 65).await?).map_err(|e| e.to_string())?;
    if meta["sha"].as_str() != Some(s.revision.as_str()) {
        return Err("仓库返回的 revision 不匹配".into());
    }
    let files: Vec<_> = meta["siblings"]
        .as_array()
        .ok_or("缺少模型文件清单")?
        .iter()
        .filter(|f| {
            f["rfilename"]
                .as_str()
                .is_some_and(|n| safe_file(n) && (s.file.is_empty() || s.file == n))
        })
        .collect();
    if files.is_empty() || files.len() > 512 {
        return Err("下载文件数量无效（最多512）".into());
    }
    let root = data
        .join("models")
        .join(s.repo.replace('/', "--"))
        .join(&s.revision);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut manifest = vec![];
    for (index, f) in files.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err("任务已取消".into());
        }
        let name = f["rfilename"].as_str().unwrap();
        let target = root.join(name);
        // Refuse symlinks anywhere below the trusted state directory.
        let mut p = target.clone();
        while p != data {
            if fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("模型目录含符号链接，拒绝下载".into());
            }
            if !p.pop() {
                return Err("模型目录越界".into());
            }
        }
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        store.update(
            id,
            "running",
            &format!("权重文件 {}/{}：{name}", index + 1, files.len()),
        )?;
        store.set_setting(&format!("progress:{id}"), &json!({"kind":"weights","source":base,"file":name,"file_index":index+1,"file_total":files.len(),"phase":"下载并校验当前文件"}))?;
        let expected = f["lfs"]["sha256"].as_str();
        if !target.exists() || expected.is_none() || hash(&target)?.as_str() != expected.unwrap() {
            let partial = target.with_extension("phyxi-part");
            if fs::symlink_metadata(&partial).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err("下载临时文件为符号链接".into());
            }
            let mut cmd = Command::new("curl");
            cmd.args([
                "--fail",
                "--location",
                "--show-error",
                "--progress-bar",
                "--retry",
                "2",
                "--max-time",
                "7200",
                "--output",
            ])
            .arg(&partial)
            .arg(format!(
                "{base}/{}/resolve/{}/{}",
                s.repo,
                s.revision,
                name.replace(' ', "%20")
            ));
            execute(cmd, Some((store.clone(), id.into())), 7210).await?;
            if let Some(expected) = expected {
                if hash(&partial)? != expected {
                    return Err(format!("文件 SHA256 不匹配：{name}"));
                }
            }
            fs::rename(&partial, &target).map_err(|e| e.to_string())?;
        }
        manifest.push(json!({"file":name,"sha256":hash(&target)?,"bytes":fs::metadata(&target).map_err(|e|e.to_string())?.len()}));
    }
    fs::write(
        root.join("phyxi-manifest.json"),
        json!({"source":s,"endpoint":base,"files":manifest}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    store.event(id, "权重准备完成，固定 revision 和逐文件 SHA256 已记录")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn download_endpoint_is_explicit_and_bounded() {
        assert_eq!(
            endpoint("https://hf-mirror.com/10"),
            Err("PHYXI_HF_ENDPOINT 仅支持 Hugging Face 官方站或 hf-mirror.com".into())
        );
        assert_eq!(
            endpoint("https://hf-mirror.com/"),
            Ok("https://hf-mirror.com")
        );
        assert!(endpoint("http://huggingface.co").is_err());
        assert!(endpoint("https://hf-mirror.com@127.0.0.1").is_err());
    }
    #[test]
    fn filters_executable_and_traversal_files() {
        for s in [
            "../model.gguf",
            "/model.gguf",
            "model.py",
            "a/../../x.json",
            "a;curl.json",
        ] {
            assert!(!safe_file(s));
        }
        assert!(safe_file("tokenizer.json"));
        assert!(safe_file("weights/model-0001.safetensors"));
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[tokio::test]
    async fn source_urls_are_normalized_without_downloading() {
        let s = resolve(Source {
            repo: "https://hf-mirror.com/Qwen/Qwen3-0.6B/".into(),
            revision: "a".repeat(40),
            endpoint: String::new(),
            file: String::new(),
        })
        .await
        .unwrap();
        assert_eq!(s.repo, "Qwen/Qwen3-0.6B");
        assert_eq!(s.endpoint, "https://hf-mirror.com");
        let mut invalid = s.clone();
        invalid.repo = "https://127.0.0.1/private".into();
        assert!(resolve(invalid).await.is_err());
        let mut invalid = s;
        invalid.endpoint = "https://huggingface.co@127.0.0.1".into();
        assert!(resolve(invalid).await.is_err());
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    #[test]
    fn missing_manifest_shard_is_not_ready() {
        let dir = std::env::temp_dir().join(format!("phyxi-readiness-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), "{}").unwrap();
        fs::write(dir.join("model.safetensors"), "x").unwrap();
        let source = Source {
            repo: "org/model".into(),
            revision: "a".repeat(40),
            endpoint: String::new(),
            file: String::new(),
        };
        fs::write(dir.join("phyxi-manifest.json"),json!({"source":source,"files":[{"file":"model.safetensors","bytes":1},{"file":"missing.safetensors","bytes":1}]}).to_string()).unwrap();
        assert_eq!(readiness(&dir, Some(&source))["ready"], false);
        fs::write(dir.join("missing.safetensors"), "x").unwrap();
        assert_eq!(readiness(&dir, Some(&source))["ready"], true);
        fs::remove_dir_all(dir).unwrap();
    }
}
