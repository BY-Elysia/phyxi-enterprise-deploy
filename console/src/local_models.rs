use crate::{catalog::Catalog, model_storage, store::Store};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
};

fn configured_model_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(value) = std::env::var_os("PHYXI_MODEL_ROOTS") {
        for path in std::env::split_paths(&value) {
            if path.is_absolute()
                && path.parent().is_some()
                && safe_path(&path)
                && !roots.contains(&path)
            {
                roots.push(path);
            }
        }
    }
    roots
}

fn safe_path(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|text| !text.is_empty() && !text.contains([',', '\n', '\r', '\0']))
        && !path.components().any(|part| part == Component::ParentDir)
}

fn existing_path(path: &Path) -> Result<PathBuf, String> {
    let resolved = fs::canonicalize(path).map_err(|_| "模型路径不存在或不可读取")?;
    if !safe_path(&resolved) {
        return Err("模型的实际路径包含不受支持的字符".into());
    }
    Ok(resolved)
}

fn existing_model_roots(data: &Path) -> impl Iterator<Item = PathBuf> {
    std::iter::once(data.join("models"))
        .chain(configured_model_roots())
        .filter_map(|root| fs::canonicalize(root).ok())
        .filter(|root| root.parent().is_some())
}

fn configured_path(data: &Path, path: &Path) -> bool {
    existing_model_roots(data).any(|root| path.starts_with(root))
}

fn authorized_path(data: &Path, path: &Path, store: &Store) -> Result<(PathBuf, bool), String> {
    let resolved = existing_path(path)?;
    if configured_path(data, &resolved) {
        return Ok((resolved, true));
    }
    if model_storage::model_location(data, &resolved) && {
        let saved = store.config()?;
        Catalog::load(store)?.model_path_registered(&resolved)
            || (saved.source.is_some() && Path::new(&saved.model_path) == resolved)
    } {
        return Ok((resolved, false));
    }
    Err("模型路径未授权，请在模型权重页面检测并添加".into())
}

pub fn status_path(data: &Path, path: &Path, store: &Store) -> Result<PathBuf, String> {
    authorized_path(data, path, store).map(|(path, _)| path)
}

pub fn launch_path(data: &Path, path: &Path, store: &Store) -> Result<PathBuf, String> {
    let (resolved, configured) = authorized_path(data, path, store)?;
    if !configured {
        validate_model_contents(data, &resolved, false)?;
    }
    Ok(resolved)
}

struct BrowseRoot {
    path: PathBuf,
    configured: bool,
}

fn selected_root(data: &Path, root: &str) -> Result<BrowseRoot, String> {
    let requested = Path::new(root);
    if !requested.is_absolute() || !safe_path(requested) {
        return Err("请输入有效的服务器绝对目录路径".into());
    }
    let path = existing_path(requested)?;
    let configured = configured_path(data, &path);
    if !path.is_dir() || path.parent().is_none() {
        return Err("该目录不可浏览".into());
    }
    if !configured && !model_storage::model_location(data, &path) {
        return Err("该位置不属于可浏览的模型存储目录".into());
    }
    Ok(BrowseRoot { path, configured })
}

impl BrowseRoot {
    fn resolve(&self, data: &Path, path: &Path) -> Result<PathBuf, String> {
        let path = beneath(&self.path, path)?;
        if !self.configured && !model_storage::model_location(data, &path) {
            return Err("该位置不属于可浏览的模型存储目录".into());
        }
        Ok(path)
    }
}

fn beneath(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if !safe_path(path) {
        return Err("路径无效，请勿使用 ..、逗号或换行".into());
    }
    let resolved = fs::canonicalize(root.join(path)).map_err(|_| "服务器路径不存在或不可读取")?;
    if !resolved.starts_with(root) || !safe_path(&resolved) {
        return Err("路径不在所选权重根目录内".into());
    }
    Ok(resolved)
}

#[derive(Default, Deserialize)]
pub struct Browse {
    pub root: Option<String>,
    pub path: Option<String>,
}

