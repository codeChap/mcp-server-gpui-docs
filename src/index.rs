use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;

use crate::sources::{REMOTES, Remote, repo_dir};

const GOTCHAS_BODY: &str = include_str!("../gotchas.md");

#[derive(Clone, Debug)]
pub struct Doc {
    pub id: String,
    pub source: String,
    pub kind: &'static str,
    pub title: String,
    pub path: PathBuf,
    pub body: String,
    body_lc: String,
}

impl Doc {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        kind: &'static str,
        title: impl Into<String>,
        path: PathBuf,
        body: impl Into<String>,
    ) -> Self {
        let body = body.into();
        let body_lc = body.to_lowercase();
        Self {
            id: id.into(),
            source: source.into(),
            kind,
            title: title.into(),
            path,
            body,
            body_lc,
        }
    }

    pub fn snippet(&self, query: &str) -> &str {
        self.body
            .lines()
            .find(|l| {
                let l = l.to_lowercase();
                query
                    .split_whitespace()
                    .any(|t| t.len() > 2 && l.contains(&t.to_lowercase()))
            })
            .or_else(|| self.body.lines().find(|l| !l.trim().is_empty()))
            .unwrap_or("")
            .trim()
    }
}

#[derive(Clone)]
pub struct Corpus {
    pub docs: Vec<Doc>,
    pub missing: Vec<String>,
}

impl Corpus {
    pub fn load(cache: &Path) -> Self {
        let mut docs = Vec::new();
        let mut missing = Vec::new();
        docs.push(Doc::new(
            "gotchas",
            "gotchas",
            "doc",
            "GPUI gotchas (current APIs)",
            PathBuf::from("gotchas.md"),
            GOTCHAS_BODY,
        ));
        for remote in REMOTES {
            let Ok(root) = repo_dir(cache, remote.id) else {
                missing.push(remote.id.to_string());
                continue;
            };
            if !root.exists() {
                missing.push(remote.id.to_string());
                eprintln!(
                    "gpui mcp: source {} not cloned yet ({})",
                    remote.id,
                    root.display()
                );
                continue;
            }
            ingest_tree(&mut docs, remote, &root);
        }
        Self { docs, missing }
    }

    pub fn search(&self, query: &str, source: Option<&str>, limit: usize) -> Vec<(u32, &Doc)> {
        let tokens: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| t.len() > 1)
            .collect();
        if tokens.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(u32, &Doc)> = self
            .docs
            .iter()
            .filter(|d| source.is_none_or(|s| d.source.eq_ignore_ascii_case(s)))
            .filter_map(|d| score_doc(d, &tokens))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.truncate(limit);
        scored
    }

    pub fn get(&self, id: &str) -> Option<&Doc> {
        let q = normalize_id(id);
        if q.is_empty() {
            return None;
        }
        if let Some(d) = self.docs.iter().find(|d| d.id == q) {
            return Some(d);
        }
        if let Some(slash) = q.rfind('/') {
            let suffix = &q[slash..]; // includes leading /
            return self.docs.iter().find(|d| d.id.ends_with(suffix));
        }
        self.docs.iter().find(|d| {
            d.path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(&q))
        })
    }

    pub fn examples(&self) -> Vec<&Doc> {
        let mut v: Vec<&Doc> = self.docs.iter().filter(|d| d.kind == "example").collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }
}

pub fn normalize_id(id: &str) -> String {
    id.trim().replace('\\', "/")
}

pub fn rel_unix(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .replace('_', " ")
}

fn ingest_tree(docs: &mut Vec<Doc>, remote: &Remote, root: &Path) {
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let rel = path.strip_prefix(root).unwrap_or(path);
        if path_is_skipped(rel) {
            continue;
        }
        let rel_s = normalize_id(&rel.to_string_lossy());
        let Some(kind) = classify(remote.id, &rel_s, ext) else {
            continue;
        };
        let title = title_from_path(path);
        let id = format!("{}/{}", remote.id, rel_s);
        push_file(docs, &id, remote.id, kind, &title, path);
    }
}

pub fn path_is_skipped(rel: &Path) -> bool {
    rel.components().any(|c| match c {
        Component::Normal(n) => n == ".git" || n == "target",
        Component::ParentDir => true,
        _ => false,
    })
}

fn classify(source: &str, rel: &str, ext: &str) -> Option<&'static str> {
    match ext {
        "md" | "MD" => Some("doc"),
        "rs" if is_example_rs(source, rel) => Some("example"),
        _ => None,
    }
}

