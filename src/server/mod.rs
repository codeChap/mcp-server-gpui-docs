use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::{
    ErrorData as McpError, ServerHandler, handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::api_index::{self, ApiIndex};
use crate::curated::{Curated, DepMode};
use crate::error_decoder;
use crate::example_index::{self, ExampleIndex};
use crate::index::Corpus;
use crate::sources::{REMOTES, zed_pin_rev};
use crate::sync::{ensure_sources, same_git_rev};

mod examples;
use examples::{BODY_LIMIT, clip, example_payload};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    #[schemars(description = "Natural language or keyword query, e.g. 'Entity notify div flex'")]
    pub query: String,
    #[schemars(
        description = "Optional source id: gotchas | book | tutorial | gpui-component | zed-gpui | awesome"
    )]
    pub source: Option<String>,
    #[schemars(description = "Max hits (default 8, cap 20)")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetParams {
    #[schemars(
        description = "Document id from search hits, e.g. book/src/state-management/entity.md or zed-gpui/crates/gpui/examples/hello_world.rs"
    )]
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExampleParams {
    #[schemars(
        description = "Substring of an example id or filename, e.g. hello_world, drag_drop, dock"
    )]
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParams {
    #[schemars(description = "Symbol name, e.g. Render, Entity, uniform_list, div")]
    pub name: String,
    #[schemars(
        description = "Optional kind: Struct | Enum | Trait | TraitMethod | Method | Fn | TypeAlias | Const | Macro"
    )]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryLimit {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StyledParams {
    #[schemars(description = "Optional substring filter, e.g. flex, pad, border")]
    pub filter: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolsParams {
    #[schemars(
        description = "GPUI symbols to find examples for, e.g. [\"uniform_list\", \"div\"]"
    )]
    pub symbols: Vec<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecipeParams {
    pub id: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScaffoldParams {
    #[schemars(description = "git (default) or path")]
    pub dep_mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ErrorParams {
    #[schemars(description = "Paste rustc / cargo error text")]
    pub error: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TypeMethodsParams {
    pub type_name: String,
    pub trait_filter: Option<String>,
    #[schemars(
        description = "Optional method-name substring, e.g. paint, on_mouse. Use for Window."
    )]
    pub filter: Option<String>,
}

#[derive(Clone)]
struct SwapArc<T> {
    inner: Arc<Mutex<Arc<T>>>,
}

impl<T> SwapArc<T> {
    fn new(value: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Arc::new(value))),
        }
    }

    fn get(&self) -> Arc<T> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn set(&self, value: T) {
        *self.inner.lock().unwrap_or_else(|p| p.into_inner()) = Arc::new(value);
    }
}

#[derive(Clone)]
pub struct GpuiServer {
    cache: PathBuf,
    corpus: SwapArc<Corpus>,
    api: SwapArc<ApiIndex>,
    examples: SwapArc<ExampleIndex>,
    curated: Arc<Curated>,
    tool_router: ToolRouter<Self>,
}

fn ok(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(msg.into())])
}