pub fn browse(data: &Path, query: Browse) -> Result<Value, String> {
    let filesystem_root = query.root.as_deref() == Some("/");
    let roots: Vec<_> = if filesystem_root {
        Vec::new()
    } else {
        let configured = configured_model_roots();
        configured
            .iter()
            .map(|path| serde_json::json!({"path":path,"available":path.is_dir(),"kind":"configured"}))
            .chain(
                model_storage::storage_roots(data)
                    .into_iter()
                    .filter(|path| !configured.contains(path))
                    .map(|path| serde_json::json!({"path":path,"available":true,"kind":"storage"})),
            )
            .collect()
    };
    let Some(root) = query.root else {
        return Ok(serde_json::json!({"roots":roots,"entries":[]}));
    };
    let (root, model_roots) = if filesystem_root {
        (
            BrowseRoot {
                path: PathBuf::from("/"),
                configured: false,
            },
            existing_model_roots(data).collect::<Vec<_>>(),
        )
    } else {
        (selected_root(data, &root)?, Vec::new())
    };
    let can_browse = |path: &Path| {
        path == Path::new("/")
            || model_storage::model_location(data, path)
            || model_roots
                .iter()
                .any(|model_root| path.starts_with(model_root) || model_root.starts_with(path))
    };
    let resolve = |path: &Path| {
        if !filesystem_root {
            return root.resolve(data, path);
        }
        let target = beneath(&root.path, path)?;
        if !can_browse(&root.path.join(path)) || !can_browse(&target) {
            return Err("该位置不属于可浏览的模型存储目录".into());
        }
        Ok(target)
    };
    let path = resolve(Path::new(query.path.as_deref().unwrap_or(".")))?;
    let mut entries = Vec::new();
    let mut scan_limit_reached = false;
    let directory = fs::read_dir(&path).map_err(|_| "无法读取这个目录")?;
    for (index, entry) in directory.enumerate() {
        if index == 4000 {
            scan_limit_reached = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        let Some(name) = entry
            .file_name()
            .into_string()
            .ok()
            .filter(|name| !name.starts_with('.'))
        else {
            continue;
        };
        let Ok(target) = resolve(&entry.path()) else {
            continue;
        };
        let directory = target.is_dir();
        if (directory && target != path) || (target.is_file() && name.ends_with(".gguf")) {
            entries.push((!directory, name, target));
        }
    }
    entries.sort();
    let truncated = scan_limit_reached || entries.len() > 200;
    let entries: Vec<_> = entries
        .into_iter()
        .take(200)
        .map(
            |(file, name, target)| serde_json::json!({"name":name,"path":target,"directory":!file}),
        )
        .collect();
    let parent = path
        .parent()
        .filter(|parent| parent.starts_with(&root.path));
    let selectable = path.join("config.json").is_file()
        && (!filesystem_root
            || model_storage::model_location(data, &path)
            || model_roots.iter().any(|root| path.starts_with(root)));
    Ok(
        serde_json::json!({"roots":roots,"path":path,"parent":parent,"entries":entries,"truncated":truncated,"selectable":selectable}),
    )
}

fn open_artifact(path: &Path) -> io::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular model file",
        ));
    }
    Ok(file)
}

fn local_json(path: &Path) -> Result<Value, String> {
    const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
    let mut input = open_artifact(path)
        .map_err(|_| "无法读取模型配置或权重分片索引")?
        .take(MAX_JSON_BYTES + 1);
    let mut raw = Vec::new();
    input
        .read_to_end(&mut raw)
        .map_err(|_| "无法读取模型配置或权重分片索引")?;
    if raw.len() as u64 > MAX_JSON_BYTES {
        return Err("模型配置或权重分片索引过大".into());
    }
    serde_json::from_slice(&raw).map_err(|_| "模型配置或权重分片索引不是有效 JSON".into())
}

pub struct CheckedModel {
    pub path: PathBuf,
    pub requires_registration: bool,
}

pub fn check_model(data: &Path, text: &str, root: Option<&str>) -> Result<CheckedModel, String> {
    let path = Path::new(text);
    if !safe_path(path) {
        return Err("请输入有效的服务器路径，请勿使用 ..、逗号或换行".into());
    }
    let path = if path.is_absolute() {
        existing_path(path)?
    } else {
        let root = selected_root(data, root.ok_or("输入相对路径前，请选择服务器权重根目录")?)?;
        root.resolve(data, path)?
    };
    let configured = configured_path(data, &path);
    if !configured
        && (!model_storage::model_location(data, &path)
            || path.components().count() < 3
            || model_storage::storage_roots(data).contains(&path))
    {
        return Err("请选择数据盘下的具体模型目录或 GGUF 文件".into());
    }
    validate_model_contents(data, &path, configured)?;
    Ok(CheckedModel {
        path,
        requires_registration: !configured,
    })
}

