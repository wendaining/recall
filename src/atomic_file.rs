use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

pub(crate) fn resolve_target(path: &Path) -> Result<std::path::PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)
            .with_context(|| format!("resolving file symlink {}", path.display())),
        Ok(_) => Ok(path.to_path_buf()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(err) => Err(err).with_context(|| format!("inspecting {}", path.display())),
    }
}

/// Replace a file atomically with bytes written to a temporary sibling.
/// Existing permissions are retained when the destination already exists.
pub(crate) fn replace(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("file has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());

    let mut temp_path = None;
    for attempt in 0..100 {
        let candidate = parent.join(format!(
            ".recall-{label}-{}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                file.write_all(bytes)?;
                file.sync_all()?;
                if let Some(permissions) = permissions.clone() {
                    fs::set_permissions(&candidate, permissions)?;
                }
                temp_path = Some(candidate);
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err).context("creating a temporary file"),
        }
    }
    let temp_path = temp_path.ok_or_else(|| anyhow!("could not create a temporary file"))?;
    replace_file(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).with_context(|| format!("replacing {}", to.display()))
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error()).context("replacing the file");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "recall-atomic-test-{}-{}-{name}",
            std::process::id(),
            ulid::Ulid::generate()
        ));
        fs::create_dir_all(&dir).unwrap();
        (dir.join("file"), dir)
    }

    #[test]
    fn replaces_existing_content() {
        let (path, dir) = temp_file("replace");
        fs::write(&path, b"before").unwrap();
        replace(&path, b"after", "test").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"after");
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let (path, dir) = temp_file("permissions");
        fs::write(&path, b"before").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        replace(&path, b"after", "test").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn resolves_a_symlink_without_replacing_it() {
        use std::os::unix::fs::symlink;

        let (link, dir) = temp_file("symlink");
        let target = dir.join("target");
        fs::write(&target, b"before").unwrap();
        symlink(&target, &link).unwrap();
        let resolved = resolve_target(&link).unwrap();
        replace(&resolved, b"after", "test").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), b"after");
        fs::remove_dir_all(dir).unwrap();
    }
}
