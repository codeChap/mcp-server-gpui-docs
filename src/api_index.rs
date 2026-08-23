use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::{ImplItem, Item, TraitItem, Type, Visibility};
use walkdir::WalkDir;

pub const INDEX_SCHEMA_VERSION: u32 = 1;
const MACRO_JSON: &str = include_str!("../data/macro_generated.json");

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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(index)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
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

pub fn build_api_index(gpui_src_root: &Path, zed_commit: &str) -> Result<ApiIndex> {
    let mut symbols = Vec::new();
    let mut trait_impls = Vec::new();
    let mut skipped_files = Vec::new();
    let src = gpui_src_root.join("src");
    if src.is_dir() {
        for entry in WalkDir::new(&src).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path
                .strip_prefix(gpui_src_root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(text) = std::fs::read_to_string(path) else {
                skipped_files.push(rel);
                continue;
            };
            match syn::parse_file(&text) {
                Ok(file) => index_file(&file, &rel, &mut symbols, &mut trait_impls),
                Err(e) => {
                    eprintln!("gpui mcp: skip {rel}: {e}");
                    skipped_files.push(rel);
                }
            }
        }
    }

    merge_macro_supplement(&mut symbols);
    symbols.sort_by(|a, b| (&a.name, &a.kind, &a.owner).cmp(&(&b.name, &b.kind, &b.owner)));

    Ok(ApiIndex {
        schema_version: INDEX_SCHEMA_VERSION,
        zed_commit: zed_commit.to_string(),
        gpui_version: read_gpui_version(gpui_src_root),
        built_at: now_stamp(),
        skipped_files,
        trait_impls,
        symbols,
    })
}

