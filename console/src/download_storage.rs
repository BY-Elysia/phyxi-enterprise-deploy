//! The chosen directory applies to newly registered downloads. Saved records retain their paths.
use crate::{artifacts::Source, model_storage, App, Error, Result};
use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn selected(a: &App) -> Result<PathBuf> {
    Ok(a.store
        .setting("download-directory")?
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("PHYXI_DOWNLOAD_DIR").map(PathBuf::from))
        .unwrap_or_else(|| a.runtime.data.join("models")))
}

pub fn target(root: &Path, s: &Source) -> PathBuf {
    let path = root.join(s.repo.replace('/', "--")).join(&s.revision);
    if s.file.is_empty() {
        path
    } else {
        path.join(&s.file)
    }
}

pub fn root_for(path: &Path, s: &Source) -> std::result::Result<PathBuf, String> {
    let suffix = target(Path::new(""), s);
    let root = path
        .ancestors()
        .nth(suffix.components().count())
        .ok_or("下载目录无效")?;
    if !root.is_absolute() || target(root, s) != path {
        return Err("下载路径与来源不匹配".into());
    }
    Ok(root.into())
}

pub fn check_destination(path: &Path) -> std::result::Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let uid = unsafe { libc::geteuid() };
    for dir in path.ancestors() {
        let m = match std::fs::symlink_metadata(dir) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        };
        if m.file_type().is_symlink()
            || (uid == 0
                && (m.uid() != 0
                    || m.mode() & 0o022 != 0
                        && !(m.is_dir() && dir != path && m.mode() & 0o1000 != 0)))
            || (m.is_file() && m.nlink() != 1)
        {
            return Err("下载目录须由执行器安全控制，请检查路径所有者、写权限及链接".into());
        }
    }
    Ok(())
}

pub fn free(path: &Path) -> Option<u64> {
    fs2::available_space(path.ancestors().find(|p| p.exists())?).ok()
}

pub async fn info(State(a): State<App>) -> Result<Json<Value>> {
    let root = selected(&a)?;
    Ok(Json(
        json!({"path":root,"available_bytes":free(&root),"locations":model_storage::storage_roots(&a.runtime.data)}),
    ))
}
#[derive(Deserialize)]
pub struct Selection {
    path: String,
}

pub async fn save(State(a): State<App>, Json(input): Json<Selection>) -> Result<Json<Value>> {
    let default = a.runtime.data.join("models");
    if input.path.trim().is_empty() || Path::new(&input.path) == default {
        a.store.set_setting("download-directory", &Value::Null)?;
    } else {
        let path = Path::new(&input.path);
        if !path.is_absolute() {
            return Err("请输入已有目录的绝对路径".to_string().into());
        }
        let root =
            std::fs::canonicalize(path).map_err(|_| Error::from("下载目录不存在".to_string()))?;
        if !root.is_dir()
            || !model_storage::model_location(&a.runtime.data, &root)
            || input.path.contains([',', '\n', '\r'])
        {
            return Err("请选择独立的数据目录".to_string().into());
        }
        check_destination(&root)?;
        // Check write access as the executor, before changing settings.
        let probe = root.join(format!(".phyxi-check-{}", uuid::Uuid::new_v4()));
        std::fs::File::create_new(&probe)
            .and_then(|_| std::fs::remove_file(probe))
            .map_err(|e| Error::from(e.to_string()))?;
        a.store.set_setting("download-directory", &json!(root))?;
    }
    info(State(a)).await
}