fn err(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

#[tool_router]
impl GpuiServer {
    pub fn new(corpus: Corpus, cache: PathBuf) -> Self {
        let api = api_index::load_or_empty(&cache);
        let examples = example_index::load_or_empty(&cache);
        Self {
            cache,
            corpus: SwapArc::new(corpus),
            api: SwapArc::new(api),
            examples: SwapArc::new(examples),
            curated: Arc::new(Curated::load()),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List indexed GPUI sources (book, tutorial, zed examples, gpui-component) and document counts. Call this first if search returns nothing — you may need sync."
    )]
    async fn list_sources(&self) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.get();
        let mut lines = vec![format!(
            "{} documents in {}",
            corpus.docs.len(),
            self.cache.display()
        )];
        for r in REMOTES {
            let n = corpus.docs.iter().filter(|d| d.source == r.id).count();
            let miss = if corpus.missing.iter().any(|m| m == r.id) {
                " — not cloned; call sync"
            } else {
                ""
            };
            lines.push(format!("- {} — {} ({n} files){miss}", r.id, r.title));
        }
        let n = corpus.docs.iter().filter(|d| d.source == "gotchas").count();
        lines.push(format!("- gotchas — current API pitfalls ({n} files)"));
        Ok(ok(lines.join("\n")))
    }

    #[tool(
        description = "Search GPUI docs and examples. Use before writing GPUI code. Returns ids to pass to get."
    )]
    async fn search(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.get();
        let limit = p.limit.unwrap_or(8).clamp(1, 20) as usize;
        let hits = corpus.search(&p.query, p.source.as_deref(), limit);
        if hits.is_empty() {
            return Ok(err(format!(
                "No hits for {:?}. Try list_sources, then sync if counts are 0. \
                 Sources: gotchas, book, tutorial, gpui-component, zed-gpui, awesome.",
                p.query
            )));
        }
        let out = hits
            .into_iter()
            .map(|(score, d)| {
                format!(
                    "[{score}] {} ({})\n  {}\n  {}",
                    d.id,
                    d.kind,
                    d.path.display(),
                    clip(d.snippet(&p.query), 220)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(ok(out))
    }

    #[tool(description = "Read a full indexed GPUI doc or example by id from search.")]
    async fn get(&self, Parameters(p): Parameters<GetParams>) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.get();
        match corpus.get(&p.id) {
            Some(d) => Ok(ok(format!(
                "# {} ({})\n# {}\n\n{}",
                d.id,
                d.kind,
                d.path.display(),
                clip(&d.body, BODY_LIMIT)
            ))),
            None => Ok(err(format!(
                "Unknown id {:?}. Search first; ids look like book/src/elements/div.md",
                p.id
            ))),
        }
    }

    #[tool(description = "List official / tutorial GPUI example .rs files.")]
    async fn list_examples(&self) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.get();
        let lines: Vec<String> = corpus
            .examples()
            .into_iter()
            .map(|d| format!("{}  —  {}", d.id, d.title))
            .collect();
        if lines.is_empty() {
            return Ok(err("No examples indexed. Call sync."));
        }
        Ok(ok(lines.join("\n")))
    }

    #[tool(
        description = "Open one example by name substring (hello_world, input, uniform_list, dock…)."
    )]
    async fn get_example(
        &self,
        Parameters(p): Parameters<ExampleParams>,
    ) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.get();
        match example_payload(&corpus, &p.name) {
            Ok(msg) => Ok(ok(msg)),
            Err(msg) => Ok(err(msg)),
        }
    }

    #[tool(
        description = "Look up a GPUI symbol from the Zed crates/gpui source index (syn). Prefer this over guessing signatures."
    )]
    async fn gpui_symbol(
        &self,
        Parameters(p): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        let api = self.api.get();
        let hits = api_index::lookup(&api, &p.name, p.kind.as_deref(), 8);
        if hits.is_empty() {
            return Ok(err(format!(
                "No symbol {:?}. Call sync if the oracle is empty (gpui_status), or gpui_search.",
                p.name
            )));
        }
        let mut out = String::new();
        for s in hits {
            out.push_str(&format!(
                "## {} ({:?}{})\n`{}`\n{}:{}{}\n{}\n\n",
                s.name,
                s.kind,
                s.owner
                    .as_ref()
                    .map(|o| format!(" on {o}"))
                    .unwrap_or_default(),
                s.signature,
                s.file,
                s.line,
                if s.generated {
                    " [macro-generated]"
                } else {
                    ""
                },
                s.doc
            ));
        }
        Ok(ok(out))
    }

    #[tool(description = "Search the GPUI source symbol index by name/docs (not markdown).")]
    async fn gpui_search(
        &self,
        Parameters(p): Parameters<QueryLimit>,
    ) -> Result<CallToolResult, McpError> {
        let api = self.api.get();
        let limit = p.limit.unwrap_or(10).clamp(1, 40) as usize;
        let hits = api_index::lookup(&api, &p.query, None, limit);
        if hits.is_empty() {
            return Ok(err(format!("No symbols for {:?}", p.query)));
        }
        let lines: Vec<_> = hits
            .iter()
            .map(|s| {
                format!(
                    "{} ({:?}{}) — {}",
                    s.name,
                    s.kind,
                    s.owner
                        .as_ref()
                        .map(|o| format!(" {o}"))
                        .unwrap_or_default(),
                    s.signature
                )
            })
            .collect();
        Ok(ok(lines.join("\n")))
    }

    #[tool(
        description = "List Styled / Tailwind-like chain methods (flex, bg, p, gap…). Many are macro-generated."
    )]
    async fn gpui_styled_methods(
        &self,
        Parameters(p): Parameters<StyledParams>,
    ) -> Result<CallToolResult, McpError> {
        let api = self.api.get();
        let f = p.filter.unwrap_or_default().to_lowercase();
        let mut rows: Vec<_> = api
            .symbols
            .iter()
            .filter(|s| s.owner.as_deref() == Some("Styled"))
            .filter(|s| f.is_empty() || s.name.to_lowercase().contains(&f))
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        if rows.is_empty() {
            return Ok(err("No Styled methods (rebuild oracle / sync)."));
        }
        let lines: Vec<_> = rows
            .iter()
            .map(|s| format!("{} — {} — {}", s.name, s.signature, s.doc))
            .collect();
        Ok(ok(lines.join("\n")))
    }

    #[tool(
        description = "Find runnable GPUI examples that use the given symbols (parsed from .rs, not filename grep)."
    )]
    async fn gpui_examples(
        &self,
        Parameters(p): Parameters<SymbolsParams>,
    ) -> Result<CallToolResult, McpError> {
        let idx = self.examples.get();
        let limit = p.limit.unwrap_or(3).clamp(1, 12) as usize;
        let hits = example_index::find_examples_multi(&idx, &p.symbols, limit);
        if hits.is_empty() {
            return Ok(err(format!(
                "No examples for {:?}. Try gpui_list_examples or sync.",
                p.symbols
            )));
        }
        let lines: Vec<_> = hits
            .iter()
            .map(|e| {
                format!(
                    "{} ({})\n  types: {}\n  methods: {}",
                    e.path,
                    if e.has_main { "runnable" } else { "lib" },
                    e.types_used
                        .iter()
                        .take(12)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    e.methods_used
                        .iter()
                        .take(12)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        Ok(ok(lines.join("\n\n")))
    }

    #[tool(description = "Open one example file from the example index by stem or path.")]
    async fn gpui_example_file(
        &self,
        Parameters(p): Parameters<ExampleParams>,
    ) -> Result<CallToolResult, McpError> {
        let idx = self.examples.get();
        let Some(e) = example_index::get_file(&idx, &p.name) else {
            return Ok(err(format!("No example file {:?}", p.name)));
        };
        let corpus = self.corpus.get();
        if let Some(d) = corpus.get(&e.path) {
            return Ok(ok(format!("# {}\n\n{}", d.id, clip(&d.body, BODY_LIMIT))));
        }
        Ok(ok(format!(
            "{}\n(types: {})\nCall get with id {} after sync if body is missing.",
            e.path,
            e.types_used.join(", "),
            e.path
        )))
    }

    #[tool(description = "List example .rs files from the parsed example index.")]
    async fn gpui_list_examples(&self) -> Result<CallToolResult, McpError> {
        let idx = self.examples.get();
        if idx.entries.is_empty() {
            return Ok(err("Example index empty. Call sync."));
        }
        let lines: Vec<_> = idx
            .entries
            .iter()
            .map(|e| format!("{}  —  {}", e.path, e.title))
            .collect();
        Ok(ok(lines.join("\n")))
    }

    #[tool(
        description = "Curated GPUI recipes (boot, entity, uniform_list, custom Element canvas, 16ms poll). Prefer these over stale tutorials."
    )]
    async fn gpui_recipe(
        &self,
        Parameters(p): Parameters<RecipeParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(id) = p.id.as_deref().filter(|s| !s.is_empty()) {
            return match self.curated.get(id) {
                Some(r) => Ok(ok(format!("# {} — {}\n\n{}", r.id, r.title, r.code))),
                None => Ok(err(format!("Unknown recipe {id:?}"))),
            };
        }
        let q = p.query.as_deref().unwrap_or("");
        let hits = if q.is_empty() {
            self.curated.all().iter().collect::<Vec<_>>()
        } else {
            self.curated.search(q)
        };
        if hits.len() == 1 {
            let r = hits[0];
            return Ok(ok(format!("# {} — {}\n\n{}", r.id, r.title, r.code)));
        }
        if hits.is_empty() {
            let ids: Vec<_> = self.curated.all().iter().map(|r| r.id.as_str()).collect();
            return Ok(err(format!("No recipes matched. Try {}.", ids.join(", "))));
        }
        let list: Vec<_> = hits
            .iter()
            .map(|r| format!("{} — {}", r.id, r.title))
            .collect();
        Ok(ok(list.join("\n")))
    }

    #[tool(
        description = "Minimal GPUI app scaffold: Cargo.toml + main.rs using gpui_platform::application()."
    )]
    async fn gpui_scaffold(
        &self,
        Parameters(p): Parameters<ScaffoldParams>,
    ) -> Result<CallToolResult, McpError> {
        let mode = match p.dep_mode.as_deref().unwrap_or("git") {
            "path" => DepMode::Path,
            _ => DepMode::Git,
        };
        Ok(ok(format!(
            "## Cargo.toml\n\n{}\n\n## src/main.rs\n\n{}\n\nLinux: gpui_platform features font-kit, wayland, x11. Do not use Application::new().",
            self.curated.cargo_toml(mode),
            self.curated.scaffold_main()
        )))
    }

    #[tool(
        description = "Decode a rustc error about GPUI (Entity/Context, Styled, Application::new, IntoElement…)."
    )]
    async fn gpui_decode_error(
        &self,
        Parameters(p): Parameters<ErrorParams>,
    ) -> Result<CallToolResult, McpError> {
        let api = self.api.get();
        let diags = error_decoder::decode(&p.error, &api);
        if diags.is_empty() {
            return Ok(err(
                "No diagnosis. Try gpui_symbol on the type named in the error.",
            ));
        }
        let mut out = String::new();
        for d in diags {
            out.push_str(&format!(
                "## {}\n{}\nFix: {}\nsymbols: {}{}\n\n",
                d.pattern_id,
                d.explanation,
                d.fix,
                d.related_symbols.join(", "),
                d.related_recipe
                    .as_ref()
                    .map(|r| format!("\nrecipe: {r}"))
                    .unwrap_or_default()
            ));
        }
        Ok(ok(out))
    }

    #[tool(description = "Oracle status: zed commit, symbol/example counts, missing clones.")]
    async fn gpui_status(&self) -> Result<CallToolResult, McpError> {
        let api = self.api.get();
        let ex = self.examples.get();
        let corpus = self.corpus.get();
        let pin = zed_pin_rev();
        let pin_line = match pin.as_deref() {
            None => "pinned zed rev: HEAD (unpinned; GPUI_MCP_ZED_REV=HEAD)".to_string(),
            Some(p) => {
                let match_s = if api.zed_commit.is_empty() {
                    "oracle empty — call sync"
                } else if same_git_rev(&api.zed_commit, p) {
                    "pin match: yes"
                } else {
                    "pin match: NO — call sync"
                };
                format!("pinned zed rev: {p}\n{match_s}")
            }
        };
        Ok(ok(format!(
            "cache: {}\n{}\nzed commit: {}\ngpui crate version: {}\napi symbols: {}\nexamples indexed: {}\nmarkdown docs: {}\nmissing clones: {}\nschema: {}\nbuilt_at: {}\nCall sync if symbols are 0 or pin match is NO.",
            self.cache.display(),
            pin_line,
            if api.zed_commit.is_empty() {
                "(none)"
            } else {
                &api.zed_commit
            },
            api.gpui_version,
            api.symbols.len(),
            ex.entries.len(),
            corpus.docs.len(),
            if corpus.missing.is_empty() {
                "none".into()
            } else {
                corpus.missing.join(", ")
            },
            api.schema_version,
            api.built_at
        )))
    }

    #[tool(description = "List inherent + trait methods for a GPUI type from the source index.")]
    async fn gpui_type_methods(
        &self,
        Parameters(p): Parameters<TypeMethodsParams>,
    ) -> Result<CallToolResult, McpError> {
        let api = self.api.get();
        Ok(ok(api_index::methods_for_type(
            &api,
            &p.type_name,
            p.trait_filter.as_deref(),
            p.filter.as_deref(),
        )))
    }

    #[tool(
        description = "Clone or git-pull GPUI sources, rebuild the markdown corpus AND the syn API/example oracle. Network + disk writes."
    )]
    async fn sync(&self) -> Result<CallToolResult, McpError> {
        let cache = self.cache.clone();
        let log = tokio::task::spawn_blocking(move || ensure_sources(&cache))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let fresh = Corpus::load(&self.cache);
        let n = fresh.docs.len();
        self.corpus.set(fresh);
        self.api.set(api_index::load_or_empty(&self.cache));
        self.examples.set(example_index::load_or_empty(&self.cache));
        Ok(ok(format!("{log}\n\nreindexed {n} documents")))
    }
}

