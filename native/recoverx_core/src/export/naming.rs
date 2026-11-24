use std::path::{Component, Path, PathBuf};

/// Sanitizes a logical disk path into a safe relative path under `dest_dir` (blocks `..` & root escapes).
pub fn safe_export_path(dest_dir: &Path, logical_path: &str, fallback_name: &str) -> PathBuf {
    let mut rel = PathBuf::new();
    for comp in Path::new(logical_path).components() {
        if let Component::Normal(os_part) = comp {
            let clean: String = os_part
                .to_string_lossy()
                .chars()
                .map(|c| match c {
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
                    _ => c,
                })
                .collect();
            if !clean.is_empty() && clean != "." && clean != ".." {
                rel.push(clean);
            }
        }
    }
    if rel.as_os_str().is_empty() {
        rel.push(fallback_name);
    }

    let candidate = dest_dir.join(&rel);
    if !candidate.exists() {
        return candidate;
    }
    let stem = candidate
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let ext = candidate
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = candidate.parent().unwrap_or(dest_dir);
    for i in 1..10_000u32 {
        let next = parent.join(format!("{stem} ({i}){ext}"));
        if !next.exists() {
            return next;
        }
    }
    parent.join(format!("{stem}_recovered{ext}"))
}
