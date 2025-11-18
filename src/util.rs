pub fn sanitize_slug_python(input: &str) -> String {
    // Lowercase; keep ascii letters/digits; others -> '_'; collapse underscores; prefix '_' if starts with digit
    let mut out = String::new();
    for ch in input.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() { out.push(c); }
        else { out.push('_'); }
    }
    let mut collapsed = String::new();
    let mut prev_us = false;
    for ch in out.chars() {
        if ch == '_' { if !prev_us { collapsed.push(ch); } prev_us = true; }
        else { collapsed.push(ch); prev_us = false; }
    }
    let mut final_slug = collapsed.trim_matches('_').to_string();
    if final_slug.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        final_slug.insert(0, '_');
    }
    final_slug
}

use std::path::{Path, PathBuf, Component};
use anyhow::{Result, anyhow};

#[cfg(windows)]
fn is_windows_reserved_basename(name: &str) -> bool {
    // Windows reserved device names are case-insensitive and reserved even with extensions
    // e.g. "CON", "PRN", "AUX", "NUL", "COM1".."COM9", "LPT1".."LPT9"
    let upper = name.to_ascii_uppercase();
    matches!(upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" |
        "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" |
        "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

pub fn is_safe_path_segment(seg: &str) -> bool {
    if seg.is_empty() { return false; }
    if seg == "." || seg == ".." { return false; }
    if seg.contains('/') || seg.contains('\\') { return false; }
    // Colon is not allowed on Windows, but allowed on Unix
    #[cfg(windows)]
    if seg.contains(':') { return false; }
    // Windows does not allow names ending with space or dot
    #[cfg(windows)]
    if seg.ends_with(' ') || seg.ends_with('.') { return false; }
    // Windows reserved device names (check base part before first dot)
    #[cfg(windows)]
    {
        let base = seg.split('.').next().unwrap_or(seg);
        if is_windows_reserved_basename(base) { return false; }
    }
    true
}

pub fn is_safe_rel_path(path: &str) -> bool {
    if path.is_empty() { return false; }
    let s = path.replace('\\', "/");
    if s.starts_with('/') { return false; }
    // Colon is not allowed on Windows, but allowed on Unix
    #[cfg(windows)]
    if s.contains(':') { return false; }
    for seg in s.split('/') {
        if !is_safe_path_segment(seg) { return false; }
    }
    true
}

// Safely resolve a relative path under a fixed root, preventing symlink traversal.
// - Validates each segment with is_safe_path_segment
// - Disallows walking through existing symlink components
// - Ensures the resolved existing ancestor stays under the canonical root
#[allow(dead_code)]
pub fn safe_resolve_under(root: &Path, rel: &Path) -> Result<PathBuf> {
    let canon_root = root.canonicalize()?;
    safe_resolve_under_canon(&canon_root, rel)
}

// Same as safe_resolve_under, but requires caller to provide canonicalized root.
// This avoids repeated canonicalize on hot paths.
pub fn safe_resolve_under_canon(canon_root: &Path, rel: &Path) -> Result<PathBuf> {
    let mut cur: PathBuf = canon_root.to_path_buf();
    for comp in rel.components() {
        match comp {
            Component::Normal(os) => {
                let seg = os.to_string_lossy();
                if !is_safe_path_segment(&seg) { return Err(anyhow!(format!("Unsafe path segment: {}", seg))); }
                let next = cur.join(os);
                if next.exists() {
                    let ft = std::fs::symlink_metadata(&next)?.file_type();
                    if ft.is_symlink() {
                        return Err(anyhow!(format!("Refusing to traverse symlink component: {}", next.display())));
                    }
                }
                cur = next;
            }
            _ => { return Err(anyhow!("Unsupported/unsafe path component in relative path")); }
        }
    }
    // If the final path exists, ensure canonical path stays under the canonical root
    if cur.exists() {
        let canon_cur = cur.canonicalize()?;
        if !canon_cur.starts_with(canon_root) {
            return Err(anyhow!(format!("Resolved path escapes root: {}", cur.display())));
        }
    }
    Ok(cur)
}