pub fn lookup<'a>(index: &'a ApiIndex, query: &str, kind: Option<&str>, limit: usize) -> Vec<&'a ApiSymbol> {
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
        .filter_map(|(i, s)| {
            let n = s.name.to_lowercase();
            let tier = if n == q {
                1
            } else if n.starts_with(&q) {
                2
            } else if n.contains(&q) {
                3
            } else if s.doc.to_lowercase().contains(&q) {
                4
            } else {
                return None;
            };
            Some((tier, i, s))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.truncate(limit);
    scored.into_iter().map(|(_, _, s)| s).collect()
}

pub fn methods_for_type(index: &ApiIndex, type_name: &str, trait_filter: Option<&str>) -> String {
    let inherent: Vec<_> = index
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method && s.owner.as_deref() == Some(type_name))
        .collect();
    let mut trait_names: Vec<String> = index
        .trait_impls
        .iter()
        .filter(|r| r.type_name == type_name)
        .map(|r| r.trait_name.clone())
        .collect();
    trait_names.sort();
    trait_names.dedup();
    if inherent.is_empty() && trait_names.is_empty() {
        let styled = index
            .symbols
            .iter()
            .any(|s| s.owner.as_deref() == Some(type_name) || s.name == type_name);
        if !styled {
            return format!("No type '{type_name}' in the GPUI index. Try gpui_symbol / gpui_search first.");
        }
    }
    let filter_l = trait_filter.map(|t| t.to_lowercase());
    let mut out = String::new();
    if filter_l.is_none() {
        out.push_str("## Inherent\n");
        if inherent.is_empty() {
            out.push_str("(none)\n");
        } else {
            for s in &inherent {
                out.push_str(&format!("- `{}` — {}\n", s.signature, one_line(&s.doc)));
            }
        }
    }
    let many = inherent.len()
        + trait_names.iter().map(|tn| {
            index
                .symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::TraitMethod && s.owner.as_deref() == Some(tn.as_str()))
                .count()
        }).sum::<usize>();
    let collapse = many > 200 && filter_l.is_none();
    for tn in trait_names {
        if let Some(f) = &filter_l {
            if tn.to_lowercase() != *f {
                continue;
            }
        }
        let methods: Vec<_> = index
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::TraitMethod && s.owner.as_deref() == Some(tn.as_str()))
            .collect();
        out.push_str(&format!("\n## via {tn}\n"));
        if collapse {
            let names: Vec<_> = methods.iter().map(|m| m.name.as_str()).collect();
            out.push_str(&names.join(", "));
            out.push('\n');
        } else {
            for s in methods {
                out.push_str(&format!("- `{}` — {}\n", s.signature, one_line(&s.doc)));
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

fn index_file(
    file: &syn::File,
    rel: &str,
    symbols: &mut Vec<ApiSymbol>,
    trait_impls: &mut Vec<TraitImplRecord>,
) {
    for item in &file.items {
        match item {
            Item::Struct(s) if is_pub(&s.vis) => {
                symbols.push(sym(
                    SymbolKind::Struct,
                    s.ident.to_string(),
                    None,
                    format!(
                        "pub struct {} /* {} fields */",
                        s.ident,
                        field_count(&s.fields)
                    ),
                    first_doc(&s.attrs),
                    rel,
                    s.span().start().line,
                    deprecated(&s.attrs),
                    cfgs(&s.attrs),
                ));
            }
            Item::Enum(e) if is_pub(&e.vis) => {
                symbols.push(sym(
                    SymbolKind::Enum,
                    e.ident.to_string(),
                    None,
                    format!("pub enum {} /* {} variants */", e.ident, e.variants.len()),
                    first_doc(&e.attrs),
                    rel,
                    e.span().start().line,
                    deprecated(&e.attrs),
                    cfgs(&e.attrs),
                ));
            }
            Item::Trait(t) if is_pub(&t.vis) => {
                let name = t.ident.to_string();
                symbols.push(sym(
                    SymbolKind::Trait,
                    name.clone(),
                    None,
                    format!("pub trait {name}"),
                    first_doc(&t.attrs),
                    rel,
                    t.span().start().line,
                    deprecated(&t.attrs),
                    cfgs(&t.attrs),
                ));
                for item in &t.items {
                    if let TraitItem::Fn(f) = item {
                        symbols.push(sym(
                            SymbolKind::TraitMethod,
                            f.sig.ident.to_string(),
                            Some(name.clone()),
                            norm_tokens(&f.sig),
                            first_doc(&f.attrs),
                            rel,
                            f.span().start().line,
                            deprecated(&f.attrs),
                            cfgs(&f.attrs),
                        ));
                    }
                }
            }
            Item::Fn(f) if is_pub(&f.vis) => {
                symbols.push(sym(
                    SymbolKind::Fn,
                    f.sig.ident.to_string(),
                    None,
                    norm_tokens(&f.sig),
                    first_doc(&f.attrs),
                    rel,
                    f.span().start().line,
                    deprecated(&f.attrs),
                    cfgs(&f.attrs),
                ));
            }
            Item::Type(t) if is_pub(&t.vis) => {
                symbols.push(sym(
                    SymbolKind::TypeAlias,
                    t.ident.to_string(),
                    None,
                    format!("pub type {}", t.ident),
                    first_doc(&t.attrs),
                    rel,
                    t.span().start().line,
                    deprecated(&t.attrs),
                    cfgs(&t.attrs),
                ));
            }
            Item::Const(c) if is_pub(&c.vis) => {
                symbols.push(sym(
                    SymbolKind::Const,
                    c.ident.to_string(),
                    None,
                    format!("pub const {}", c.ident),
                    first_doc(&c.attrs),
                    rel,
                    c.span().start().line,
                    deprecated(&c.attrs),
                    cfgs(&c.attrs),
                ));
            }
            Item::Macro(m) => {
                if let Some(id) = &m.ident {
                    symbols.push(sym(
                        SymbolKind::Macro,
                        id.to_string(),
                        None,
                        format!("macro_rules! {id}"),
                        first_doc(&m.attrs),
                        rel,
                        m.span().start().line,
                        deprecated(&m.attrs),
                        cfgs(&m.attrs),
                    ));
                }
            }
            Item::Impl(imp) => {
                let Some(type_name) = type_last_ident(&imp.self_ty) else {
                    continue;
                };
                if impl_is_generic_self(imp, &type_name) {
                    continue;
                }
                if let Some((_, trait_path, _)) = &imp.trait_ {
                    let trait_name = trait_path
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    if !trait_name.is_empty() {
                        trait_impls.push(TraitImplRecord {
                            trait_name,
                            type_name,
                        });
                    }
                    continue;
                }
                for item in &imp.items {
                    if let ImplItem::Fn(f) = item {
                        if !is_pub(&f.vis) {
                            continue;
                        }
                        symbols.push(sym(
                            SymbolKind::Method,
                            f.sig.ident.to_string(),
                            Some(type_name.clone()),
                            norm_tokens(&f.sig),
                            first_doc(&f.attrs),
                            rel,
                            f.span().start().line,
                            deprecated(&f.attrs),
                            cfgs(&f.attrs),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

fn impl_is_generic_self(imp: &syn::ItemImpl, type_name: &str) -> bool {
    imp.generics
        .type_params()
        .any(|p| p.ident == type_name)
}

fn type_last_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => type_last_ident(&r.elem),
        _ => None,
    }
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
        let exists = symbols.iter().any(|e| {
            e.name == s.name && e.owner.as_deref() == Some("Styled")
        });
        if !exists {
            symbols.push(s);
        }
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn field_count(fields: &syn::Fields) -> usize {
    match fields {
        syn::Fields::Named(n) => n.named.len(),
        syn::Fields::Unnamed(u) => u.unnamed.len(),
        syn::Fields::Unit => 0,
    }
}

fn first_doc(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for a in attrs {
        if a.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &a.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    let t = s.value();
                    let t = t.strip_prefix(' ').unwrap_or(&t);
                    if t.is_empty() && !lines.is_empty() {
                        break;
                    }
                    lines.push(t.to_string());
                }
            }
        }
    }
    let mut d = lines.join(" ");
    if d.len() > 400 {
        d.truncate(400);
    }
    d
}

fn deprecated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("deprecated"))
}

fn cfgs(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("cfg"))
        .map(|a| a.to_token_stream().to_string())
        .collect()
}

fn norm_tokens(t: &impl ToTokens) -> String {
    let raw = t.to_token_stream().to_string();
    let mut out = String::new();
    let mut prev_space = false;
    for c in raw.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            prev_space = false;
            out.push(c);
        }
    }
    out.replace(" (", "(")
        .replace(" ,", ",")
        .replace(" ;", ";")
        .replace(" >", ">")
        .replace("< ", "<")
        .trim()
        .to_string()
}

fn sym(
    kind: SymbolKind,
    name: String,
    owner: Option<String>,
    signature: String,
    doc: String,
    file: &str,
    line: usize,
    deprecated: bool,
    cfg: Vec<String>,
) -> ApiSymbol {
    ApiSymbol {
        kind,
        name,
        owner,
        via_trait: None,
        signature,
        doc,
        file: file.to_string(),
        line,
        deprecated,
        cfg,
        generated: false,
    }
}

fn read_gpui_version(gpui_root: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(gpui_root.join("Cargo.toml")) else {
        return "unknown".into();
    };
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("version") {
            if let Some(q) = rest.split('"').nth(1) {
                return q.to_string();
            }
        }
    }
    "unknown".into()
}

fn now_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("unix:{}", d.as_secs()))
        .unwrap_or_else(|_| "unknown".into())
}

fn one_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_tiers() {
        let mut idx = ApiIndex::empty();
        idx.symbols = vec![
            sym(SymbolKind::Trait, "Render".into(), None, "pub trait Render".into(), "view".into(), "a.rs", 1, false, vec![]),
            sym(SymbolKind::Fn, "uniform_list".into(), None, "fn uniform_list".into(), "list".into(), "b.rs", 1, false, vec![]),
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
            "flex".into(),
            Some("Styled".into()),
            "fn flex".into(),
            String::new(),
            "styled.rs",
            45,
            false,
            vec![],
        )];
        merge_macro_supplement(&mut v);
        assert_eq!(
            v.iter().filter(|s| s.name == "flex" && s.owner.as_deref() == Some("Styled")).count(),
            1
        );
        assert!(v.iter().any(|s| s.name == "flex_col" && s.generated));
    }
}
