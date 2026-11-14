//! Window-local reading guide. Line indices are valid only with retained text;
//! reload resolves a geometric anchor rather than reusing old line identities.

use crate::{coordinates::Point, pdf::PageText};
use std::sync::Arc;

pub(super) mod furniture;

#[derive(Default)]
pub(super) struct Ruler {
    pub(super) enabled: bool,
    pub(super) target: Option<Target>,
    pub(super) pending: Option<(usize, Placement)>,
    pub(super) traversal: Option<Traversal>,
    pub(super) notice: Option<&'static str>,
    pub(super) browse_task: Option<gpui_kit::Task<()>>,
    pub(super) evidence: furniture::Evidence,
}

#[derive(Clone, Copy)]
pub(super) enum Placement {
    /// Deliberate page turns use reading order, not distance to the old viewport.
    First,
    /// Displayed page coordinates, resolved after extraction supplies its transform.
    Page(Point),
    ExactPage(Point),
    /// Identifiable destination text (such as search) in PDF document coordinates.
    Document(Point),
    /// Reopening resolves against current raw geometry, never traversal indices.
    Remembered(Point),
    /// Refresh retained text after reload without creating a new movement.
    Reload(Point),
}

#[derive(Clone, Copy)]
pub(super) struct Traversal {
    page: usize,
    forward: bool,
    remaining: usize,
    enter: bool,
}

pub(super) struct Target {
    pub(super) page: usize,
    pub(super) line: usize,
    pub(super) text: Arc<PageText>,
}

impl Ruler {
    pub(super) fn cancel_work(&mut self) {
        self.pending = None;
        self.traversal = None;
        self.notice = None;
        self.browse_task = None;
    }

    pub(super) fn text_priority(&self) -> Option<usize> {
        self.traversal
            .map(|work| work.page)
            .or_else(|| self.pending.map(|(page, _)| page))
    }

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
            if document_rect(&target.text, next).is_some()
                && !self.evidence.excludes(target.page, next)
            {
                target.line = next;
                return true;
            }
        }
    }
}

fn adjacent(page: usize, forward: bool, count: usize) -> Option<usize> {
    if forward {
        page.checked_add(1).filter(|page| *page < count)
    } else {
        page.checked_sub(1)
    }
}

impl super::ReaderView {
    pub(super) fn refresh_ruler_evidence(&mut self) {
        if let Some(page) = self.ruler.text_priority() {
            let keep = furniture::window(page, self.pages.len());
            // Visible/selected text can outlive the ruler's evidence window.
            // Reuse its worker-prepared summary instead of extracting it again.
            for page in keep.clone() {
                if let Some(evidence) = self.page_text.evidence(page) {
                    self.ruler
                        .evidence
                        .insert(page, evidence.clone(), keep.clone());
                }
            }
        }
    }

    /// Navigation owns the viewport. Text completion may resolve a target, but
    /// must not refocus the reader or replay a jump after the user moves on.
    pub(super) fn retarget_ruler(
        &mut self,
        page: usize,
        placement: Placement,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.ruler.cancel_work();
        if !self.ruler.enabled {
            return;
        }
        self.ruler.target = None;
        self.ruler.pending = Some((page, placement));
        self.finish_ruler_placement(cx);
        self.schedule_text(cx);
    }

    pub(super) fn ruler_editing(&self) -> bool {
        self.selection.dragging_page().is_some()
            || self.selection.has_selection()
            || self.selected_annotation.is_some()
            || self.annotation_choice.is_some()
    }

