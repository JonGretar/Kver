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
                    .map(|glyph| glyph.text.as_str())
                    .collect(),
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
    use std::path::Path;

    #[test]
    fn real_pdf_with_positioned_words_searches_and_displays_the_gap() {
        // A modest TJ adjustment separates words without encoding a space,
        // as in TeX-produced PDFs. It must reach every text consumer.
        for stream in [
            b"BT /F1 16 Tf 40 100 Td [(incorrect) -180 (matches)] TJ ET".as_slice(),
            b"BT /F1 16 Tf 0.4 Tc 40 100 Td [(incorrect) -180 (matches)] TJ ET".as_slice(),
            b"BT /F1 16 Tf 40 100 Td [(incorrect) -180] TJ (matches) Tj ET".as_slice(),
            b"BT /F1 16 Tf 1 0 0 1 104.224 100 Tm (matches) Tj 1 0 0 1 40 100 Tm (incorrect) Tj ET"
                .as_slice(),
        ] {
            let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
            format!("<< /Length {} >>\nstream\n", stream.len())
                .into_bytes()
                .into_iter()
                .chain(stream.iter().copied())
                .chain(b"\nendstream".iter().copied())
                .collect(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ];
            let mut data = b"%PDF-1.4\n".to_vec();
            let mut offsets = Vec::new();
            for (index, object) in objects.iter().enumerate() {
                offsets.push(data.len());
                data.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
                data.extend_from_slice(object);
                data.extend_from_slice(b"\nendobj\n");
            }
            let xref = data.len();
            data.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
            for offset in offsets {
                data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            data.extend_from_slice(
                format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n")
                    .as_bytes(),
            );
            let path = std::env::temp_dir().join(format!("kver-56-gap-{}.pdf", std::process::id()));
            std::fs::write(&path, data).unwrap();
            let page = crate::pdf::page_text(&path, 0).unwrap();
            std::fs::remove_file(path).unwrap();
            assert_eq!(
                page.glyphs
                    .iter()
                    .map(|glyph| glyph.text.as_str())
                    .collect::<String>(),
                "incorrect matches",
            );
            let words: Vec<_> = page
                .word_units(0)
                .into_iter()
                .map(|unit| {
                    page.glyphs[unit]
                        .iter()
                        .map(|g| g.text.as_str())
                        .collect::<String>()
                })
                .collect();
            assert_eq!(words, ["incorrect", " ", "matches"]);
            let matches = matches_on_page(&page, 0, "incorrect matches");
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].snippet, "incorrect matches");
        }
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
