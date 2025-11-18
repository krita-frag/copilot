use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn svn_available() -> bool {
    Command::new("svn").arg("--version").output().is_ok()
}

pub fn has_gitmodules(repo: &Path) -> bool {
    repo.join(".gitmodules").exists()
}

pub fn git_submodule_sync(repo: &Path, recursive: bool) -> Result<()> {
    if !git_available() { anyhow::bail!("git is not available on PATH"); }
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).arg("submodule").arg("sync");
    if recursive { cmd.arg("--recursive"); }
    let st = cmd.status().with_context(|| "Failed to execute git submodule sync")?;
    if !st.success() { anyhow::bail!("git submodule sync failed"); }
    Ok(())
}

pub fn git_submodule_update_init(repo: &Path, recursive: bool, jobs: Option<usize>) -> Result<()> {
    if !git_available() { anyhow::bail!("git is not available on PATH"); }
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).arg("submodule").arg("update").arg("--init");
    if recursive { cmd.arg("--recursive"); }
    if let Some(n) = jobs { cmd.arg(format!("--jobs={}", n)); }
    let st = cmd.status().with_context(|| "Failed to execute git submodule update --init")?;
    if !st.success() { anyhow::bail!("git submodule update --init failed"); }
    Ok(())
}

pub fn has_svn_meta(repo: &Path) -> bool {
    repo.join(".svn").exists()
}

pub fn svn_update(repo: &Path) -> Result<()> {
    if !svn_available() { anyhow::bail!("svn is not available on PATH"); }
    let st = Command::new("svn").arg("update").arg(repo).status()
        .with_context(|| "Failed to execute svn update")?;
    if !st.success() { anyhow::bail!("svn update failed"); }
    Ok(())
}