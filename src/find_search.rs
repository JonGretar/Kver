//! Mapped-text phrase matching. Geometry stays in PDF document coordinates.

use crate::{coordinates::Point, pdf::PageText};

#[derive(Clone, Debug)]
pub struct FindMatch {
    pub page: usize,
    pub snippet: String,
    pub transform: crate::coordinates::PageTransform,
    /// Four document-space corners enclosing the mapped glyphs.
    pub corners: [Point; 4],
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
                snippet: glyphs[snippet_start..snippet_end]
                    .iter()
                    .map(|g| g.text.as_str())
                    .collect::<String>(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinates::PageTransform;
    use std::path::Path;

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
