//! Continuous reader: geometry drives layout; only the viewport owns bitmaps.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    Context, FocusHandle, PathPromptOptions, RenderImage, ScrollHandle, Window, div, img, point,
    prelude::*, px, rgb,
};
use image::{Frame, RgbaImage};

use crate::pdf;

const GAP: f32 = 16.0;
const BITMAP_BUDGET: usize = 128 * 1024 * 1024;

fn document_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn raster_scale(zoom: f32, density: f32, (width, height): (f32, f32)) -> f32 {
    let max_scale = (32_000_000.0 / (width * height))
        .sqrt()
        .min(8192.0 / width)
        .min(8192.0 / height);
    (zoom * density).clamp(0.25, 8.0).min(max_scale)
}

/// Page-local position stays in PDF points when layout or zoom changes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Position {
    page: usize,
    y: f32,
}

fn page_top(pages: &[(f32, f32)], zoom: f32, index: usize) -> f32 {
    pages.iter().take(index).map(|&(_, h)| h * zoom + GAP).sum()
}

fn position_at(pages: &[(f32, f32)], zoom: f32, offset: f32) -> Position {
    let mut top = 0.0;
    for (index, &(_, height)) in pages.iter().enumerate() {
        let next = top + height * zoom + GAP;
        if offset < next || index + 1 == pages.len() {
            return Position {
                page: index,
                y: ((offset - top) / zoom).clamp(0.0, height),
            };
        }
        top = next;
    }
    Position { page: 0, y: 0.0 }
}

fn visible_range(pages: &[(f32, f32)], zoom: f32, top: f32, height: f32) -> std::ops::Range<usize> {
    let mut y = 0.0;
    let mut first = pages.len();
    let mut end = pages.len();
    for (index, &(_, page_height)) in pages.iter().enumerate() {
        let bottom = y + page_height * zoom;
        if bottom >= top && y <= top + height {
            first = first.min(index);
            end = index + 1;
        } else if y > top + height {
            break;
        }
        y = bottom + GAP;
    }
    first..end
}

struct Bitmap {
    image: Arc<RenderImage>,
    bytes: usize,
}

pub struct Preview {
    path: Option<PathBuf>,
    scale: f32,
    display_scale: f32,
    pages: Vec<(f32, f32)>,
    images: HashMap<usize, Bitmap>,
    // Only one rasterization runs at once; the next job is chosen from the
    // *current* viewport, rather than queued ahead of a later jump.
    in_flight: Option<(usize, u64)>,
    generation: u64,
    failed: Option<usize>,
    page_index: usize,
    last_viewport: (f32, f32),
    scroll: ScrollHandle,
    focus: FocusHandle,
    focus_initialized: bool,
    status: String,
}

impl Preview {
    pub fn new(path: Option<PathBuf>, display_scale: f32, cx: &mut Context<Self>) -> Self {
        let mut reader = Self {
            path,
            scale: 1.0,
            display_scale,
            pages: Vec::new(),
            images: HashMap::new(),
            in_flight: None,
            generation: 0,
            failed: None,
            page_index: 0,
            last_viewport: (f32::NAN, f32::NAN),
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            focus_initialized: false,
            status: "Open a PDF to start reading".into(),
        };
        if reader.path.is_some() {
            reader.status = "Loading document…".into();
            reader.load(cx);
        }
        reader
    }

