//! Bounded geometric reading order for ordinary horizontal prose.
//!
//! Recursive whitespace cuts separate full-width bands, then columns within
//! each band. This is not semantic PDF analysis: tables, floating sidebars,
//! bidirectional text, and mathematical notation can need a different order.

use std::ops::Range;

use super::TextGlyph;

struct Fragment {
    range: Range<usize>,
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

/// Larger than ordinary word spacing, but allows conventional 18pt gutters
/// beside 10pt body type. This is a conservative geometric cue, not a column
/// count or a page-midpoint assumption.
pub(super) fn gutter_width(height: f64) -> f64 {
    (height * 1.2).max(12.0)
}

pub(super) fn reading_order(glyphs: &[TextGlyph], lines: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut fragments: Vec<_> = lines
        .into_iter()
        .map(|range| {
            let line = &glyphs[range.clone()];
            Fragment {
                range,
                left: line.iter().map(|g| g.start.x).fold(f64::INFINITY, f64::min),
                right: line
                    .iter()
                    .map(|g| g.end.x)
                    .fold(f64::NEG_INFINITY, f64::max),
                top: line.iter().map(|g| g.top).fold(f64::NEG_INFINITY, f64::max),
                bottom: line.iter().map(|g| g.bottom).fold(f64::INFINITY, f64::min),
            }
        })
        .collect();
    order(&mut fragments, 0);
    fragments
        .into_iter()
        .map(|fragment| fragment.range)
        .collect()
}

fn order(fragments: &mut [Fragment], depth: usize) {
    // Bound pathological PDFs rather than recursing once per isolated glyph.
    // Original glyph ranges retain baseline / left-to-right order, even when
    // large type overlaps the ink bounds of a preceding line.
    if fragments.len() < 2 || depth == 32 {
        fragments.sort_by_key(|fragment| fragment.range.start);
        return;
    }

    let mut heights: Vec<_> = fragments.iter().map(|f| f.top - f.bottom).collect();
    heights.sort_by(f64::total_cmp);
    let gutter = gutter_width(heights[heights.len() / 2]);

    // Prefer a vertical cut when a gutter runs through this entire band.
    // Requiring multiple lines on both sides avoids interpreting a spaced
    // heading or a lone equation as two prose columns.
    fragments.sort_by(|a, b| a.left.total_cmp(&b.left));
    let mut right = fragments[0].right;
    let mut vertical = None;
    let mut largest = gutter;
    for index in 1..fragments.len() {
        let gap = fragments[index].left - right;
        if index >= 3
            && fragments.len() - index >= 3
            && gap > largest
            && column_has_depth(&fragments[..index], gutter)
            && column_has_depth(&fragments[index..], gutter)
        {
            vertical = Some(index);
            largest = gap;
        }
        right = right.max(fragments[index].right);
    }
    if let Some(index) = vertical {
        let (left, right) = fragments.split_at_mut(index);
        order(left, depth + 1);
        order(right, depth + 1);
        return;
    }

    // A spanning heading/abstract/footer prevents a page-wide gutter.
    // Peel apart horizontal bands at the largest clear whitespace first.
    fragments.sort_by(|a, b| b.top.total_cmp(&a.top).then(a.left.total_cmp(&b.left)));
    let mut bottom = fragments[0].bottom;
    let mut horizontal = None;
    let mut largest = 0.0;
    for (index, fragment) in fragments.iter().enumerate().skip(1) {
        let gap = bottom - fragment.top;
        if gap > largest {
            horizontal = Some(index);
            largest = gap;
        }
        bottom = bottom.min(fragment.bottom);
    }
    if let Some(index) = horizontal {
        let (above, below) = fragments.split_at_mut(index);
        order(above, depth + 1);
        order(below, depth + 1);
    } else {
        fragments.sort_by_key(|fragment| fragment.range.start);
    }
}

fn column_has_depth(fragments: &[Fragment], gutter: f64) -> bool {
    let top = fragments
        .iter()
        .map(|f| f.top)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = fragments
        .iter()
        .map(|f| f.top)
        .fold(f64::INFINITY, f64::min);
    top - bottom > gutter
}

#[cfg(test)]
mod tests;
