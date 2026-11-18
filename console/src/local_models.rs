use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

fn model_roots(data: &Path) -> Vec<PathBuf> {
    let mut roots = vec![data.join("models"), PathBuf::from("/srv/phyxi/models")];
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

pub fn launch_path(data: &Path, path: &Path) -> Result<PathBuf, String> {
    resolve_path(data, path, crate::executor::enabled())
}

fn resolve_path(data: &Path, path: &Path, restrict_roots: bool) -> Result<PathBuf, String> {
    let resolved = fs::canonicalize(path).map_err(|_| "模型路径不存在或不可读取")?;
    if !safe_path(&resolved) {
        return Err("模型的实际路径包含不受支持的字符".into());
    }
    if restrict_roots
        && !model_roots(data)
            .iter()
            .filter_map(|root| fs::canonicalize(root).ok())
            .filter(|root| root.parent().is_some())
            .any(|root| resolved.starts_with(root))
    {
        return Err("模型路径不在执行器允许的权重目录内，请使用页面列出的根目录".into());
    }
    Ok(resolved)
}

fn selected_root(data: &Path, root: &str) -> Result<PathBuf, String> {
    let root = Path::new(root);
    if !model_roots(data).iter().any(|allowed| allowed == root) {
        return Err("请选择服务器允许的权重根目录".into());
    }
    fs::canonicalize(root)
        .ok()
        .filter(|root| root.is_dir() && root.parent().is_some())
        .ok_or_else(|| "该权重根目录不存在或不可读取".into())
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
    let roots: Vec<_> = model_roots(data)
        .into_iter()
        .map(|path| serde_json::json!({"path":path,"available":path.is_dir()}))
        .collect();
    let Some(root) = query.root else {
        return Ok(serde_json::json!({"roots":roots,"entries":[]}));
    };
    let root = selected_root(data, &root)?;
    let path = beneath(&root, Path::new(query.path.as_deref().unwrap_or(".")))?;
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
        let Ok(target) = beneath(&root, &entry.path()) else {
            continue;
        };
        let directory = target.is_dir();
        if directory || (target.is_file() && name.ends_with(".gguf")) {
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
    let parent = path.parent().filter(|parent| parent.starts_with(&root));
    Ok(
        serde_json::json!({"roots":roots,"path":path,"parent":parent,"entries":entries,"truncated":truncated}),
    )
}

fn local_json(path: &Path) -> Result<Value, String> {
    const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
    let mut input = fs::File::open(path)
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

pub fn check_model(data: &Path, text: &str) -> Result<PathBuf, String> {
    let path = Path::new(text);
    if !path.is_absolute() || !safe_path(path) {
        return Err("请输入服务器上以 / 开头的绝对路径，请勿使用 ..、逗号或换行".into());
    }
    let path = resolve_path(data, path, true)?;
    if path.extension().is_some_and(|ext| ext == "gguf") && path.is_file() {
        let mut header = [0; 4];
        if fs::File::open(&path)
            .and_then(|mut file| file.read_exact(&mut header))
            .is_err()
            || &header != b"GGUF"
        {
            return Err("文件缺少有效的 GGUF 文件头".into());
        }
        return Ok(path);
    }
    if !path.is_dir() {
        return Err("请选择包含模型权重的目录或一个 GGUF 文件".into());
    }
    let config = resolve_path(data, &path.join("config.json"), true)
        .map_err(|_| "模型目录缺少可读取的 config.json")?;
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
        let index = resolve_path(data, &index, true)?;
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
                && !resolve_path(data, &path.join(shard), true)
                    .ok()
                    .and_then(|p| p.metadata().ok())
                    .is_some_and(|meta| meta.is_file() && meta.len() > 0)
            {
                return Err(format!("缺少权重分片或文件为空：{name}"));
            }
        }
    }
    let has_weights = fs::read_dir(&path).ok().is_some_and(|rows| {
        rows.filter_map(Result::ok).any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "safetensors" || ext == "bin")
                && resolve_path(data, &entry.path(), true)
                    .ok()
                    .and_then(|file| file.metadata().ok())
                    .is_some_and(|meta| meta.is_file() && meta.len() > 0)
        })
    });
    if !has_weights {
        return Err("目录中没有可读取的 .safetensors 或 .bin 权重文件".into());
    }
    Ok(path)
}
