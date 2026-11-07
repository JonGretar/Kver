//! Viewport-owned bitmaps and one-current-job raster scheduling.
//!
//! The view executes requests through the PDF adapter and returns replies here.
//! Invalidating document/zoom/density inputs retires the current request; an
//! already-running backend call may finish, but cannot install its old result.
//! There is no prefetch queue or offscreen bitmap cache.

use std::{collections::HashMap, ops::Range, sync::Arc};

use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};

use super::navigation::PageLayout;
use crate::pdf::RenderedBitmap;

pub(super) const BITMAP_BUDGET: usize = 128 * 1024 * 1024;

#[derive(Default)]
pub(super) struct PageRasters {
    images: HashMap<usize, Bitmap>,
    in_flight: Option<RasterRequest>,
    next_request: u64,
    failed: Option<usize>,
}

struct Bitmap {
    image: Arc<RenderImage>,
    bytes: usize,
}

/// An opaque ticket, unique even if the same page and scale are requested again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RasterRequest {
    id: u64,
    page: usize,
    scale: f32,
}

impl RasterRequest {
    pub(super) fn page(self) -> usize {
        self.page
    }

    pub(super) fn scale(self) -> f32 {
        self.scale
    }
}

pub(super) enum Schedule {
    /// A job is current, or the visible failure must retain its existing status.
    Unchanged,
    /// No more work fits the viewport/budget. Unrendered pages stay placeholders.
    Ready,
    Render(RasterRequest),
}

pub(super) enum Completion {
    Obsolete,
    /// A current reply; offscreen pixels are discarded without conversion.
    Accepted,
    Failed(String),
}

impl PageRasters {
    /// A document, zoom, or density change retires all cached work and failures.
    pub(super) fn invalidate(&mut self) {
        self.images.clear();
        self.in_flight = None;
        self.failed = None;
    }

    /// Preserve the explicit-turn retry policy, including when a previously
    /// failed neighbor remains visible beside a newly selected short page.
    pub(super) fn on_page_selected(&mut self, page: usize) {
        if self.failed != Some(page) {
            self.failed = None;
        }
    }

    pub(super) fn image(&self, page: usize) -> Option<&Arc<RenderImage>> {
        self.images.get(&page).map(|bitmap| &bitmap.image)
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.images.values().map(|bitmap| bitmap.bytes).sum()
    }

    #[cfg(test)]
    pub(super) fn cached_pages(&self) -> impl Iterator<Item = usize> + '_ {
        self.images.keys().copied()
    }

    pub(super) fn schedule(
        &mut self,
        layout: &PageLayout<'_>,
        top: f32,
        height: f32,
        display_scale: f32,
    ) -> Schedule {
        let visible = layout.visible_pages(top, height);
        if self.failed.is_some_and(|page| !visible.contains(&page)) {
            self.failed = None;
        }
        // Eviction still happens while a current job is running.
        self.images.retain(|page, _| visible.contains(page));
        if self.in_flight.is_some() {
            return Schedule::Unchanged;
        }
        let page = visible
            .filter(|page| !self.images.contains_key(page) && self.failed != Some(*page))
            .min_by_key(|page| (layout.page_top(*page) - top).abs() as i64);
        let Some(page) = page else {
            return if self.failed.is_none() {
                Schedule::Ready
            } else {
                Schedule::Unchanged
            };
        };
        let Some(dimensions) = layout.page_dimensions(page) else {
            return Schedule::Ready;
        };
        let scale = raster_scale(layout.scale(), display_scale, dimensions);
        let estimated =
            (dimensions.0 * scale).ceil() as usize * (dimensions.1 * scale).ceil() as usize * 4;
        let used = self.retained_bytes();
        if used > 0 && used.saturating_add(estimated) > BITMAP_BUDGET {
            return Schedule::Ready;
        }
        self.next_request += 1;
        let request = RasterRequest {
            id: self.next_request,
            page,
            scale,
        };
        self.in_flight = Some(request);
        Schedule::Render(request)
    }

    pub(super) fn finish(
        &mut self,
        request: RasterRequest,
        result: Result<RenderedBitmap, String>,
        visible: Range<usize>,
    ) -> Completion {
        if self.in_flight != Some(request) {
            return Completion::Obsolete;
        }
        self.in_flight = None;
        if !visible.contains(&request.page) {
            return Completion::Accepted;
        }
        let result = result.and_then(|rendered| {
            let bytes = rendered.bgra.len();
            let pixels = RgbaImage::from_raw(rendered.width, rendered.height, rendered.bgra)
                .ok_or_else(|| "Cannot display page image: invalid pixel dimensions".to_owned())?;
            Ok(Bitmap {
                image: Arc::new(RenderImage::new(vec![Frame::new(pixels)])),
                bytes,
            })
        });
        match result {
            Ok(bitmap) => {
                self.images.insert(request.page, bitmap);
                Completion::Accepted
            }
            Err(error) => {
                self.failed = Some(request.page);
                Completion::Failed(error)
            }
        }
    }
}

fn raster_scale(zoom: f32, density: f32, (width, height): (f32, f32)) -> f32 {
    let max_scale = (32_000_000.0 / (width * height))
        .sqrt()
        .min(8192.0 / width)
        .min(8192.0 / height);
    (zoom * density).clamp(0.25, 8.0).min(max_scale)
}

#[cfg(test)]
mod tests;
