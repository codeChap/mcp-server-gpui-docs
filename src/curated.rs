use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Recipe {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub symbols: Vec<String>,
    pub code: &'static str,
}

#[derive(Deserialize)]
struct RecipeMeta {
    id: String,
    title: String,
    tags: Vec<String>,
    symbols: Vec<String>,
}

pub struct Curated {
    recipes: Vec<Recipe>,
}

pub enum DepMode {
    Git,
    Path,
}

impl Curated {
    pub fn load() -> Self {
        let metas: Vec<RecipeMeta> =
            serde_json::from_str(include_str!("../data/curated/recipes.json")).unwrap_or_default();
        let recipes = metas
            .into_iter()
            .filter_map(|m| {
                let code = recipe_code(&m.id)?;
                Some(Recipe {
                    id: m.id,
                    title: m.title,
                    tags: m.tags,
                    symbols: m.symbols,
                    code,
                })
            })
            .collect();
        Self { recipes }
    }

    pub fn get(&self, id: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id == id)
    }

    pub fn search(&self, query: &str) -> Vec<&Recipe> {
        let tokens: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| t.len() > 1)
            .collect();
        if tokens.is_empty() {
            return self.recipes.iter().collect();
        }
        let mut scored: Vec<(u32, &Recipe)> = self
            .recipes
            .iter()
            .filter_map(|r| {
                let title = r.title.to_lowercase();
                let mut score = 0u32;
                for t in &tokens {
                    if r.id == *t {
                        score += 20;
                    } else if r.id.contains(t.as_str()) {
                        score += 10;
                    }
                    if title.contains(t) {
                        score += 8;
                    }
                    if r.tags.iter().any(|tag| {
                        let tag = tag.to_lowercase();
                        tag == *t || (t.len() >= 3 && tag.contains(t.as_str()))
                    }) {
                        score += 6;
                    }
                    if r.symbols.iter().any(|s| {
                        let s = s.to_lowercase();
                        s == *t || s.contains(t)
                    }) {
                        score += 6;
                    }
                }
                (score > 0).then_some((score, r))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        scored.into_iter().map(|(_, r)| r).collect()
    }

    pub fn all(&self) -> &[Recipe] {
        &self.recipes
    }

    pub fn cargo_toml(&self, mode: DepMode) -> String {
        match mode {
            DepMode::Git => include_str!("../data/curated/scaffolds/Cargo.toml.snippet")
                .replace("ZED_PINNED_REV", crate::sources::ZED_PINNED_REV),
            DepMode::Path => {
                include_str!("../data/curated/scaffolds/Cargo_local.toml.snippet").to_string()
            }
        }
    }

    pub fn scaffold_main(&self) -> &'static str {
        include_str!("../data/curated/scaffolds/app_main.rs")
    }
}

fn recipe_code(id: &str) -> Option<&'static str> {
    match id {
        "window_open" => Some(include_str!("../data/curated/recipes/window_open.rs")),
        "entity_state" => Some(include_str!("../data/curated/recipes/entity_state.rs")),
        "uniform_list_usage" => Some(include_str!(
            "../data/curated/recipes/uniform_list_usage.rs"
        )),
        "custom_element_canvas" => Some(include_str!(
            "../data/curated/recipes/custom_element_canvas.rs"
        )),
        "poll_timer" => Some(include_str!("../data/curated/recipes/poll_timer.rs")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_recipes() {
        let c = Curated::load();
        assert!(c.get("window_open").is_some());
        assert!(c.get("custom_element_canvas").is_some());
        assert!(c.get("poll_timer").is_some());
        assert_eq!(c.search("entity")[0].id, "entity_state");
        assert!(c.scaffold_main().contains("gpui_platform::application"));
        assert!(
            c.cargo_toml(DepMode::Git)
                .contains(crate::sources::ZED_PINNED_REV)
        );
    }

    #[test]
    fn recipe_search_tokens_hit_canvas() {
        let c = Curated::load();
        let hits = c.search("custom element paint canvas");
        assert_eq!(
            hits.len(),
            1,
            "{:?}",
            hits.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].id, "custom_element_canvas");
        assert_eq!(c.search("16ms timer poll")[0].id, "poll_timer");
        assert_eq!(c.search("PathBuilder")[0].id, "custom_element_canvas");
    }
}