    /// Replace the active document immediately, before any asynchronous work completes.
    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.generation += 1;
        self.path = Some(path);
        self.scale = 1.0;
        self.pages.clear();
        self.images.clear();
        self.in_flight = None;
        self.failed = None;
        self.page_index = 0;
        self.scroll.set_offset(point(px(0.0), px(0.0)));
        self.last_viewport = (f32::NAN, f32::NAN);
        self.status = "Loading document…".into();
        self.load(cx);
        cx.notify();
    }

    pub fn open_dialog(&mut self, cx: &mut Context<Self>) {
        let generation = self.generation;
        let choice = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open PDF".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = choice.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.open_path(path, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.status = format!("Cannot choose PDF: {error}");
                        cx.notify();
                    }
                    Err(_) => {
                        this.status = "Cannot choose PDF: file picker closed unexpectedly".into();
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let generation = self.generation;
        let work = cx
            .background_executor()
            .spawn(async move { pdf::document_pages(&path) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                match result {
                    Ok(pages) if !pages.is_empty() => {
                        self::Preview::install_pages(this, pages, cx);
                    }
                    Ok(_) => this.status = "PDF has no pages".into(),
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install_pages(&mut self, pages: Vec<(f32, f32)>, cx: &mut Context<Self>) {
        self.pages = pages;
        self.status = self.ready_status();
        self.schedule(cx);
    }

    fn ready_status(&self) -> String {
        format!(
            "Page {} of {} · {}%",
            self.page_index + 1,
            self.pages.len(),
            (self.scale * 100.0).round()
        )
    }

    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn page_index(&self) -> usize {
        self.page_index
    }
    /// Page and PDF-point offset at the top of the viewport.
    pub fn reading_position(&self) -> (usize, f32) {
        let (top, _) = self.viewport();
        let position = position_at(&self.pages, self.scale, top);
        (position.page, position.y)
    }
    pub fn page_count(&self) -> Option<usize> {
        (!self.pages.is_empty()).then_some(self.pages.len())
    }
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
    pub fn displayed_page_index(&self) -> Option<usize> {
        self.images
            .contains_key(&self.page_index)
            .then_some(self.page_index)
    }
    /// Diagnostic for asserting that sustained scrolling does not retain every page.
    pub fn retained_bitmap_bytes(&self) -> usize {
        self.images.values().map(|image| image.bytes).sum()
    }

    fn viewport(&self) -> (f32, f32) {
        let top = (-f32::from(self.scroll.offset().y)).max(0.0);
        let measured = f32::from(self.scroll.bounds().size.height);
        let height = if measured > 0.0 { measured } else { 700.0 };
        (top, height)
    }

    fn sync_viewport(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let (top, height) = self.viewport();
        self.last_viewport = (top, height);
        self.page_index = position_at(&self.pages, self.scale, top).page;
        self.schedule(cx);
        cx.notify();
    }

    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        self.select_page(self.page_index.saturating_add(1), cx);
    }
    pub fn previous_page(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.page_index.checked_sub(1) {
            self.select_page(index, cx);
        }
    }
    pub fn select_page(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.pages.len() {
            return;
        }
        self.page_index = index;
        if self.failed != Some(index) {
            self.failed = None;
        }
        self.scroll.set_offset(point(
            self.scroll.offset().x,
            px(-page_top(&self.pages, self.scale, index)),
        ));
        self.schedule(cx);
        cx.notify();
    }
    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.change_scale(1.25, cx);
    }
    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.change_scale(0.8, cx);
    }
    fn change_scale(&mut self, factor: f32, cx: &mut Context<Self>) {
        let next = (self.scale * factor).clamp(0.25, 4.0);
        if next == self.scale {
            return;
        }
        let (top, _) = self.viewport();
        let position = position_at(&self.pages, self.scale, top);
        self.scale = next;
        self.generation += 1;
        self.images.clear();
        self.in_flight = None;
        self.failed = None;
        self.scroll.set_offset(point(
            self.scroll.offset().x,
            px(-page_top(&self.pages, next, position.page) - position.y * next),
        ));
        self.schedule(cx);
        cx.notify();
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.pages.is_empty() {
            return;
        }
        let (top, height) = self.viewport();
        let range = visible_range(&self.pages, self.scale, top, height);
        if self.failed.is_some_and(|index| !range.contains(&index)) {
            self.failed = None;
        }
        // No offscreen cache: bitmap ownership and memory scale with the viewport.
        self.images.retain(|index, _| range.contains(index));
        if self.in_flight.is_some() {
            return;
        }
        let used = self.retained_bitmap_bytes();
        let index = range
            .filter(|index| !self.images.contains_key(index) && self.failed != Some(*index))
            .min_by_key(|index| (page_top(&self.pages, self.scale, *index) - top).abs() as i64);
        let Some(index) = index else {
            if self.failed.is_none() {
                self.status = self.ready_status();
            }
            return;
        };
        let dimensions = self.pages[index];
        let density = raster_scale(self.scale, self.display_scale, dimensions);
        let estimated =
            (dimensions.0 * density).ceil() as usize * (dimensions.1 * density).ceil() as usize * 4;
        if used > 0 && used.saturating_add(estimated) > BITMAP_BUDGET {
            self.status = self.ready_status();
            return;
        }
        let generation = self.generation;
        self.in_flight = Some((index, generation));
        self.status = format!(
            "Rendering page {} at {}%…",
            index + 1,
            (self.scale * 100.0).round()
        );
        let Some(path) = self.path.clone() else {
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { pdf::render_bitmap(&path, index, density) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.generation {
                    return;
                }
                this.in_flight = None;
                let (top, height) = this.viewport();
                if visible_range(&this.pages, this.scale, top, height).contains(&index) {
                    match result {
                        Ok(rendered) => {
                            let bytes = rendered.bgra.len();
                            if let Some(pixels) =
                                RgbaImage::from_raw(rendered.width, rendered.height, rendered.bgra)
                            {
                                this.images.insert(
                                    index,
                                    Bitmap {
                                        image: Arc::new(RenderImage::new(vec![Frame::new(pixels)])),
                                        bytes,
                                    },
                                );
                            } else {
                                this.status =
                                    "Cannot display page image: invalid pixel dimensions".into();
                                this.failed = Some(index);
                            }
                        }
                        Err(error) => {
                            this.status = error;
                            this.failed = Some(index);
                        }
                    }
                }
                this.schedule(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_initialized {
            window.focus(&self.focus, cx);
            self.focus_initialized = true;
        }
        let density = window.scale_factor();
        if self.display_scale != density {
            self.display_scale = density;
            self.generation += 1;
            self.images.clear();
            self.in_flight = None;
            self.schedule(cx);
        }
        if !self.pages.is_empty() && self.viewport() != self.last_viewport {
            self.sync_viewport(cx);
        }
        let (top, height) = self.viewport();
        let range = visible_range(&self.pages, self.scale, top, height);
        let top_space = page_top(&self.pages, self.scale, range.start);
        let end_space = page_top(&self.pages, self.scale, self.pages.len())
            - page_top(&self.pages, self.scale, range.end);
        let mut content = div()
            .w_full()
            .flex()
            .flex_col()
            .child(div().h(px(top_space)).flex_shrink_0());
        for index in range {
            let (width, page_height) = self.pages[index];
            let page_width = px(width * self.scale);
            let page_height = px(page_height * self.scale);
            let image = self.images.get(&index).map(|bitmap| bitmap.image.clone());
            content = content.child(
                div()
                    .w_full()
                    .min_w(page_width)
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(page_width)
                            .h(page_height)
                            .flex_shrink_0()
                            .bg(rgb(0xffffff))
                            .when_some(image, |element, image| {
                                element.child(img(image).size_full())
                            }),
                    )
                    .mb(px(GAP)),
            );
        }
        content = content.child(div().h(px(end_space)).flex_shrink_0());

        div()
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.modifiers == gpui::Modifiers::default() {
                    match event.keystroke.key.as_str() {
                        "right" | "n" => this.next_page(cx),
                        "left" | "p" => this.previous_page(cx),
                        "down" | "j" => this.scroll_by(100.0, cx),
                        "up" | "k" => this.scroll_by(-100.0, cx),
                        "=" | "+" => this.zoom_in(cx),
                        "-" => this.zoom_out(cx),
                        _ => {}
                    }
                } else if event.keystroke.modifiers
                    == (gpui::Modifiers {
                        shift: true,
                        ..Default::default()
                    })
                    && matches!(event.keystroke.key.as_str(), "=" | "+")
                {
                    this.zoom_in(cx);
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
                    .child(
                        div()
                            .id("open-pdf")
                            .cursor_pointer()
                            .px_3()
                            .bg(rgb(0x545b65))
                            .child("Open PDF")
                            .on_click(cx.listener(|this, _, _, cx| this.open_dialog(cx))),
                    )
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
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_out(cx))),
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
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap_4()
                            .child(
                                div().flex_1().min_w(px(0.0)).truncate().child(
                                    self.path
                                        .as_ref()
                                        .map(|path| document_name(path))
                                        .unwrap_or_default(),
                                ),
                            )
                            .child(div().min_w(px(0.0)).truncate().child(self.status.clone())),
                    ),
            )
            .child(
                div()
                    .id("page")
                    .flex_1()
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .track_scroll(&self.scroll)
                    .on_scroll(cx.listener(|this, _, _, cx| this.sync_viewport(cx)))
                    .p_4()
                    .child(if self.path.is_none() || self.pages.is_empty() {
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .child(self.status.clone())
                            .into_any_element()
                    } else {
                        content.into_any_element()
                    }),
            )
    }
}

