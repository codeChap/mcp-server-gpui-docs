use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

mod parse;

pub use parse::build_api_index;

pub const INDEX_SCHEMA_VERSION: u32 = 1;
const MACRO_JSON: &str = include_str!("../../data/macro_generated.json");

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind {
    Struct,
    Enum,
    Trait,
    TraitMethod,
    Method,
    Fn,
    TypeAlias,
    Const,
    Macro,
}

impl SymbolKind {
    fn parse_filter(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "struct" => Some(Self::Struct),
            "enum" => Some(Self::Enum),
            "trait" => Some(Self::Trait),
            "traitmethod" | "trait_method" => Some(Self::TraitMethod),
            "method" => Some(Self::Method),
            "fn" | "function" => Some(Self::Fn),
            "typealias" | "type_alias" | "type" => Some(Self::TypeAlias),
            "const" => Some(Self::Const),
            "macro" => Some(Self::Macro),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiSymbol {
    pub kind: SymbolKind,
    pub name: String,
    pub owner: Option<String>,
    pub via_trait: Option<String>,
    pub signature: String,
    pub doc: String,
    pub file: String,
    pub line: usize,
    pub deprecated: bool,
    pub cfg: Vec<String>,
    pub generated: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraitImplRecord {
    pub trait_name: String,
    pub type_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiIndex {
    pub schema_version: u32,
    pub zed_commit: String,
    pub gpui_version: String,
    pub built_at: String,
    pub skipped_files: Vec<String>,
    pub trait_impls: Vec<TraitImplRecord>,
    pub symbols: Vec<ApiSymbol>,
}

impl ApiIndex {
    pub fn empty() -> Self {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            zed_commit: String::new(),
            gpui_version: "unknown".into(),
            built_at: String::new(),
            skipped_files: Vec::new(),
            trait_impls: Vec::new(),
            symbols: Vec::new(),
        }
    }
}

pub fn index_path(cache: &Path) -> PathBuf {
    cache.join("index").join("api_index.json")
}

pub fn save(index: &ApiIndex, path: &Path) -> Result<()> {
    crate::persist::save_json(path, index)
}

pub fn load(path: &Path) -> Result<ApiIndex> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let idx: ApiIndex = serde_json::from_slice(&bytes)?;
    if idx.schema_version != INDEX_SCHEMA_VERSION {
        anyhow::bail!(
            "api index schema {} != {}",
            idx.schema_version,
            INDEX_SCHEMA_VERSION
        );
    }
    Ok(idx)
}

pub fn load_or_empty(cache: &Path) -> ApiIndex {
    match load(&index_path(cache)) {
        Ok(i) => i,
        Err(_) => ApiIndex::empty(),
    }
}

pub fn lookup<'a>(
    index: &'a ApiIndex,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Vec<&'a ApiSymbol> {
    if query.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let kind_f = match kind {
        Some(k) => match SymbolKind::parse_filter(k) {
            Some(k) => Some(k),
            None => return Vec::new(),
        },
        None => None,
    };
    let q = query.to_lowercase();
    let mut scored: Vec<(u8, usize, &ApiSymbol)> = index
        .symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| kind_f.is_none_or(|k| s.kind == k))
        .filter_map(|(i, s)| lookup_tier(s, &q).map(|tier| (tier, i, s)))
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.truncate(limit);
    scored.into_iter().map(|(_, _, s)| s).collect()
}

fn lookup_tier(s: &ApiSymbol, q: &str) -> Option<u8> {
    let n = s.name.to_lowercase();
    if n == q {
        Some(1)
    } else if n.starts_with(q) {
        Some(2)
    } else if n.contains(q) {
        Some(3)
    } else if s.doc.to_lowercase().contains(q) {
        Some(4)
    } else {
        None
    }
}

pub fn methods_for_type(index: &ApiIndex, type_name: &str, trait_filter: Option<&str>) -> String {
    let inherent: Vec<_> = index
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method && s.owner.as_deref() == Some(type_name))
        .collect();
    let trait_names = traits_implemented(index, type_name);
    if inherent.is_empty() && trait_names.is_empty() && !type_known(index, type_name) {
        return format!(
            "No type '{type_name}' in the GPUI index. Try gpui_symbol / gpui_search first."
        );
    }
    let filter_l = trait_filter.map(|t| t.to_lowercase());
    let trait_sections: Vec<_> = trait_names
        .into_iter()
        .filter(|tn| filter_l.as_ref().is_none_or(|f| tn.to_lowercase() == *f))
        .map(|tn| {
            let methods = trait_methods(index, &tn);
            (tn, methods)
        })
        .collect();
    let many = inherent.len() + trait_sections.iter().map(|(_, m)| m.len()).sum::<usize>();
    let collapse = many > 200 && filter_l.is_none();

    let mut out = String::new();
    if filter_l.is_none() {
        out.push_str("## Inherent\n");
        if inherent.is_empty() {
            out.push_str("(none)\n");
        } else {
            for s in &inherent {
                push_method_line(&mut out, s);
            }
        }
    }
    for (tn, methods) in trait_sections {
        out.push_str(&format!("\n## via {tn}\n"));
        if collapse {
            let names: Vec<_> = methods.iter().map(|m| m.name.as_str()).collect();
            out.push_str(&names.join(", "));
            out.push('\n');
        } else {
            for s in methods {
                push_method_line(&mut out, s);
            }
        }
    }
    if collapse {
        out.push_str("\nPass trait_filter to see full signatures for one trait.\n");
    }
    if out.is_empty() {
        format!("No methods for '{type_name}' with that trait filter.")
    } else {
        out
    }
}

