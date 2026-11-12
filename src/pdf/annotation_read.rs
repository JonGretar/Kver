//! Imported text markup. Never interprets `/Contents` as marked text.

use hayro::hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object, String as PdfString};

use super::{
    Pdf,
    annotations::{Annotation, AnnotationColor, AnnotationId, MarkupKind, Quad},
};
use crate::coordinates::{PageTransform, Point};

pub(super) fn page_annotations(
    pdf: &Pdf,
    page_ix: usize,
    epoch: u64,
) -> Result<Vec<Annotation>, String> {
    let page = pdf
        .pages()
        .get(page_ix)
        .ok_or("Annotation page is missing")?;
    let Some(array) = page.raw().get::<Array<'_>>(b"Annots") else {
        return Ok(vec![]);
    };
    let mut entries = Vec::new();
    for (slot, object) in array.raw_iter().enumerate() {
        let dict = match object {
            MaybeRef::Ref(reference) => pdf.xref().get::<Dict<'_>>(reference.into()),
            MaybeRef::NotRef(Object::Dict(dict)) => Some(dict),
            _ => None,
        };
        let Some(dict) = dict else { continue };
        let kind = match dict.get::<Name<'_>>(b"Subtype").as_ref().map(AsRef::as_ref) {
            Some(b"Highlight") => MarkupKind::Highlight,
            Some(b"Underline") => MarkupKind::Underline,
            Some(b"StrikeOut") => MarkupKind::StrikeOut,
            Some(b"Squiggly") => MarkupKind::Squiggly,
            _ => continue,
        };
        let mut malformed = false;
        let values = dict.get::<Array<'_>>(b"QuadPoints").and_then(numbers);
        let mut quads = Vec::new();
        if let Some(values) = values.filter(|v| !v.is_empty() && v.len() % 8 == 0) {
            for values in values.chunks_exact(8) {
                if let Some(quad) = Quad::new(std::array::from_fn(|i| Point {
                    x: values[i * 2],
                    y: values[i * 2 + 1],
                })) {
                    quads.push(quad);
                } else {
                    malformed = true;
                }
            }
        } else {
            malformed = true;
        }
        let rect = dict
            .get::<Array<'_>>(b"Rect")
            .and_then(numbers)
            .and_then(|v| {
                if v.len() != 4 {
                    return None;
                }
                let rect = [
                    v[0].min(v[2]),
                    v[1].min(v[3]),
                    v[0].max(v[2]),
                    v[1].max(v[3]),
                ];
                (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect)
            });
        let color = match dict.get::<Array<'_>>(b"C").and_then(numbers) {
            None if dict.get::<Object<'_>>(b"C").is_none() => AnnotationColor::Unspecified,
            Some(v) if v.iter().all(|v| (0.0..=1.0).contains(v)) => match v.as_slice() {
                [] => AnnotationColor::Unspecified,
                [g] => AnnotationColor::Gray(*g as f32),
                [r, g, b] => AnnotationColor::Rgb([*r as f32, *g as f32, *b as f32]),
                [c, m, y, k] => AnnotationColor::Cmyk([*c as f32, *m as f32, *y as f32, *k as f32]),
                _ => {
                    malformed = true;
                    AnnotationColor::Unspecified
                }
            },
            _ => {
                malformed = true;
                AnnotationColor::Unspecified
            }
        };
        let opacity = match dict.get::<f64>(b"CA") {
            Some(v) if v.is_finite() && (0.0..=1.0).contains(&v) => v as f32,
            None if dict.get::<Object<'_>>(b"CA").is_none() => 1.,
            _ => {
                malformed = true;
                1.
            }
        };
        let metadata = |key: &[u8]| {
            dict.get::<PdfString<'_>>(key)
                .map(|s| super::outline_title(s.as_bytes()))
        };
        // Slots are provenance inside this immutable baseline only. The epoch
        // prevents accidental reuse after reload; authors reserve the high bit.
        let serial = ((page_ix as u64) << 32) | slot as u64;
        entries.push(
            Annotation::new(
                AnnotationId::new(epoch, serial),
                page_ix,
                kind,
                quads,
                color,
            )
            .with_opacity(opacity)
            .with_metadata(metadata(b"T"), metadata(b"Contents"), metadata(b"NM"))
            .with_readonly(malformed)
            .with_fallback_rect(rect),
        );
    }
    let transform = PageTransform::from_coefficients(page.initial_transform(true).as_coeffs());
    entries.sort_by(|a, b| {
        let key = |a: &Annotation| {
            let bounds = page_bounds(a, transform);
            bounds
                .map(|b| (b[1], b[0]))
                .unwrap_or((f64::INFINITY, f64::INFINITY))
        };
        let (ay, ax) = key(a);
        let (by, bx) = key(b);
        ay.total_cmp(&by)
            .then(ax.total_cmp(&bx))
            .then(a.id().serial().cmp(&b.id().serial()))
    });
    Ok(entries)
}

fn numbers(array: Array<'_>) -> Option<Vec<f64>> {
    // Iterating as f64 would silently stop at the first malformed element.
    array
        .iter::<Object<'_>>()
        .map(|o| match o {
            Object::Number(n) if n.as_f64().is_finite() => Some(n.as_f64()),
            _ => None,
        })
        .collect()
}

pub(super) fn page_bounds(annotation: &Annotation, transform: PageTransform) -> Option<[f64; 4]> {
    let points: Vec<_> = if annotation.quads().is_empty() {
        let [x0, y0, x1, y1] = annotation.fallback_rect()?;
        vec![Point { x: x0, y: y0 }, Point { x: x1, y: y1 }]
    } else {
        annotation.quads().iter().flat_map(|q| q.points()).collect()
    };
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    // Rect fallback needs all four corners when a page is rotated.
    let points = if annotation.quads().is_empty() {
        let [x0, y0, x1, y1] = annotation.fallback_rect()?;
        vec![
            Point { x: x0, y: y0 },
            Point { x: x0, y: y1 },
            Point { x: x1, y: y0 },
            Point { x: x1, y: y1 },
        ]
    } else {
        points
    };
    for point in points {
        let p = transform.document_to_page(point);
        bounds[0] = bounds[0].min(p.x);
        bounds[1] = bounds[1].min(p.y);
        bounds[2] = bounds[2].max(p.x);
        bounds[3] = bounds[3].max(p.y);
    }
    bounds
        .iter()
        .all(|v| v.is_finite() && v.abs() <= f32::MAX as f64)
        .then_some(bounds)
}
