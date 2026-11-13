//! Standard annotation encoding, separate from Hayro rendering and filesystem
//! transactions. All entry points run on a background worker.
//!
//! Only newly authored highlights are appended. Existing dictionaries, including
//! unsupported annotations, are never interpreted as instructions to rewrite.

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
pub(super) fn append_highlights(
    baseline: &[u8],
    additions: &[Annotation],
) -> Result<Vec<u8>, String> {
    let previous = editable_document(baseline)?;
    let pages: Vec<_> = previous.get_pages().into_values().collect();
    let mut incremental = IncrementalDocument::create_from(baseline.to_vec(), previous);
    for (page_ix, &page) in pages.iter().enumerate() {
        let annotations: Vec<_> = additions
            .iter()
            .filter(|annotation| annotation.page() == page_ix)
            .collect();
        if annotations.is_empty() {
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
            let id = highlight(&mut incremental.new_document, page, annotation, &name)?;
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

fn highlight(
    document: &mut Document,
    page: ObjectId,
    annotation: &Annotation,
    name: &str,
) -> Result<ObjectId, String> {
    if annotation.kind() != MarkupKind::Highlight || annotation.quads().is_empty() {
        return Err("Only selected-text highlights can be created in this version.".into());
    }
    let AnnotationColor::Rgb(color) = annotation.color() else {
        return Err("New highlights require an RGB color.".into());
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
    let bounds = [
        points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
        points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
    ];
    // Absolute document coordinates in both Rect and BBox avoid applying page
    // crop or rotation twice. Draw each quad, not the annotation's union Rect.
    let graphics = document.add_object(dictionary! {
        "Type" => "ExtGState", "BM" => "Multiply",
        "ca" => annotation.opacity(),
    });
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec!["GS".into()]),
        Operation::new("rg", color.into_iter().map(Object::Real).collect()),
    ];
    for quad in annotation.quads() {
        let [ul, ur, ll, lr] = quad.points();
        for (operator, point) in [("m", ul), ("l", ur), ("l", lr), ("l", ll)] {
            operations.push(Operation::new(
                operator,
                vec![Object::Real(point.x as f32), Object::Real(point.y as f32)],
            ));
        }
        operations.push(Operation::new("h", vec![]));
        operations.push(Operation::new("f", vec![]));
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
    let mut dict = dictionary! {
        "Type" => "Annot", "Subtype" => "Highlight", "P" => page,
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
}
