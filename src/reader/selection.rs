//! Document-space text selection.
//!
//! The reader supplies visible pages and executes text requests off-thread.
//! This module owns hit testing, drag state, copy/highlight derivation, and
//! retention of the selected page only. PageTexts owns shared extraction.
//! It has no window, clipboard, PDF path, or independent document identity.

use std::{ops::Range, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    coordinates::Point,
    pdf::{PageText, TextGlyph},
};

#[derive(Default)]
pub(super) struct Selection {
    text: Option<Arc<PageText>>,
    span: Option<Span>,
    drag: Option<Drag>,
}

#[derive(Clone, Copy)]
enum Granularity {
    Character,
    Word,
    Paragraph,
}

struct Drag {
    origin: Span,
    granularity: Granularity,
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Anchor {
    line: usize,
    boundary: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Span {
    page: usize,
    start: Anchor,
    end: Anchor,
}

impl Selection {
    /// Replacing a document invalidates the selected text and gesture.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Release the selected page without discarding shared cached text.
    pub(super) fn clear(&mut self) {
        self.span = None;
        self.drag = None;
        self.text = None;
    }

    pub(super) fn page(&self) -> Option<usize> {
        self.span.map(|span| span.page)
    }

    pub(super) fn dragging_page(&self) -> Option<usize> {
        self.page().filter(|_| self.drag.is_some())
    }

    pub(super) fn has_selection(&self) -> bool {
        self.span.is_some_and(|span| {
            (span.start.line, span.start.boundary) != (span.end.line, span.end.boundary)
        })
    }

    /// A miss (including unavailable text/geometry) clears the old selection.
    pub(super) fn begin(
        &mut self,
        page: usize,
        at: Option<Point>,
        text: Option<Arc<PageText>>,
        click_count: usize,
    ) {
        let granularity = match click_count {
            0 | 1 => Granularity::Character,
            2 => Granularity::Word,
            _ => Granularity::Paragraph,
        };
        self.text = text;
        self.span = at.and_then(|at| unit_span(self.text.as_ref()?, page, at, granularity));
        self.drag = self.span.map(|origin| Drag {
            origin,
            granularity,
        });
    }

    /// Extend only on the starting page. A miss preserves the last valid end.
    /// The caller notifies its view when this returns true.
    pub(super) fn extend(&mut self, at: Point) -> bool {
        let Some(drag) = &self.drag else {
            return false;
        };
        let Some(target) = self
            .text
            .as_ref()
            .and_then(|text| unit_span(text, drag.origin.page, at, drag.granularity))
        else {
            return false;
        };
        let span = Span {
            page: drag.origin.page,
            start: drag.origin.start.min(target.start),
            end: drag.origin.end.max(target.end),
        };
        let changed = self.span != Some(span);
        self.span = Some(span);
        changed
    }

    pub(super) fn finish(&mut self) {
        self.drag = None;
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        let span = self.span?;
        selected_text(span, self.text.as_ref()?)
    }

    /// Capture the selected line fragments before any crop, rotation, zoom, or
    /// viewport projection. These are the same glyph envelopes shown by the
    /// selection, not rectangles reconstructed from screen-space bounds.
    /// Vertex order is upper-left/right, lower-left/right in document space.
    pub(super) fn document_quads(&self) -> Vec<[Point; 4]> {
        let Some(span) = self.span else {
            return Vec::new();
        };
        let Some(text) = self.text.as_ref() else {
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
            .zip(self.text.as_ref());
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
            if paragraph_gap(last, first) {
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

fn unit_span(text: &PageText, page: usize, at: Point, granularity: Granularity) -> Option<Span> {
    let anchor = hit_boundary(text, at)?;
    let mut span = Span {
        page,
        start: anchor,
        end: anchor,
    };
    match granularity {
        Granularity::Character => {}
        Granularity::Word => {
            let line = &text.lines[anchor.line];
            // Use the glyph under the pointer, not the nearest caret boundary:
            // the right half of a word's last letter still belongs to that word.
            let index = line
                .clone()
                .find(|&index| {
                    let glyph = &text.glyphs[index];
                    at.x >= glyph.start.x && at.x < glyph.end.x
                })
                .or_else(|| {
                    line.clone().min_by(|&a, &b| {
                        let distance = |index: usize| {
                            let glyph = &text.glyphs[index];
                            (glyph.start.x - at.x).max(at.x - glyph.end.x).max(0.0)
                        };
                        distance(a).total_cmp(&distance(b))
                    })
                })?;
            let contents: String = text.glyphs[line.clone()]
                .iter()
                .map(|glyph| glyph.text.as_str())
                .collect();
            let offset: usize = text.glyphs[line.start..index]
                .iter()
                .map(|glyph| glyph.text.len())
                .sum();
            let (start, word) = contents
                .split_word_bound_indices()
                .find(|(start, word)| *start <= offset && offset < start + word.len())?;
            let end = start + word.len();
            let mut byte = 0;
            for (index, glyph) in text.glyphs[line.clone()].iter().enumerate() {
                let next = byte + glyph.text.len();
                if byte <= start && start < next {
                    span.start.boundary = line.start + index;
                }
                if byte < end && end <= next {
                    span.end.boundary = line.start + index + 1;
                }
                byte = next;
            }
        }
        Granularity::Paragraph => {
            while span.start.line > 0 && paragraph_continues(text, span.start.line - 1) {
                span.start.line -= 1;
            }
            while span.end.line + 1 < text.lines.len() && paragraph_continues(text, span.end.line) {
                span.end.line += 1;
            }
            span.start.boundary = text.lines[span.start.line].start;
            span.end.boundary = text.lines[span.end.line].end;
        }
    }
    Some(span)
}

fn paragraph_continues(text: &PageText, line: usize) -> bool {
    let previous = &text.glyphs[text.lines[line].clone()];
    let next = &text.glyphs[text.lines[line + 1].clone()];
    let (Some(first), Some(last), Some(next_first), Some(next_last)) =
        (previous.first(), previous.last(), next.first(), next.last())
    else {
        return false;
    };
    // PDFs need not carry semantic paragraphs. Match copy's existing 1.7-line
    // gap heuristic, but never merge a same-baseline fragment or a new column.
    let descent = first.start.y - next_first.start.y;
    descent > 3.0
        && !paragraph_gap(last, next_first)
        && first.start.x < next_last.end.x
        && next_first.start.x < last.end.x
}

fn paragraph_gap(previous: &TextGlyph, next: &TextGlyph) -> bool {
    (previous.start.y - next.start.y).abs() > (previous.top - previous.bottom) * 1.7
}

#[cfg(test)]
mod tests;
