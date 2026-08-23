mod index;
mod server;
mod sources;
mod sync;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

use index::Corpus;
use server::GpuiServer;
use sources::cache_dir;
use sync::ensure_sources;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cache = cache_dir();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--sync") {
        println!("{}", ensure_sources(&cache)?);
        let corpus = Corpus::load(&cache);
        println!("indexed {} documents", corpus.docs.len());
        return Ok(());
    }
    // Best-effort: index whatever is already on disk so startup stays fast.
    // Call the `sync` tool to clone/pull (or set GPUI_MCP_SYNC_ON_START=1).
    if std::env::var("GPUI_MCP_SYNC_ON_START").ok().as_deref() == Some("1") {
        if let Err(e) = ensure_sources(&cache) {
            eprintln!("gpui mcp sync on start failed: {e:#}");
        }
    }
    let corpus = Corpus::load(&cache);
    eprintln!(
        "gpui mcp: {} docs from {}",
        corpus.docs.len(),
        cache.display()
    );

    let service = GpuiServer::new(corpus).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
