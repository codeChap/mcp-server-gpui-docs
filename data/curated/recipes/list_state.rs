// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// Variable-height virtualized rows. Store ListState on the view.
// Equal-height rows: use recipe uniform_list_usage instead.

use gpui::{
    App, Context, ListAlignment, ListState, Render, Window, WindowOptions, div, list, prelude::*,
    px, rgb,
};
use gpui_platform::application;

struct Rows {
    list_state: ListState,
}

impl Rows {
    fn new() -> Self {
        Self {
            list_state: ListState::new(500, ListAlignment::Top, px(80.)),
        }
    }
}

impl Render for Rows {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        list(self.list_state.clone(), |ix, _window, _cx| {
            let h = px(24. + (ix % 4) as f32 * 8.);
            div()
                .h(h)
                .w_full()
                .px_2()
                .child(format!("row {ix}"))
                .into_any()
        })
        .h(px(400.))
        .w_full()
        .bg(rgb(0x111111))
        .text_color(rgb(0xeeeeee))
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_| Rows::new())
        })
        .unwrap();
    });
}
