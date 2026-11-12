use super::*;
use crate::pdf::annotations::{AnnotationColor, MarkupKind};

#[test]
fn imported_markup_retains_all_types_and_malformed_navigable_entries_in_spatial_order() {
    let pdf = super::open_pdf(std::fs::read("examples/annotations.pdf").unwrap()).unwrap();
    let annotations = annotation_read::page_annotations(&pdf, 0, 42).unwrap();
    assert_eq!(annotations.len(), 7);
    assert_eq!(
        annotations
            .iter()
            .take(4)
            .map(|a| a.kind())
            .collect::<Vec<_>>(),
        vec![
            MarkupKind::Highlight,
            MarkupKind::Underline,
            MarkupKind::StrikeOut,
            MarkupKind::Squiggly
        ]
    );
    assert_eq!(annotations[0].id().epoch(), 42);
    assert_eq!(annotations[0].id().serial(), 3); // original Annots slot, not sorted row
    assert_eq!(annotations[0].name(), Some("imported-highlight"));
    assert_eq!(annotations[0].author(), Some("Fixture author"));
    assert_eq!(
        annotations[0].quads()[0].points()[0],
        Point { x: 40., y: 496. }
    );
    assert_eq!(annotations[4].color(), AnnotationColor::Gray(0.5));
    assert!(annotations[5].is_readonly());
    assert!(annotations[5].fallback_rect().is_some());
    assert!(annotations[6].is_readonly());
    assert!(annotations[6].quads().is_empty());
    let other_epoch = annotation_read::page_annotations(&pdf, 0, 43).unwrap();
    assert_ne!(annotations[0].id(), other_epoch[0].id());
}

#[cfg(feature = "gui")]
#[test]
fn quotation_excludes_text_in_an_annotation_appearance_as_well_as_its_comment() {
    let stream = |text: &str, extra: &str| {
        format!(
            "<< /Length {} {extra} >>\nstream\n{text}\nendstream",
            text.len()
        )
    };
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [6 0 R] >>".to_owned(),
        stream("BT /F1 16 Tf 40 480 Td (Marked text) Tj ET", ""),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        "<< /Type /Annot /Subtype /Highlight /Rect [40 476 190 496] /QuadPoints [40 496 190 496 40 476 190 476] /Contents (Attached comment) /AP << /N 7 0 R >> >>".to_owned(),
        stream("BT /F1 16 Tf 40 480 Td (Comment) Tj ET", "/Type /XObject /Subtype /Form /BBox [40 476 190 496] /Resources << /Font << /F1 5 0 R >> >>"),
    ];
    let mut data = "%PDF-1.7\n".to_owned();
    let mut offsets = vec![0];
    for (ix, object) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.push_str(&format!("{} 0 obj\n{object}\nendobj\n", ix + 1));
    }
    let xref = data.len();
    data.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()));
    for offset in &offsets[1..] {
        data.push_str(&format!("{offset:010} 00000 n \n"));
    }
    data.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len()
    ));
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), data).unwrap();
    let document = Document::open(file.path()).unwrap();
    let annotation = document.page_annotations(0, 1).unwrap();
    let rendered_text: String = document
        .page_text(0)
        .unwrap()
        .glyphs
        .iter()
        .map(|g| g.text.as_str())
        .collect();
    assert!(
        rendered_text.len() > "Marked text".len(),
        "specimen must exercise an appearance containing text"
    );
    assert_eq!(
        document.annotation_excerpts(0, &annotation).unwrap()[0]
            .1
            .as_deref(),
        Some("Marked text")
    );
}
