// gpui rev: d9ad6aff67e47de43abb270d22de75dd950f1b48
// Custom Element: request_layout → prepaint (hitbox) → paint (quads/paths + on_mouse_event).
// Node graphs: ONE Element, not a div per wire. Zed painting.rs uses canvas() — doodles only.

use gpui::{
    relative, px, quad, point, size, App, Bounds, BorderStyle, Context, CursorStyle, Element,
    ElementId, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement,
    LayoutId, MouseDownEvent, MouseMoveEvent, PathBuilder, Pixels, Render, Style, Window,
    WindowOptions,
};
use gpui_platform::application;

struct GraphView;

impl Render for GraphView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        GraphCanvas
    }
}

struct GraphCanvas;

impl IntoElement for GraphCanvas {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for GraphCanvas {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name("graph-canvas".into()))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size = size(relative(1.).into(), relative(1.).into());
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let hitbox = prepaint.clone();

        window.paint_quad(quad(
            bounds,
            px(0.),
            gpui::rgb(0x0b0b0b),
            px(0.),
            gpui::rgb(0x2a2a2a),
            BorderStyle::Solid,
        ));

        // Node as a quad (not a nested div).
        let card = Bounds::new(
            point(bounds.origin.x + px(80.), bounds.origin.y + px(80.)),
            size(px(160.), px(56.)),
        );
        window.paint_quad(quad(
            card,
            px(5.),
            gpui::rgb(0x161616),
            px(1.),
            gpui::rgb(0x3d3d3d),
            BorderStyle::Solid,
        ));

        // Wire as a stroked path in the same paint pass.
        let mut builder = PathBuilder::stroke(px(2.));
        builder.move_to(point(card.origin.x, card.origin.y + px(28.)));
        builder.line_to(point(card.origin.x - px(60.), card.origin.y + px(28.)));
        if let Ok(path) = builder.build() {
            window.paint_path(path, gpui::rgb(0x888888));
        }

        if hitbox.is_hovered(window) {
            window.set_cursor_style(CursorStyle::OpenHand, &hitbox);
        }

        // on_mouse_event is paint-phase only.
        window.on_mouse_event({
            let hitbox = hitbox.clone();
            move |ev: &MouseDownEvent, phase, window, _cx| {
                if phase.bubble() && hitbox.is_hovered(window) {
                    let _ = ev.position;
                }
            }
        });
        window.on_mouse_event({
            let hitbox = hitbox.clone();
            move |_ev: &MouseMoveEvent, phase, window, _cx| {
                let _ = (phase, window, hitbox.is_hovered(window));
            }
        });
    }
}

fn main() {
    application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_| GraphView)
        })
        .unwrap();
    });
}
