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

Pin the rev. This server's oracle defaults to the same SHA (`d9ad6aff67e47de43abb270d22de75dd950f1b48`). Override with `GPUI_MCP_ZED_REV`.

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "d9ad6aff67e47de43abb270d22de75dd950f1b48" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "d9ad6aff67e47de43abb270d22de75dd950f1b48", features = ["font-kit", "wayland", "x11"] }
```

Linux: enable at least one of `wayland` / `x11` on **`gpui_platform`** (not the empty feature names on `gpui`). macOS needs `font-kit` or glyphs do not rasterize. Windows needs no extra features. `gpui_platform::headless()` is the test/CI boot.

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

Keys fire on the **focused** subtree, not globally.

1. `actions!(my_view, [Save]);` or `#[gpui::action] struct Save;`
2. On the view: `.key_context("MyView").track_focus(&self.focus_handle).on_action(cx.listener(Self::save))`
3. `impl Focusable` + `cx.focus_handle()` + focus the view after `cx.new`
4. `cx.bind_keys([KeyBinding::new("ctrl-s", Save, Some("MyView"))])` — the context string must match `key_context`

`cx.bind_keys` in `main` with `None` context is only for app-global bindings (quit). Recipe: `actions_focus`.

## Subscriptions

Store `Subscription`s on the view (drop unsubscribes). `observe` = any `cx.notify()` on that entity. `subscribe` = typed `EventEmitter`. `.detach()` only for process-lifetime listeners. Recipe: `subscriptions`.

## Async

`cx.spawn` first argument is `WeakEntity<Self>`. Cannot hold `&mut Context` across `.await`. Keep the `Task` on the view if drop should cancel. `cx.background_spawn` is for `Send` work only. GPUI's executor is not Tokio — if you need Tokio, `gpui_tokio::init(cx)` then `gpui_tokio::Tokio::spawn`. Recipes: `spawn_weak` (one-shot), `poll_timer` (16ms loop).

## Lists

- `uniform_list` — equal-height rows, cheapest (recipe `uniform_list_usage`)
- `list` + `ListState` — variable height, virtualized (recipe `list_state`)

## Overlays

In-window float: `deferred(anchored().anchor(Anchor::TopLeft).child(...))` (recipe `deferred_anchored`). That is not a new `Window`. OS-level menus use `WindowKind::AnchoredPopup`.

## Custom drawing (graph canvas)

Do **not** nest a `div` per node or wire. Implement `Element`:

`request_layout` → `prepaint` (`insert_hitbox`) → `paint` (`paint_quad` / `PathBuilder` / `paint_path`).

Register mouse handlers **in `paint`** with `window.on_mouse_event` (paint-phase only). Recipe: `custom_element_canvas`.

Zed `painting.rs` uses the `canvas()` closure helper — fine for doodles, **wrong** for a node graph (pin hit-testing, pan/zoom, wires). The book page `implementing-element` is a stub; use the recipe plus `zed-gpui` examples `painting.rs` / `paths_bench.rs` for path APIs.

Engine I/O does not belong in paint. Drain work on a 16ms `cx.spawn` + `background_executor().timer` (recipe `poll_timer`).

## Do not

- Copy `eframe`/`egui` patterns (immediate `ui.horizontal`, memory IDs).
- Assume crates.io `gpui = "0.1"` docs still apply.
- Clone the **full** Zed repo into an app; depend on git + use `crates/gpui` examples as the source of truth.
