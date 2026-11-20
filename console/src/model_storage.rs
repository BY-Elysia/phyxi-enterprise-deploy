//! Model discovery surfaces the server's data locations, not arbitrary system paths.
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub fn storage_roots(data: &Path) -> Vec<PathBuf> {
    let mut roots = if cfg!(target_os = "linux") {
        fs::read_to_string("/proc/self/mountinfo")
            .map(|mounts| linux_mount_roots(&mounts))
            .unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        fs::read_dir("/Volumes")
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect()
    } else {
        Vec::new()
    };
    roots.retain(|path| {
        fs::canonicalize(path).is_ok_and(|resolved| {
            resolved == *path && model_location(data, &resolved) && resolved.is_dir()
        })
    });
    roots.sort();
    roots.dedup();
    roots
}

pub fn model_location(data: &Path, path: &Path) -> bool {
    let mut components = path.components();
    if components.next() != Some(Component::RootDir) {
        return false;
    }
    let Some(Component::Normal(first)) = components.next() else {
        return false;
    };
    let protected = [
        "bin",
        "boot",
        "dev",
        "etc",
        "lib",
        "lib32",
        "lib64",
        "libx32",
        "proc",
        "root",
        "run",
        "sbin",
        "sys",
        "usr",
        "var",
        "System",
        "Library",
        "Applications",
    ];
    !protected.iter().any(|dir| *dir == first)
        && path
            .components()
            .filter_map(|part| match part {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .all(|name| !matches!(name, ".ssh" | ".gnupg" | ".aws" | ".kube"))
        && !path.starts_with("/private/etc")
        && !path.starts_with("/private/var/db")
        && !path.starts_with("/private/var/root")
        && !path.starts_with("/srv/phyxi")
        && !path.starts_with(data)
        && data
            .parent()
            .is_none_or(|parent| !path.starts_with(parent.join("console-auth")))
}

fn linux_mount_roots(mounts: &str) -> Vec<PathBuf> {
    mounts
        .lines()
        .filter_map(|line| {
            let (options, filesystem) = line.split_once(" - ")?;
            let mut fs_fields = filesystem.split_whitespace();
            let fs_type = fs_fields.next()?;
            let source = fs_fields.next()?;
            if !source.starts_with("/dev/")
                && !matches!(
                    fs_type,
                    "btrfs" | "zfs" | "nfs" | "nfs4" | "cifs" | "smb3" | "ceph" | "lustre"
                )
            {
                return None;
            }
            let mut fields = options.split_whitespace().skip(2);
            // Ignore non-root bind mounts. Their host source can lie outside the visible data tree.
            if fields.nth(1)? != "/" && !matches!(fs_type, "btrfs" | "zfs") {
                return None;
            }
            let mount = fields.next()?.replace("\\040", " ").replace("\\134", "\\");
            Some(PathBuf::from(mount))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_volumes_without_assuming_a_data_mount_name() {
        let mounts = "21 1 8:2 / /work\x5c040disk rw - ext4 /dev/sdb2 rw\n\
                      22 1 0:3 / /proc rw - proc proc rw\n\
                      23 1 8:1 /etc /exposed rw - ext4 /dev/sda1 rw\n\
                      24 1 0:5 / /shared rw - nfs4 files:/team rw";
        assert_eq!(
            linux_mount_roots(mounts),
            [PathBuf::from("/work disk"), PathBuf::from("/shared")]
        );
    }

    #[test]
    fn excludes_system_and_service_credentials() {
        let data = Path::new("/srv/phyxi/data");
        for path in [
            "/",
            "/etc",
            "/proc/1",
            "/var/lib/docker",
            "/srv/phyxi/console-auth",
            "/data/.ssh",
        ] {
            assert!(!model_location(data, Path::new(path)), "{path}");
        }
        assert!(model_location(data, Path::new("/extra disk/weights/model")));
        assert!(model_location(data, Path::new("/mnt/models/model.gguf")));
    }
}
