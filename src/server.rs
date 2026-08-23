use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::index::Corpus;
use crate::sources::{cache_dir, REMOTES};
use crate::sync::ensure_sources;

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
    #[schemars(description = "Substring of an example id or filename, e.g. hello_world, drag_drop, dock")]
    pub name: String,
}

#[derive(Clone)]
pub struct GpuiServer {
    cache: PathBuf,
    corpus: Arc<Mutex<Corpus>>,
    tool_router: ToolRouter<Self>,
}

fn ok(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(msg.into())])
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…\n\n[truncated — call get with this id for the full file]", &s[..max])
    }
}

#[tool_router]
impl GpuiServer {
    pub fn new(corpus: Corpus) -> Self {
        Self {
            cache: cache_dir(),
            corpus: Arc::new(Mutex::new(corpus)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List indexed GPUI sources (book, tutorial, zed examples, gpui-component) and document counts. Call this first if search returns nothing — you may need sync."
    )]
    async fn list_sources(&self) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.lock().unwrap();
        let mut lines = vec![format!(
            "{} documents in {}",
            corpus.docs.len(),
            self.cache.display()
        )];
        for r in REMOTES {
            let n = corpus.docs.iter().filter(|d| d.source == r.id).count();
            lines.push(format!("- {} — {} ({n} files)", r.id, r.title));
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
        let corpus = self.corpus.lock().unwrap();
        let limit = p.limit.unwrap_or(8).clamp(1, 20) as usize;
        let hits = corpus.search(&p.query, p.source.as_deref(), limit);
        if hits.is_empty() {
            return Ok(ok(format!(
                "No hits for {:?}. Try list_sources, then sync if counts are 0. \
                 Sources: gotchas, book, tutorial, gpui-component, zed-gpui, awesome.",
                p.query
            )));
        }
        let mut out = Vec::new();
        for (score, d) in hits {
            out.push(format!(
                "[{score}] {} ({})\n  {}\n  {}",
                d.id,
                d.kind,
                d.path.display(),
                clip(d.snippet(&p.query), 220)
            ));
        }
        Ok(ok(out.join("\n\n")))
    }

    #[tool(description = "Read a full indexed GPUI doc or example by id from search.")]
    async fn get(&self, Parameters(p): Parameters<GetParams>) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.lock().unwrap();
        match corpus.get(&p.id) {
            Some(d) => Ok(ok(format!(
                "# {} ({})\n# {}\n\n{}",
                d.id,
                d.kind,
                d.path.display(),
                clip(&d.body, 24_000)
            ))),
            None => Ok(ok(format!(
                "Unknown id {:?}. Search first; ids look like book/src/elements/div.md",
                p.id
            ))),
        }
    }

    #[tool(description = "List official / tutorial GPUI example .rs files.")]
    async fn list_examples(&self) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.lock().unwrap();
        let lines: Vec<String> = corpus
            .examples()
            .into_iter()
            .map(|d| format!("{}  —  {}", d.id, d.title))
            .collect();
        if lines.is_empty() {
            return Ok(ok("No examples indexed. Call sync."));
        }
        Ok(ok(lines.join("\n")))
    }

    #[tool(description = "Open one example by name substring (hello_world, input, uniform_list, dock…).")]
    async fn get_example(
        &self,
        Parameters(p): Parameters<ExampleParams>,
    ) -> Result<CallToolResult, McpError> {
        let corpus = self.corpus.lock().unwrap();
        let q = p.name.to_lowercase();
        let matches: Vec<_> = corpus
            .examples()
            .into_iter()
            .filter(|d| d.id.to_lowercase().contains(&q) || d.title.to_lowercase().contains(&q))
            .collect();
        if matches.is_empty() {
            return Ok(ok(format!("No example matching {:?}", p.name)));
        }
        if matches.len() > 1 && !matches.iter().any(|d| d.id.ends_with(&format!("{}.rs", p.name)))
        {
            let list: Vec<_> = matches.iter().map(|d| d.id.as_str()).collect();
            return Ok(ok(format!(
                "Multiple examples:\n{}\nCall get with one id.",
                list.join("\n")
            )));
        }
        let d = matches
            .iter()
            .find(|d| d.id.contains(&p.name))
            .unwrap_or(&matches[0]);
        Ok(ok(format!(
            "# {}\n# {}\n\n{}",
            d.id,
            d.path.display(),
            clip(&d.body, 24_000)
        )))
    }

    #[tool(
        description = "Clone or git-pull GPUI book, tutorial, gpui-component, and sparse Zed crates/gpui, then reindex. Run when sources are empty or APIs look stale."
    )]
    async fn sync(&self) -> Result<CallToolResult, McpError> {
        let cache = self.cache.clone();
        let log = tokio::task::spawn_blocking(move || ensure_sources(&cache))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let fresh = Corpus::load(&self.cache);
        let n = fresh.docs.len();
        *self.corpus.lock().unwrap() = fresh;
        Ok(ok(format!("{log}\n\nreindexed {n} documents")))
    }
}

#[tool_handler]
impl ServerHandler for GpuiServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("gpui", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "GPUI documentation MCP. Before writing GPUI/Rust UI code: \
                 1) search for the API (Entity, div, Render, actions, list, window). \
                 2) get the matching book page or zed-gpui example. \
                 3) Prefer zed-gpui examples + gotchas over old Application::new tutorials. \
                 Current boot is gpui_platform::application(). \
                 Do not invent Tailwind-on-div methods — confirm in book/src/styling.",
            )
    }
}
