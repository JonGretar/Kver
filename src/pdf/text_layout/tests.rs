use crate::{
    coordinates::Point,
    pdf::{TextGlyph, order_visible_lines},
};

fn layout(fragments: &[(&str, f64, f64)]) -> Vec<String> {
    let mut glyphs = Vec::new();
    for &(text, left, y) in fragments.iter().rev() {
        for (offset, character) in text.chars().enumerate() {
            let x = left + offset as f64 * 5.0;
            glyphs.push(TextGlyph {
                text: character.to_string(),
                start: Point { x, y },
                end: Point { x: x + 5.0, y },
                ink_right: Some(x + 5.0),
                top: y + 8.0,
                bottom: y - 2.0,
            });
        }
    }
    order_visible_lines(&mut glyphs)
        .iter()
        .map(|range| {
            glyphs[range.clone()]
                .iter()
                .map(|glyph| glyph.text.as_str())
                .collect()
        })
        .collect()
}

#[test]
fn spanning_bands_surround_uneven_columns_and_ignore_callback_order() {
    assert_eq!(
        layout(&[
            ("A full width heading spanning columns", 0.0, 160.0),
            ("left one", 0.0, 130.0),
            ("right one", 120.0, 128.0),
            ("left two", 0.0, 116.0),
            ("right two", 120.0, 114.0),
            ("left three", 0.0, 102.0),
            ("right three", 120.0, 100.0),
            ("left four", 0.0, 88.0),
            ("A full width closing note after columns", 0.0, 50.0),
        ]),
        [
            "A full width heading spanning columns",
            "left one",
            "left two",
            "left three",
            "left four",
            "right one",
            "right two",
            "right three",
            "A full width closing note after columns",
        ]
    );
}

#[test]
fn overlapping_tall_type_does_not_reverse_single_column_baselines() {
    let mut glyphs = vec![
        TextGlyph {
            text: "body above".into(),
            start: Point { x: 0.0, y: 100.0 },
            end: Point { x: 50.0, y: 100.0 },
            ink_right: Some(50.0),
            top: 108.0,
            bottom: 98.0,
        },
        TextGlyph {
            text: "large heading below".into(),
            start: Point { x: 0.0, y: 80.0 },
            end: Point { x: 100.0, y: 80.0 },
            ink_right: Some(100.0),
            top: 120.0,
            bottom: 78.0,
        },
    ];
    assert_eq!(order_visible_lines(&mut glyphs), [0..1, 1..2]);
}

#[test]
fn single_column_paragraphs_and_word_spaces_keep_their_order() {
    assert_eq!(
        layout(&[
            ("ordinary words here", 0.0, 100.0),
            ("next line", 0.0, 86.0),
            ("paragraph after a gap", 0.0, 50.0),
        ]),
        ["ordinary words here", "next line", "paragraph after a gap"]
    );
}

#[test]
fn three_columns_follow_each_other_not_the_shared_baseline() {
    assert_eq!(
        layout(&[
            ("A1", 0.0, 100.0),
            ("B1", 50.0, 100.0),
            ("C1", 100.0, 100.0),
            ("A2", 0.0, 86.0),
            ("B2", 50.0, 86.0),
            ("C2", 100.0, 86.0),
            ("A3", 0.0, 72.0),
            ("B3", 50.0, 72.0),
            ("C3", 100.0, 72.0),
        ]),
        ["A1", "A2", "A3", "B1", "B2", "B3", "C1", "C2", "C3"]
    );
}

#[test]
fn isolated_fragments_are_separate_highlights_without_inventing_columns() {
    assert_eq!(
        layout(&[
            ("one", 0.0, 100.0),
            ("two", 50.0, 100.0),
            ("below", 0.0, 86.0),
        ]),
        ["one", "two", "below"]
    );
}
