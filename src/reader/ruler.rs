//! Window-local reading guide. Line indices are valid only with retained text;
//! replacement drops the state rather than reusing identities from an old PDF.

use crate::{coordinates::Point, pdf::PageText};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Ruler {
    pub(super) enabled: bool,
    pub(super) target: Option<Target>,
    pub(super) pending: Option<(usize, Point, bool)>,
}

pub(super) struct Target {
    pub(super) page: usize,
    pub(super) line: usize,
    pub(super) text: Arc<PageText>,
}

impl Ruler {
    pub(super) fn place(
        &mut self,
        page: usize,
        text: Arc<PageText>,
        at: Point,
        exact: bool,
    ) -> bool {
        let line = text
            .lines
            .iter()
            .enumerate()
            .filter_map(|(ix, _)| {
                let [left, bottom, right, top] = document_rect(&text, ix)?;
                let dx = (left - at.x).max(0.0).max(at.x - right);
                let dy = (bottom - at.y).max(0.0).max(at.y - top);
                let center_distance = at.y - (bottom + top) / 2.;
                (!exact || (dx <= 3.0 && dy <= 3.0))
                    .then_some((ix, dx * dx + center_distance * center_distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(ix, _)| ix);
        self.pending = None;
        if let Some(line) = line {
            self.target = Some(Target { page, line, text });
            true
        } else {
            false
        }
    }

    pub(super) fn step(&mut self, forward: bool) -> bool {
        let Some(target) = &mut self.target else {
            return false;
        };
        let mut next = target.line;
        loop {
            let candidate = if forward {
                next.checked_add(1)
            } else {
                next.checked_sub(1)
            };
            let Some(candidate) = candidate.filter(|ix| *ix < target.text.lines.len()) else {
                return false;
            };
            next = candidate;
            if document_rect(&target.text, next).is_some() {
                target.line = next;
                return true;
            }
        }
    }
}

pub(super) fn document_rect(text: &PageText, line: usize) -> Option<[f64; 4]> {
    let glyphs = text.glyphs.get(text.lines.get(line)?.clone())?;
    if glyphs.is_empty() || glyphs.iter().all(|glyph| glyph.text.trim().is_empty()) {
        return None;
    }
    let left = glyphs
        .iter()
        .filter(|g| !g.text.trim().is_empty())
        .map(|g| g.start.x)
        .fold(f64::INFINITY, f64::min);
    let right = glyphs
        .iter()
        .filter(|g| !g.text.trim().is_empty())
        .map(|g| g.ink_right.unwrap_or(g.end.x))
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = glyphs
        .iter()
        .map(|g| g.bottom)
        .fold(f64::INFINITY, f64::min);
    let top = glyphs
        .iter()
        .map(|g| g.top)
        .fold(f64::NEG_INFINITY, f64::max);
    [left, bottom, right, top]
        .iter()
        .all(|n| n.is_finite())
        .then_some([left, bottom, right, top])
}

impl Target {
    /// Project every corner, including cropped/rotated pages. All values are PDF
    /// page points, not interface spacing; band and marker scale with the PDF.
    pub(super) fn page_rect(&self, size: (f32, f32)) -> Option<[f64; 4]> {
        let [left, bottom, right, top] = document_rect(&self.text, self.line)?;
        let corners = [(left, bottom), (right, bottom), (left, top), (right, top)]
            .map(|(x, y)| self.text.transform.document_to_page(Point { x, y }));
        Some([
            corners
                .iter()
                .map(|p| p.x)
                .fold(f64::INFINITY, f64::min)
                .clamp(0., size.0 as f64),
            corners
                .iter()
                .map(|p| p.y)
                .fold(f64::INFINITY, f64::min)
                .clamp(0., size.1 as f64),
            corners
                .iter()
                .map(|p| p.x)
                .fold(f64::NEG_INFINITY, f64::max)
                .clamp(0., size.0 as f64),
            corners
                .iter()
                .map(|p| p.y)
                .fold(f64::NEG_INFINITY, f64::max)
                .clamp(0., size.1 as f64),
        ])
    }
}

/// Keep visible targets still. Oversized text reveals its reading edge instead
/// of alternating between two impossible-to-satisfy edges.
pub(super) fn reveal_delta(
    start: f64,
    end: f64,
    viewport_start: f64,
    viewport_end: f64,
    margin: f64,
) -> f64 {
    if end - start > viewport_end - viewport_start - margin * 2. || start < viewport_start + margin
    {
        start - viewport_start - margin
    } else if end > viewport_end - margin {
        end - viewport_end + margin
    } else {
        0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinates::PageTransform;

    fn specimen() -> Arc<PageText> {
        Arc::new(
            crate::pdf::page_text(std::path::Path::new("examples/reading-order.pdf"), 2).unwrap(),
        )
    }

    #[test]
    fn forward_reverse_follow_extracted_column_order_and_stop_without_wrapping() {
        let text = specimen();
        let mut ruler = Ruler::default();
        let first = text.glyphs[text.lines[0].start].start;
        assert!(ruler.place(2, text.clone(), first, true));
        assert!(!ruler.step(false));
        for line in 1..text.lines.len() {
            assert!(ruler.step(true));
            assert_eq!(ruler.target.as_ref().unwrap().line, line);
        }
        assert!(!ruler.step(true));
        for line in (0..text.lines.len() - 1).rev() {
            assert!(ruler.step(false));
            assert_eq!(ruler.target.as_ref().unwrap().line, line);
        }
        assert_eq!(ruler.target.as_ref().unwrap().page, 2);
    }

    #[test]
    fn placement_distinguishes_columns_and_geometry_is_clipped_after_rotation() {
        let text = specimen();
        for line in 0..text.lines.len() {
            let [left, bottom, right, top] = document_rect(&text, line).unwrap();
            let at = Point {
                x: (left + right) / 2.,
                y: (bottom + top) / 2.,
            };
            let mut ruler = Ruler::default();
            assert!(ruler.place(2, text.clone(), at, true));
            assert_eq!(ruler.target.as_ref().unwrap().line, line);
        }
        let mut text = Arc::try_unwrap(text).ok().unwrap();
        text.transform = PageTransform::from_coefficients([0., 1., 1., 0., -30., -30.]);
        let target = Target {
            page: 2,
            line: 0,
            text: Arc::new(text),
        };
        let [left, top, right, bottom] = target.page_rect((20., 30.)).unwrap();
        assert!(left >= 0. && right <= 20. && top >= 0. && bottom <= 30.);
    }

    #[test]
    fn reveal_keeps_visible_lines_still_and_handles_both_edges_and_wide_text() {
        assert_eq!(reveal_delta(30., 40., 0., 100., 10.), 0.);
        assert_eq!(reveal_delta(5., 15., 0., 100., 10.), -5.);
        assert_eq!(reveal_delta(85., 95., 0., 100., 10.), 5.);
        assert_eq!(reveal_delta(20., 200., 0., 100., 10.), 10.);
    }
}
