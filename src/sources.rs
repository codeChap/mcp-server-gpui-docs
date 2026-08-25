use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// Zed commit the oracle and `zed-gpui` clone track by default.
/// Matches RustRivetGPUI's pinned `gpui` / `gpui_platform` rev.
/// Override with `GPUI_MCP_ZED_REV` (full SHA, or `HEAD`/`main` to follow the default branch).
pub const ZED_PINNED_REV: &str = "d9ad6aff67e47de43abb270d22de75dd950f1b48";

#[derive(Clone, Copy, Debug)]
pub struct Remote {
    pub id: &'static str,
    pub title: &'static str,
    pub git: &'static str,
    /// Sparse paths relative to repo root. Empty = full clone.
    pub sparse: &'static [&'static str],
    /// If set, `sync` checks out this commit instead of pulling the default branch.
    pub rev: Option<&'static str>,
}

pub const REMOTES: &[Remote] = &[
    Remote {
        id: "book",
        title: "GPUI Book (MatinAniss)",
        git: "https://github.com/MatinAniss/gpui-book.git",
        sparse: &[],
        rev: None,
    },
    Remote {
        id: "tutorial",
        title: "gpui-tutorial (hedge-ops)",
        git: "https://github.com/hedge-ops/gpui-tutorial.git",
        sparse: &[],
        rev: None,
    },
    Remote {
        id: "gpui-component",
        title: "gpui-component (Longbridge)",
        git: "https://github.com/longbridge/gpui-component.git",
        sparse: &[],
        rev: None,
    },
    Remote {
        id: "zed-gpui",
        title: "Zed crates/gpui (README + examples)",
        git: "https://github.com/zed-industries/zed.git",
        sparse: &["crates/gpui"],
        rev: Some(ZED_PINNED_REV),
    },
    Remote {
        id: "awesome",
        title: "awesome-gpui",
        git: "https://github.com/zed-industries/awesome-gpui.git",
        sparse: &[],
        rev: None,
    },
];

/// Pin for `zed-gpui`. `None` means follow the default branch.
pub fn resolve_zed_rev(env: Option<&str>) -> Option<String> {
    match env {
        Some(v) if v.trim().is_empty() => Some(ZED_PINNED_REV.to_string()),
        Some(v)
            if v.eq_ignore_ascii_case("head")
                || v.eq_ignore_ascii_case("main")
                || v.eq_ignore_ascii_case("master") =>
        {
            None
        }
        Some(v) => Some(v.trim().to_string()),
        None => Some(ZED_PINNED_REV.to_string()),
    }
}

pub fn zed_pin_rev() -> Option<String> {
    resolve_zed_rev(std::env::var("GPUI_MCP_ZED_REV").ok().as_deref())
}

/// Commit `sync` should check out for this remote, if any.
pub fn remote_checkout_rev(remote: &Remote) -> Option<String> {
    if remote.id == "zed-gpui" {
        zed_pin_rev()
    } else {
        remote.rev.map(str::to_string)
    }
}

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
            return Ok(PathBuf::from(home)
                .join(".cache")
                .join("mcp-server-gpui-docs"));
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

    #[test]
    fn zed_rev_defaults_to_pin() {
        assert_eq!(resolve_zed_rev(None).as_deref(), Some(ZED_PINNED_REV));
        assert_eq!(resolve_zed_rev(Some("")).as_deref(), Some(ZED_PINNED_REV));
        assert_eq!(
            resolve_zed_rev(Some("  abcdef1  ")).as_deref(),
            Some("abcdef1")
        );
        assert_eq!(resolve_zed_rev(Some("HEAD")), None);
        assert_eq!(resolve_zed_rev(Some("main")), None);
        assert_eq!(resolve_zed_rev(Some("master")), None);
    }

    #[test]
    fn zed_remote_declares_the_pin() {
        let zed = REMOTES.iter().find(|r| r.id == "zed-gpui").unwrap();
        assert_eq!(zed.rev, Some(ZED_PINNED_REV));
        assert_eq!(zed.sparse, &["crates/gpui"]);
    }
}