fn traits_implemented(index: &ApiIndex, type_name: &str) -> Vec<String> {
    let mut names: Vec<String> = index
        .trait_impls
        .iter()
        .filter(|r| r.type_name == type_name)
        .map(|r| r.trait_name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn trait_methods<'a>(index: &'a ApiIndex, trait_name: &str) -> Vec<&'a ApiSymbol> {
    index
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::TraitMethod && s.owner.as_deref() == Some(trait_name))
        .collect()
}

fn type_known(index: &ApiIndex, type_name: &str) -> bool {
    index
        .symbols
        .iter()
        .any(|s| s.owner.as_deref() == Some(type_name) || s.name == type_name)
}

fn push_method_line(out: &mut String, s: &ApiSymbol) {
    out.push_str(&format!("- `{}` — {}\n", s.signature, one_line(&s.doc)));
}

fn merge_macro_supplement(symbols: &mut Vec<ApiSymbol>) {
    let extra: Vec<ApiSymbol> = match serde_json::from_str(MACRO_JSON) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("gpui mcp: macro_generated.json: {e}");
            return;
        }
    };
    for s in extra {
        let exists = symbols
            .iter()
            .any(|e| e.name == s.name && e.owner.as_deref() == Some("Styled"));
        if !exists {
            symbols.push(s);
        }
    }
}

fn one_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(
        kind: SymbolKind,
        name: &str,
        owner: Option<&str>,
        signature: &str,
        doc: &str,
    ) -> ApiSymbol {
        ApiSymbol {
            kind,
            name: name.into(),
            owner: owner.map(str::to_string),
            via_trait: None,
            signature: signature.into(),
            doc: doc.into(),
            file: "t.rs".into(),
            line: 1,
            deprecated: false,
            cfg: vec![],
            generated: false,
        }
    }

    #[test]
    fn lookup_tiers() {
        let mut idx = ApiIndex::empty();
        idx.symbols = vec![
            sym(
                SymbolKind::Trait,
                "Render",
                None,
                "pub trait Render",
                "view",
            ),
            sym(
                SymbolKind::Fn,
                "uniform_list",
                None,
                "fn uniform_list",
                "list",
            ),
        ];
        let hits = lookup(&idx, "Render", None, 8);
        assert_eq!(hits[0].name, "Render");
        assert!(lookup(&idx, "uniform", None, 8)[0].name.contains("uniform"));
        assert!(lookup(&idx, "x", Some("nope"), 8).is_empty());
    }

    #[test]
    fn merge_skips_existing_styled() {
        let mut v = vec![sym(
            SymbolKind::TraitMethod,
            "flex",
            Some("Styled"),
            "fn flex",
            "",
        )];
        merge_macro_supplement(&mut v);
        assert_eq!(
            v.iter()
                .filter(|s| s.name == "flex" && s.owner.as_deref() == Some("Styled"))
                .count(),
            1
        );
        assert!(v.iter().any(|s| s.name == "flex_col" && s.generated));
    }

    #[test]
    fn methods_unknown_type() {
        let out = methods_for_type(&ApiIndex::empty(), "Nope", None);
        assert!(out.contains("No type 'Nope'"));
    }

    #[test]
    fn methods_inherent_and_trait() {
        let mut idx = ApiIndex::empty();
        idx.symbols = vec![
            sym(
                SymbolKind::Method,
                "notify",
                Some("Entity"),
                "fn notify",
                "ping\nmore",
            ),
            sym(
                SymbolKind::TraitMethod,
                "render",
                Some("Render"),
                "fn render",
                "draw",
            ),
        ];
        idx.trait_impls.push(TraitImplRecord {
            trait_name: "Render".into(),
            type_name: "Entity".into(),
        });
        let out = methods_for_type(&idx, "Entity", None);
        assert!(out.contains("## Inherent"));
        assert!(out.contains("- `fn notify` — ping"));
        assert!(out.contains("## via Render"));
        assert!(out.contains("- `fn render` — draw"));
        let filtered = methods_for_type(&idx, "Entity", Some("missing"));
        assert_eq!(filtered, "No methods for 'Entity' with that trait filter.");
    }

    #[test]
    fn methods_collapse_when_many() {
        let mut idx = ApiIndex::empty();
        idx.trait_impls.push(TraitImplRecord {
            trait_name: "Styled".into(),
            type_name: "Div".into(),
        });
        for i in 0..201 {
            idx.symbols.push(sym(
                SymbolKind::TraitMethod,
                &format!("m{i}"),
                Some("Styled"),
                &format!("fn m{i}"),
                "",
            ));
        }
        let out = methods_for_type(&idx, "Div", None);
        assert!(out.contains("## Inherent\n(none)"));
        assert!(out.contains("via Styled"));
        assert!(out.contains("m0, m1"));
        assert!(out.contains("Pass trait_filter"));
        let filtered = methods_for_type(&idx, "Div", Some("Styled"));
        assert!(filtered.contains("- `fn m0`"));
        assert!(!filtered.contains("Pass trait_filter"));
        assert!(!filtered.contains("## Inherent"));
    }
}