fn is_example_rs(source: &str, rel: &str) -> bool {
    match source {
        "tutorial" => true,
        "gpui-component" => rel.contains("examples/") || rel.contains("/story"),
        _ => rel.contains("examples/"),
    }
}

fn score_doc<'a>(d: &'a Doc, tokens: &[String]) -> Option<(u32, &'a Doc)> {
    let title = d.title.to_lowercase();
    let id = d.id.to_lowercase();
    let mut score = 0u32;
    for t in tokens {
        if title.contains(t) {
            score += 8;
        }
        if id.contains(t) {
            score += 4;
        }
        score += d.body_lc.matches(t.as_str()).count().min(12) as u32;
    }
    (score > 0).then_some((score, d))
}

fn push_file(
    docs: &mut Vec<Doc>,
    id: &str,
    source: &str,
    kind: &'static str,
    title: &str,
    path: &Path,
) {
    let Ok(body) = std::fs::read_to_string(path) else {
        return;
    };
    if body.trim().is_empty() {
        return;
    }
    docs.push(Doc::new(id, source, kind, title, path.to_path_buf(), body));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, source: &str, kind: &'static str, title: &str, body: &str) -> Doc {
        Doc::new(id, source, kind, title, PathBuf::from(id), body)
    }

    fn corpus(docs: Vec<Doc>) -> Corpus {
        Corpus {
            docs,
            missing: Vec::new(),
        }
    }

    #[test]
    fn search_ranks_title_hits() {
        let c = corpus(vec![
            doc("book/a.md", "book", "doc", "Entity notify", "other text"),
            doc(
                "book/b.md",
                "book",
                "doc",
                "unrelated",
                "entity appears in body",
            ),
        ]);
        let hits = c.search("entity", None, 8);
        assert_eq!(hits[0].1.id, "book/a.md");
        assert!(hits[0].0 > hits[1].0);
    }

    #[test]
    fn search_filters_source() {
        let c = corpus(vec![
            doc("book/a.md", "book", "doc", "div", "div"),
            doc("tutorial/a.md", "tutorial", "doc", "div", "div"),
        ]);
        let hits = c.search("div", Some("book"), 8);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.source, "book");
    }

    #[test]
    fn get_matches_path_suffix_and_filename_not_suffix_collision() {
        let c = corpus(vec![
            doc(
                "zed-gpui/crates/gpui/examples/hello_world.rs",
                "zed-gpui",
                "example",
                "hello world",
                "fn main() {}",
            ),
            doc("book/barfoo.md", "book", "doc", "barfoo", "x"),
            doc("book/foo.md", "book", "doc", "foo", "y"),
        ]);
        assert_eq!(
            c.get("zed-gpui/crates/gpui/examples/hello_world.rs")
                .unwrap()
                .id,
            "zed-gpui/crates/gpui/examples/hello_world.rs"
        );
        assert!(c.get("hello_world.rs").is_some());
        assert!(c.get("HELLO_WORLD.RS").is_some());
        assert_eq!(c.get("foo.md").unwrap().id, "book/foo.md");
        assert!(c.get("missing").is_none());
        assert_eq!(
            c.get("examples/hello_world.rs").unwrap().id,
            "zed-gpui/crates/gpui/examples/hello_world.rs"
        );
    }

    #[test]
    fn snippet_prefers_matching_line() {
        let d = doc(
            "x.md",
            "book",
            "doc",
            "t",
            "blankish\n\nEntity::new lives here\nfooter",
        );
        assert!(d.snippet("entity").contains("Entity::new"));
    }

    #[test]
    fn skip_git_and_target_on_any_separator() {
        assert!(path_is_skipped(Path::new(".git/config")));
        assert!(path_is_skipped(Path::new("foo/target/x.rs")));
        assert!(path_is_skipped(Path::new("a/../b.md")));
        assert!(!path_is_skipped(Path::new("src/lib.rs")));
    }

    #[test]
    fn gotchas_are_embedded() {
        assert!(GOTCHAS_BODY.contains("gpui_platform"));
    }

    #[test]
    fn title_from_path_replaces_underscores() {
        assert_eq!(title_from_path(Path::new("hello_world.rs")), "hello world");
    }

    #[test]
    fn rel_unix_strips_root() {
        let root = Path::new("/cache/src");
        let path = Path::new("/cache/src/book/foo.md");
        assert_eq!(rel_unix(path, root), "book/foo.md");
    }
}
