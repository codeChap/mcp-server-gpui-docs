// gpui rev: zed git HEAD  verified: 2026-03-23
// Boot: gpui_platform::application, NOT Application::new().

use gpui::{
    App, Context, Render, Window, WindowOptions, div, prelude::*, px, rgb,
};
use gpui_platform::application;

struct HelloWorld;

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .child("hello gpui")
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_| HelloWorld)
        })
        .unwrap();
    });
}
