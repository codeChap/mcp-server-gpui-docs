use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;

use crate::api_index::{ApiIndex, lookup};

#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub pattern_id: String,
    pub explanation: String,
    pub fix: String,
    pub related_symbols: Vec<String>,
    pub related_recipe: Option<String>,
}

#[derive(Deserialize)]
struct Pattern {
    id: String,
    regexes: Vec<String>,
    explanation: String,
    fix: String,
    symbols: Vec<String>,
    recipe: Option<String>,
}

struct CompiledPattern {
    id: String,
    regexes: Vec<Regex>,
    explanation: String,
    fix: String,
    symbols: Vec<String>,
    recipe: Option<String>,
}

impl CompiledPattern {
    fn to_diagnosis(&self) -> Diagnosis {
        Diagnosis {
            pattern_id: self.id.clone(),
            explanation: self.explanation.clone(),
            fix: self.fix.clone(),
            related_symbols: self.symbols.clone(),
            related_recipe: self.recipe.clone(),
        }
    }
}

struct FallbackRegexes {
    gpui_path: Regex,
    method_named: Regex,
}

fn patterns() -> &'static [CompiledPattern] {
    static PATTERNS: OnceLock<Vec<CompiledPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let raw: Vec<Pattern> =
            serde_json::from_str(include_str!("../data/curated/error_patterns.json"))
                .unwrap_or_default();
        raw.into_iter()
            .map(|p| CompiledPattern {
                id: p.id,
                regexes: p
                    .regexes
                    .iter()
                    .filter_map(|r| Regex::new(&format!("(?is){r}")).ok())
                    .collect(),
                explanation: p.explanation,
                fix: p.fix,
                symbols: p.symbols,
                recipe: p.recipe,
            })
            .collect()
    })
}

fn fallback_regexes() -> &'static FallbackRegexes {
    static RE: OnceLock<FallbackRegexes> = OnceLock::new();
    RE.get_or_init(|| FallbackRegexes {
        gpui_path: Regex::new(r"gpui::([A-Za-z0-9_]+)").expect("static regex"),
        method_named: Regex::new(r"method named [`']([A-Za-z0-9_]+)[`']").expect("static regex"),
    })
}

pub fn decode(error_text: &str, api: &ApiIndex) -> Vec<Diagnosis> {
    let mut out: Vec<_> = patterns()
        .iter()
        .filter(|p| p.regexes.iter().any(|re| re.is_match(error_text)))
        .map(CompiledPattern::to_diagnosis)
        .collect();
    if out.is_empty() {
        out.extend(fallback_lookups(error_text, api));
    }
    out
}

fn fallback_lookups(error_text: &str, api: &ApiIndex) -> Vec<Diagnosis> {
    let re = fallback_regexes();
    let mut names = Vec::new();
    for cap in re.gpui_path.captures_iter(error_text) {
        names.push(cap[1].to_string());
    }
    for cap in re.method_named.captures_iter(error_text) {
        names.push(cap[1].to_string());
    }
    names.sort();
    names.dedup();
    names
        .into_iter()
        .take(5)
        .filter_map(|n| {
            let hits = lookup(api, &n, None, 3);
            let docs: Vec<_> = hits
                .iter()
                .map(|s| format!("{} ({:?}) {}", s.name, s.kind, s.signature))
                .collect();
            (!docs.is_empty()).then(|| Diagnosis {
                pattern_id: "fallback_symbol_lookup".into(),
                explanation: format!("No pattern matched; nearest GPUI symbols for `{n}`:"),
                fix: docs.join("\n"),
                related_symbols: vec![n],
                related_recipe: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_index::{ApiIndex, ApiSymbol, SymbolKind};

    #[test]
    fn old_application_new() {
        let d = decode(
            "cannot find type `Application` in crate `gpui`",
            &ApiIndex::empty(),
        );
        assert_eq!(d[0].pattern_id, "old_application_new");
    }

    #[test]
    fn missing_flex() {
        let d = decode(
            "no method named `flex` found for struct `HelloWorld`",
            &ApiIndex::empty(),
        );
        assert_eq!(d[0].pattern_id, "missing_styled");
    }

    #[test]
    fn fallback_looks_up_gpui_path() {
        let mut idx = ApiIndex::empty();
        idx.symbols.push(ApiSymbol {
            kind: SymbolKind::Struct,
            name: "Window".into(),
            owner: None,
            via_trait: None,
            signature: "pub struct Window".into(),
            doc: String::new(),
            file: "window.rs".into(),
            line: 1,
            deprecated: false,
            cfg: vec![],
            generated: false,
        });
        let d = decode("failed to resolve gpui::Window in this crate", &idx);
        assert_eq!(d[0].pattern_id, "fallback_symbol_lookup");
        assert!(d[0].fix.contains("Window"));
    }
}
