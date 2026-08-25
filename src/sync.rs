use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::sources::{REMOTES, remote_checkout_rev, repo_dir};

const GIT_TIMEOUT: Duration = Duration::from_secs(120);
const LOCK_TIMEOUT: Duration = Duration::from_secs(180);

struct DirLock(PathBuf);

impl Drop for DirLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn acquire_lock(cache: &Path) -> Result<DirLock> {
    std::fs::create_dir_all(cache)?;
    let p = cache.join(".sync.lock");
    let start = Instant::now();
    loop {
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(DirLock(p)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if start.elapsed() > LOCK_TIMEOUT {
                    bail!("timed out waiting for cache lock {}", p.display());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e).context("create cache lock dir"),
        }
    }
}

pub fn ensure_sources(cache: &Path) -> Result<String> {
    let _lock = acquire_lock(cache)?;
    std::fs::create_dir_all(cache.join("src"))?;
    let mut log = Vec::new();
    for remote in REMOTES {
        match sync_one(cache, remote) {
            Ok(msg) => log.push(format!("[{}] {msg}", remote.id)),
            Err(e) => log.push(format!("[{}] ERROR: {e:#}", remote.id)),
        }
    }
    match rebuild_oracle(cache) {
        Ok(msg) => log.push(msg),
        Err(e) => log.push(format!("[oracle] ERROR: {e:#}")),
    }
    Ok(log.join("\n"))
}

pub fn zed_root(cache: &Path) -> PathBuf {
    cache.join("src").join("zed-gpui")
}

pub fn gpui_crate(cache: &Path) -> PathBuf {
    zed_root(cache).join("crates").join("gpui")
}

