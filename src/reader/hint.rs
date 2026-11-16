//! Word-anchored targeting, independent of the meaning of a chosen target.
//!
//! A caller prepares an ordered subset and page projections. One immutable
//! snapshot enforces visibility, placement and capacity before any labels appear.
//! Retain the snapshot to offer another subset later without relabelling.

use crate::coordinates::{PageViewport, Point};

mod presentation;
pub use presentation::measure_labels;

// Prefer the home row, then the upper and lower rows. Keep the complete
// lowercase alphabet so every snapshot still has 26 × 26 unique labels.
const LETTERS: &[u8; 26] = b"asdfghjklqwertyuiopzxcvbnm";

/// Window-logical geometry. Unlike persistent anchors, this is snapshot-local.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    edges: [f64; 4],
}

impl Rect {
    pub fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            edges: [left, top, right, bottom],
        }
    }

    pub fn edges(self) -> [f64; 4] {
        self.edges
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let [a, b, c, d] = self.edges;
        let [e, f, g, h] = other.edges;
        let rect = Self::new(a.max(e), b.max(f), c.min(g), d.min(h));
        let [l, t, r, b] = rect.edges;
        (l.is_finite() && t.is_finite() && r.is_finite() && b.is_finite() && l < r && t < b)
            .then_some(rect)
    }

    fn contains(self, other: Self) -> bool {
        self.intersection(other) == Some(other)
    }
}

/// A caller-owned identity anchored to a word's document-space envelope.
#[derive(Clone)]
pub struct Target<T> {
    id: T,
    page: usize,
    corners: [Point; 4],
}

impl<T> Target<T> {
    pub fn new(id: T, page: usize, corners: [Point; 4]) -> Self {
        Self { id, page, corners }
    }
    pub fn id(&self) -> &T {
        &self.id
    }
    pub fn page(&self) -> usize {
        self.page
    }
}

/// Page projection and actual viewport. Their intersection is enforced here,
/// rather than requiring each target-preparation caller to clip correctly.
pub struct Page {
    index: usize,
    projection: PageViewport,
    clip: Option<Rect>,
}

impl Page {
    pub fn new(index: usize, projection: PageViewport, bounds: Rect, viewport: Rect) -> Self {
        Self {
            index,
            projection,
            clip: bounds.intersection(viewport),
        }
    }
}

/// Resolved from the same font, line height and padding as the label renderer.
#[derive(Clone, Copy)]
pub struct Metrics {
    width: f64,
    height: f64,
    gap: f64,
}

