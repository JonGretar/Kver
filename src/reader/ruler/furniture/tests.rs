use super::*;
use crate::reader::ruler::Ruler;
use std::{path::Path, sync::Arc};

fn specimen(page: usize) -> Arc<PageText> {
    Arc::new(crate::pdf::page_text(Path::new("examples/ruler-furniture.pdf"), page).unwrap())
}

fn contents(text: &PageText, line: usize) -> String {
    text.glyphs[text.lines[line].clone()]
        .iter()
        .map(|glyph| glyph.text.as_str())
        .collect()
}

#[test]
fn reading_order_specimen_automatic_steps_skip_repeated_footer_and_page_numbers() {
    let path = Path::new("examples/reading-order.pdf");
    let sizes = crate::pdf::document_pages(path).unwrap();
    let pages: Vec<_> = (0..sizes.len())
        .map(|page| Arc::new(crate::pdf::page_text(path, page).unwrap()))
        .collect();
    let mut ruler = Ruler::default();
    for (page, text) in pages.iter().enumerate() {
        ruler
            .evidence
            .insert(page, candidates(text, sizes[page]), 0..pages.len());
    }
    for (page, text) in pages.iter().enumerate() {
        let footer = text.lines.len() - 2;
        let number = text.lines.len() - 1;
        assert_eq!(contents(text, footer), "KVER / MARGINS & METHODS");
        assert_eq!(contents(text, number), (page + 1).to_string());
        let at = text.glyphs[text.lines[0].start].start;
        assert!(ruler.place(page, text.clone(), at, true));
        let mut reached = vec![0];
        while ruler.step(true) {
            let line = ruler.target.as_ref().unwrap().line;
            assert!(
                line != footer && line != number,
                "automatic traversal reached page {} footer: {:?}",
                page + 1,
                contents(text, line)
            );
            reached.push(line);
        }
        assert_eq!(reached, (0..footer).collect::<Vec<_>>());
        for expected in (0..footer - 1).rev() {
            assert!(ruler.step(false));
            assert_eq!(ruler.target.as_ref().unwrap().line, expected);
        }
        assert!(!ruler.step(false));
    }
}

#[test]
fn furniture_only_changes_automatic_movement_in_both_directions() {
    let mut ruler = Ruler::default();
    for group in [0..5, 5..10, 10..15] {
        for page in group.clone() {
            ruler.evidence.insert(
                page,
                candidates(&specimen(page), (600., 800.)),
                group.clone(),
            );
        }
        for page in group.clone() {
            let text = specimen(page);
            let filtered: Vec<_> = (0..text.lines.len())
                .filter(|line| ruler.evidence.excludes(page, *line))
                .collect();
            let omitted: Vec<_> = filtered.iter().map(|line| contents(&text, *line)).collect();
            assert_eq!(omitted.len(), 2, "{omitted:?}");
            assert!(
                omitted
                    .iter()
                    .any(|line| line.contains("Coastal observations"))
            );
            assert!(omitted.iter().any(|line| {
                if page < 5 {
                    line.contains("Page ")
                } else {
                    line.trim().parse::<u32>().is_ok()
                }
            }));
            let kept: Vec<_> = (0..text.lines.len())
                .filter(|line| !filtered.contains(line))
                .collect();
            let kept_text = kept
                .iter()
                .map(|line| contents(&text, *line))
                .collect::<Vec<_>>()
                .join("\n");
            for expected in [
                "Chapter",
                "Across the page",
                "Left column begins",
                "Left column ends",
                "Right column begins",
                "Right column ends",
                "Figure",
                "unique footnote",
                "next footnote",
            ] {
                assert!(kept_text.contains(expected), "{expected}: {kept_text}");
            }
            let order: Vec<_> = kept.iter().map(|line| contents(&text, *line)).collect();
            let position =
                |needle: &str| order.iter().position(|line| line.contains(needle)).unwrap();
            assert!(position("Across the page") < position("Left column begins"));
            assert!(position("Left column ends") < position("Right column begins"));
            assert!(position("Right column ends") < position("Figure"));
            assert!(position("Figure") < position("unique footnote"));
            // Explicit placement addresses every original line, including excluded
            // furniture. Continuation is the nearest retained source-order successor
            // or predecessor, not a reindexed/durable filtered identity.
            for line in 0..text.lines.len() {
                let [left, bottom, right, top] =
                    crate::reader::ruler::document_rect(&text, line).unwrap();
                let at = Point {
                    x: (left + right) / 2.,
                    y: (bottom + top) / 2.,
                };
                for forward in [true, false] {
                    assert!(ruler.place(page, text.clone(), at, true));
                    assert_eq!(ruler.target.as_ref().unwrap().line, line);
                    let expected = if forward {
                        kept.iter().copied().find(|ix| *ix > line)
                    } else {
                        kept.iter().copied().rev().find(|ix| *ix < line)
                    };
                    assert_eq!(ruler.step(forward), expected.is_some());
                    assert_eq!(
                        ruler.target.as_ref().unwrap().line,
                        expected.unwrap_or(line)
                    );
                }
            }
            let original = contents(&text, filtered[0]);
            assert!(!original.is_empty()); // Original glyphs and line ranges survive.
        }
    }
}

