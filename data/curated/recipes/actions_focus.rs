// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// Keys only fire on the focused subtree. Stamp key_context, track_focus,
// on_action, then bind_keys with that same context string.

use gpui::{
    App, Context, FocusHandle, Focusable, KeyBinding, Render, Window, WindowOptions, actions,
    div, prelude::*, rgb,
};
use gpui_platform::application;

actions!(counter, [Increment, Decrement]);

struct Counter {
    n: i32,
    focus_handle: FocusHandle,
}

impl Counter {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            n: 0,
            focus_handle: cx.focus_handle(),
        }
    }

    fn increment(&mut self, _: &Increment, _: &mut Window, cx: &mut Context<Self>) {
        self.n += 1;
        cx.notify();
    }

    fn decrement(&mut self, _: &Decrement, _: &mut Window, cx: &mut Context<Self>) {
        self.n -= 1;
        cx.notify();
    }
}

impl Focusable for Counter {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Counter {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("counter")
            .key_context("Counter")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::increment))
            .on_action(cx.listener(Self::decrement))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .child(format!("count {}  (up/down)", self.n))
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("up", Increment, Some("Counter")),
            KeyBinding::new("down", Decrement, Some("Counter")),
        ]);
        cx.open_window(WindowOptions::default(), |window, cx| {
            let view = cx.new(Counter::new);
            view.focus_handle(cx).focus(window, cx);
            view
        })
        .unwrap();
    });
}
