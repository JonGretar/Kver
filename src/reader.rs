//! The reader preview shared by the desktop app and visual tests.

use std::{path::PathBuf, sync::Arc};

use gpui::{Context, FocusHandle, RenderImage, Window, div, img, prelude::*, px, rgb};
use image::{Frame, RgbaImage};

use crate::pdf;

pub struct Preview {
    path: PathBuf,
    scale: f32,
    display_scale: f32,
    request: u64,
    page_index: usize,
    page_count: Option<usize>,
    focus: FocusHandle,
    focus_initialized: bool,
    status: String,
    // Last completed image (which can be the previous page while rendering).
    // Dimensions are unscaled PDF points.
    page: Option<(Arc<RenderImage>, usize, f32, f32)>,
}

fn raster_scale(zoom: f32, display_scale: f32, page_size: Option<(f32, f32)>) -> f32 {
    let requested = (zoom * display_scale).clamp(0.25, 8.0);
    let Some((width, height)) = page_size else {
        return requested;
    };
    // Match the PDF adapter's bitmap bounds. An old image remains visible
    // at logical zoom while a density-limited replacement is rendered.
    let max_scale = (32_000_000.0 / (width * height))
        .sqrt()
        .min(8192.0 / width)
        .min(8192.0 / height);
    requested.min(max_scale)
}

impl Preview {
    pub fn new(path: PathBuf, display_scale: f32, cx: &mut Context<Self>) -> Self {
        let mut preview = Self {
            path,
            scale: 1.0,
            display_scale,
            request: 0,
            page_index: 0,
            page_count: None,
            focus: cx.focus_handle(),
            focus_initialized: false,
            status: String::new(),
            page: None,
        };
        preview.request_render(cx);
        preview
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn page_index(&self) -> usize {
        self.page_index
    }

    pub fn page_count(&self) -> Option<usize> {
        self.page_count
    }

    /// The page currently on screen, which may lag behind the requested page.
    pub fn displayed_page_index(&self) -> Option<usize> {
        self.page.as_ref().map(|(_, index, _, _)| *index)
    }

    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        self.select_page(self.page_index.saturating_add(1), cx);
    }

    pub fn previous_page(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.page_index.checked_sub(1) {
            self.select_page(index, cx);
        }
    }

    /// Select a zero-based page; out-of-range requests leave the view unchanged.
    pub fn select_page(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.page_count.is_some_and(|count| index < count) && index != self.page_index {
            self.page_index = index;
            self.request_render(cx);
        }
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.change_scale(1.25, cx);
    }

    fn change_scale(&mut self, factor: f32, cx: &mut Context<Self>) {
        let next = (self.scale * factor).clamp(0.25, 4.0);
        if next != self.scale {
            self.scale = next;
            self.request_render(cx);
        }
    }

    fn request_render(&mut self, cx: &mut Context<Self>) {
        self.request += 1;
        let request = self.request;
        let path = self.path.clone();
        let scale = self.scale;
        let page_index = self.page_index;
        // Zoom determines the logical image size; display density determines
        // how many pixels we need to rasterize for that size.
        let raster_scale = raster_scale(
            scale,
            self.display_scale,
            self.page
                .as_ref()
                .filter(|(_, index, _, _)| *index == page_index)
                .map(|(_, _, width, height)| (*width, *height)),
        );
        self.status = format!(
            "Rendering page {} at {}%…",
            page_index + 1,
            (scale * 100.0).round()
        );
        cx.notify();

        // Parsing, rasterization, and conversion to display pixels happen off the UI thread.
        let work = cx
            .background_executor()
            .spawn(async move { pdf::render_bitmap(&path, page_index, raster_scale) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                // A slower earlier page or zoom must never overwrite the latest request.
                if this.request != request {
                    return;
                }
                match result {
                    Ok(rendered) => {
                        let Some(pixels) =
                            RgbaImage::from_raw(rendered.width, rendered.height, rendered.bgra)
                        else {
                            this.status =
                                "Cannot display page image: invalid pixel dimensions".into();
                            cx.notify();
                            return;
                        };
                        this.page_count = Some(rendered.page_count);
                        this.page = Some((
                            Arc::new(RenderImage::new(vec![Frame::new(pixels)])),
                            page_index,
                            rendered.page_width,
                            rendered.page_height,
                        ));
                        this.status = format!(
                            "Page {} of {} · {}%",
                            page_index + 1,
                            rendered.page_count,
                            (scale * 100.0).round()
                        );
                    }
                    Err(error) => {
                        this.status = error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_initialized {
            window.focus(&self.focus, cx);
            self.focus_initialized = true;
        }
        // A window moved between displays needs a new bitmap, but retains its
        // logical size and keeps showing the old one until rendering completes.
        let display_scale = window.scale_factor();
        if self.display_scale != display_scale {
            self.display_scale = display_scale;
            self.request_render(cx);
        }
        let page = self.page.as_ref().map(|(image, _, width, height)| {
            let page_width = px(*width * self.scale);
            div()
                .w_full()
                .min_w(page_width)
                .flex()
                .justify_center()
                .child(
                    img(image.clone())
                        .w(page_width)
                        .h(px(*height * self.scale))
                        .flex_shrink_0(),
                )
        });

        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.modifiers == gpui::Modifiers::default() {
                    match event.keystroke.key.as_str() {
                        "right" | "n" => this.next_page(cx),
                        "left" | "p" => this.previous_page(cx),
                        _ => {}
                    }
                }
            }))
            .flex()
            .flex_col()
            .bg(rgb(0x30343b))
            .text_color(rgb(0xf5f5f5))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .p_3()
                    .child(self.path.display().to_string())
                    .child(
                        div()
                            .id("previous-page")
                            .cursor_pointer()
                            .px_3()
                            .bg(rgb(0x545b65))
                            .child("←")
                            .on_click(cx.listener(|this, _, _, cx| this.previous_page(cx))),
                    )
                    .child(
                        div()
                            .id("next-page")
                            .cursor_pointer()
                            .px_3()
                            .bg(rgb(0x545b65))
                            .child("→")
                            .on_click(cx.listener(|this, _, _, cx| this.next_page(cx))),
                    )
                    .child(
                        div()
                            .id("zoom-out")
                            .cursor_pointer()
                            .px_3()
                            .bg(rgb(0x545b65))
                            .child("−")
                            .on_click(cx.listener(|this, _, _, cx| this.change_scale(0.8, cx))),
                    )
                    .child(
                        div()
                            .id("zoom-in")
                            .cursor_pointer()
                            .px_3()
                            .bg(rgb(0x545b65))
                            .child("+")
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_in(cx))),
                    )
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .id("page")
                    .flex_1()
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .p_4()
                    .when_some(page, |this, page| this.child(page)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_increases_pixels_without_changing_logical_zoom() {
        assert_eq!(raster_scale(1.0, 2.0, Some((612.0, 792.0))), 2.0);
        assert_eq!(raster_scale(1.0, 1.0, Some((612.0, 792.0))), 1.0);
    }

    #[test]
    fn high_zoom_is_bounded_for_a4_scans() {
        let scale = raster_scale(4.0, 2.0, Some((595.44, 842.04)));
        assert!(scale < 8.0);
        assert!(scale > 7.0);
        assert!(595.44 * 842.04 * scale * scale <= 32_000_000.0);
    }
}
