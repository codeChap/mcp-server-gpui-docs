# GPUI gotchas (2026)

Pin this. GPUI APIs move with Zed. Prefer **current Zed `crates/gpui` examples** over blog posts from 2023–2024.

## Boot

Current pattern (Zed main):

```rust
use gpui::*;
use gpui_platform::application;

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_| HelloWorld)
        })
        .unwrap();
    });
}
```

Older tutorials use `Application::new().run(...)`. That may still compile on some crates.io snapshots (`gpui` 0.2.x) and **will not** match Zed-git `gpui` + `gpui_platform`.

## Cargo.toml (Zed-git, recommended)

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit", "wayland", "x11"] }
```

Linux: need `gpui_platform` features for Wayland/X11 + fonts.

## gpui-component

Call `gpui_component::init(cx)` **before** opening windows that use its widgets.
The window root should be their `Root` wrapper (see hello_world example in that repo).

```toml
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
```

## Mental model

- **Entity&lt;T&gt;** — owned state. `cx.new(|cx| ...)`.
- **Render** — view: `fn render(&mut self, window: &mut Window, cx: &mut Context&lt;Self&gt;) -> impl IntoElement`
- **div()** — Tailwind-ish builder: `.flex()`, `.v_flex()`, `.size_full()`, `.bg(...)`, `.child(...)`
- Layout is **Taffy flex/grid**, not immediate-mode cursor layout.
- Notify-driven: idle CPU can be ~0. Call `cx.notify()` after mutating.

## Actions / keys

Define with `actions!`, bind with `cx.bind_keys([KeyBinding::new("ctrl-s", Save, None)])`, handle on the focused entity.

## Lists

- `uniform_list` — equal-height rows, cheapest
- `list` + `ListState` — variable height, virtualized

## Custom drawing (graph canvas)

Implement `Element`: `request_layout` → `prepaint` → `paint`. Paint quads/paths on `Window`. This is how a node graph should work — not nested `div`s for every wire.

## Do not

- Copy `eframe`/`egui` patterns (immediate `ui.horizontal`, memory IDs).
- Assume crates.io `gpui = "0.1"` docs still apply.
- Clone the **full** Zed repo into an app; depend on git + use `crates/gpui` examples as the source of truth.
