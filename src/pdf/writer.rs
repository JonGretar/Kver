//! Standard annotation encoding, separate from Hayro rendering and filesystem
//! transactions. All entry points run on a background worker.
//!
//! Imported dictionaries are copied without geometry/metadata normalization.
//! Only explicitly edited text markup is patched; unsupported entries stay intact.

use lopdf::{
    Document, IncrementalDocument, Object, ObjectId, Stream,
    content::{Content, Operation},
    dictionary,
};

use super::annotations::{Annotation, AnnotationColor, MarkupKind};

fn error(error: impl std::fmt::Display) -> String {
    format!("Cannot write PDF annotations: {error}")
}

/// Reject editing without decrypting, bypassing permissions, or attempting to
/// preserve a signature's validity. An empty signature field alone is not a
/// signature; actual signature dictionaries may be direct or indirect.
pub(super) fn editing_rejection(bytes: &[u8]) -> Option<String> {
    editable_document(bytes).err()
}

fn editable_document(bytes: &[u8]) -> Result<Document, String> {
    // lopdf's normal loader automatically decrypts an empty-user-password PDF
    // and removes /Encrypt from its parsed trailer. Inspect the original
    // encryption declaration first; successful decryption is not editing consent.
    let metadata = Document::load_metadata_mem(bytes)
        .map_err(|error| format!("Annotation editing is unavailable: {error}"))?;
    if metadata.encrypted {
        return Err("Annotation editing is unavailable for encrypted PDFs.".into());
    }
    let document = Document::load_mem(bytes).map_err(error)?;
    if let Some(reason) = rejection_in_document(&document) {
        return Err(reason);
    }
    Ok(document)
}

fn rejection_in_document(document: &Document) -> Option<String> {
    if document.trailer.has(b"Encrypt") || document.is_encrypted() {
        return Some("Annotation editing is unavailable for encrypted PDFs.".into());
    }
    let signed = document
        .objects
        .values()
        .any(|object| contains_signature(object, 0));
    signed.then(|| "Annotation editing is unavailable for signed PDFs.".into())
}

fn contains_signature(object: &Object, depth: usize) -> bool {
    // References are inspected as independent loaded objects; do not recursively
    // resolve them and risk cycles. Excessive nesting fails closed for editing.
    if depth >= 128 {
        return true;
    }
    match object {
        Object::Dictionary(dict) => {
            dict.get(b"Type").and_then(Object::as_name).ok() == Some(b"Sig")
                || dict.has(b"ByteRange")
                || dict.has(b"DocMDP")
                || dict
                    .iter()
                    .any(|(_, value)| contains_signature(value, depth + 1))
        }
        Object::Array(array) => array
            .iter()
            .any(|value| contains_signature(value, depth + 1)),
        Object::Stream(stream) => stream
            .dict
            .iter()
            .any(|(_, value)| contains_signature(value, depth + 1)),
        _ => false,
    }
}

/// Encode a coherent snapshot against the exact loaded bytes, never the current
/// contents of the source path. The caller supplies only additions since that
/// baseline; successful Save As establishes a new baseline for the next call.
#[cfg(test)]
pub(super) fn append_highlights(
    baseline: &[u8],
    additions: &[Annotation],
) -> Result<Vec<u8>, String> {
    write_highlights(baseline, additions, &[])
}

/// Imported identities refer to slots in this exact immutable editing source.
/// A missing current entry means deletion; unchanged entries are never rewritten.
pub(super) fn reconcile_highlights(
    baseline: &[u8],
    original: &[Annotation],
    current: &[Annotation],
) -> Result<Vec<u8>, String> {
    let additions: Vec<_> = current
        .iter()
        .filter(|annotation| annotation.id().serial() & (1 << 63) != 0)
        .cloned()
        .collect();
    let by_id: std::collections::HashMap<_, _> = current.iter().map(|a| (a.id(), a)).collect();
    let changes: Vec<_> = original
        .iter()
        .filter_map(|before| {
            let after = by_id.get(&before.id()).copied();
            (after != Some(before)).then_some((before, after))
        })
        .collect();
    write_highlights(baseline, &additions, &changes)
}

