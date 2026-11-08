//! PDF link annotations decoded into document and unscaled rendered-page geometry.
//! This module is the only place that knows about Hayro's annotation dictionaries.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use hayro::hayro_syntax::object::{
    Array, Dict, MaybeRef, Object, ObjectIdentifier, String as PdfString,
};

use crate::coordinates::{PageTransform, Point};

use super::{OutlineDestination, named_destinations, open_pdf, outline_destination};

#[derive(Clone, Debug)]
pub struct PageLink {
    /// Rectangle in PDF document coordinates, clipped to the page's visible crop.
    document_rect: [f64; 4],
    /// Rectangle in unscaled, rotated rendered-page coordinates.
    page_rect: [f32; 4],
    destination: Option<LinkDestination>,
}

/// Backend-neutral destinations of supported PDF link annotations.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkDestination {
    SameDocument(OutlineDestination),
    AnotherDocument { path: PathBuf, page: usize },
}

impl PageLink {
    pub fn document_rect(&self) -> [f64; 4] {
        self.document_rect
    }
    pub fn page_rect(&self) -> [f32; 4] {
        self.page_rect
    }
    pub fn target(&self) -> Option<&LinkDestination> {
        self.destination.as_ref()
    }
    pub fn destination(&self) -> Option<OutlineDestination> {
        match self.destination.as_ref()? {
            LinkDestination::SameDocument(destination) => Some(*destination),
            LinkDestination::AnotherDocument { .. } => None,
        }
    }
}

/// Read only one page's links off the UI thread; invalid geometry is ignored,
/// unsupported actions retain a clickable region for non-modal feedback.
pub fn page_links(path: &Path, page_index: usize) -> Result<Vec<PageLink>, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    links_bytes(data, path, page_index)
}

fn links_bytes(data: Vec<u8>, path: &Path, page_index: usize) -> Result<Vec<PageLink>, String> {
    let pdf = open_pdf(data)?;
    let page = pdf
        .pages()
        .get(page_index)
        .ok_or_else(|| format!("PDF has no page {}", page_index + 1))?;
    let xref = pdf.xref();
    let Some(catalog) = xref.get::<Dict<'_>>(xref.root_id()) else {
        return Ok(Vec::new());
    };
    let page_ids: HashMap<_, _> = pdf
        .pages()
        .iter()
        .enumerate()
        .filter_map(|(ix, page)| page.raw().obj_id().map(|id| (id, ix)))
        .collect::<HashMap<ObjectIdentifier, usize>>();
    let names = named_destinations(&catalog);
    let crop = page.intersected_crop_box();
    let transform = PageTransform::from_coefficients(page.initial_transform(true).as_coeffs());
    let mut links = Vec::new();
    if let Some(annotations) = page.raw().get::<Array<'_>>(b"Annots") {
        for annotation in annotations.iter::<Dict<'_>>().take(10_000) {
            let Some(kind) = annotation.get::<hayro::hayro_syntax::object::Name<'_>>(b"Subtype")
            else {
                continue;
            };
            if kind.as_ref() != b"Link" {
                continue;
            }
            let Some(rect) = annotation.get::<Array<'_>>(b"Rect") else {
                continue;
            };
            let values: Vec<_> = rect.iter::<f64>().take(5).collect();
            if values.len() != 4 || values.iter().any(|v| !v.is_finite()) {
                continue;
            }
            let [x0, y0, x1, y1] = [values[0], values[1], values[2], values[3]];
            // PDF rectangles can be authored with reversed corners (Typst
            // writes top-to-bottom y coordinates). Normalize before clipping.
            let [x0, x1] = [x0.min(x1), x0.max(x1)];
            let [y0, y1] = [y0.min(y1), y0.max(y1)];
            if x0 == x1 || y0 == y1 {
                continue;
            }
            let [x0, y0, x1, y1] = [
                x0.max(crop.x0),
                y0.max(crop.y0),
                x1.min(crop.x1),
                y1.min(crop.y1),
            ];
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let corners = [
                Point { x: x0, y: y0 },
                Point { x: x0, y: y1 },
                Point { x: x1, y: y0 },
                Point { x: x1, y: y1 },
            ]
            .map(|point| transform.document_to_page(point));
            let [left, top, right, bottom] = [
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
            ];
            if [left, top, right, bottom]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > f32::MAX as f64)
            {
                continue;
            }
            let destination = annotation
                .get::<Object<'_>>(b"Dest")
                .and_then(|object| outline_destination(object, &pdf, &catalog, &names, &page_ids))
                .map(LinkDestination::SameDocument)
                .or_else(|| {
                    let action = annotation.get::<Dict<'_>>(b"A")?;
                    match action
                        .get::<hayro::hayro_syntax::object::Name<'_>>(b"S")?
                        .as_ref()
                    {
                        b"GoTo" => outline_destination(
                            action.get::<Object<'_>>(b"D")?,
                            &pdf,
                            &catalog,
                            &names,
                            &page_ids,
                        )
                        .map(LinkDestination::SameDocument),
                        b"GoToR" => remote_destination(&action, path)
                            .map(|(path, page)| LinkDestination::AnotherDocument { path, page }),
                        _ => None,
                    }
                });
            links.push(PageLink {
                document_rect: [x0, y0, x1, y1],
                page_rect: [left as f32, top as f32, right as f32, bottom as f32],
                destination,
            });
        }
    }
    Ok(links)
}

