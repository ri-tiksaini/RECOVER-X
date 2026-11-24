use crate::error::{RxError, RxResult};
use std::path::{Path, PathBuf};

/// Resolves a path (or its nearest existing ancestor) to a canonical PathBuf.
pub fn canonical_existing(path: &Path) -> RxResult<PathBuf> {
    let mut cur = path.to_path_buf();
    while !cur.exists() {
        cur = cur
            .parent()
            .ok_or_else(|| RxError::DestInvalid(format!("no existing parent for {}", path.display())))?
            .to_path_buf();
    }
    cur.canonicalize()
        .map_err(|e| RxError::DestInvalid(format!("canonicalize failed: {e}")))
}

#[cfg(unix)]
fn physical_disk_id_unix(path: &Path) -> RxResult<String> {
    use std::os::unix::fs::MetadataExt;
    let canon = canonical_existing(path)?;
    let meta = std::fs::metadata(&canon)?;
    let is_blk = (meta.mode() & libc::S_IFMT) == libc::S_IFBLK;
    let dev = if is_blk { meta.rdev() } else { meta.dev() };
    let maj = libc::major(dev);
    let min = libc::minor(dev);

    // Resolve sysfs block device -> walk up to parent physical disk if partition.
    let sys_link = PathBuf::from(format!("/sys/dev/block/{maj}:{min}"));
    if let Ok(resolved) = sys_link.canonicalize() {
        if resolved.join("partition").exists() {
            if let Some(parent) = resolved.parent() {
                if let Some(name) = parent.file_name() {
                    return Ok(name.to_string_lossy().into_owned());
                }
            }
        }
        if let Some(name) = resolved.file_name() {
            return Ok(name.to_string_lossy().into_owned());
        }
    }
    Ok(format!("dev:{maj}:{min}"))
}

/// Returns true if `source` and `dest` reside on the same PHYSICAL device [LAW-3].
pub fn same_physical_device(source: &Path, dest: &Path) -> RxResult<bool> {
    #[cfg(unix)]
    {
        let a = physical_disk_id_unix(source)?;
        let b = physical_disk_id_unix(dest)?;
        Ok(a == b)
    }
    #[cfg(not(unix))]
    {
        let a = canonical_existing(source)?;
        let b = canonical_existing(dest)?;
        Ok(a.components().next() == b.components().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn same_dir_detected_as_same_device() {
        let d = tempdir().unwrap();
        let a = d.path().join("src.img");
        std::fs::write(&a, b"test").unwrap();
        let b = d.path().join("export_out");
        assert!(same_physical_device(&a, &b).unwrap());
    }
}
