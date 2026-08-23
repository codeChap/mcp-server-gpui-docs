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
        let q = query.to_lowercase();
        self.recipes
            .iter()
            .filter(|r| {
                r.id.contains(&q)
                    || r.title.to_lowercase().contains(&q)
                    || r.tags.iter().any(|t| t.to_lowercase().contains(&q))
                    || r.symbols.iter().any(|s| s.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn all(&self) -> &[Recipe] {
        &self.recipes
    }

    pub fn cargo_toml(&self, mode: DepMode) -> &'static str {
        match mode {
            DepMode::Git => include_str!("../data/curated/scaffolds/Cargo.toml.snippet"),
            DepMode::Path => include_str!("../data/curated/scaffolds/Cargo_local.toml.snippet"),
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
        "uniform_list_usage" => {
            Some(include_str!("../data/curated/recipes/uniform_list_usage.rs"))
        }
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
        assert!(c.search("entity")[0].id == "entity_state");
        assert!(c.scaffold_main().contains("gpui_platform::application"));
    }
}
