//! Document-space text selection and the page text it needs.
//!
//! The reader supplies visible pages and executes text requests off-thread.
//! This module owns hit testing, drag state, copy/highlight derivation, and
//! retention: visible pages plus the selected page, with one current request.
//! It has no window, clipboard, PDF path, or independent document identity.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use crate::{
    coordinates::{PageTransform, Point},
    pdf::{PageText, TextGlyph},
};

#[derive(Default)]
pub(super) struct Selection {
    text: HashMap<usize, PageText>,
    failed: HashSet<usize>,
    in_flight: Option<TextRequest>,
    span: Option<Span>,
    dragging: bool,
}

/// Carries the reader's document generation, not its zoom/raster generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextRequest {
    page: usize,
    generation: u64,
}

impl TextRequest {
    pub(super) fn page(self) -> usize {
        self.page
    }
}

/// Page-local PDF points, after crop/rotation and before logical zoom.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub(super) struct Highlight {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// Boundary indices refer to visually ordered document-space glyphs, not pixels.
#[derive(Clone, Copy)]
struct Anchor {
    line: usize,
    boundary: usize,
}

#[derive(Clone, Copy)]
struct Span {
    page: usize,
    start: Anchor,
    end: Anchor,
}

impl Selection {
    /// Replacing a document invalidates both selected text and pending replies.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Clear a gesture/selection without discarding this document's page text.
    pub(super) fn clear(&mut self) {
        self.span = None;
        self.dragging = false;
    }

    pub(super) fn page(&self) -> Option<usize> {
        self.span.map(|span| span.page)
    }

    pub(super) fn dragging_page(&self) -> Option<usize> {
        self.page().filter(|_| self.dragging)
    }

    pub(super) fn page_transform(&self, page: usize) -> Option<PageTransform> {
        self.text.get(&page).map(|text| text.transform)
    }

    /// A miss (including unavailable text/geometry) clears the old selection.
    pub(super) fn begin(&mut self, page: usize, at: Option<Point>) {
        self.span = at
            .and_then(|at| hit_boundary(self.text.get(&page)?, at))
            .map(|anchor| Span {
                page,
                start: anchor,
                end: anchor,
            });
        self.dragging = self.span.is_some();
    }

    /// Extend only on the starting page. A miss preserves the last valid end.
    /// The caller notifies its view when this returns true.
    pub(super) fn extend(&mut self, at: Point) -> bool {
        let Some(page) = self.dragging_page() else {
            return false;
        };
        let Some(anchor) = self.text.get(&page).and_then(|text| hit_boundary(text, at)) else {
            return false;
        };
        if let Some(span) = &mut self.span {
            span.end = anchor;
        }
        true
    }

    pub(super) fn finish(&mut self) {
        self.dragging = false;
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        let span = self.span?;
        selected_text(span, self.text.get(&span.page)?)
    }

    /// Capture the selected line fragments before any crop, rotation, zoom, or
    /// viewport projection. These are the same glyph envelopes shown by the
    /// selection, not rectangles reconstructed from screen-space bounds.
    /// Vertex order is upper-left/right, lower-left/right in document space.
    pub(super) fn document_quads(&self) -> Vec<[Point; 4]> {
        let Some(span) = self.span else {
            return Vec::new();
        };
        let Some(text) = self.text.get(&span.page) else {
            return Vec::new();
        };
        selected_ranges(span, text)
            .into_iter()
            .map(|range| selection_quad(&text.glyphs[range]))
            .collect()
    }

