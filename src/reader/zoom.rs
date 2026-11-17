//! Pointer zoom uses the same document projection as selection and links.

use super::*;
use gpui_kit::{HitboxBehavior, ScrollDelta, ScrollEvent, ScrollSource, canvas};

impl ReaderView {
    pub(super) fn zoom_at(
        &mut self,
        factor: f32,
        pointer: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.pages.is_empty() || !factor.is_finite() || factor <= 0.0 || factor == 1.0 {
            return;
        }
        let next = (self.scale * factor).clamp(0.25, 4.0);
        if next == self.scale {
            return;
        }
        let (top, _, _) = self.viewport();
        let anchor = f32::from(pointer.y - self.scroll.bounds().origin.y) - self.page_padding;
        let position = self.layout().position_at(top + anchor);
        let Some(document_point) = self.pointer_document(position.page, pointer) else {
            return;
        };
        self.invalidate_hints(cx);
        self.position_touched = true;
        self.zoom_mode = ZoomMode::Fixed;
        self.set_scale(next, position, anchor);
        // Reproject after scaling, including horizontal centering when a page
        // crosses the viewport width. Scroll limits may constrain the anchor.
        if let Some(viewport) = self.page_viewport(position.page) {
            let projected = viewport.document_to_screen(document_point);
            self.scroll_to(
                -f32::from(self.scroll.offset().x) + projected.x as f32 - f32::from(pointer.x),
                self.viewport().0 + projected.y as f32 - f32::from(pointer.y),
            );
        }
        self.sync_viewport(cx);
    }

    pub(super) fn wheel_zoom_input(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let scroll = self.scroll.clone();
        let reader = cx.entity().downgrade();
        // Div's built-in scrolling runs before its on_scroll callback. A
        // non-occluding child hitbox handles zoom first, without replacing
        // ordinary scrolling or stealing clicks from links and selection.
        canvas(
            move |_, window, _| window.insert_hitbox(scroll.bounds(), HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                    if phase.bubble()
                        && hitbox.should_handle_scroll(window)
                        && wheel_zoom_modifier(event)
                    {
                        let factor = match event.delta {
                            ScrollDelta::Lines(delta) => 1.25_f32.powf(delta.y),
                            ScrollDelta::Pixels(delta) => (f32::from(delta.y) * 0.005).exp(),
                        };
                        let _ = reader.update(cx, |reader, cx| {
                            reader.zoom_at(factor, event.position, cx);
                        });
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
    }
}

fn wheel_zoom_modifier(event: &ScrollEvent) -> bool {
    let modifiers = event.modifiers;
    event.source == ScrollSource::Pointer
        && !modifiers.alt
        && !modifiers.shift
        && if cfg!(target_os = "macos") {
            modifiers.platform && !modifiers.control
        } else {
            modifiers.control && !modifiers.platform
        }
}
