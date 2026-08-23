use std::path::{Path, PathBuf};

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

pub fn cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("GPUI_MCP_CACHE") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".cache").join("mcp-server-gpui")
}

pub fn repo_dir(cache: &Path, id: &str) -> PathBuf {
    cache.join("src").join(id)
}

pub fn bundled_gotchas() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gotchas.md")
}
