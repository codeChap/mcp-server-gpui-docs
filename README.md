# mcp-server-gpui-docs

Local **stdio MCP** that indexes **GPUI documentation and examples** so an agent can search them instead of guessing APIs.

This is not a GPUI runtime. It clones/pulls public GPUI sources and serves `search` / `get` over MCP.

Indexed sources (after `sync`):

| id | what |
|---|---|
| `gotchas` | bundled current-API pitfalls (always present) |
| `book` | [GPUI Book](https://github.com/MatinAniss/gpui-book) |
| `tutorial` | [gpui-tutorial](https://github.com/hedge-ops/gpui-tutorial) |
| `gpui-component` | [Longbridge gpui-component](https://github.com/longbridge/gpui-component) |
| `zed-gpui` | sparse clone of Zed `crates/gpui` (README + examples) |
| `awesome` | [awesome-gpui](https://github.com/zed-industries/awesome-gpui) |

## Tools

Corpus (markdown + example files):

- `list_sources` — counts
- `search` / `get` — keyword docs
- `list_examples` / `get_example`

Oracle (from Zed `crates/gpui` source via `syn` — **prefer these**):

- `gpui_scaffold` — Cargo.toml + `gpui_platform::application()` main (git deps pinned)
- `gpui_recipe` — curated boot / entity / `uniform_list` / **custom Element canvas** / 16ms poll
- `gpui_symbol` / `gpui_search` / `gpui_type_methods` — signatures from source (`type_methods` lists trait methods when the name is a trait; pass `filter` for `Window.paint_*`)
- `gpui_styled_methods` — `flex` / `bg` / padding (macro-generated)
- `gpui_examples` / `gpui_list_examples` / `gpui_example_file` — examples that *use* a symbol
- `gpui_decode_error` — paste rustc output
- `gpui_status` — index freshness
- `sync` — clone/pull **and** rebuild the oracle (`--rebuild` on the CLI rebuilds only the oracle)

Cache: `~/.cache/mcp-server-gpui-docs` (override `GPUI_MCP_CACHE` or `XDG_CACHE_HOME`). `HOME` or `GPUI_MCP_CACHE` is required — the server will not use `/tmp`.  
Gotchas are compiled into the binary (`include_str!`), so `cargo install` still serves them.  
Set `GPUI_MCP_SYNC_ON_START=1` to clone on launch (slow first time; zed is sparse). `sync` writes to the cache (git clone/pull).

**Zed pin:** `sync` checks out `crates/gpui` at `d9ad6aff67e47de43abb270d22de75dd950f1b48` (RustRivetGPUI's `gpui` rev) so symbol signatures match the app, not Zed `main`. Override with `GPUI_MCP_ZED_REV=<sha>` or `GPUI_MCP_ZED_REV=HEAD` to follow the default branch.

**Search:** unscoped `search` ranks `gotchas` and `zed-gpui` above `gpui-component`. Pass `source` to restrict. The GPUI book page `implementing-element` is a stub — use `gpui_recipe(query="custom element canvas")`.

## Build

Needs Rust (edition 2024) and `git` on `PATH`.

```bash
cargo build --release
```

Binary: `target/release/mcp-server-gpui-docs`

Or:

```bash
cargo install --git https://github.com/codeChap/mcp-server-gpui-docs
```

## MCP config (Grok / Claude / similar)

Point the client at the built binary (stdio). Server id should be `gpui-docs` so it is distinct from GPUI the framework:

```toml
[mcp_servers.gpui-docs]
command = "/path/to/mcp-server-gpui-docs"
enabled = true
startup_timeout_sec = 60
```

Then call `sync` once, then `search` / `get` before writing GPUI.

## License

MIT