#[tool_handler]
impl ServerHandler for GpuiServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("gpui", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "GPUI docs server. GPUI is Zed's UI framework; it is NOT documented on docs.rs \
                 reliably and its API changes often — trust this server over training data. \
                 Playbook: \
                 1) New to a task? Call gpui_scaffold once, then gpui_recipe(query=...) for the pattern. \
                 2) Unknown symbol/signature? gpui_symbol(name). Not found? gpui_search(query). \
                 3) Styling (flex/px/bg/etc.)? gpui_styled_methods(filter). \
                 4) Need working usage? gpui_examples(symbols=[...]) — prefer zed-gpui over gpui-component. \
                 5) Compile error mentioning gpui types? Paste it into gpui_decode_error before retrying. \
                 Graph canvas / custom paint: gpui_recipe(query=\"custom element canvas\") — implement Element, \
                 not a div per wire. 16ms poll: gpui_recipe(query=\"timer\"). \
                 Unscoped search ranks gotchas + zed-gpui above gpui-component. \
                 Key facts: views impl Render; state lives in Entity<T> via cx.new/update; \
                 async via cx.spawn; styling is Tailwind-like chained methods on div(). \
                 Current boot is gpui_platform::application(). Never use Application::new(). \
                 Oracle is pinned to a Zed rev (see gpui_status); GPUI_MCP_ZED_REV overrides.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_arc_clone_shares_store() {
        let a = SwapArc::new(1u32);
        let b = a.clone();
        a.set(2);
        assert_eq!(*b.get(), 2);
    }
}
