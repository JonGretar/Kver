//! Page layout and navigation coordinates, independent of GPUI and the PDF backend.

use std::ops::Range;

use crate::coordinates::{PageTransform, PageViewport, Point};

pub(super) const GAP: f32 = 16.0;
pub(super) const PAGE_PADDING: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomMode {
    Fixed,
    FitWidth,
    FitPage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadingMode {
    #[default]
    Continuous,
    SinglePage,
}

/// A borrowed projection of the reader's live geometry, never a second viewport.
/// The reader supplies a valid current page when the document is nonempty.
pub(super) struct PageLayout<'a> {
    pages: &'a [(f32, f32)],
    zoom: f32,
    mode: ReadingMode,
    current_page: usize,
}

/// Fit target and the document point to hold steady during a viewport update.
#[non_exhaustive]
pub(super) struct Refit {
    pub page: usize,
    pub selected_page: Option<usize>,
    pub position: Position,
    pub anchor: f32,
}

impl<'a> PageLayout<'a> {
    pub(super) fn new(
        pages: &'a [(f32, f32)],
        zoom: f32,
        mode: ReadingMode,
        current_page: usize,
    ) -> Self {
        Self {
            pages,
            zoom,
            mode,
            current_page,
        }
    }

    pub(super) fn page_dimensions(&self, page: usize) -> Option<(f32, f32)> {
        self.pages.get(page).copied()
    }

    pub(super) fn scale(&self) -> f32 {
        self.zoom
    }

    pub(super) fn page_top(&self, page: usize) -> f32 {
        match self.mode {
            ReadingMode::Continuous => page_top(self.pages, self.zoom, page),
            ReadingMode::SinglePage => 0.0,
        }
    }

    pub(super) fn position_at(&self, top: f32) -> Position {
        if self.pages.is_empty() {
            return Position { page: 0, y: 0.0 };
        }
        match self.mode {
            ReadingMode::Continuous => position_at(self.pages, self.zoom, top),
            ReadingMode::SinglePage => Position {
                page: self.current_page,
                y: (top / self.zoom).clamp(0.0, self.pages[self.current_page].1),
            },
        }
    }

    pub(super) fn visible_pages(&self, top: f32, height: f32) -> Range<usize> {
        if self.pages.is_empty() {
            return 0..0;
        }
        match self.mode {
            ReadingMode::Continuous => visible_range(self.pages, self.zoom, top, height),
            ReadingMode::SinglePage => self.current_page..self.current_page + 1,
        }
    }

    pub(super) fn current_page_at(&self, top: f32, height: f32) -> usize {
        if self.pages.is_empty() {
            return 0;
        }
        match self.mode {
            ReadingMode::Continuous => current_page_at(self.pages, self.zoom, top, height),
            ReadingMode::SinglePage => self.current_page,
        }
    }

    fn maximum_scroll(&self, height: f32) -> f32 {
        if self.pages.is_empty() {
            return 0.0;
        }
        match self.mode {
            ReadingMode::Continuous => {
                (page_top(self.pages, self.zoom, self.pages.len()) - height).max(0.0)
            }
            ReadingMode::SinglePage => {
                (self.pages[self.current_page].1 * self.zoom + GAP + PAGE_PADDING * 2.0 - height)
                    .max(0.0)
            }
        }
    }

    /// Horizontal maximum is resolved by GPUI after layout; vertical bounds
    /// must already reflect a newly selected Single Page before its next frame.
    pub(super) fn clamp_scroll(&self, x: f32, top: f32, height: f32) -> (f32, f32) {
        (x.max(0.0), top.clamp(0.0, self.maximum_scroll(height)))
    }

    pub(super) fn offset_for(&self, position: Position, anchor: f32) -> f32 {
        self.page_top(position.page) + position.y * self.zoom - anchor
    }

    /// Logical spacers around the materialized pages; Single Page has neither.
    pub(super) fn spacers(&self, visible: Range<usize>) -> (f32, f32) {
        (
            self.page_top(visible.start),
            self.page_top(self.pages.len()) - self.page_top(visible.end),
        )
    }

