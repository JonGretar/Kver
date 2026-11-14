//! Conservative ruler-only evidence. No glyphs or extraction order are changed.
use std::{collections::HashMap, ops::Range};

use crate::{coordinates::Point, pdf::PageText};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(in crate::reader) struct Evidence {
    pages: HashMap<usize, Vec<Candidate>>,
}

#[derive(Clone)]
pub(in crate::reader) struct Candidate {
    line: usize,
    signature: String,
    number: Option<u64>,
    x: f64,
    y: f64,
    height: f64,
}

pub(in crate::reader) fn window(page: usize, count: usize) -> Range<usize> {
    let start = page.saturating_sub(2).min(count.saturating_sub(5));
    start..(start + 5).min(count)
}

impl Evidence {
    pub(in crate::reader) fn insert(
        &mut self,
        page: usize,
        candidates: Vec<Candidate>,
        keep: Range<usize>,
    ) {
        self.pages.retain(|page, _| keep.contains(page));
        if keep.contains(&page) {
            self.pages.insert(page, candidates);
        }
    }

    pub(in crate::reader) fn missing(&self, page: usize, count: usize) -> Option<usize> {
        // No margin candidates means no reason to delay ordinary line movement.
        if self.pages.get(&page).is_none_or(Vec::is_empty) {
            return None;
        }
        window(page, count).find(|page| !self.pages.contains_key(page))
    }

    pub(in crate::reader) fn excludes(&self, page: usize, line: usize) -> bool {
        let Some(candidate) = self
            .pages
            .get(&page)
            .and_then(|items| items.iter().find(|candidate| candidate.line == line))
        else {
            return false;
        };
        [false, true].into_iter().any(|sequential| {
            self.pages
                .iter()
                .filter(|(other, items)| {
                    page.abs_diff(**other) <= 4
                        && items.iter().any(|item| {
                            item.signature == candidate.signature
                                && match (candidate.number, item.number) {
                                    (Some(a), Some(b)) if sequential => {
                                        b as i128 - a as i128 == **other as i128 - page as i128
                                    }
                                    (Some(a), Some(b)) => a == b,
                                    (None, None) => true,
                                    _ => false,
                                }
                                && (item.x - candidate.x).abs() <= 0.025
                                && (item.y - candidate.y).abs() <= 0.006
                                && (item.height - candidate.height).abs() <= 0.003
                        })
                })
                .count()
                >= 3
        })
    }
}

// Deliberately do not replace arbitrary digits: "Chapter 1" is not "Chapter 2".
// Accept bare Arabic numbers or an explicitly labelled page counter only.
fn signature(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let number = |s: &str| !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit());
    if number(&text) {
        return "#page".into();
    }
    let lower = text.to_lowercase();
    if let Some(rest) = lower.strip_prefix("page ") {
        let parts: Vec<_> = rest.split_whitespace().collect();
        if parts.len() == 1 && number(parts[0]) {
            return "#labelled-page".into();
        }
        if parts.len() == 3 && number(parts[0]) && parts[1] == "of" && number(parts[2]) {
            return format!("#labelled-page of {}", parts[2]);
        }
    }
    text
}

/// Called on the extraction worker. Work and retained evidence are capped even
/// for pathological pages. Missing/ambiguous evidence always keeps the text.
pub(in crate::reader) fn candidates(text: &PageText, size: (f32, f32)) -> Vec<Candidate> {
    if text.lines.len() > 4096 || text.glyphs.len() > 100_000 || size.0 <= 0. || size.1 <= 0. {
        return Vec::new();
    }
    let rects: Vec<_> = (0..text.lines.len())
        .map(|line| {
            let [left, bottom, right, top] = super::document_rect(text, line)?;
            let corners = [(left, bottom), (right, bottom), (left, top), (right, top)]
                .map(|(x, y)| text.transform.document_to_page(Point { x, y }));
            Some([
                corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
                corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
                corners
                    .iter()
                    .map(|p| p.x)
                    .fold(f64::NEG_INFINITY, f64::max),
                corners
                    .iter()
                    .map(|p| p.y)
                    .fold(f64::NEG_INFINITY, f64::max),
            ])
        })
        .collect();
    let width = size.0 as f64;
    let height = size.1 as f64;
    let mut result = Vec::new();
    for (line, rect) in rects.iter().enumerate() {
        let Some([left, top, right, bottom]) = *rect else {
            continue;
        };
        let line_height = bottom - top;
        if line_height <= 0.
            || line_height > height * 0.025
            || !(bottom < height * 0.065 || top > height * 0.935)
        {
            continue;
        }
        // A footnote paragraph or margin-adjacent body line has nearby text.
        // Require clear separation on both sides, across columns too: the
        // first line of a footnote paragraph must not lose its following lines.
        // Separate fragments on the same row (such as a footer and its page
        // number) do not make a paragraph. Both still need repetition evidence.
        if rects.iter().enumerate().any(|(other, rect)| {
            other != line
                && rect.is_some_and(|[l, t, r, b]| {
                    let same_row = (t - top).abs() <= line_height * 0.25
                        && (b - bottom).abs() <= line_height * 0.25;
                    let separate = (l - right).max(left - r) > line_height * 1.5;
                    !(same_row && separate) && (t - bottom).max(top - b).max(0.) < line_height * 1.5
                })
        }) {
            continue;
        }
        let glyphs = &text.glyphs[text.lines[line].clone()];
        let bytes: usize = glyphs.iter().map(|g| g.text.len()).sum();
        if bytes > 160 {
            continue;
        }
        let raw = glyphs.iter().map(|g| g.text.as_str()).collect::<String>();
        let signature = signature(&raw);
        if signature.is_empty() {
            continue;
        }
        result.push(Candidate {
            line,
            number: if signature.starts_with("#") {
                raw.split_whitespace().find_map(|word| word.parse().ok())
            } else {
                None
            },
            signature,
            x: (left + right) / (2. * width),
            y: (top + bottom) / (2. * height),
            height: line_height / height,
        });
        if result.len() > 8 {
            return Vec::new();
        }
    }
    result
}
