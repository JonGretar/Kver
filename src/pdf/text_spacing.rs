//! Recover word separators that PDF producers encode as positioning rather
//! than Unicode. Normalize once, before shared glyph identities are published.

use super::{TextGlyph, text_layout};
use crate::coordinates::Point;

pub(super) struct DecodedGlyph {
    glyph: TextGlyph,
    em: f64,
}

impl DecodedGlyph {
    pub(super) fn new(glyph: TextGlyph, em: f64) -> Self {
        Self { glyph, em }
    }

    pub(super) fn glyph(&self) -> &TextGlyph {
        &self.glyph
    }
}

pub(super) fn normalize(decoded: Vec<DecodedGlyph>) -> Vec<TextGlyph> {
    let mut glyphs: Vec<TextGlyph> = Vec::with_capacity(decoded.len());
    let mut previous_em: f64 = 0.;
    let mut baseline: Option<f64> = None;
    for DecodedGlyph { glyph, em } in decoded {
        let same_line = baseline.is_some_and(|y| (glyph.start.y - y).abs() <= 3.);
        if !same_line {
            baseline = Some(glyph.start.y);
        }
        if let Some(previous) = glyphs.last() {
            let gap = glyph.start.x - previous.end.x;
            let height = (previous.top - previous.bottom).min(glyph.top - glyph.bottom);
            // PDF spaces commonly occupy about a quarter em. Half that
            // separates modest word gaps from kerning and small tracking.
            // Never bridge a line/column, or duplicate mapped whitespace.
            if same_line
                && previous_em.min(em).is_finite()
                && previous_em.min(em) > 0.
                && gap > previous_em.min(em) * 0.125
                && gap < text_layout::gutter_width(height)
                && glyph.start.x - previous.ink_right.unwrap_or(previous.end.x)
                    <= text_layout::gutter_width(height)
                && !previous
                    .text
                    .chars()
                    .last()
                    .is_some_and(char::is_whitespace)
                && !glyph.text.chars().next().is_some_and(char::is_whitespace)
            {
                glyphs.push(TextGlyph {
                    text: " ".into(),
                    start: previous.end,
                    end: Point {
                        x: glyph.start.x,
                        y: previous.end.y,
                    },
                    ink_right: None,
                    top: previous.top.min(glyph.top),
                    bottom: previous.bottom.max(glyph.bottom),
                });
            }
        }
        previous_em = em;
        glyphs.push(glyph);
    }
    glyphs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(text: &str, x: f64, y: f64, em: f64) -> DecodedGlyph {
        DecodedGlyph::new(
            TextGlyph {
                text: text.into(),
                start: Point { x, y },
                end: Point { x: x + em * 0.5, y },
                // An ink edge is not an advance: a narrow letter can have a large
                // sidebearing without a word separator.
                ink_right: Some(x + em * 0.2),
                top: y + em * 0.8,
                bottom: y - em * 0.2,
            },
            em,
        )
    }

    fn text(glyphs: Vec<DecodedGlyph>) -> String {
        normalize(glyphs)
            .iter()
            .map(|glyph| glyph.text.as_str())
            .collect()
    }

    #[test]
    fn infers_modest_spaces_at_font_scale_not_ink_sidebearings() {
        for em in [4., 10., 24., 72.] {
            assert_eq!(
                text(vec![
                    glyph("a", 0., 80., em),
                    glyph("b", em * 0.68, 80., em)
                ]),
                "a b"
            );
            assert_eq!(
                text(vec![glyph("a", 0., 80., em), glyph("b", em * 0.5, 80., em)]),
                "ab"
            );
            assert_eq!(
                text(vec![
                    glyph("a", 0., 80., em),
                    glyph("b", em * 0.55, 80., em)
                ]),
                "ab"
            );
            assert_eq!(
                text(vec![
                    glyph("a", 0., 80., em),
                    glyph("b", em * 0.45, 80., em)
                ]),
                "ab"
            );
        }
    }

    #[test]
    fn does_not_duplicate_unicode_spaces_or_bridge_lines_and_columns() {
        for space in [" ", "\u{a0}", "\u{2009}"] {
            assert_eq!(
                text(vec![
                    glyph("a", 0., 80., 10.),
                    glyph(space, 7., 80., 10.),
                    glyph("b", 14., 80., 10.)
                ]),
                format!("a{space}b"),
            );
        }
        assert_eq!(
            text(vec![glyph("a", 0., 80., 10.), glyph("b", 8., 60., 10.)]),
            "ab"
        );
        assert_eq!(
            text(vec![glyph("a", 0., 80., 10.), glyph("b", 80., 80., 10.)]),
            "ab"
        );
    }
}
