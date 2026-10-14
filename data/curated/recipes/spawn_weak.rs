// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// cx.spawn first arg is WeakEntity<Self>. Cannot keep &mut Context across await.
// Store the Task on the view if drop should cancel. Use background_spawn for Send work.
// Do not use Tokio unless you gpui_tokio::init(cx) and Tokio::spawn.

use gpui::{
    App, AsyncApp, Context, Render, Task, Window, WindowOptions, div, prelude::*, rgb,
};
use gpui_platform::application;

struct Loader {
    status: String,
    _load: Task<()>,
}

impl Loader {
    fn new(cx: &mut Context<Self>) -> Self {
        let load = cx.spawn(async move |this, cx: &mut AsyncApp| {
            let data = cx
                .background_spawn(async { "loaded".to_string() })
                .await;
            this.update(cx, |this, cx| {
                this.status = data;
                cx.notify();
            })
            .ok();
        });
        Self {
            status: "loading".into(),
            _load: load,
        }
    }
}

impl Render for Loader {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_4()
            .bg(rgb(0x111111))
            .text_color(rgb(0xeeeeee))
            .child(self.status.clone())
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(Loader::new)
        })
        .unwrap();
    });
}
