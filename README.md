# mcp-server-gpui

Local **stdio MCP** so an agent can **search and read GPUI docs/examples** instead of guessing APIs.

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

- `list_sources` — counts
- `search` — query (+ optional source)
- `get` — full file by id
- `list_examples` / `get_example`
- `sync` — `git clone` / `pull` then reindex

Cache: `~/.cache/mcp-server-gpui` (override `GPUI_MCP_CACHE`).  
Set `GPUI_MCP_SYNC_ON_START=1` to clone on launch (slow first time; zed is sparse).

## Build

Needs Rust (edition 2024) and `git` on `PATH`.

```bash
cargo build --release
```

Binary: `target/release/mcp-server-gpui`

Or:

```bash
cargo install --git https://github.com/codeChap/mcp-server-gpui
```

## MCP config (Grok / Claude / similar)

Point the client at the built binary (stdio):

```toml
[mcp_servers.gpui]
command = "/path/to/mcp-server-gpui"
enabled = true
startup_timeout_sec = 60
```

Then call `sync` once, then `search` / `get` before writing GPUI.

## License

MIT
