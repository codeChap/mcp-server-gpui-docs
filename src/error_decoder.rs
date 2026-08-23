use regex::Regex;
use serde::Deserialize;

use crate::api_index::{lookup, ApiIndex};

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

pub fn decode(error_text: &str, api: &ApiIndex) -> Vec<Diagnosis> {
    let patterns: Vec<Pattern> =
        serde_json::from_str(include_str!("../data/curated/error_patterns.json")).unwrap_or_default();
    let mut out = Vec::new();
    for p in &patterns {
        let hit = p.regexes.iter().any(|r| {
            Regex::new(&format!("(?is){r}"))
                .map(|re| re.is_match(error_text))
                .unwrap_or(false)
        });
        if hit {
            out.push(Diagnosis {
                pattern_id: p.id.clone(),
                explanation: p.explanation.clone(),
                fix: p.fix.clone(),
                related_symbols: p.symbols.clone(),
                related_recipe: p.recipe.clone(),
            });
        }
    }
    if out.is_empty() {
        let mut names = Vec::new();
        for cap in Regex::new(r"gpui::([A-Za-z0-9_]+)").unwrap().captures_iter(error_text)
        {
            names.push(cap[1].to_string());
        }
        for cap in Regex::new(r"method named [`']([A-Za-z0-9_]+)[`']")
            .unwrap()
            .captures_iter(error_text)
        {
            names.push(cap[1].to_string());
        }
        names.sort();
        names.dedup();
        for n in names.into_iter().take(5) {
            let hits = lookup(api, &n, None, 3);
            let docs: Vec<_> = hits
                .iter()
                .map(|s| format!("{} ({:?}) {}", s.name, s.kind, s.signature))
                .collect();
            if !docs.is_empty() {
                out.push(Diagnosis {
                    pattern_id: "fallback_symbol_lookup".into(),
                    explanation: format!("No pattern matched; nearest GPUI symbols for `{n}`:"),
                    fix: docs.join("\n"),
                    related_symbols: vec![n],
                    related_recipe: None,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_index::ApiIndex;

    #[test]
    fn old_application_new() {
        let d = decode("cannot find type `Application` in crate `gpui`", &ApiIndex::empty());
        assert_eq!(d[0].pattern_id, "old_application_new");
    }

    #[test]
    fn missing_flex() {
        let d = decode("no method named `flex` found for struct `HelloWorld`", &ApiIndex::empty());
        assert_eq!(d[0].pattern_id, "missing_styled");
    }
}
