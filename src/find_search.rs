//! Mapped-text phrase matching. Geometry stays in PDF document coordinates.

use crate::{
    coordinates::Point,
    pdf::{PageText, TextGlyph},
};

#[derive(Clone, Debug)]
pub struct FindMatch {
    pub page: usize,
    pub snippet: String,
    pub transform: crate::coordinates::PageTransform,
    /// Four document-space corners enclosing the mapped glyphs.
    pub corners: [Point; 4],
}

/// Some PDFs position words apart without mapping a space character. Infer
/// only a clear word-sized gap; small kerning/tracking gaps remain within a
/// word, and mapped whitespace is never duplicated.
fn has_word_gap(previous: &TextGlyph, next: &TextGlyph) -> bool {
    if previous
        .text
        .chars()
        .last()
        .is_some_and(char::is_whitespace)
        || next.text.chars().next().is_some_and(char::is_whitespace)
    {
        return false;
    }
    let previous_width =
        (previous.end.x - previous.start.x) / previous.text.chars().count().max(1) as f64;
    let next_width = (next.end.x - next.start.x) / next.text.chars().count().max(1) as f64;
    let height = (previous.top - previous.bottom)
        .min(next.top - next.bottom)
        .max(0.);
    let threshold = ((previous_width + next_width) * 0.23)
        .max(height * 0.23)
        .max(1.5);
    // A run's advance may already include the gap before the next origin;
    // only the outline gives us an ink edge independent of that advance.
    let right = previous.ink_right.unwrap_or(previous.end.x);
    next.start.x - right > threshold
}

fn snippet_text(glyphs: &[TextGlyph]) -> String {
    let mut snippet = String::new();
    for (index, glyph) in glyphs.iter().enumerate() {
        if index > 0 && has_word_gap(&glyphs[index - 1], glyph) {
            snippet.push(' ');
        }
        snippet.push_str(&glyph.text);
    }
    snippet
}

/// Match within visual lines only: synthesizing reading order across columns
/// or missing Unicode mappings would produce misleading search locations.
pub fn matches_on_page(text: &PageText, page: usize, query: &str) -> Vec<FindMatch> {
    let needle: Vec<char> = query.to_lowercase().chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for line in &text.lines {
        let glyphs = &text.glyphs[line.clone()];
        let mut folded = Vec::new();
        let mut owners = Vec::new();
        for (index, glyph) in glyphs.iter().enumerate() {
            if index > 0 && has_word_gap(&glyphs[index - 1], glyph) {
                folded.push(' ');
                owners.push(index);
            }
            for c in glyph.text.to_lowercase().chars() {
                folded.push(c);
                owners.push(index);
            }
        }
        for start in 0..folded.len().saturating_sub(needle.len()) + 1 {
            if folded.get(start..start + needle.len()) != Some(needle.as_slice()) {
                continue;
            }
            let first = owners[start];
            let last = owners[start + needle.len() - 1] + 1;
            let hit = &glyphs[first..last];
            let left = hit.iter().map(|g| g.start.x).fold(f64::INFINITY, f64::min);
            let right = hit
                .iter()
                .map(|g| g.end.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let top = hit.iter().map(|g| g.top).fold(f64::NEG_INFINITY, f64::max);
            let bottom = hit.iter().map(|g| g.bottom).fold(f64::INFINITY, f64::min);
            let snippet_start = first.saturating_sub(24);
            let snippet_end = (last + 24).min(glyphs.len());
            matches.push(FindMatch {
                page,
                transform: text.transform,
                snippet: snippet_text(&glyphs[snippet_start..snippet_end]),
                corners: [
                    Point { x: left, y: top },
                    Point { x: right, y: top },
                    Point { x: left, y: bottom },
                    Point {
                        x: right,
                        y: bottom,
                    },
                ],
            });
        }
    }
    matches
}

pub fn page_rect(hit: &FindMatch, size: (f32, f32)) -> (f32, f32, f32, f32) {
    let corners = hit
        .corners
        .map(|point| hit.transform.document_to_page(point));
    let left = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min)
        .clamp(0., size.0 as f64);
    let right = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max)
        .clamp(0., size.0 as f64);
    let top = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min)
        .clamp(0., size.1 as f64);
    let bottom = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .clamp(0., size.1 as f64);
    (
        left as f32,
        top as f32,
        (right - left).max(0.) as f32,
        (bottom - top).max(0.) as f32,
    )
}

