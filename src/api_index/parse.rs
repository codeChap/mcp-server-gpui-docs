use std::path::Path;

use anyhow::Result;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::{ImplItem, Item, TraitItem, Type, Visibility};
use walkdir::WalkDir;

use super::{
    ApiIndex, ApiSymbol, INDEX_SCHEMA_VERSION, SymbolKind, TraitImplRecord, merge_macro_supplement,
};

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
            let rel = crate::index::rel_unix(path, gpui_src_root);
            let Ok(text) = std::fs::read_to_string(path) else {
                skipped_files.push(rel);
                continue;
            };
            match syn::parse_file(&text) {
                Ok(file) => index_file(&file, &rel, &mut symbols, &mut trait_impls),
                Err(e) => {
                    eprintln!("gpui-docs mcp: skip {rel}: {e}");
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
        built_at: crate::persist::unix_stamp(),
        skipped_files,
        trait_impls,
        symbols,
    })
}

// Syn item dispatcher: one arm per public kind. Per-kind helpers would only relocate the same arms.
fn index_file(
    file: &syn::File,
    rel: &str,
    symbols: &mut Vec<ApiSymbol>,
    trait_impls: &mut Vec<TraitImplRecord>,
) {
    for item in &file.items {
        match item {
            Item::Struct(s) if is_pub(&s.vis) => {
                push_symbol(
                    symbols,
                    SymbolKind::Struct,
                    s.ident.to_string(),
                    None,
                    format!(
                        "pub struct {} /* {} fields */",
                        s.ident,
                        field_count(&s.fields)
                    ),
                    &s.attrs,
                    rel,
                    s.span().start().line,
                );
            }
            Item::Enum(e) if is_pub(&e.vis) => {
                push_symbol(
                    symbols,
                    SymbolKind::Enum,
                    e.ident.to_string(),
                    None,
                    format!("pub enum {} /* {} variants */", e.ident, e.variants.len()),
                    &e.attrs,
                    rel,
                    e.span().start().line,
                );
            }
            Item::Trait(t) if is_pub(&t.vis) => {
                let name = t.ident.to_string();
                push_symbol(
                    symbols,
                    SymbolKind::Trait,
                    name.clone(),
                    None,
                    format!("pub trait {name}"),
                    &t.attrs,
                    rel,
                    t.span().start().line,
                );
                for item in &t.items {
                    if let TraitItem::Fn(f) = item {
                        push_symbol(
                            symbols,
                            SymbolKind::TraitMethod,
                            f.sig.ident.to_string(),
                            Some(name.clone()),
                            norm_tokens(&f.sig),
                            &f.attrs,
                            rel,
                            f.span().start().line,
                        );
                    }
                }
            }
            Item::Fn(f) if is_pub(&f.vis) => {
                push_symbol(
                    symbols,
                    SymbolKind::Fn,
                    f.sig.ident.to_string(),
                    None,
                    norm_tokens(&f.sig),
                    &f.attrs,
                    rel,
                    f.span().start().line,
                );
            }
            Item::Type(t) if is_pub(&t.vis) => {
                push_symbol(
                    symbols,
                    SymbolKind::TypeAlias,
                    t.ident.to_string(),
                    None,
                    format!("pub type {}", t.ident),
                    &t.attrs,
                    rel,
                    t.span().start().line,
                );
            }
            Item::Const(c) if is_pub(&c.vis) => {
                push_symbol(
                    symbols,
                    SymbolKind::Const,
                    c.ident.to_string(),
                    None,
                    format!("pub const {}", c.ident),
                    &c.attrs,
                    rel,
                    c.span().start().line,
                );
            }
            Item::Macro(m) => {
                if let Some(id) = &m.ident {
                    push_symbol(
                        symbols,
                        SymbolKind::Macro,
                        id.to_string(),
                        None,
                        format!("macro_rules! {id}"),
                        &m.attrs,
                        rel,
                        m.span().start().line,
                    );
                }
            }
            Item::Impl(imp) => index_impl(imp, rel, symbols, trait_impls),
            _ => {}
        }
    }
}

fn index_impl(
    imp: &syn::ItemImpl,
    rel: &str,
    symbols: &mut Vec<ApiSymbol>,
    trait_impls: &mut Vec<TraitImplRecord>,
) {
    let Some(type_name) = type_last_ident(&imp.self_ty) else {
        return;
    };
    if impl_is_generic_self(imp, &type_name) {
        return;
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
        return;
    }
    for item in &imp.items {
        if let ImplItem::Fn(f) = item {
            if !is_pub(&f.vis) {
                continue;
            }
            push_symbol(
                symbols,
                SymbolKind::Method,
                f.sig.ident.to_string(),
                Some(type_name.clone()),
                norm_tokens(&f.sig),
                &f.attrs,
                rel,
                f.span().start().line,
            );
        }
    }
}

fn push_symbol(
    symbols: &mut Vec<ApiSymbol>,
    kind: SymbolKind,
    name: String,
    owner: Option<String>,
    signature: String,
    attrs: &[syn::Attribute],
    file: &str,
    line: usize,
) {
    symbols.push(ApiSymbol {
        kind,
        name,
        owner,
        via_trait: None,
        signature,
        doc: first_doc(attrs),
        file: file.to_string(),
        line,
        deprecated: deprecated(attrs),
        cfg: cfgs(attrs),
        generated: false,
    });
}

fn impl_is_generic_self(imp: &syn::ItemImpl, type_name: &str) -> bool {
    imp.generics.type_params().any(|p| p.ident == type_name)
}

fn type_last_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        Type::Reference(r) => type_last_ident(&r.elem),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_symbols(src: &str) -> (Vec<ApiSymbol>, Vec<TraitImplRecord>) {
        let file = syn::parse_file(src).unwrap();
        let mut symbols = Vec::new();
        let mut impls = Vec::new();
        index_file(&file, "t.rs", &mut symbols, &mut impls);
        (symbols, impls)
    }

    #[test]
    fn indexes_public_items_and_skips_private_and_generic_impl() {
        let src = r#"
            struct Hidden;
            pub struct Foo { pub a: u8, b: u8 }
            pub enum Bar { A, B, C }
            pub trait T { fn m(&self); }
            impl Foo {
                pub fn inherent(&self) {}
                fn privm(&self) {}
            }
            impl T for Foo {}
            impl<T> Clone for T {}
        "#;
        let (symbols, impls) = parse_symbols(src);
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Foo" && s.kind == SymbolKind::Struct)
        );
        assert!(symbols.iter().all(|s| s.name != "Hidden"));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "inherent" && s.owner.as_deref() == Some("Foo"))
        );
        assert!(symbols.iter().all(|s| s.name != "privm"));
        assert!(symbols.iter().any(|s| {
            s.name == "m" && s.kind == SymbolKind::TraitMethod && s.owner.as_deref() == Some("T")
        }));
        assert!(
            impls
                .iter()
                .any(|r| r.trait_name == "T" && r.type_name == "Foo")
        );
        assert!(impls.iter().all(|r| r.trait_name != "Clone"));
        let foo = symbols.iter().find(|s| s.name == "Foo").unwrap();
        assert!(foo.signature.contains("2 fields"));
        let bar = symbols.iter().find(|s| s.name == "Bar").unwrap();
        assert!(bar.signature.contains("3 variants"));
    }
}
