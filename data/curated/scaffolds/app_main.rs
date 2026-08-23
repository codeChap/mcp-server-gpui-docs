use gpui::{
    App, Context, Render, Window, WindowOptions, div, prelude::*, rgb,
};
use gpui_platform::application;

struct Root;

impl Render for Root {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .items_center()
            .justify_center()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xfafafa))
            .child("Root")
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_| Root)
        })
        .unwrap();
    });
}