fn validate_model_contents(data: &Path, path: &Path, configured: bool) -> Result<(), String> {
    let artifact = |file: &Path| -> Result<PathBuf, String> {
        let target = existing_path(file)?;
        let within_scope = if configured {
            configured_path(data, &target)
        } else {
            target.starts_with(path)
        };
        if within_scope {
            Ok(target)
        } else {
            Err("权重文件的实际路径超出模型目录".into())
        }
    };
    if path.extension().is_some_and(|ext| ext == "gguf") && path.is_file() {
        let mut header = [0; 4];
        if open_artifact(path)
            .and_then(|mut file| file.read_exact(&mut header))
            .is_err()
            || &header != b"GGUF"
        {
            return Err("文件缺少有效的 GGUF 文件头".into());
        }
        return Ok(());
    }
    if !path.is_dir() {
        return Err("请选择包含模型权重的目录或一个 GGUF 文件".into());
    }
    let config =
        artifact(&path.join("config.json")).map_err(|_| "模型目录缺少可读取的 config.json")?;
    if !config.is_file() || !local_json(&config)?.is_object() {
        return Err("模型目录的 config.json 格式无效".into());
    }
    for name in [
        "model.safetensors.index.json",
        "pytorch_model.bin.index.json",
    ] {
        let index = path.join(name);
        if !index.exists() {
            continue;
        }
        let index = artifact(&index)?;
        if !index.is_file() {
            return Err("模型权重分片索引无效".into());
        }
        let value = local_json(&index)?;
        let map = value["weight_map"]
            .as_object()
            .filter(|map| !map.is_empty())
            .ok_or("模型权重分片索引缺少 weight_map")?;
        let mut checked = HashSet::new();
        for file in map.values() {
            let name = file.as_str().ok_or("模型权重分片名称无效")?;
            let shard = Path::new(name);
            if shard.is_absolute() || !safe_path(shard) {
                return Err("模型权重分片路径无效".into());
            }
            if checked.insert(name)
                && !artifact(&path.join(shard))
                    .ok()
                    .and_then(|p| p.metadata().ok())
                    .is_some_and(|meta| meta.is_file() && meta.len() > 0)
            {
                return Err(format!("缺少权重分片或文件为空：{name}"));
            }
        }
    }
    let has_weights = fs::read_dir(path).ok().is_some_and(|rows| {
        rows.filter_map(Result::ok).any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "safetensors" || ext == "bin")
                && artifact(&entry.path())
                    .ok()
                    .and_then(|file| file.metadata().ok())
                    .is_some_and(|meta| meta.is_file() && meta.len() > 0)
        })
    });
    if !has_weights {
        return Err("目录中没有可读取的 .safetensors 或 .bin 权重文件".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::symlink;

    #[test]
    fn external_directory_requires_an_exact_persisted_model_registration() {
        let temp =
            std::env::temp_dir().join(format!("phyxi-model-storage-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&temp).unwrap();
        let temp = fs::canonicalize(temp).unwrap();
        let data = temp.join("state");
        let storage = temp.join("storage");
        let model = storage.join("valid-model");
        let adjacent = storage.join("adjacent-model");
        fs::create_dir(&data).unwrap();
        for path in [&model, &adjacent] {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join("config.json"), b"{}").unwrap();
            fs::write(path.join("model.safetensors"), b"fixture").unwrap();
        }
        symlink(&data, storage.join("state-link")).unwrap();
        let listing = browse(
            &data,
            Browse {
                root: Some(storage.to_str().unwrap().into()),
                path: None,
            },
        )
        .unwrap();
        let entries: Vec<_> = listing["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| entry["name"].as_str())
            .collect();
        assert_eq!(entries, ["adjacent-model", "valid-model"]);

        let checked = check_model(&data, model.to_str().unwrap(), None).unwrap();
        assert!(checked.requires_registration);
        let store = Store::open(&data.join("state.db")).unwrap();
        let mut catalog = Catalog::default();
        catalog
            .put(
                "models",
                "verified",
                crate::catalog::Record {
                    name: "example".into(),
                    revision: 0,
                    data: json!({"model_path":model,"model_name":"example","source":null}),
                },
            )
            .unwrap();
        catalog.save(&store).unwrap();
        assert!(launch_path(&data, &model, &store).is_err());
        catalog.register_model_path("verified", checked.path.to_string_lossy().into_owned());
        catalog.save(&store).unwrap();
        assert_eq!(launch_path(&data, &model, &store).unwrap(), model);
        assert!(launch_path(&data, &adjacent, &store).is_err());
        fs::remove_file(model.join("model.safetensors")).unwrap();
        symlink(
            adjacent.join("model.safetensors"),
            model.join("model.safetensors"),
        )
        .unwrap();
        assert!(launch_path(&data, &model, &store).is_err());
        catalog.delete("models", "verified").unwrap();
        catalog.save(&store).unwrap();
        assert!(status_path(&data, &model, &store).is_err());
        drop(store);
        fs::remove_dir_all(&temp).unwrap();
    }
}
