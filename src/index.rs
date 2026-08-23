use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::sources::{bundled_gotchas, repo_dir, Remote, REMOTES};

#[derive(Clone, Debug)]
pub struct Doc {
    pub id: String,
    pub source: String,
    pub kind: &'static str,
    pub title: String,
    pub path: PathBuf,
    pub body: String,
}

impl Doc {
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
}

impl Corpus {
    pub fn load(cache: &Path) -> Self {
        let mut docs = Vec::new();
        push_file(
            &mut docs,
            "gotchas",
            "gotchas",
            "doc",
            "GPUI gotchas (current APIs)",
            &bundled_gotchas(),
        );
        for remote in REMOTES {
            let root = repo_dir(cache, remote.id);
            if !root.exists() {
                continue;
            }
            ingest_tree(&mut docs, remote, &root);
        }
        Self { docs }
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
        let q = id.trim();
        self.docs.iter().find(|d| d.id == q).or_else(|| {
            self.docs.iter().find(|d| {
                d.id.ends_with(q)
                    || d.path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.eq_ignore_ascii_case(q))
            })
        })
    }

    pub fn examples(&self) -> Vec<&Doc> {
        let mut v: Vec<&Doc> = self.docs.iter().filter(|d| d.kind == "example").collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }
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
        let rel_s = rel.to_string_lossy();
        if rel_s.contains("/target/") || rel_s.contains("/.git/") {
            continue;
        }
        let Some(kind) = classify(remote.id, &rel_s, ext) else {
            continue;
        };
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .replace('_', " ");
        let id = format!("{}/{}", remote.id, rel_s.replace('\\', "/"));
        push_file(docs, &id, remote.id, kind, &title, path);
    }
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
    let hay = format!("{} {} {}", title, id, d.body.to_lowercase());
    let mut score = 0u32;
    for t in tokens {
        if title.contains(t) {
            score += 8;
        }
        if id.contains(t) {
            score += 4;
        }
        score += hay.matches(t.as_str()).count().min(12) as u32;
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
    docs.push(Doc {
        id: id.to_string(),
        source: source.to_string(),
        kind,
        title: title.to_string(),
        path: path.to_path_buf(),
        body,
    });
}