fn write_highlights(
    baseline: &[u8],
    additions: &[Annotation],
    changes: &[(&Annotation, Option<&Annotation>)],
) -> Result<Vec<u8>, String> {
    let previous = editable_document(baseline)?;
    let pages: Vec<_> = previous.get_pages().into_values().collect();
    let mut incremental = IncrementalDocument::create_from(baseline.to_vec(), previous);
    for (page_ix, &page) in pages.iter().enumerate() {
        let annotations: Vec<_> = additions
            .iter()
            .filter(|annotation| annotation.page() == page_ix)
            .collect();
        let edits: Vec<_> = changes
            .iter()
            .filter(|(before, _)| before.page() == page_ix)
            .collect();
        if annotations.is_empty() && edits.is_empty() {
            continue;
        }
        let previous = incremental.get_prev_documents();
        let mut page_dict = previous.get_dictionary(page).map_err(error)?.clone();
        // A malformed Annots value is not permission to drop existing data.
        let mut entries = match page_dict.get(b"Annots") {
            Ok(value) => previous
                .dereference(value)
                .map_err(error)?
                .1
                .as_array()
                .map_err(error)?
                .clone(),
            Err(lopdf::Error::DictKey(_)) => Vec::new(),
            Err(failure) => return Err(error(failure)),
        };
        let mut names: std::collections::HashSet<String> = entries
            .iter()
            .filter_map(|entry| {
                let dictionary = previous.dereference(entry).ok()?.1.as_dict().ok()?;
                Some(super::outline_title(
                    dictionary.get(b"NM").ok()?.as_str().ok()?,
                ))
            })
            .collect();
        // Apply in reverse slot order so deleting an entry never shifts another
        // target. Clone dictionaries instead of changing potentially shared objects.
        let mut edits = edits;
        edits.sort_by_key(|(before, _)| std::cmp::Reverse(before.id().serial()));
        for (before, after) in edits {
            if before.is_readonly() {
                return Err("This annotation cannot be edited.".into());
            }
            let slot = (before.id().serial() & 0xffff_ffff) as usize;
            let entry = entries
                .get(slot)
                .ok_or("Annotation reference is obsolete.")?;
            if let Some(after) = after {
                let mut dictionary = incremental
                    .get_prev_documents()
                    .dereference(entry)
                    .map_err(error)?
                    .1
                    .as_dict()
                    .map_err(error)?
                    .clone();
                let generated = markup(&mut incremental.new_document, page, after, "")?;
                let appearance = incremental
                    .new_document
                    .get_dictionary(generated)
                    .map_err(error)?
                    .clone();
                // Keep exact stored QuadPoints, Rect, author, comment, flags,
                // private keys and all unrelated metadata from the imported PDF.
                dictionary.set("C", appearance.get(b"C").map_err(error)?.clone());
                dictionary.set("AP", appearance.get(b"AP").map_err(error)?.clone());
                entries[slot] = Object::Reference(incremental.new_document.add_object(dictionary));
            } else {
                entries.remove(slot);
            }
        }
        for annotation in annotations {
            // Epoch/serial is UI identity, not a durable globally unique name.
            // A later application process can reuse that pair after reopening.
            let id = annotation.id();
            let base = format!("kver-{}-{}", id.epoch(), id.serial());
            let mut name = base.clone();
            let mut suffix = 1_u64;
            while !names.insert(name.clone()) {
                name = format!("{base}-{suffix}");
                suffix += 1;
            }
            let id = markup(&mut incremental.new_document, page, annotation, &name)?;
            entries.push(Object::Reference(id));
        }
        page_dict.set("Annots", entries);
        incremental
            .new_document
            .objects
            .insert(page, page_dict.into());
    }
    if additions
        .iter()
        .any(|annotation| annotation.page() >= pages.len())
    {
        return Err("Cannot write annotation on a missing PDF page.".into());
    }
    let mut bytes = Vec::new();
    incremental.save_to(&mut bytes).map_err(error)?;
    if !bytes.starts_with(baseline) {
        return Err("PDF writer did not preserve the loaded baseline.".into());
    }
    Ok(bytes)
}