    /// A single extraction is allowed to finish, but only the current intent
    /// consumes it. Reversing never queues behind an obsolete traversal.
    pub(super) fn traverse_ruler(&mut self, forward: bool, cx: &mut gpui_kit::Context<Self>) {
        if !self.ruler.enabled || self.pages.is_empty() || self.ruler_editing() {
            return;
        }
        self.ruler.browse_task = None;
        self.ruler.notice = None;
        if let Some(work) = &mut self.ruler.traversal
            && work.forward == forward
        {
            // Bound key-repeat debt so a delayed extractor cannot leave the
            // reader moving long after the key is released.
            work.remaining = work.remaining.saturating_add(1).min(32);
        } else {
            self.ruler.traversal = None;
            self.ruler.pending = None;
            let page = if self.ruler_visible() {
                self.ruler.target.as_ref().unwrap().page
            } else {
                self.ruler.target = None;
                self.page_index
            };
            self.ruler.traversal = Some(Traversal {
                page,
                forward,
                remaining: 1,
                enter: self.ruler.target.is_none(),
            });
        }
        self.continue_ruler(cx);
        self.schedule_text(cx);
        cx.notify();
    }

    pub(super) fn continue_ruler(&mut self, cx: &mut gpui_kit::Context<Self>) {
        while let Some(mut work) = self.ruler.traversal {
            self.refresh_ruler_evidence();
            if !self.ruler.enabled {
                self.ruler.traversal = None;
                return;
            }
            if self
                .ruler
                .evidence
                .missing(work.page, self.pages.len())
                .is_some()
            {
                return;
            }
            let moved = if work.enter {
                if self.page_text.failed(work.page) {
                    self.ruler.traversal = None;
                    self.ruler.notice =
                        Some("Reading ruler: couldn’t read page text — browse to continue");
                    return;
                }
                let Some(text) = self.page_text.get(work.page).cloned() else {
                    return;
                };
                let line = if work.forward {
                    (0..text.lines.len()).find(|ix| {
                        document_rect(&text, *ix).is_some()
                            && !self.ruler.evidence.excludes(work.page, *ix)
                    })
                } else {
                    (0..text.lines.len()).rev().find(|ix| {
                        document_rect(&text, *ix).is_some()
                            && !self.ruler.evidence.excludes(work.page, *ix)
                    })
                };
                if let Some(line) = line {
                    self.ruler.target = Some(Target {
                        page: work.page,
                        line,
                        text,
                    });
                    true
                } else {
                    false
                }
            } else {
                self.ruler.step(work.forward)
            };
            if moved {
                work.remaining -= 1;
                work.enter = false;
                self.ruler.traversal = (work.remaining > 0).then_some(work);
                self.remember_ruler(cx);
                self.reveal_ruler(cx);
            } else if let Some(page) = adjacent(work.page, work.forward, self.pages.len()) {
                work.page = page;
                work.enter = true;
                self.ruler.traversal = Some(work);
            } else {
                self.ruler.traversal = None;
                self.ruler.notice = Some(if work.forward {
                    "Reading ruler: end of document"
                } else {
                    "Reading ruler: start of document"
                });
            }
        }
    }

    /// Only browsing input calls this, never layout/refit or ruler reveal.
    /// Keep a visible target, hide an offscreen guide during the gesture, and
    /// retarget once after wheel momentum settles without scrolling back.
    pub(super) fn browse_ruler(&mut self, cx: &mut gpui_kit::Context<Self>) {
        self.ruler.cancel_work();
        if !self.ruler.enabled || self.ruler_editing() {
            return;
        }
        let generation = self.document_generation;
        self.ruler.browse_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(180))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.document_generation != generation || !this.ruler.enabled {
                    return;
                }
                this.ruler.browse_task = None;
                if !this.ruler_visible() {
                    this.ruler.target = None;
                    let center = this.scroll.bounds().center();
                    let (top, _, height) = this.viewport();
                    let page = this.layout().current_page_at(top, height);
                    this.queue_ruler_placement(page, center, false);
                    this.finish_ruler_placement(cx);
                    this.schedule_text(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
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

    #[test]
    fn page_adjacency_stops_at_both_document_ends() {
        assert_eq!(adjacent(0, false, 1000), None);
        assert_eq!(adjacent(999, true, 1000), None);
        assert_eq!(adjacent(999, false, 1000), Some(998));
        assert_eq!(adjacent(0, true, 1000), Some(1));
        assert_eq!(adjacent(usize::MAX, true, usize::MAX), None);
    }
}