impl Preview {
    pub fn scroll_by(&mut self, amount: f32, cx: &mut Context<Self>) {
        let top = (-f32::from(self.scroll.offset().y) + amount).max(0.0);
        let maximum =
            (page_top(&self.pages, self.scale, self.pages.len()) - self.viewport().1).max(0.0);
        self.scroll
            .set_offset(point(self.scroll.offset().x, px(-top.min(maximum))));
        self.sync_viewport(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_uses_filename_instead_of_parent_directories() {
        assert_eq!(
            document_name(Path::new("/long/parent/directory/report.pdf")),
            "report.pdf"
        );
        assert_eq!(
            document_name(Path::new("/long/parent/directory/very-long-file-name.pdf")),
            "very-long-file-name.pdf"
        );
    }

    #[test]
    fn display_density_changes_bitmap_resolution_not_logical_zoom() {
        assert_eq!(raster_scale(1.0, 2.0, (612.0, 792.0)), 2.0);
        assert_eq!(raster_scale(1.0, 1.0, (612.0, 792.0)), 1.0);
    }

    #[test]
    fn high_zoom_is_bounded_for_a4_scans() {
        let scale = raster_scale(4.0, 2.0, (595.44, 842.04));
        assert!(scale < 8.0);
        assert!(scale > 7.0);
        assert!(595.44 * 842.04 * scale * scale <= 32_000_000.0);
    }

    #[test]
    fn long_document_only_lays_out_visible_pages() {
        let pages = vec![(612.0, 792.0); 1000];
        assert_eq!(visible_range(&pages, 1.0, 0.0, 700.0), 0..1);
        assert_eq!(
            visible_range(&pages, 1.0, page_top(&pages, 1.0, 500), 700.0),
            500..501
        );
        assert_eq!(
            visible_range(&pages, 1.0, page_top(&pages, 1.0, 999), 700.0),
            999..1000
        );
    }
    #[test]
    fn page_local_position_survives_zoom() {
        let pages = vec![(100.0, 200.0), (150.0, 300.0)];
        let position = position_at(&pages, 1.0, 216.0 + 75.0);
        assert_eq!(position, Position { page: 1, y: 75.0 });
        assert_eq!(
            position_at(&pages, 2.0, page_top(&pages, 2.0, 1) + 150.0),
            position
        );
    }
}