/// Expand the overlay in page/document points, clamped to the visible crop.
/// The underlying match corners remain unchanged for navigation and zoom.
pub fn highlight_rect(hit: &FindMatch, size: (f32, f32)) -> (f32, f32, f32, f32) {
    let (left, top, width, height) = page_rect(hit, size);
    let x = (left - 2.).max(0.);
    let y = (top - 2.).max(0.);
    let right = (left + width + 2.).min(size.0);
    let bottom = (top + height + 2.).min(size.1);
    (x, y, right - x, bottom - y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinates::PageTransform;
    use crate::pdf::TextGlyph;
    use std::path::Path;

    #[test]
    fn unmapped_word_gap_appears_in_find_snippet_and_matches_phrase() {
        let mut glyphs = Vec::new();
        let mut x = 20.;
        for (index, c) in "incorrectmatches".chars().enumerate() {
            if index == "incorrect".len() {
                x += 5.;
            }
            glyphs.push(TextGlyph {
                text: c.to_string(),
                start: Point { x, y: 80. },
                end: Point { x: x + 6., y: 80. },
                ink_right: Some(x + 6.),
                top: 90.,
                bottom: 78.,
            });
            x += 6.;
        }
        let page = PageText {
            transform: PageTransform::from_coefficients([1., 0., 0., 1., 0., 0.]),
            lines: std::iter::once(0..glyphs.len()).collect(),
            glyphs,
        };
        assert_eq!(
            matches_on_page(&page, 0, "incorrect")[0].snippet,
            "incorrect matches"
        );
        assert_eq!(matches_on_page(&page, 0, "incorrect matches").len(), 1);

        // Hayro may include the word gap in a glyph's advance within one run.
        let mut in_run = PageText {
            transform: page.transform,
            lines: page.lines.clone(),
            glyphs: page.glyphs.clone(),
        };
        let boundary = "incorrect".len();
        in_run.glyphs[boundary - 1].end.x = in_run.glyphs[boundary].start.x;
        assert_eq!(
            matches_on_page(&in_run, 0, "incorrect")[0].snippet,
            "incorrect matches"
        );
        assert_eq!(matches_on_page(&in_run, 0, "incorrect matches").len(), 1);

        // A small tracking adjustment is not a word boundary.
        let mut tracked = page;
        for glyph in &mut tracked.glyphs["incorrect".len()..] {
            glyph.start.x -= 4.;
            glyph.end.x -= 4.;
        }
        assert_eq!(
            matches_on_page(&tracked, 0, "incorrect")[0].snippet,
            "incorrectmatches"
        );
        assert!(matches_on_page(&tracked, 0, "incorrect matches").is_empty());

        // A mapped space must not be duplicated by its own visual advance.
        let mut mapped = tracked;
        let boundary = "incorrect".len();
        let left = mapped.glyphs[boundary - 1].end.x;
        mapped.glyphs.insert(
            boundary,
            TextGlyph {
                text: " ".into(),
                start: Point { x: left, y: 80. },
                end: Point {
                    x: left + 2.,
                    y: 80.,
                },
                ink_right: None,
                top: 90.,
                bottom: 78.,
            },
        );
        mapped.lines = std::iter::once(0..mapped.glyphs.len()).collect();
        assert_eq!(
            matches_on_page(&mapped, 0, "incorrect")[0].snippet,
            "incorrect matches"
        );
    }

    #[test]
    fn finds_repeated_matches_on_long_real_pdf_without_retaining_text() {
        let path = Path::new("examples/readme-sample.pdf");
        let mut total = 0;
        for index in 0..100 {
            for page in crate::pdf::page_text_batch(path, index..index + 1) {
                total += matches_on_page(&page.unwrap(), index, "reader").len();
            }
        }
        assert!(total >= 100);
    }

    #[test]
    fn maps_case_insensitive_phrase_and_document_geometry() {
        let page = crate::pdf::page_text(Path::new("examples/readme-sample.pdf"), 0).unwrap();
        let hits = matches_on_page(&page, 0, "kver");
        assert!(!hits.is_empty());
        let rect = page_rect(&hits[0], (612., 792.));
        assert!(rect.2 > 0. && rect.3 > 0.);
        assert!(matches_on_page(&page, 0, "").is_empty());
    }

    #[test]
    fn handles_unicode_case_fold_and_rotated_crop_projection() {
        use crate::pdf::TextGlyph;
        let glyphs = "ÄBC"
            .chars()
            .enumerate()
            .map(|(i, c)| TextGlyph {
                text: c.to_string(),
                start: Point {
                    x: 40. + i as f64 * 10.,
                    y: 80.,
                },
                end: Point {
                    x: 50. + i as f64 * 10.,
                    y: 80.,
                },
                ink_right: Some(50. + i as f64 * 10.),
                top: 92.,
                bottom: 78.,
            })
            .collect();
        let page = PageText {
            transform: PageTransform::from_coefficients([0., 1., 1., 0., -50., -30.]),
            glyphs,
            lines: std::iter::once(0..3).collect(),
        };
        let hit = &matches_on_page(&page, 0, "äb")[0];
        let rect = page_rect(hit, (200., 100.));
        assert_eq!(rect, (28., 10., 14., 20.));
        assert_eq!(highlight_rect(hit, (200., 100.)), (26., 8., 18., 24.));
        let original = crate::coordinates::PageViewport::new(
            hit.transform,
            1.,
            Point { x: 30., y: 50. },
            Point { x: 0., y: 0. },
        )
        .unwrap();
        let moved = crate::coordinates::PageViewport::new(
            hit.transform,
            2.,
            Point { x: 30., y: 50. },
            Point { x: 10., y: 20. },
        )
        .unwrap();
        let corner = hit.corners[0];
        let old = original.document_to_screen(corner);
        let new = moved.document_to_screen(corner);
        assert_eq!(new.x, 30. + (old.x - 30.) * 2. - 10.);
        assert_eq!(new.y, 50. + (old.y - 50.) * 2. - 20.);
    }
}
