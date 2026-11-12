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
    pub file: String,
}
impl Source {
    pub fn validate(&self) -> Result<(), String> {
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
pub async fn download(
    data: &Path,
    c: &Config,
    id: &str,
    store: Arc<Store>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let s = c.source.as_ref().ok_or("请先填写模型来源")?;
    s.validate()?;
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
            "https://huggingface.co/api/models/{}/revision/{}?blobs=true",
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
                "https://huggingface.co/{}/resolve/{}/{}",
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
        json!({"source":s,"files":manifest}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    store.event(id, "权重准备完成，固定 revision 和逐文件 SHA256 已记录")
}
#[cfg(test)]
mod tests {
    use super::*;
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
