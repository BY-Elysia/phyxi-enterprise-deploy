//! Offline bounded, authenticated backups. Never include model weights or live sessions.
use argon2::Argon2;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use fs2::FileExt;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;
const MAGIC: &[u8; 8] = b"PHYXI001";
const MAX: usize = 64 * 1024 * 1024;
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    let result = f.write_all(bytes).and_then(|_| f.sync_all());
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result.map_err(|e| e.to_string())
}
fn directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}
fn permitted(name: &str) -> bool {
    if [
        "manifest.json",
        "state.db",
        "admin.hash",
        "owner",
        "token",
        "access/access.db",
        "service-secrets/access.token",
    ]
    .contains(&name)
    {
        return true;
    }
    let parts: Vec<_> = name.split('/').collect();
    if parts.len() == 2 && parts[0] == "gateway" {
        return [
            "server.pem",
            "server.key",
            "ca.pem",
            "admin.pem",
            "admin.key",
            "admin-ca.pem",
            "nginx.conf",
            "management.conf",
            "certificate-config.json",
            "active-container.json",
            "client.token",
        ]
        .contains(&parts[1]);
    }
    parts.len() == 3
        && parts[0] == "certificates"
        && uuid::Uuid::parse_str(parts[1]).is_ok()
        && ["server.pem", "server.key", "ca.pem"].contains(&parts[2])
}
fn append(
    builder: &mut tar::Builder<Vec<u8>>,
    name: &str,
    bytes: &[u8],
    total: &mut usize,
) -> Result<(), String> {
    *total = total.checked_add(bytes.len() + 1024).ok_or("备份过大")?;
    if *total > MAX {
        return Err("配置备份超过 64 MiB 上限，请先归档旧记录；模型权重不应进入备份".into());
    }
    let mut h = tar::Header::new_gnu();
    h.set_size(bytes.len() as u64);
    h.set_mode(0o600);
    h.set_cksum();
    builder
        .append_data(&mut h, name, bytes)
        .map_err(|e| e.to_string())
}
fn derive(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    if !(12..=256).contains(&password.len()) {
        return Err("备份口令需要 12–256 字节".into());
    }
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}
pub fn create(root: &Path, out: &Path, password: &str) -> Result<(), String> {
    for base in ["access", "service-secrets"] {
        if let Ok(meta) = fs::symlink_metadata(root.join(base)) {
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err("备份目录不能是符号链接或普通文件".into());
            }
        }
    }
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("access/access.lock"));
    let _access_lock = if root.join("access").exists() {
        let f = lock.map_err(|e| e.to_string())?;
        f.try_lock_exclusive()
            .map_err(|_| "请先停止鉴权服务，再做一致性备份")?;
        Some(f)
    } else {
        None
    };
    if out.exists() {
        return Err("备份输出已存在，拒绝覆盖".into());
    }
    let temp = Temp(root.join(format!(".backup-{}", uuid::Uuid::new_v4())));
    directory(&temp.0)?;
    let mut builder = tar::Builder::new(Vec::new());
    let mut total = 0;
    append(&mut builder,"manifest.json",serde_json::json!({"version":1,"source_path":root,"models_included":false,"sessions_included":false}).to_string().as_bytes(),&mut total)?;
    let mut paths = vec![
        "state.db".to_string(),
        "admin.hash".into(),
        "owner".into(),
        "token".into(),
        "access/access.db".into(),
        "service-secrets/access.token".into(),
    ];
    for base in ["gateway", "certificates"] {
        let dir = root.join(base);
        if !dir.exists() {
            continue;
        }
        if fs::symlink_metadata(&dir)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("备份目录不能是符号链接".into());
        }
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = format!("{base}/{}", entry.file_name().to_string_lossy());
            if base == "certificates" && entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                for f in fs::read_dir(entry.path()).map_err(|e| e.to_string())? {
                    let f = f.map_err(|e| e.to_string())?;
                    paths.push(format!("{name}/{}", f.file_name().to_string_lossy()));
                }
            } else {
                paths.push(name);
            }
        }
    }
    paths.sort();
    paths.dedup();
    for name in paths {
        if !permitted(&name) {
            continue;
        }
        let path = root.join(&name);
        if !path.exists() {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !meta.file_type().is_file() {
            return Err("备份只允许普通文件，禁止符号链接".into());
        }
        if meta.len() > MAX as u64 {
            return Err("单个配置文件超过备份上限".into());
        }
        let bytes = if name.ends_with(".db") {
            let snap = temp.0.join(name.replace('/', "_"));
            let db = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|e| e.to_string())?;
            db.backup(rusqlite::DatabaseName::Main, &snap, None)
                .map_err(|e| e.to_string())?;
            fs::read(snap).map_err(|e| e.to_string())?
        } else {
            fs::read(path).map_err(|e| e.to_string())?
        };
        append(&mut builder, &name, &bytes, &mut total)?;
    }
    let plain = Zeroizing::new(builder.into_inner().map_err(|e| e.to_string())?);
    if plain.len() > MAX {
        return Err("备份超过上限".into());
    }
    let salt = *uuid::Uuid::new_v4().as_bytes();
    let mut nonce = [0u8; 24];
    nonce[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    nonce[16..].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..8]);
    let key = derive(password, &salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&*key).map_err(|_| "加密初始化失败")?;
    let mut header = Vec::from(&MAGIC[..]);
    header.extend(salt);
    header.extend(nonce);
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: &header,
            },
        )
        .map_err(|_| "备份加密失败")?;
    header.extend(encrypted);
    private(out, &header)
}
pub fn restore(input: &Path, target: &Path, password: &str) -> Result<(), String> {
    if target
        .to_str()
        .is_none_or(|s| s.contains(['"', '\n', '\r', '$', '\\']))
    {
        return Err("恢复路径含不允许的字符".into());
    }
    if target.exists() {
        return Err("恢复目标必须不存在；先停止服务并移走旧目录，不自动覆盖数据".into());
    }
    if fs::metadata(input).map_err(|e| e.to_string())?.len() > (MAX + 64) as u64 {
        return Err("备份超过上限".into());
    }
    let data = fs::read(input).map_err(|e| e.to_string())?;
    if data.len() < 64 || &data[..8] != MAGIC {
        return Err("备份格式不支持".into());
    }
    let key = derive(password, &data[8..24])?;
    let cipher = XChaCha20Poly1305::new_from_slice(&*key).map_err(|_| "解密初始化失败")?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&data[24..48]),
                Payload {
                    msg: &data[48..],
                    aad: &data[..48],
                },
            )
            .map_err(|_| "口令错误或备份已被篡改；未恢复任何文件")?,
    );
    let parent = target.parent().ok_or("恢复目标需绝对路径")?;
    let temp = Temp(parent.join(format!(".phyxi-restore-{}", uuid::Uuid::new_v4())));
    directory(&temp.0)?;
    let mut archive = tar::Archive::new(&plain[..]);
    let mut manifest = false;
    let mut count = 0;
    let mut seen = std::collections::HashSet::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        count += 1;
        if count > 5000 {
            return Err("备份文件数量超限".into());
        }
        let mut e = entry.map_err(|e| e.to_string())?;
        let name = e
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        if !e.header().entry_type().is_file() || !permitted(&name) || !seen.insert(name.clone()) {
            return Err("备份包含不允许的文件类型或路径".into());
        }
        let mut bytes = Zeroizing::new(Vec::new());
        e.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if name == "manifest.json" {
            let m: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if m["version"] != 1 || m["source_path"].as_str() != target.to_str() {
                return Err(
                    "备份版本或恢复路径不匹配；请恢复到原绝对路径，避免证书和权重路径失效".into(),
                );
            }
            manifest = true;
            continue;
        }
        let path = temp.0.join(name);
        directory(path.parent().unwrap())?;
        private(&path, &bytes)?;
    }
    if !manifest || !temp.0.join("state.db").exists() {
        return Err("备份缺少清单或配置数据库".into());
    }
    // Restore configuration, never an assertion that a model or gateway is still running.
    let gateway = temp.0.join("gateway");
    if gateway.exists() {
        let active = gateway.join("active-container.json");
        if active.exists() {
            fs::remove_file(active).map_err(|e| e.to_string())?;
        }
        let config = gateway.join("nginx.conf");
        if config.exists() {
            fs::remove_file(&config).map_err(|e| e.to_string())?;
        }
        let original = target.join("gateway");
        let management = if gateway.join("management.conf").exists() {
            format!("include \"{}/management.conf\";", original.display())
        } else {
            String::new()
        };
        let listen = if management.is_empty() {
            "127.0.0.1"
        } else {
            "0.0.0.0"
        };
        let bootstrap = format!("pid \"{}/nginx.pid\";\nerror_log /dev/stderr warn;\nevents {{ worker_connections 128; }}\nhttp {{ access_log off; {management} server {{ listen {listen}:18765; location = /health {{ return 200 'ready'; }} }} }}\n", original.display());
        private(&config, bootstrap.as_bytes())?;
    }
    let db = rusqlite::Connection::open(temp.0.join("state.db")).map_err(|e| e.to_string())?;
    let settings: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if settings {
        db.execute("DELETE FROM settings WHERE key='active_gateway'", [])
            .map_err(|e| e.to_string())?;
    }
    drop(db);
    fs::rename(&temp.0, target).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_paths_outside_backup_contract() {
        assert!(permitted("state.db"));
        assert!(permitted("gateway/server.key"));
        for bad in [
            "../state.db",
            "/state.db",
            "models/model.bin",
            "gateway/../admin.hash",
            "gateway/body/prompt",
            "certificates/../../token",
        ] {
            assert!(!permitted(bad));
        }
    }
}
