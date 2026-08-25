// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// State lives in the view struct. Mutate then cx.notify().

use gpui::{
    App, Context, MouseButton, Render, Window, WindowOptions, div, prelude::*, px, rgb,
};
use gpui_platform::application;

struct Counter {
    n: i32,
}

impl Render for Counter {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.n;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(format!("count: {n}"))
            .child(
                div()
                    .id("inc")
                    .px_3()
                    .py_1()
                    .bg(rgb(0x334155))
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .child("increment")
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _window, cx| {
                        this.n += 1;
                        cx.notify();
                    })),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_| Counter { n: 0 })
        })
        .unwrap();
    });
}
