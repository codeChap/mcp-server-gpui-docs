// gpui rev: zed git HEAD  verified: 2026-03-23
// Equal-height virtualized rows. Prefer this over nested divs for long lists.
// Full API: zed-gpui examples/data_table.rs and src/elements/uniform_list.rs

use gpui::{
    App, Context, Render, Window, WindowOptions, div, prelude::*, px, rgb, uniform_list,
};
use gpui_platform::application;

struct Rows;

impl Render for Rows {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        uniform_list("rows", 10_000, |range, _window, _cx| {
            range
                .map(|ix| div().h(px(24.)).child(format!("row {ix}")))
                .collect()
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
            cx.new(|_| Rows)
        })
        .unwrap();
    });
}
