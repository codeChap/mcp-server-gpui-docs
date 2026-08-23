use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

#[derive(Clone, Copy, Debug)]
pub struct Remote {
    pub id: &'static str,
    pub title: &'static str,
    pub git: &'static str,
    /// Sparse paths relative to repo root. Empty = full clone.
    pub sparse: &'static [&'static str],
}

pub const REMOTES: &[Remote] = &[
    Remote {
        id: "book",
        title: "GPUI Book (MatinAniss)",
        git: "https://github.com/MatinAniss/gpui-book.git",
        sparse: &[],
    },
    Remote {
        id: "tutorial",
        title: "gpui-tutorial (hedge-ops)",
        git: "https://github.com/hedge-ops/gpui-tutorial.git",
        sparse: &[],
    },
    Remote {
        id: "gpui-component",
        title: "gpui-component (Longbridge)",
        git: "https://github.com/longbridge/gpui-component.git",
        sparse: &[],
    },
    Remote {
        id: "zed-gpui",
        title: "Zed crates/gpui (README + examples)",
        git: "https://github.com/zed-industries/zed.git",
        sparse: &["crates/gpui"],
    },
    Remote {
        id: "awesome",
        title: "awesome-gpui",
        git: "https://github.com/zed-industries/awesome-gpui.git",
        sparse: &[],
    },
];

pub fn cache_dir() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("GPUI_MCP_CACHE") {
        let p = PathBuf::from(p);
        if p.as_os_str().is_empty() {
            bail!("GPUI_MCP_CACHE is empty");
        }
        return Ok(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return Ok(PathBuf::from(xdg).join("mcp-server-gpui-docs"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Ok(PathBuf::from(home).join(".cache").join("mcp-server-gpui-docs"));
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.is_empty() {
            return Ok(PathBuf::from(local).join("mcp-server-gpui-docs"));
        }
    }
    bail!("set GPUI_MCP_CACHE or HOME (refusing world-writable /tmp as a cache)");
}

pub fn repo_dir(cache: &Path, id: &str) -> Result<PathBuf> {
    if !is_safe_source_id(id) {
        bail!("invalid source id {id:?}");
    }
    Ok(cache.join("src").join(id))
}

pub fn is_safe_source_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.contains("..")
        && !id.contains('/')
        && !id.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_ids() {
        assert!(!is_safe_source_id("../x"));
        assert!(!is_safe_source_id("a/b"));
        assert!(!is_safe_source_id("-evil"));
        assert!(is_safe_source_id("book"));
    }

}
