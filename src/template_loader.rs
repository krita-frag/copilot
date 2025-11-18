use anyhow::{Result, Context};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub enum TemplateSource {
    // Local directory path
    Local(PathBuf),
    // Cloned git repository stored in a TempDir; path points to the clone root
    Git { path: PathBuf },
}

// Always copies the source template into a fresh temp directory for atomic processing.
pub fn load_template(source: &str) -> Result<TemplateSource> {
    // Support local paths and git URLs (http/https/.git)
    let is_url = source.starts_with("http://") || source.starts_with("https://") || source.ends_with(".git");
    if is_url {
        let temp = tempfile::tempdir().context("Failed to create temporary directory for git clone")?;
        let dst = temp.path().join("repo");
        fs::create_dir_all(&dst)?;
        let status = Command::new("git")
            .arg("clone")
            .arg("--depth").arg("1")
            .arg(source)
            .arg(&dst)
            .status()
            .context("Failed to run git clone. Is git installed and in PATH?")?;
        if !status.success() {
            anyhow::bail!("git clone failed for: {}", source);
        }
        return Ok(TemplateSource::Git { path: dst });
    }

    let p = PathBuf::from(source);
    if !p.exists() { anyhow::bail!("Template path does not exist: {}", p.display()); }
    Ok(TemplateSource::Local(p))
}

pub fn copy_to_temp_root(src_root: &Path) -> Result<(TempDir, PathBuf)> {
    let temp = tempfile::tempdir().context("Failed to create temporary directory")?;
    let dst = temp.path().join("template");
    fs::create_dir_all(&dst)?;
    // Copy recursively, excluding .git and .svn directories
    for entry in walkdir::WalkDir::new(src_root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel = path
            .strip_prefix(src_root)
            .with_context(|| format!("Path not under source root: {}", path.display()))?;
        if rel.components().any(|c| c.as_os_str() == ".git") { continue; }
        if rel.components().any(|c| c.as_os_str() == ".svn") { continue; }
        let target = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
            fs::copy(path, &target)?;
        }
    }
    Ok((temp, dst))
}

pub fn template_root(ts: &TemplateSource) -> &Path {
    match ts {
        TemplateSource::Local(p) => p.as_path(),
        TemplateSource::Git { path, .. } => path.as_path(),
    }
}