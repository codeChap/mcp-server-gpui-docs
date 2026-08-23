use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::sources::{repo_dir, REMOTES};

pub fn ensure_sources(cache: &Path) -> Result<String> {
    std::fs::create_dir_all(cache.join("src"))?;
    let mut log = Vec::new();
    for remote in REMOTES {
        match sync_one(cache, remote.id, remote.git, remote.sparse) {
            Ok(msg) => log.push(format!("[{}] {msg}", remote.id)),
            Err(e) => log.push(format!("[{}] ERROR: {e:#}", remote.id)),
        }
    }
    Ok(log.join("\n"))
}

fn sync_one(cache: &Path, id: &str, url: &str, sparse: &[&str]) -> Result<String> {
    let dir = repo_dir(cache, id);
    if dir.join(".git").exists() {
        return pull(&dir);
    }
    if sparse.is_empty() {
        clone_full(url, &dir)
    } else {
        clone_sparse(url, &dir, sparse)
    }
}

fn pull(dir: &Path) -> Result<String> {
    let out = git_cmd()
        .current_dir(dir)
        .args(["pull", "--ff-only"])
        .output()
        .context("git pull")?;
    if !out.status.success() {
        bail!(
            "git pull failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(format!(
        "pulled {}",
        String::from_utf8_lossy(&out.stdout).trim()
    ))
}

fn clone_full(url: &str, dir: &Path) -> Result<String> {
    let st = git_cmd()
        .args(["clone", "--depth", "1", url, dir.to_str().unwrap()])
        .status()
        .context("git clone")?;
    if !st.success() {
        bail!("git clone {url} failed");
    }
    Ok("cloned".into())
}

fn clone_sparse(url: &str, dir: &Path, sparse: &[&str]) -> Result<String> {
    std::fs::create_dir_all(dir)?;
    git_ok(dir, &["init"])?;
    git_ok(dir, &["remote", "add", "origin", url])?;
    git_ok(dir, &["config", "core.sparseCheckout", "true"])?;
    std::fs::write(dir.join(".git/info/sparse-checkout"), sparse.join("\n") + "\n")?;
    fetch_default_branch(dir, url)?;
    git_ok(dir, &["checkout", "FETCH_HEAD"])?;
    Ok("sparse-cloned".into())
}

fn fetch_default_branch(dir: &Path, url: &str) -> Result<()> {
    for branch in ["main", "master"] {
        let st = git_cmd()
            .current_dir(dir)
            .args(["fetch", "--depth", "1", "origin", branch])
            .status()
            .with_context(|| format!("git fetch origin {branch}"))?;
        if st.success() {
            return Ok(());
        }
    }
    bail!("git fetch origin main/master failed for {url}")
}

fn git_cmd() -> Command {
    Command::new("git")
}

fn git_ok(dir: &Path, args: &[&str]) -> Result<()> {
    let st = git_cmd()
        .current_dir(dir)
        .args(args)
        .status()
        .with_context(|| format!("git {}", args.join(" ")))?;
    if !st.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}