impl Metrics {
    pub fn new(width: f64, height: f64, gap: f64) -> Self {
        Self { width, height, gap }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoText,
    Crowded,
}

pub struct Label<T> {
    target: Target<T>,
    letters: String,
    bounds: Rect,
}

impl<T> Label<T> {
    pub fn target(&self) -> &Target<T> {
        &self.target
    }
    pub fn letters(&self) -> &str {
        &self.letters
    }
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
}

pub struct Snapshot<T> {
    labels: Vec<Label<T>>,
}

impl<T> Snapshot<T> {
    /// Input order is reading order. Offscreen/fully occluded anchors and clipped
    /// slivers too small for a label are ineligible. Prefer collision-free
    /// placement, but retain every eligible target when labels overlap.
    pub fn prepare(
        targets: impl IntoIterator<Item = Target<T>>,
        pages: &[Page],
        occlusions: &[Rect],
        metrics: Metrics,
    ) -> Result<Self, Refusal> {
        if ![metrics.width, metrics.height, metrics.gap]
            .iter()
            .all(|n| n.is_finite())
            || metrics.width <= 0.
            || metrics.height <= 0.
            || metrics.gap < 0.
        {
            return Err(Refusal::Crowded);
        }
        let mut labels: Vec<Label<T>> = Vec::new();
        for target in targets {
            let Some(page) = pages.iter().find(|page| page.index == target.page) else {
                continue;
            };
            let Some(clip) = page.clip else { continue };
            let points = target
                .corners
                .map(|point| page.projection.document_to_screen(point));
            let anchor = Rect::new(
                points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
                points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
                points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
                points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
            );
            let Some(visible) = anchor.intersection(clip) else {
                continue;
            };
            if occlusions
                .iter()
                .any(|occlusion| occlusion.contains(visible))
            {
                continue;
            }
            let [left, top, right, bottom] = visible.edges;
            let partial = visible != anchor;
            // Clipped words are eligible only when their visible portion can
            // intrinsically hold a label. This decision must not depend on
            // other labels.
            if partial && (right - left < metrics.width || bottom - top < metrics.height) {
                continue;
            }
            if labels.len() == 676 {
                return Err(Refusal::Crowded);
            }
            // Keep the label's leading edge on its word. Prefer just above it;
            // at a clipped top edge, move into the visible part of the word.
            // Short words may need the adjacent gap.
            let y = if partial {
                top
            } else {
                (top - metrics.height).max(clip.edges[1])
            };
            // A short word may land a fraction of a pixel inside the previous
            // label's clearance. Try adjacent free edges too, but keep the
            // label's centre over this word rather than drifting into a gap.
            let free_edges = labels
                .iter()
                .filter(|label| {
                    label.bounds.edges[1] - metrics.gap < y + metrics.height
                        && label.bounds.edges[3] + metrics.gap > y
                })
                .flat_map(|label| {
                    [
                        label.bounds.edges[2] + metrics.gap,
                        label.bounds.edges[0] - metrics.gap - metrics.width,
                    ]
                })
                .filter(|x| {
                    let centre = x + metrics.width / 2.;
                    left < centre && centre < right
                });
            let at_x = |x: f64| {
                let x = x.max(clip.edges[0]).min(clip.edges[2] - metrics.width);
                Rect::new(x, y, x + metrics.width, y + metrics.height)
            };
            let visible_placement = |bounds: &Rect| {
                (!partial || visible.contains(*bounds))
                    && right > bounds.edges[0]
                    && bottom > bounds.edges[1]
                    && clip.contains(*bounds)
                    && !occlusions.iter().any(|r| r.intersection(*bounds).is_some())
            };
            let bounds = [left, right - metrics.width]
                .into_iter()
                .chain(free_edges)
                .map(at_x)
                .find(|bounds| {
                    visible_placement(bounds)
                        && !labels.iter().any(|label| {
                            let [l, t, r, b] = label.bounds.edges;
                            Rect::new(
                                l - metrics.gap,
                                t - metrics.gap,
                                r + metrics.gap,
                                b + metrics.gap,
                            )
                            .intersection(*bounds)
                            .is_some()
                        })
                })
                // Preserve every eligible target: prefix filtering thins overlap.
                // Page, viewport and occlusion constraints still apply.
                .or_else(|| {
                    [left, right - metrics.width]
                        .map(at_x)
                        .into_iter()
                        .find(visible_placement)
                })
                .ok_or(Refusal::Crowded)?;
            let ix = labels.len();
            labels.push(Label {
                target,
                letters: format!(
                    "{}{}",
                    // Spread each first-letter prefix through reading order.
                    LETTERS[ix % LETTERS.len()] as char,
                    LETTERS[ix / LETTERS.len()] as char
                ),
                bounds,
            });
        }
        if labels.is_empty() {
            Err(Refusal::NoText)
        } else {
            Ok(Self { labels })
        }
    }

    pub fn labels(&self) -> &[Label<T>] {
        &self.labels
    }

    pub(super) fn paint_labels(&self, page: usize, input: &Input) -> Vec<&Label<T>> {
        let mut labels: Vec<_> = self
            .labels
            .iter()
            .filter(|label| label.target.page == page && input.matches(label))
            .collect();
        // Paint leftmost first: a right neighbour can cover the trailing
        // letter, leaving the leading prefix exposed. Never reorder identities.
        labels.sort_by(|a, b| a.bounds.edges[0].total_cmp(&b.bounds.edges[0]));
        labels
    }

    /// Offer a caller-selected subset in a later stage, preserving assignments.
    /// This does not re-run placement against a changed viewport.
    pub fn subset(&self, include: impl Fn(&T) -> bool) -> Self
    where
        T: Clone,
    {
        Self {
            labels: self
                .labels
                .iter()
                .filter(|label| include(label.target.id()))
                .map(|label| Label {
                    target: label.target.clone(),
                    letters: label.letters.clone(),
                    bounds: label.bounds,
                })
                .collect(),
        }
    }
}

#[derive(Default)]
pub struct Input {
    prefix: String,
    retry: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Chosen(usize),
    Cancelled,
}

impl Input {
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
    pub fn is_retry(&self) -> bool {
        self.retry
    }
    pub fn matches<T>(&self, label: &Label<T>) -> bool {
        label.letters.starts_with(&self.prefix)
    }

    /// Keys come only from the hint focus scope, never a global keyboard hook.
    pub fn key<T>(&mut self, key: &str, snapshot: Option<&Snapshot<T>>) -> Outcome {
        self.retry = false;
        match key {
            "escape" => {
                self.prefix.clear();
                return Outcome::Cancelled;
            }
            "backspace" => {
                self.prefix.pop();
            }
            key if key.len() == 1
                && key.as_bytes()[0].is_ascii_lowercase()
                && snapshot.is_some() =>
            {
                self.prefix.push_str(key);
                if self.prefix.len() == 2 {
                    let chosen = snapshot.and_then(|snapshot| {
                        snapshot
                            .labels
                            .iter()
                            .position(|label| label.letters == self.prefix)
                    });
                    self.prefix.clear();
                    if let Some(ix) = chosen {
                        return Outcome::Chosen(ix);
                    }
                    self.retry = true;
                }
            }
            _ => {}
        }
        Outcome::Pending
    }
}

#[cfg(test)]
mod tests;