fn numbers(values: impl IntoIterator<Item = f64>) -> Object {
    Object::Array(
        values
            .into_iter()
            .map(|value| Object::Real(value as f32))
            .collect(),
    )
}

fn markup(
    document: &mut Document,
    page: ObjectId,
    annotation: &Annotation,
    name: &str,
) -> Result<ObjectId, String> {
    if annotation.quads().is_empty() {
        return Err("Cannot write annotation without quadrilaterals.".into());
    }
    let AnnotationColor::Rgb(color) = annotation.color() else {
        return Err("New markup requires an RGB color.".into());
    };
    if !color
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        || !annotation.opacity().is_finite()
        || !(0.0..=1.0).contains(&annotation.opacity())
    {
        return Err("Cannot write invalid annotation color or opacity.".into());
    }
    let points: Vec<_> = annotation
        .quads()
        .iter()
        .flat_map(|quad| quad.points())
        .collect();
    if points.iter().any(|point| {
        !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > f32::MAX as f64
            || point.y.abs() > f32::MAX as f64
    }) {
        return Err("Cannot write invalid annotation geometry.".into());
    }
    let bounds = annotation.fallback_rect().unwrap_or([
        points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
        points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
    ]);
    // Absolute document coordinates in both Rect and BBox avoid applying page
    // crop or rotation twice. Draw each quad, not the annotation's union Rect.
    let graphics = document.add_object(dictionary! {
        "Type" => "ExtGState", "BM" => "Multiply",
        "ca" => annotation.opacity(), "CA" => annotation.opacity(),
    });
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec!["GS".into()]),
        Operation::new("rg", color.into_iter().map(Object::Real).collect()),
        Operation::new("RG", color.into_iter().map(Object::Real).collect()),
    ];
    for quad in annotation.quads() {
        if annotation.kind() == MarkupKind::Highlight {
            for (ix, point) in quad.perimeter().into_iter().enumerate() {
                let operator = if ix == 0 { "m" } else { "l" };
                operations.push(Operation::new(
                    operator,
                    vec![Object::Real(point.x as f32), Object::Real(point.y as f32)],
                ));
            }
            operations.push(Operation::new("h", vec![]));
            operations.push(Operation::new("f", vec![]));
        } else {
            line_markup(&mut operations, *quad, annotation.kind());
        }
    }
    operations.push(Operation::new("Q", vec![]));
    let appearance = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "FormType" => 1,
            "BBox" => numbers(bounds),
            "Resources" => dictionary! { "ExtGState" => dictionary! { "GS" => graphics } },
        },
        Content { operations }.encode().map_err(error)?,
    ));
    let subtype = match annotation.kind() {
        MarkupKind::Highlight => "Highlight",
        MarkupKind::Underline => "Underline",
        MarkupKind::StrikeOut => "StrikeOut",
        MarkupKind::Squiggly => "Squiggly",
    };
    let mut dict = dictionary! {
        "Type" => "Annot", "Subtype" => subtype, "P" => page,
        "Rect" => numbers(bounds),
        "QuadPoints" => numbers(points.iter().flat_map(|point| [point.x, point.y])),
        "C" => Object::Array(color.into_iter().map(Object::Real).collect()),
        "CA" => annotation.opacity(), "F" => 4,
        "NM" => Object::string_literal(name),
        "AP" => dictionary! { "N" => appearance },
    };
    // Authorship is explicit metadata, never the user's OS account. Quotation
    // recovery belongs to the reader, not the PDF's attached comment field.
    if let Some(author) = annotation.author().filter(|author| !author.is_empty()) {
        let mut encoded = vec![0xfe, 0xff];
        encoded.extend(author.encode_utf16().flat_map(u16::to_be_bytes));
        dict.set("T", Object::string_literal(encoded));
    }
    Ok(document.add_object(dict))
}

