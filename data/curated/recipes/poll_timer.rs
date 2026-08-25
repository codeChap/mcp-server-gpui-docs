// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// Drain engine/work on a 16ms timer. Do not HTTP in paint or Render.
// Keep the Task on the view so the loop is not dropped.

use std::time::Duration;

use gpui::{
    App, AsyncApp, Context, Render, Task, Window, WindowOptions, div, prelude::*, rgb,
};
use gpui_platform::application;

struct Shell {
    footer: String,
    _tick: Task<()>,
}

impl Shell {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tick = cx.spawn(async move |this, cx: &mut AsyncApp| loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            this.update(cx, |_state, cx| {
                // engine.poll(); apply events; then:
                cx.notify();
            })
            .ok();
        });
        Self {
            footer: "polling".into(),
            _tick: tick,
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x111111))
            .text_color(rgb(0xeeeeee))
            .p_4()
            .child(self.footer.clone())
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| Shell::new(window, cx))
        })
        .unwrap();
    });
}
