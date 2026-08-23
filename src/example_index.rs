use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use syn::visit::Visit;
use syn::{Expr, UseTree};
use walkdir::WalkDir;

use crate::api_index::INDEX_SCHEMA_VERSION;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExampleEntry {
    pub source: String,
    pub path: String,
    pub title: String,
    pub types_used: Vec<String>,
    pub methods_used: Vec<String>,
    pub loc: usize,
    pub has_main: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExampleIndex {
    pub schema_version: u32,
    pub built_at: String,
    pub entries: Vec<ExampleEntry>,
}

impl ExampleIndex {
    pub fn empty() -> Self {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            built_at: String::new(),
            entries: Vec::new(),
        }
    }
}

pub fn index_path(cache: &Path) -> PathBuf {
    cache.join("index").join("example_index.json")
}

pub fn save(index: &ExampleIndex, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(index)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<ExampleIndex> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub fn load_or_empty(cache: &Path) -> ExampleIndex {
    load(&index_path(cache)).unwrap_or_else(|_| ExampleIndex::empty())
}

pub fn build_example_index(roots: &[(String, PathBuf)]) -> ExampleIndex {
    let mut entries = Vec::new();
    for (source, root) in roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if rel.contains("/target/") || rel.contains("/.git/") {
                continue;
            }
            let is_example = rel.contains("examples/") || source == "tutorial";
            if !is_example {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(file) = syn::parse_file(&text) else {
                continue;
            };
            let mut v = Collector::default();
            v.visit_file(&file);
            let title = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .replace('_', " ");
            let mut types: Vec<_> = v.types.into_iter().collect();
            types.sort();
            let mut methods: Vec<_> = v.methods.into_iter().collect();
            methods.sort();
            entries.push(ExampleEntry {
                source: source.clone(),
                path: format!("{source}/{rel}"),
                title,
                types_used: types,
                methods_used: methods,
                loc: text.lines().count(),
                has_main: v.has_main,
            });
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    ExampleIndex {
        schema_version: INDEX_SCHEMA_VERSION,
        built_at: stamp(),
        entries,
    }
}

fn stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("unix:{}", d.as_secs()))
        .unwrap_or_else(|_| "unknown".into())
}

#[derive(Default)]
struct Collector {
    types: std::collections::HashSet<String>,
    methods: std::collections::HashSet<String>,
    has_main: bool,
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if i.sig.ident == "main" {
            self.has_main = true;
        }
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_expr_method_call(&mut self, i: &'ast syn::ExprMethodCall) {
        self.methods.insert(i.method.to_string());
        syn::visit::visit_expr_method_call(self, i);
    }

    fn visit_expr(&mut self, i: &'ast Expr) {
        match i {
            Expr::Path(p) => {
                if let Some(seg) = p.path.segments.last() {
                    let n = seg.ident.to_string();
                    if n.chars().next().is_some_and(|c| c.is_uppercase()) {
                        self.types.insert(n);
                    }
                }
            }
            Expr::Struct(s) => {
                if let Some(seg) = s.path.segments.last() {
                    self.types.insert(seg.ident.to_string());
                }
            }
            Expr::Call(c) => {
                if let Expr::Path(p) = &*c.func {
                    if let Some(seg) = p.path.segments.last() {
                        let n = seg.ident.to_string();
                        if n.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
                            self.methods.insert(n);
                        } else {
                            self.types.insert(n);
                        }
                    }
                }
            }
            _ => {}
        }
        syn::visit::visit_expr(self, i);
    }

    fn visit_item_use(&mut self, i: &'ast syn::ItemUse) {
        collect_use(&i.tree, &mut self.types);
        syn::visit::visit_item_use(self, i);
    }
}

fn collect_use(tree: &UseTree, types: &mut std::collections::HashSet<String>) {
    match tree {
        UseTree::Name(n) => {
            let s = n.ident.to_string();
            if s.chars().next().is_some_and(|c| c.is_uppercase()) {
                types.insert(s);
            }
        }
        UseTree::Rename(r) => {
            let s = r.rename.to_string();
            if s.chars().next().is_some_and(|c| c.is_uppercase()) {
                types.insert(s);
            }
        }
        UseTree::Path(p) => collect_use(&p.tree, types),
        UseTree::Group(g) => {
            for t in &g.items {
                collect_use(t, types);
            }
        }
        UseTree::Glob(_) => {}
    }
}

#[allow(dead_code)]
pub fn find_examples<'a>(idx: &'a ExampleIndex, symbol: &str, limit: usize) -> Vec<&'a ExampleEntry> {
    find_examples_multi(idx, &[symbol.to_string()], limit)
}

pub fn find_examples_multi<'a>(
    idx: &'a ExampleIndex,
    symbols: &[String],
    limit: usize,
) -> Vec<&'a ExampleEntry> {
    if symbols.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut scored: Vec<(i32, &ExampleEntry)> = idx
        .entries
        .iter()
        .filter_map(|e| {
            let mut score = 0i32;
            let mut all = true;
            for sym in symbols {
                let (s, present) = score_one(e, sym);
                score += s;
                if !present {
                    all = false;
                }
            }
            if symbols.len() > 1 && all {
                score += 200;
            }
            (score > 0).then_some((score, e))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    scored.truncate(limit);
    scored.into_iter().map(|(_, e)| e).collect()
}

fn score_one(e: &ExampleEntry, symbol: &str) -> (i32, bool) {
    let q = symbol;
    let in_types = e.types_used.iter().any(|t| t == q);
    let in_methods = e.methods_used.iter().any(|m| m == q);
    let in_path = e.path.contains(q) || e.title.contains(q);
    if !in_types && !in_methods && !in_path {
        return (0, false);
    }
    let mut score = 0i32;
    if in_methods || in_types {
        score += 100;
    } else if in_path {
        score += 40;
    }
    if e.has_main {
        score += 30;
    } else {
        score += 10;
    }
    if e.loc > 300 {
        score -= 25;
    }
    (score, true)
}

pub fn get_file<'a>(idx: &'a ExampleIndex, name: &str) -> Option<&'a ExampleEntry> {
    let q = name.to_lowercase();
    idx.entries.iter().find(|e| {
        e.path.to_lowercase().ends_with(&format!("/{q}"))
            || e.path.to_lowercase().ends_with(&format!("/{q}.rs"))
            || Path::new(&e.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(path: &str, types: &[&str], methods: &[&str], loc: usize, has_main: bool) -> ExampleEntry {
        ExampleEntry {
            source: "zed-gpui".into(),
            path: path.into(),
            title: path.into(),
            types_used: types.iter().map(|s| s.to_string()).collect(),
            methods_used: methods.iter().map(|s| s.to_string()).collect(),
            loc,
            has_main,
        }
    }

    #[test]
    fn ranks_method_use_above_path() {
        let idx = ExampleIndex {
            schema_version: 1,
            built_at: String::new(),
            entries: vec![
                e("zed-gpui/examples/hello_world.rs", &["App"], &["flex"], 80, true),
                e("zed-gpui/src/styled.rs", &[], &[], 2000, false),
            ],
        };
        let hits = find_examples(&idx, "flex", 5);
        assert_eq!(hits[0].path, "zed-gpui/examples/hello_world.rs");
    }
}