pub fn git_rev(dir: &Path) -> Result<String> {
    let out = git_run(Some(dir), ["rev-parse", "HEAD"])?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn rebuild_oracle(cache: &Path) -> Result<String> {
    let gpui = gpui_crate(cache);
    if !gpui.join("src").is_dir() {
        return Ok("[oracle] skipped (clone zed-gpui first)".into());
    }
    let commit = git_rev(&zed_root(cache)).unwrap_or_default();
    let api = crate::api_index::build_api_index(&gpui, &commit)?;
    crate::api_index::save(&api, &crate::api_index::index_path(cache))?;
    let roots = [
        ("zed-gpui".into(), zed_root(cache)),
        ("tutorial".into(), cache.join("src").join("tutorial")),
        (
            "gpui-component".into(),
            cache.join("src").join("gpui-component"),
        ),
    ];
    let examples = crate::example_index::build_example_index(&roots);
    crate::example_index::save(&examples, &crate::example_index::index_path(cache))?;
    Ok(format!(
        "[oracle] {} symbols, {} examples, zed {}",
        api.symbols.len(),
        examples.entries.len(),
        &commit[..commit.len().min(12)]
    ))
}

pub fn same_git_rev(a: &str, b: &str) -> bool {
    let n = a.len().min(b.len());
    n >= 7 && a[..n].eq_ignore_ascii_case(&b[..n])
}

fn sync_one(cache: &Path, remote: &crate::sources::Remote) -> Result<String> {
    let dir = repo_dir(cache, remote.id)?;
    let pin = remote_checkout_rev(remote);
    if dir.join(".git").exists() {
        if let Some(rev) = pin.as_deref() {
            return checkout_pinned(&dir, rev);
        }
        return pull(&dir);
    }
    if remote.sparse.is_empty() {
        clone_full(remote.git, &dir)?;
        if let Some(rev) = pin.as_deref() {
            return checkout_pinned(&dir, rev).map(|m| format!("cloned; {m}"));
        }
        return Ok("cloned".into());
    }
    clone_sparse(remote.git, &dir, remote.sparse, pin.as_deref())
}

fn checkout_pinned(dir: &Path, rev: &str) -> Result<String> {
    if let Ok(current) = git_rev(dir) {
        if same_git_rev(&current, rev) {
            return Ok(format!("already at {current}"));
        }
    }
    fetch_rev(dir, rev)?;
    git_run(Some(dir), ["checkout", "--detach", "FETCH_HEAD"])?;
    let got = git_rev(dir).unwrap_or_default();
    Ok(format!("checked out {got}"))
}

fn fetch_rev(dir: &Path, rev: &str) -> Result<()> {
    match git_run(Some(dir), ["fetch", "--depth", "1", "origin", rev]) {
        Ok(_) => Ok(()),
        Err(shallow) => match git_run(Some(dir), ["fetch", "origin", rev]) {
            Ok(_) => Ok(()),
            Err(full) => bail!("git fetch {rev} failed: {shallow:#}; fallback: {full:#}"),
        },
    }
}

fn pull(dir: &Path) -> Result<String> {
    let out = git_run(Some(dir), ["pull", "--ff-only"])?;
    Ok(format!(
        "pulled {}",
        String::from_utf8_lossy(&out.stdout).trim()
    ))
}

fn clone_full(url: &str, dir: &Path) -> Result<String> {
    let mut cmd = git_cmd();
    cmd.args(["clone", "--depth", "1", "--"]);
    cmd.arg(url);
    cmd.arg(dir.as_os_str());
    let _ = run_cmd(cmd, "git clone")?;
    Ok("cloned".into())
}

fn clone_sparse(url: &str, dir: &Path, sparse: &[&str], rev: Option<&str>) -> Result<String> {
    std::fs::create_dir_all(dir)?;
    git_run(Some(dir), ["init"])?;
    git_run(Some(dir), ["remote", "add", "origin", url])?;
    git_run(Some(dir), ["config", "core.sparseCheckout", "true"])?;
    std::fs::write(
        dir.join(".git/info/sparse-checkout"),
        sparse.join("\n") + "\n",
    )?;
    if let Some(rev) = rev {
        fetch_rev(dir, rev)?;
        git_run(Some(dir), ["checkout", "--detach", "FETCH_HEAD"])?;
        Ok(format!("sparse-cloned at {rev}"))
    } else {
        fetch_default_branch(dir, url)?;
        git_run(Some(dir), ["checkout", "FETCH_HEAD"])?;
        Ok("sparse-cloned".into())
    }
}

fn fetch_default_branch(dir: &Path, url: &str) -> Result<()> {
    let mut last = String::new();
    for branch in ["main", "master"] {
        match git_run(Some(dir), ["fetch", "--depth", "1", "origin", branch]) {
            Ok(_) => return Ok(()),
            Err(e) => last = format!("{e:#}"),
        }
    }
    bail!("git fetch origin main/master failed for {url}: {last}")
}

fn git_cmd() -> Command {
    let mut c = Command::new("git");
    c.env("GIT_TERMINAL_PROMPT", "0");
    c.stdin(Stdio::null());
    c.stdout(Stdio::piped());
    c.stderr(Stdio::piped());
    c
}

fn git_run(
    dir: Option<&Path>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<Output> {
    let mut cmd = git_cmd();
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    cmd.args(args);
    run_cmd(cmd, "git")
}

fn run_cmd(mut cmd: Command, label: &str) -> Result<Output> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = tx.send(cmd.output());
    });
    let out = match rx.recv_timeout(GIT_TIMEOUT) {
        Ok(r) => r.with_context(|| format!("{label} spawn"))?,
        Err(_) => bail!("{label} timed out after {}s", GIT_TIMEOUT.as_secs()),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!(
            "{label} failed: {}",
            err.trim().chars().take(800).collect::<String>()
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_git_rev_accepts_prefix() {
        let full = "d9ad6aff67e47de43abb270d22de75dd950f1b48";
        assert!(same_git_rev(full, full));
        assert!(same_git_rev(full, "d9ad6af"));
        assert!(same_git_rev("d9ad6af", full));
        assert!(!same_git_rev(full, "6e2fae619c45"));
        assert!(!same_git_rev("abc", "abcdef1"));
        assert!(!same_git_rev("", full));
    }
}
