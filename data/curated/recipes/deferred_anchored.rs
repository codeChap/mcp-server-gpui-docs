// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// In-window overlay: deferred(anchored().child(...)). Not a new Window.
// OS popups are WindowKind::AnchoredPopup — different API.

use gpui::{
    Anchor, App, Context, Render, Window, WindowOptions, anchored, deferred, div, prelude::*, px,
    rgb,
};
use gpui_platform::application;

struct Menu {
    open: bool,
}

impl Render for Menu {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_8()
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .id("open")
                    .px_3()
                    .py_1()
                    .bg(rgb(0x334155))
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .child("open overlay")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    }))
                    .when(self.open, |this| {
                        this.child(
                            deferred(
                                anchored()
                                    .anchor(Anchor::TopLeft)
                                    .snap_to_window_with_margin(px(8.))
                                    .child(
                                        div()
                                            .p_3()
                                            .bg(rgb(0xffffff))
                                            .text_color(rgb(0x111111))
                                            .border_1()
                                            .child("overlay")
                                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                                this.open = false;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .priority(1),
                        )
                    }),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_| Menu { open: false })
        })
        .unwrap();
    });
}