fn remote_destination(action: &Dict<'_>, source: &Path) -> Option<(PathBuf, usize)> {
    // Named remote destinations cannot be resolved against the source PDF.
    let file = match action.get::<Object<'_>>(b"F")? {
        Object::String(file) => file,
        Object::Dict(dict) => dict
            .get::<PdfString<'_>>(b"UF")
            .or_else(|| dict.get::<PdfString<'_>>(b"F"))?,
        _ => return None,
    };
    let filename = std::str::from_utf8(file.as_bytes()).ok()?;
    let file = Path::new(filename);
    if !file.extension()?.eq_ignore_ascii_case("pdf") {
        return None;
    }
    let path = if file.is_absolute() {
        file.to_path_buf()
    } else {
        source.parent()?.join(file)
    };
    let destination = action.get::<Object<'_>>(b"D")?;
    let page = match destination {
        Object::Array(array) => {
            let mut items = array.raw_iter();
            let page = match items.next()? {
                MaybeRef::NotRef(Object::Number(number)) => number.as_f64(),
                _ => return None,
            };
            match items.next()? {
                MaybeRef::NotRef(Object::Name(kind))
                    if matches!(kind.as_ref(), b"Fit" | b"FitB") => {}
                _ => return None,
            }
            page
        }
        Object::Number(number) => number.as_f64(),
        _ => return None,
    };
    if !page.is_finite() || page < 0.0 || page.fract() != 0.0 || page >= usize::MAX as f64 {
        return None;
    }
    Some((path, page as usize))
}

#[cfg(test)]
pub(crate) fn linked_fixture() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 13 0 R >> >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /CropBox [20 50 320 550] /Rotate 90 /Annots [5 0 R 6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R 12 0 R 14 0 R 16 0 R 17 0 R 18 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 1200] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 80 120 105] /Dest [4 0 R /XYZ 30 700 1.5] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 110 120 135] /A << /S /GoTo /D (chapter) >> >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 140 120 165] /Dest [4 0 R /Fit] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 170 120 195] /Dest [4 0 R /FitH 300] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 200 120 225] /Dest [99 0 R /Fit] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 230 120 255] /Dest [3 0 R /XYZ 0 0 0] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 260 120 285] /A << /S /URI /URI (https://example.org) >> >>",
        "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /Dest [4 0 R /Fit] >>",
        "<< /Names [(chapter) 15 0 R] >>",
        "<< /Type /Annot /Subtype /Link /Rect [0 0 60 90] /Dest [4 0 R /FitR 0 0 40 40] >>",
        "<< /D [3 0 R /XYZ 100 80 0] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 320 120 345] /Dest [4 0 R /XYZ (bad) 500 0] >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 350 120 375] /A << /S /GoToR /F (other.pdf) /D [1 /Fit] >> >>",
        "<< /Type /Annot /Subtype /Link /Rect [40 380 120 405] /A << /S /GoToR /F (other.txt) /D [0 /Fit] >> >>",
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (ix, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", ix + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::DestinationFit;

    #[test]
    fn annotations_decode_named_fit_rotated_and_invalid_destinations() {
        let links = links_bytes(linked_fixture(), Path::new("/books/source.pdf"), 0).unwrap();
        assert_eq!(links.len(), 11); // The annotation wholly outside the crop is skipped.
        assert_eq!(links[0].document_rect(), [40.0, 80.0, 120.0, 105.0]);
        assert_eq!(links[7].document_rect(), [20.0, 50.0, 60.0, 90.0]);
        let first = links[0].destination().unwrap();
        assert_eq!(first.page(), 1);
        assert_eq!(first.zoom(), Some(1.5));
        let named = links[1].destination().unwrap();
        assert_eq!(named.page(), 0);
        assert_eq!(named.point(), Some(Point { x: 100.0, y: 80.0 }));
        assert!((named.page_position().unwrap().1 - 80.0).abs() < 0.001);
        assert_eq!(
            links[2].destination().unwrap().fit(),
            Some(DestinationFit::Page)
        );
        assert_eq!(
            links[3].destination().unwrap().fit(),
            Some(DestinationFit::Width)
        );
        assert_eq!(
            links[3].destination().unwrap().page_position().unwrap().1,
            900.0
        );
        assert!(links[4..9].iter().all(|link| link.target().is_none()));
        assert_eq!(
            links[9].target(),
            Some(&LinkDestination::AnotherDocument {
                path: PathBuf::from("/books/other.pdf"),
                page: 1
            })
        );
        assert!(links[10].target().is_none());
        assert!(links_bytes(linked_fixture(), Path::new("/books/source.pdf"), 2).is_err());
    }
}
