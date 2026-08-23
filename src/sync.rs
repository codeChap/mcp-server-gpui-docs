use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::sources::{repo_dir, REMOTES};

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
        match sync_one(cache, remote.id, remote.git, remote.sparse) {
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

fn sync_one(cache: &Path, id: &str, url: &str, sparse: &[&str]) -> Result<String> {
    let dir = repo_dir(cache, id)?;
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

fn clone_sparse(url: &str, dir: &Path, sparse: &[&str]) -> Result<String> {
    std::fs::create_dir_all(dir)?;
    git_run(Some(dir), ["init"])?;
    git_run(Some(dir), ["remote", "add", "origin", url])?;
    git_run(Some(dir), ["config", "core.sparseCheckout", "true"])?;
    std::fs::write(
        dir.join(".git/info/sparse-checkout"),
        sparse.join("\n") + "\n",
    )?;
    fetch_default_branch(dir, url)?;
    git_run(Some(dir), ["checkout", "FETCH_HEAD"])?;
    Ok("sparse-cloned".into())
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

fn git_run(dir: Option<&Path>, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<Output> {
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