/// Paint in the quad's text frame, not page axes: rotation/crop are applied by
/// the page. The first edge defines text direction in both PDF and Z ordering.
fn line_markup(operations: &mut Vec<Operation>, quad: super::annotations::Quad, kind: MarkupKind) {
    let [top_left, top_right, a, b] = quad.points();
    let dx = top_right.x - top_left.x;
    let dy = top_right.y - top_left.y;
    let projection =
        |p: crate::coordinates::Point| (p.x - top_left.x) * dx + (p.y - top_left.y) * dy;
    let (bottom_left, bottom_right) = if projection(a) < projection(b) {
        (a, b)
    } else {
        (b, a)
    };
    let height = (top_left.x - bottom_left.x).hypot(top_left.y - bottom_left.y);
    let width = dx.hypot(dy);
    operations.push(Operation::new(
        "w",
        vec![Object::Real((height / 16.) as f32)],
    ));
    let segments = if kind == MarkupKind::Squiggly {
        ((width / (height * 0.15)).ceil() as usize).clamp(2, 4096)
    } else {
        1
    };
    for ix in 0..=segments {
        let t = ix as f64 / segments as f64;
        let up = match kind {
            MarkupKind::StrikeOut => 0.5,
            MarkupKind::Squiggly if ix % 2 == 1 => 0.2,
            _ => 0.12,
        };
        let bottom = crate::coordinates::Point {
            x: bottom_left.x + (bottom_right.x - bottom_left.x) * t,
            y: bottom_left.y + (bottom_right.y - bottom_left.y) * t,
        };
        let top = crate::coordinates::Point {
            x: top_left.x + dx * t,
            y: top_left.y + dy * t,
        };
        operations.push(Operation::new(
            if ix == 0 { "m" } else { "l" },
            vec![
                Object::Real((bottom.x + (top.x - bottom.x) * up) as f32),
                Object::Real((bottom.y + (top.y - bottom.y) * up) as f32),
            ],
        ));
    }
    operations.push(Operation::new("S", vec![]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        coordinates::Point,
        pdf::annotations::{AnnotationId, Quad},
    };

    fn annotation() -> Annotation {
        let quads =
            [[48., 720., 170., 734.], [48., 700., 145., 714.]].map(|[left, bottom, right, top]| {
                Quad::new([
                    Point { x: left, y: top },
                    Point { x: right, y: top },
                    Point { x: left, y: bottom },
                    Point {
                        x: right,
                        y: bottom,
                    },
                ])
                .unwrap()
            });
        Annotation::new(
            AnnotationId::new(7, (1 << 63) | 1),
            0,
            MarkupKind::Highlight,
            quads.to_vec(),
            AnnotationColor::Rgb([1., 0.85, 0.]),
        )
        .with_opacity(0.45)
    }

    /// Opt-in independent renderer check; outputs are also disposable native
    /// reader specimens. Never clean malformed arrays to make this check pass.
    #[test]
    #[ignore = "requires Poppler pdftoppm; writes target/markup-interchange specimens"]
    fn all_created_types_render_independently_in_cropped_rotated_regions() {
        use super::super::annotations::HighlightColor;
        let directory = std::path::Path::new("target/markup-interchange");
        std::fs::create_dir_all(directory).unwrap();
        for rotation in [0, 90, 180, 270] {
            for kind in MarkupKind::ALL {
                let mut source = Document::load("examples/selection-sample.pdf").unwrap();
                let page = source.get_pages()[&1];
                let page_dict = source.get_dictionary_mut(page).unwrap();
                page_dict.set("CropBox", numbers([20., 30., 590., 780.]));
                page_dict.set("Rotate", rotation);
                let mut baseline = Vec::new();
                source.save_to(&mut baseline).unwrap();
                let region = annotation();
                let authored = Annotation::new(
                    region.id(),
                    0,
                    kind,
                    region.quads().to_vec(),
                    HighlightColor::Red.color(),
                )
                .with_opacity(if kind == MarkupKind::Highlight {
                    0.45
                } else {
                    1.
                })
                .with_metadata(Some("Kver interchange".into()), None, None);
                let output =
                    reconcile_highlights(&baseline, &[], std::slice::from_ref(&authored)).unwrap();
                let stem = directory.join(format!("{kind:?}-{rotation}"));
                let path = stem.with_extension("pdf");
                std::fs::write(&path, output).unwrap();
                // Hayro is an independent parser from lopdf, and this reopens
                // the saved bytes rather than reading the in-memory catalog.
                let reopened = super::super::open_pdf(std::fs::read(&path).unwrap()).unwrap();
                let readback =
                    super::super::annotation_read::page_annotations(&reopened, 0, 99).unwrap();
                assert_eq!(readback.len(), 1);
                assert_eq!(readback[0].kind(), kind);
                assert_eq!(readback[0].quads(), authored.quads());
                assert_eq!(readback[0].author(), authored.author());
                let transform = super::super::page_geometry(&path, 0).unwrap().transform;
                let result = std::process::Command::new("pdftoppm")
                    .args(["-f", "1", "-l", "1", "-r", "72", "-cropbox", "-singlefile"])
                    .arg(&path)
                    .arg(&stem)
                    .output()
                    .expect("install Poppler pdftoppm");
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let ppm = std::fs::read(stem.with_extension("ppm")).unwrap();
                let mut parts = ppm.splitn(4, |&byte| byte == b'\n');
                assert_eq!(parts.next().unwrap(), b"P6");
                let dimensions = std::str::from_utf8(parts.next().unwrap())
                    .unwrap()
                    .split_whitespace()
                    .map(|n| n.parse::<usize>().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(parts.next().unwrap(), b"255");
                let pixels = parts.next().unwrap();
                let [width, height] = [dimensions[0], dimensions[1]];
                assert_eq!(pixels.len(), width * height * 3);
                // Every disjoint region must actually paint. Outside all quads
                // must not acquire a fill connecting the two text fragments.
                let regions = authored
                    .quads()
                    .iter()
                    .map(|quad| {
                        let points = quad.points().map(|p| transform.document_to_page(p));
                        [
                            points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
                            points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
                            points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
                            points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
                        ]
                    })
                    .collect::<Vec<_>>();
                let mut counts = vec![0; regions.len()];
                for (ix, rgb) in pixels.as_chunks::<3>().0.iter().enumerate() {
                    if rgb[0] > 150
                        && rgb[0] > rgb[1].saturating_add(50)
                        && rgb[0] > rgb[2].saturating_add(50)
                    {
                        let (x, y) = ((ix % width) as f64 + 0.5, (ix / width) as f64 + 0.5);
                        let region = regions.iter().position(|b| {
                            x >= b[0] - 1. && y >= b[1] - 1. && x <= b[2] + 1. && y <= b[3] + 1.
                        });
                        let region = region.expect("appearance painted outside selected regions");
                        counts[region] += 1;
                        if kind != MarkupKind::Highlight {
                            let document = transform.page_to_document(Point { x, y }).unwrap();
                            let points = authored.quads()[region].points();
                            let up = (document.y - points[2].y) / (points[0].y - points[2].y);
                            let band = if kind == MarkupKind::StrikeOut {
                                0.4..0.6
                            } else {
                                0.03..0.29
                            };
                            assert!(
                                band.contains(&up),
                                "{kind:?}/{rotation}: line at wrong text height {up}"
                            );
                        }
                    }
                }
                assert!(
                    counts.iter().all(|&count| count > 20),
                    "{kind:?}/{rotation}: {counts:?}"
                );
                if kind != MarkupKind::Highlight {
                    assert!(
                        counts.iter().all(|&count| count < 500),
                        "line appearance became a fill: {counts:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn append_preserves_baseline_page_content_metadata_and_other_annotations() {
        let original = std::fs::read("examples/field-guide.pdf").unwrap();
        let previous = Document::load_mem(&original).unwrap();
        let output = append_highlights(&original, &[annotation()]).unwrap();
        assert!(output.starts_with(&original));
        let saved = Document::load_mem(&output).unwrap();
        assert_eq!(saved.get_pages(), previous.get_pages());
        assert_eq!(
            saved.trailer.get(b"Info").ok(),
            previous.trailer.get(b"Info").ok()
        );
        if let Ok(info) = previous.trailer.get(b"Info") {
            assert_eq!(
                previous.dereference(info).unwrap().1,
                saved
                    .dereference(saved.trailer.get(b"Info").unwrap())
                    .unwrap()
                    .1
            );
        }
        assert_eq!(saved.catalog().unwrap(), previous.catalog().unwrap());
        for page in previous.get_pages().into_values() {
            assert_eq!(
                saved.get_page_content(page),
                previous.get_page_content(page)
            );
            let before = previous.get_dictionary(page).unwrap();
            let after = saved.get_dictionary(page).unwrap();
            let mut before_page = before.clone();
            let mut after_page = after.clone();
            before_page.remove(b"Annots");
            after_page.remove(b"Annots");
            assert_eq!(before_page, after_page);
            if let Ok(annots) = before.get(b"Annots") {
                let old = previous.dereference(annots).unwrap().1.as_array().unwrap();
                let new = saved
                    .dereference(after.get(b"Annots").unwrap())
                    .unwrap()
                    .1
                    .as_array()
                    .unwrap();
                assert_eq!(&new[..old.len()], old);
                for object in old {
                    assert_eq!(
                        previous.dereference(object).unwrap().1,
                        saved.dereference(object).unwrap().1
                    );
                }
            }
        }
        let page = saved.get_pages()[&1];
        let annots = saved
            .get_dictionary(page)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        let added = saved
            .dereference(annots.last().unwrap())
            .unwrap()
            .1
            .as_dict()
            .unwrap();
        assert_eq!(
            added.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Highlight"
        );
        assert_eq!(
            added.get(b"QuadPoints").unwrap().as_array().unwrap().len(),
            16
        );
        assert!(
            !added.has(b"T"),
            "blank author must not infer a personal identity"
        );
        assert!(
            !added.has(b"Contents"),
            "quotation is not an attached comment"
        );
        let normal = added
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap();
        assert!(saved.dereference(normal).unwrap().1.as_stream().is_ok());
        // Independent production read adapter, not the writer's own lookup.
        let parsed = super::super::open_pdf(output).unwrap();
        let annotations = super::super::annotation_read::page_annotations(&parsed, 0, 99).unwrap();
        let readback = annotations.last().unwrap();
        assert_eq!(readback.kind(), MarkupKind::Highlight);
        assert_eq!(readback.quads(), annotation().quads());
    }

    #[test]
    fn saved_unicode_author_preserves_imported_authorship_and_metadata() {
        let original = std::fs::read("examples/annotations.pdf").unwrap();
        let previous = Document::load_mem(&original).unwrap();
        let authored = annotation().with_metadata(Some("Jón 中文".into()), None, None);
        let output = append_highlights(&original, &[authored]).unwrap();
        let saved = Document::load_mem(&output).unwrap();
        // Independent parser verifies the actual PDF /T string, not writer readback.
        let first_page = previous.get_pages()[&1];
        let old = previous
            .get_dictionary(first_page)
            .unwrap()
            .get(b"Annots")
            .unwrap();
        let old = previous.dereference(old).unwrap().1.as_array().unwrap();
        let new = saved
            .get_dictionary(first_page)
            .unwrap()
            .get(b"Annots")
            .unwrap();
        let new = saved.dereference(new).unwrap().1.as_array().unwrap();
        assert_eq!(new.len(), old.len() + 1);
        for object in old {
            assert_eq!(
                previous.dereference(object).unwrap().1,
                saved.dereference(object).unwrap().1
            );
        }
        let added = saved
            .dereference(new.last().unwrap())
            .unwrap()
            .1
            .as_dict()
            .unwrap();
        assert_eq!(
            lopdf::decode_text_string(added.get(b"T").unwrap()).unwrap(),
            "Jón 中文"
        );
        assert_eq!(previous.catalog().unwrap(), saved.catalog().unwrap());
        for page in previous.get_pages().into_values() {
            assert_eq!(
                previous.get_page_content(page),
                saved.get_page_content(page)
            );
        }
        if let Ok(info) = previous.trailer.get(b"Info") {
            assert_eq!(
                previous.dereference(info).unwrap().1,
                saved
                    .dereference(saved.trailer.get(b"Info").unwrap())
                    .unwrap()
                    .1
            );
        }
        let parsed = super::super::open_pdf(output).unwrap();
        let annotations = super::super::annotation_read::page_annotations(&parsed, 0, 99).unwrap();
        assert!(
            annotations
                .iter()
                .any(|annotation| annotation.author() == Some("Jón 中文"))
        );
    }

    #[test]
    fn rejects_signature_and_encryption_declarations_without_writing() {
        let baseline = std::fs::read("examples/field-guide.pdf").unwrap();
        for marker in ["signature", "encryption"] {
            let mut source = Document::load_mem(&baseline).unwrap();
            if marker == "signature" {
                source.add_object(dictionary! {
                    "Type" => "Sig", "ByteRange" => vec![0.into(), 10.into(), 20.into(), 30.into()],
                });
            } else {
                source
                    .trailer
                    .set("Encrypt", dictionary! { "Filter" => "Standard" });
            }
            let mut bytes = Vec::new();
            source.save_to(&mut bytes).unwrap();
            assert!(editing_rejection(&bytes).is_some(), "{marker}");
            assert!(
                append_highlights(&bytes, &[annotation()]).is_err(),
                "{marker}"
            );
        }
    }

    #[test]
    fn reopening_in_another_process_does_not_reuse_an_existing_annotation_name() {
        let baseline = std::fs::read("examples/selection-sample.pdf").unwrap();
        let first = append_highlights(&baseline, &[annotation()]).unwrap();
        let old = annotation();
        let next = Annotation::new(
            old.id(),
            0,
            MarkupKind::Highlight,
            vec![
                Quad::new([
                    Point { x: 54., y: 680. },
                    Point { x: 160., y: 680. },
                    Point { x: 54., y: 666. },
                    Point { x: 160., y: 666. },
                ])
                .unwrap(),
            ],
            old.color(),
        )
        .with_opacity(old.opacity());
        let second = append_highlights(&first, &[next]).unwrap();
        let parsed = super::super::open_pdf(second).unwrap();
        let annotations = super::super::annotation_read::page_annotations(&parsed, 0, 99).unwrap();
        assert_eq!(annotations.len(), 2);
        assert_ne!(annotations[0].name(), annotations[1].name());
        assert!(
            annotations
                .iter()
                .all(|annotation| annotation.name().is_some())
        );
    }

    #[test]
    fn recolor_and_delete_preserve_exact_imported_geometry_metadata_and_unrelated_objects() {
        use super::super::annotations::HighlightColor;
        let directory = tempfile::tempdir().unwrap();
        for (rotation, subtype) in [0, 90, 180, 270].into_iter().flat_map(|rotation| {
            ["Highlight", "Underline", "StrikeOut", "Squiggly"].map(|subtype| (rotation, subtype))
        }) {
            let mut source = Document::load("examples/annotations.pdf").unwrap();
            let page = (3, 0);
            let page_dict = source.get_dictionary_mut(page).unwrap();
            page_dict.set("Rotate", rotation);
            page_dict.set("CropBox", numbers([20., 30., 380., 570.]));
            let imported = source.get_dictionary_mut((7, 0)).unwrap();
            imported.set("Subtype", Object::Name(subtype.as_bytes().to_vec()));
            // Two skewed, disjoint quads, one in perimeter order and one in Z
            // order. Rect deliberately exceeds the geometry: AP must not stretch.
            imported.set(
                "QuadPoints",
                numbers([
                    40.125, 496.25, 110.5, 499.75, 109.25, 480.125, 39.5, 476.75, 125.75, 500.5,
                    190.125, 497.25, 124.5, 480.75, 189.25, 477.125,
                ]),
            );
            imported.set("Rect", numbers([35., 470., 200., 505.]));
            imported.set("C", numbers([0.2, 0.3, 0.4, 0.1]));
            imported.set("CA", Object::Real(0.37));
            imported.set(
                "PrivateMetadata",
                dictionary! {
                    "Nested" => vec![Object::string_literal("preserve me"), Object::Integer(42)],
                },
            );
            let mut baseline = Vec::new();
            source.save_to(&mut baseline).unwrap();
            let parsed = super::super::open_pdf(baseline.clone()).unwrap();
            let original = super::super::annotation_read::page_annotations(&parsed, 0, 77).unwrap();
            let target = original
                .iter()
                .find(|a| a.name() == Some("imported-highlight"))
                .unwrap();
            let current: Vec<_> = original
                .iter()
                .map(|a| {
                    if a.id() == target.id() {
                        a.clone().with_color(HighlightColor::Blue.color())
                    } else {
                        a.clone()
                    }
                })
                .collect();
            let output = reconcile_highlights(&baseline, &original, &current).unwrap();
            let path = directory
                .path()
                .join(format!("recolored-{subtype}-{rotation}.pdf"));
            std::fs::write(&path, &output).unwrap();
            let saved = Document::load(&path).unwrap();
            let before_entries = source
                .get_dictionary(page)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap();
            let after_entries = saved
                .get_dictionary(page)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap();
            assert_eq!(after_entries.len(), before_entries.len());
            for (slot, (before, after)) in before_entries.iter().zip(after_entries).enumerate() {
                if slot != 3 {
                    assert_eq!(before, after);
                    assert_eq!(
                        source.dereference(before).unwrap().1,
                        saved.dereference(after).unwrap().1
                    );
                } else {
                    let before = source.dereference(before).unwrap().1.as_dict().unwrap();
                    let after = saved.dereference(after).unwrap().1.as_dict().unwrap();
                    for (key, value) in before
                        .iter()
                        .filter(|(key, _)| key.as_slice() != b"C" && key.as_slice() != b"AP")
                    {
                        assert_eq!(
                            after.get(key).unwrap(),
                            value,
                            "key {key:?}, rotation {rotation}"
                        );
                    }
                    let normal = after
                        .get(b"AP")
                        .unwrap()
                        .as_dict()
                        .unwrap()
                        .get(b"N")
                        .unwrap();
                    let appearance = saved.dereference(normal).unwrap().1.as_stream().unwrap();
                    assert_eq!(
                        appearance.dict.get(b"BBox").unwrap(),
                        before.get(b"Rect").unwrap()
                    );
                    let operations = Content::decode(&appearance.content).unwrap().operations;
                    let paint = if subtype == "Highlight" { "f" } else { "S" };
                    assert_eq!(
                        operations.iter().filter(|op| op.operator == paint).count(),
                        2
                    );
                }
            }
            assert_eq!(source.get_page_content(page), saved.get_page_content(page));
            // Read back with the independent Hayro adapter, not lopdf.
            let reopened = super::super::open_pdf(std::fs::read(path).unwrap()).unwrap();
            let readback =
                super::super::annotation_read::page_annotations(&reopened, 0, 78).unwrap();
            let readback = readback.iter().find(|a| a.name() == target.name()).unwrap();
            assert_eq!(readback.quads(), target.quads());
            assert_eq!(readback.author(), target.author());
            assert_eq!(readback.comment(), target.comment());
            assert_eq!(readback.opacity(), target.opacity());
            assert_eq!(readback.color(), HighlightColor::Blue.color());
            assert_eq!(readback.kind(), target.kind());

            let current: Vec<_> = current
                .into_iter()
                .filter(|a| a.id() != target.id())
                .collect();
            let deleted = reconcile_highlights(&baseline, &original, &current).unwrap();
            let path = directory
                .path()
                .join(format!("deleted-{subtype}-{rotation}.pdf"));
            std::fs::write(&path, deleted).unwrap();
            let deleted = Document::load(path).unwrap();
            let entries = deleted
                .get_dictionary(page)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap();
            let mut expected = before_entries.clone();
            expected.remove(3);
            assert_eq!(entries, &expected, "only selected Annots slot is removed");
            assert_eq!(
                source.get_page_content(page),
                deleted.get_page_content(page)
            );
            for (&id, object) in &source.objects {
                if id != page {
                    assert_eq!(
                        deleted.get_object(id).unwrap(),
                        object,
                        "unrelated object {id:?}"
                    );
                }
            }
        }
    }
}
