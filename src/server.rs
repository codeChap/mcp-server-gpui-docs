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
use crate::sources::REMOTES;
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
    corpus: Arc<Mutex<Arc<Corpus>>>,
    tool_router: ToolRouter<Self>,
}

fn ok(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(msg.into())])
}

fn err(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

pub(crate) fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}…\n\n[truncated — call get with this id for the full file]",
        &s[..end]
    )
}

#[tool_router]
impl GpuiServer {
    pub fn new(corpus: Corpus, cache: PathBuf) -> Self {
        Self {
            cache,
            corpus: Arc::new(Mutex::new(Arc::new(corpus))),
            tool_router: Self::tool_router(),
        }
    }

    fn snapshot(&self) -> Arc<Corpus> {
        self.corpus
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    #[tool(
        description = "List indexed GPUI sources (book, tutorial, zed examples, gpui-component) and document counts. Call this first if search returns nothing — you may need sync."
    )]
    async fn list_sources(&self) -> Result<CallToolResult, McpError> {
        let corpus = self.snapshot();
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
        let corpus = self.snapshot();
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
        let corpus = self.snapshot();
        match corpus.get(&p.id) {
            Some(d) => Ok(ok(format!(
                "# {} ({})\n# {}\n\n{}",
                d.id,
                d.kind,
                d.path.display(),
                clip(&d.body, 24_000)
            ))),
            None => Ok(err(format!(
                "Unknown id {:?}. Search first; ids look like book/src/elements/div.md",
                p.id
            ))),
        }
    }

    #[tool(description = "List official / tutorial GPUI example .rs files.")]
    async fn list_examples(&self) -> Result<CallToolResult, McpError> {
        let corpus = self.snapshot();
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

    #[tool(description = "Open one example by name substring (hello_world, input, uniform_list, dock…).")]
    async fn get_example(
        &self,
        Parameters(p): Parameters<ExampleParams>,
    ) -> Result<CallToolResult, McpError> {
        let corpus = self.snapshot();
        match example_payload(&corpus, &p.name) {
            Ok(msg) => Ok(ok(msg)),
            Err(msg) => Ok(err(msg)),
        }
    }

    #[tool(
        description = "Clone or git-pull GPUI book, tutorial, gpui-component, and sparse Zed crates/gpui, then reindex. Run when sources are empty or APIs look stale. Network + disk writes."
    )]
    async fn sync(&self) -> Result<CallToolResult, McpError> {
        let cache = self.cache.clone();
        let log = tokio::task::spawn_blocking(move || ensure_sources(&cache))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let fresh = Corpus::load(&self.cache);
        let n = fresh.docs.len();
        *self.corpus.lock().unwrap_or_else(|p| p.into_inner()) = Arc::new(fresh);
        Ok(ok(format!("{log}\n\nreindexed {n} documents")))
    }
}

pub(crate) fn example_payload(corpus: &Corpus, name: &str) -> Result<String, String> {
    let stem = example_stem(name);
    if stem.is_empty() {
        return Err("Empty example name".into());
    }
    let q = stem.to_lowercase();
    let matches: Vec<_> = corpus
        .examples()
        .into_iter()
        .filter(|d| d.id.to_lowercase().contains(&q) || d.title.to_lowercase().contains(&q))
        .collect();
    if matches.is_empty() {
        return Err(format!("No example matching {name:?}"));
    }
    let exact: Vec<_> = matches
        .iter()
        .copied()
        .filter(|d| {
            d.path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(&format!("{stem}.rs")))
        })
        .collect();
    if exact.len() == 1 {
        return Ok(format_example(exact[0]));
    }
    if exact.len() > 1 {
        let list: Vec<_> = exact.iter().map(|d| d.id.as_str()).collect();
        return Err(format!(
            "Multiple examples:\n{}\nCall get with one id.",
            list.join("\n")
        ));
    }
    if matches.len() > 1 {
        let list: Vec<_> = matches.iter().map(|d| d.id.as_str()).collect();
        return Err(format!(
            "Multiple examples:\n{}\nCall get with one id.",
            list.join("\n")
        ));
    }
    Ok(format_example(matches[0]))
}

fn example_stem(name: &str) -> String {
    let n = name.trim();
    let lower = n.to_lowercase();
    if lower.ends_with(".rs") {
        n[..n.len() - 3].to_string()
    } else {
        n.to_string()
    }
}

fn format_example(d: &crate::index::Doc) -> String {
    format!(
        "# {}\n# {}\n\n{}",
        d.id,
        d.path.display(),
        clip(&d.body, 24_000)
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Doc;
    use std::path::PathBuf;

    fn doc(id: &str, title: &str, body: &str) -> Doc {
        Doc::new(
            id,
            "zed-gpui",
            "example",
            title,
            PathBuf::from(id),
            body,
        )
    }

    fn corpus(docs: Vec<Doc>) -> Corpus {
        Corpus {
            docs,
            missing: Vec::new(),
        }
    }

    #[test]
    fn clip_does_not_panic_on_multibyte() {
        let s = "é".repeat(200);
        let out = clip(&s, 10);
        assert!(out.contains('…'));
        assert!(out.is_char_boundary(out.find('…').unwrap()));
    }

    #[test]
    fn clip_short_unchanged() {
        assert_eq!(clip("hi", 10), "hi");
    }

    #[test]
    fn example_stem_strips_rs() {
        let c = corpus(vec![doc(
            "zed-gpui/crates/gpui/examples/hello_world.rs",
            "hello world",
            "fn main() {}",
        )]);
        let msg = example_payload(&c, "hello_world.rs").unwrap();
        assert!(msg.contains("hello_world.rs"));
        let msg = example_payload(&c, "hello_world").unwrap();
        assert!(msg.contains("hello_world.rs"));
    }

    #[test]
    fn example_ambiguous_without_exact_filename() {
        let c = corpus(vec![
            doc("a/examples/foo_hello.rs", "foo hello", "a"),
            doc("b/examples/bar_hello.rs", "bar hello", "b"),
        ]);
        let err = example_payload(&c, "hello").unwrap_err();
        assert!(err.starts_with("Multiple examples:"));
    }
}
