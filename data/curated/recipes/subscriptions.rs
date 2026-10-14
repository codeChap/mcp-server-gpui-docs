// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// Keep Subscriptions on the view (drop = unsubscribe).
// observe = any cx.notify on that entity. subscribe = typed EventEmitter.
// .detach() only for process-lifetime listeners (e.g. in application().run).

use gpui::{
    App, Context, Entity, EventEmitter, Render, Subscription, Window, WindowOptions, div,
    prelude::*, rgb,
};
use gpui_platform::application;

struct Source {
    n: i32,
}

struct Changed;

impl EventEmitter<Changed> for Source {}

struct Mirror {
    source: Entity<Source>,
    n: i32,
    events: u32,
    _subs: Vec<Subscription>,
}

impl Mirror {
    fn new(source: Entity<Source>, cx: &mut Context<Self>) -> Self {
        let n = source.read(cx).n;
        let mut _subs = Vec::new();
        _subs.push(cx.observe(&source, |this, src, cx| {
            this.n = src.read(cx).n;
            cx.notify();
        }));
        _subs.push(cx.subscribe(&source, |this, _src, _: &Changed, cx| {
            this.events += 1;
            cx.notify();
        }));
        Self {
            source,
            n,
            events: 0,
            _subs,
        }
    }
}

impl Render for Mirror {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .child(format!("n {}  events {}", self.n, self.events))
            .child(
                div()
                    .id("bump")
                    .px_3()
                    .py_1()
                    .bg(rgb(0x334155))
                    .cursor_pointer()
                    .child("bump source")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.source.update(cx, |src, cx| {
                            src.n += 1;
                            cx.notify();
                            cx.emit(Changed);
                        });
                    })),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let source = cx.new(|_| Source { n: 1 });
        cx.open_window(WindowOptions::default(), |_window, cx| {
            let source = source.clone();
            cx.new(|cx| Mirror::new(source, cx))
        })
        .unwrap();
    });
}