    pub(super) fn fit_scale(
        &self,
        mode: ZoomMode,
        page: usize,
        viewport: (f32, f32),
    ) -> Option<f32> {
        fit_scale(mode, *self.pages.get(page)?, viewport)
    }

    /// Keep explicit page turns current while visible. When scrolling changes
    /// the fit target, anchor the center so refitting cannot jump back across
    /// the same page boundary. Unchanged targets and explicit turns keep the top.
    pub(super) fn refit(&self, top: f32, height: f32, selected_page: Option<usize>) -> Refit {
        let visible = self.visible_pages(top, height);
        let selected_page = selected_page.filter(|page| visible.contains(page));
        let page = if self.mode == ReadingMode::SinglePage {
            self.current_page_at(top, height)
        } else {
            selected_page.unwrap_or_else(|| self.current_page_at(top, height))
        };
        let anchor = if selected_page.is_none() && page != self.current_page {
            height / 2.0
        } else {
            0.0
        };
        Refit {
            page,
            selected_page,
            position: self.position_at(top + anchor),
            anchor,
        }
    }

    /// Deliberate turns and a short final page remain the saved reading place
    /// even when the viewport cannot scroll far enough to put that page on top.
    pub(super) fn position_to_save(
        &self,
        top: f32,
        height: f32,
        selected_page: Option<usize>,
    ) -> Position {
        let position = self.position_at(top);
        if let Some(page) = selected_page
            && page != position.page
        {
            return Position { page, y: 0.0 };
        }
        if self.mode == ReadingMode::Continuous
            && selected_page.is_none()
            && !self.pages.is_empty()
            && self.current_page == self.pages.len() - 1
            && self.current_page != position.page
            && self.maximum_scroll(height) < self.page_top(self.current_page)
        {
            return Position {
                page: self.current_page,
                y: 0.0,
            };
        }
        position
    }

    /// Center a page that fits; wide pages start at the content inset and scroll.
    /// Inputs are measured window logical points, not display-density pixels.
    pub(super) fn page_viewport(
        &self,
        page: usize,
        transform: PageTransform,
        origin: Point,
        viewport_width: f32,
        scroll: Point,
    ) -> Option<PageViewport> {
        let width = self.pages.get(page)?.0 * self.zoom;
        let inner_width = (viewport_width - PAGE_PADDING * 2.0).max(0.0);
        let origin = Point {
            x: origin.x + PAGE_PADDING as f64 + ((inner_width - width).max(0.0) / 2.0) as f64,
            y: origin.y + PAGE_PADDING as f64 + self.page_top(page) as f64,
        };
        PageViewport::new(transform, self.zoom as f64, origin, scroll)
    }
}

fn fit_scale(mode: ZoomMode, page: (f32, f32), viewport: (f32, f32)) -> Option<f32> {
    let (width, height) = viewport;
    if width <= PAGE_PADDING * 2.0 || height <= PAGE_PADDING * 2.0 {
        return None;
    }
    let width_scale = (width - PAGE_PADDING * 2.0) / page.0;
    let scale = match mode {
        ZoomMode::Fixed => return None,
        ZoomMode::FitWidth => width_scale,
        ZoomMode::FitPage => width_scale.min((height - PAGE_PADDING * 2.0) / page.1),
    };
    Some(scale.clamp(0.25, 4.0))
}

/// Page-local position stays in PDF points when layout or zoom changes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub(super) struct Position {
    pub page: usize,
    pub y: f32,
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

fn current_page_at(pages: &[(f32, f32)], zoom: f32, top: f32, height: f32) -> usize {
    let last = pages.len() - 1;
    let document_end = page_top(pages, zoom, pages.len());
    // A short final page cannot be scrolled to the top; reaching the end
    // must still make it the current page.
    if top + height >= document_end - 1.0 {
        last
    } else {
        position_at(pages, zoom, top + height / 2.0).page
    }
}

#[cfg(test)]
mod tests;