#[test]
fn insufficient_failed_or_changing_evidence_retains_text() {
    let text = specimen(0);
    let mut evidence = Evidence::default();
    for page in 0..2 {
        evidence.insert(page, candidates(&text, (600., 800.)), 0..5);
    }
    for line in 0..text.lines.len() {
        assert!(!evidence.excludes(0, line));
    }
    evidence.insert(2, Vec::new(), 0..5); // Failed extraction supplies no votes.
    assert_eq!(evidence.missing(0, 5), Some(3));
    for line in 0..text.lines.len() {
        assert!(!evidence.excludes(0, line));
    }
    assert_ne!(signature("Chapter 1"), signature("Chapter 2"));
    assert_eq!(signature("12"), signature("13"));
    assert_eq!(signature("Page 12 of 100"), signature("Page 13 of 100"));
    assert_ne!(signature("Page 12 of 100"), signature("Page 13 of 101"));
    for page in 0..1000 {
        evidence.insert(page, Vec::new(), window(page, 1000));
        assert!(evidence.pages.len() <= 5);
        assert!(window(page, 1000).len() <= 5);
    }
}

fn margin_text(rows: &[(&str, f64)]) -> PageText {
    use crate::{coordinates::PageTransform, pdf::TextGlyph};
    PageText {
        transform: PageTransform::from_coefficients([1., 0., 0., -1., 0., 800.]),
        glyphs: rows
            .iter()
            .map(|(text, y)| TextGlyph {
                text: (*text).into(),
                start: Point {
                    x: 50.,
                    y: 800. - y - 8.,
                },
                end: Point {
                    x: 150.,
                    y: 800. - y - 8.,
                },
                ink_right: None,
                top: 800. - y,
                bottom: 800. - y - 10.,
            })
            .collect(),
        lines: (0..rows.len()).map(|line| line..line + 1).collect(),
    }
}

#[test]
fn margin_location_alone_does_not_suppress_titles_footnotes_or_arbitrary_numbers() {
    let mut evidence = Evidence::default();
    for (page, number) in ["1", "80", "33"].into_iter().enumerate() {
        let title = format!("Chapter {}", page + 1);
        let text = margin_text(&[
            (&title, 30.),
            ("Body text", 100.),
            ("Repeated footnote first line", 750.),
            ("Repeated footnote second line", 764.),
            (number, 790.),
        ]);
        evidence.insert(page, candidates(&text, (600., 800.)), 0..3);
    }
    for page in 0..3 {
        for line in 0..5 {
            assert!(!evidence.excludes(page, line), "page {page} line {line}");
        }
    }
}

#[test]
fn repetition_requires_matching_position_and_three_distinct_pages() {
    let mut evidence = Evidence::default();
    for page in 0..3 {
        let text = margin_text(&[
            ("Running header", if page == 2 { 45. } else { 30. }),
            ("Body", 120.),
        ]);
        evidence.insert(page, candidates(&text, (600., 800.)), 0..3);
    }
    assert!(!evidence.excludes(0, 0));
    // Inserting the same page again never counts as another vote.
    evidence.insert(
        1,
        candidates(&margin_text(&[("Running header", 30.)]), (600., 800.)),
        0..3,
    );
    assert!(!evidence.excludes(0, 0));
    evidence.insert(
        2,
        candidates(&margin_text(&[("Running header", 30.)]), (600., 800.)),
        0..3,
    );
    assert!(evidence.excludes(0, 0));
}

#[test]
fn inconsistent_numbers_do_not_mix_repetition_and_sequence_votes() {
    let mut evidence = Evidence::default();
    for (page, number) in ["1", "1", "3"].into_iter().enumerate() {
        evidence.insert(
            page,
            candidates(&margin_text(&[(number, 30.)]), (600., 800.)),
            0..3,
        );
    }
    for page in 0..3 {
        assert!(!evidence.excludes(page, 0));
    }
}