    pub(super) fn highlights(
        &self,
        page: usize,
        (width, height): (f32, f32),
    ) -> impl Iterator<Item = Highlight> + '_ {
        let selected = self
            .span
            .filter(|span| span.page == page)
            .and_then(|span| self.text.get(&page).map(|text| (span, text)));
        selected.into_iter().flat_map(move |(span, text)| {
            selected_ranges(span, text).into_iter().map(move |range| {
                let corners = selection_quad(&text.glyphs[range])
                    .map(|corner| text.transform.document_to_page(corner));
                let left = corners
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::INFINITY, f64::min)
                    .clamp(0.0, width as f64);
                let right = corners
                    .iter()
                    .map(|point| point.x)
                    .fold(f64::NEG_INFINITY, f64::max)
                    .clamp(0.0, width as f64);
                let top = corners
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::INFINITY, f64::min)
                    .clamp(0.0, height as f64);
                let bottom = corners
                    .iter()
                    .map(|point| point.y)
                    .fold(f64::NEG_INFINITY, f64::max)
                    .clamp(0.0, height as f64);
                Highlight {
                    left,
                    top,
                    width: right - left,
                    height: bottom - top,
                }
            })
        })
    }

    /// Choose from the current viewport, not a queue captured before a scroll.
    /// Retain the selected page so copying still works after scrolling away.
    pub(super) fn request_text(
        &mut self,
        mut visible: Range<usize>,
        generation: u64,
    ) -> Option<TextRequest> {
        if self.in_flight.is_some() {
            return None;
        }
        let selected = self.page();
        self.text
            .retain(|page, _| visible.contains(page) || selected == Some(*page));
        let page =
            visible.find(|page| !self.text.contains_key(page) && !self.failed.contains(page))?;
        let request = TextRequest { page, generation };
        self.in_flight = Some(request);
        Some(request)
    }

    /// An old document's reply cannot install text or clear a newer request.
    /// Failed pages are not retried until document replacement.
    pub(super) fn finish_text(
        &mut self,
        request: TextRequest,
        result: Result<PageText, String>,
    ) -> bool {
        if self.in_flight != Some(request) {
            return false;
        }
        self.in_flight = None;
        match result {
            Ok(text) => {
                self.text.insert(request.page, text);
            }
            Err(_) => {
                self.failed.insert(request.page);
            }
        }
        true
    }
}

fn selection_quad(glyphs: &[TextGlyph]) -> [Point; 4] {
    // Ranges are nonempty by construction. Keep each line/column fragment
    // separate; a union over the whole selection would cover column gutters.
    let first = &glyphs[0];
    let last = &glyphs[glyphs.len() - 1];
    let top = glyphs
        .iter()
        .map(|glyph| glyph.top)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = glyphs
        .iter()
        .map(|glyph| glyph.bottom)
        .fold(f64::INFINITY, f64::min);
    [
        Point {
            x: first.start.x,
            y: top,
        },
        Point {
            x: last.end.x,
            y: top,
        },
        Point {
            x: first.start.x,
            y: bottom,
        },
        Point {
            x: last.end.x,
            y: bottom,
        },
    ]
}

fn selected_ranges(span: Span, text: &PageText) -> Vec<Range<usize>> {
    let (start, end) =
        if (span.start.line, span.start.boundary) <= (span.end.line, span.end.boundary) {
            (span.start, span.end)
        } else {
            (span.end, span.start)
        };
    (start.line..=end.line)
        .filter_map(|line| {
            let range = text.lines.get(line)?;
            let from = if line == start.line {
                start.boundary
            } else {
                range.start
            };
            let to = if line == end.line {
                end.boundary
            } else {
                range.end
            };
            (from < to && from >= range.start && to <= range.end).then_some(from..to)
        })
        .collect()
}

fn selected_text(span: Span, text: &PageText) -> Option<String> {
    let ranges = selected_ranges(span, text);
    let mut result = String::new();
    let mut previous: Option<&TextGlyph> = None;
    for range in ranges {
        let first = text.glyphs.get(range.start)?;
        if let Some(last) = previous {
            result.push('\n');
            let height = last.top - last.bottom;
            if (last.start.y - first.start.y).abs() > height * 1.7 {
                result.push('\n');
            }
        }
        for glyph in &text.glyphs[range.clone()] {
            result.push_str(&glyph.text);
        }
        previous = text.glyphs.get(range.end - 1);
    }
    (!result.is_empty()).then_some(result)
}

fn hit_boundary(text: &PageText, at: Point) -> Option<Anchor> {
    text.lines.iter().enumerate().find_map(|(line, range)| {
        text.glyphs[range.clone()]
            .iter()
            .enumerate()
            .filter(|(_, glyph)| {
                at.y >= glyph.bottom - 3.0
                    && at.y <= glyph.top + 3.0
                    && at.x >= glyph.start.x - 8.0
                    && at.x <= glyph.end.x + 8.0
            })
            .flat_map(|(offset, glyph)| {
                [
                    (range.start + offset, glyph.start),
                    (range.start + offset + 1, glyph.end),
                ]
            })
            .min_by(|(_, a), (_, b)| (a.x - at.x).abs().total_cmp(&(b.x - at.x).abs()))
            .map(|(boundary, _)| Anchor { line, boundary })
    })
}

#[cfg(test)]
mod tests;
