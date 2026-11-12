//! Disposable annotation-writing experiment, not an application save API.
use std::{error::Error, fs, path::Path};

use hayro::{
    PixmapSettings, RenderCache, RenderSettings, hayro_interpret::InterpreterSettings,
    hayro_syntax::Pdf, vello_cpu::color::palette::css::WHITE,
};
use lopdf::{
    Dictionary, Document, IncrementalDocument, Object, ObjectId, Stream,
    content::{Content, Operation},
    dictionary,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

mod geometry;

const NAME: &str = "kver-spike-selection";
const YELLOW: [f32; 3] = [1.0, 0.85, 0.0];
const PINK: [f32; 3] = [1.0, 0.25, 0.65];
// Three separate text regions: two left-column lines, then one right-column line.
const REGIONS: [[f32; 4]; 3] = [
    [60.0, 417.0, 215.0, 435.0],
    [60.0, 392.0, 215.0, 410.0],
    [320.0, 417.0, 485.0, 435.0],
];

fn numbers(values: impl IntoIterator<Item = f32>) -> Object {
    Object::Array(values.into_iter().map(Object::Real).collect())
}

fn appearance(doc: &mut Document, regions: &[[f32; 4]], color: [f32; 3]) -> ObjectId {
    // Absolute PDF-point coordinates and an identity appearance matrix.
    // Matching Rect/BBox avoids applying crop/rotation twice.
    let gs = doc.add_object(dictionary! {
        "Type" => "ExtGState", "BM" => "Multiply", "ca" => 0.45_f32,
    });
    let mut operations = vec![
        Operation::new("q", vec![]),
        Operation::new("gs", vec!["GS".into()]),
        Operation::new("rg", color.into_iter().map(Object::Real).collect()),
    ];
    for [x0, y0, x1, y1] in regions {
        operations.push(Operation::new(
            "re",
            [*x0, *y0, x1 - x0, y1 - y0]
                .into_iter()
                .map(Object::Real)
                .collect(),
        ));
        operations.push(Operation::new("f", vec![]));
    }
    operations.push(Operation::new("Q", vec![]));
    doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "FormType" => 1,
            "BBox" => bounds(regions),
            "Resources" => dictionary! { "ExtGState" => dictionary! { "GS" => gs } },
        },
        Content { operations }
            .encode()
            .expect("finite fixture operations"),
    ))
}

fn bounds(regions: &[[f32; 4]]) -> Object {
    numbers([
        regions.iter().map(|r| r[0]).fold(f32::INFINITY, f32::min),
        regions.iter().map(|r| r[1]).fold(f32::INFINITY, f32::min),
        regions
            .iter()
            .map(|r| r[2])
            .fold(f32::NEG_INFINITY, f32::max),
        regions
            .iter()
            .map(|r| r[3])
            .fold(f32::NEG_INFINITY, f32::max),
    ])
}

fn highlight(
    doc: &mut Document,
    page: ObjectId,
    regions: &[[f32; 4]],
    color: [f32; 3],
    name: &str,
) -> ObjectId {
    let ap = appearance(doc, regions, color);
    let quads = regions.iter().flat_map(|[x0, y0, x1, y1]| {
        // Interoperable text-markup order: upper-left/right, lower-left/right.
        [*x0, *y1, *x1, *y1, *x0, *y0, *x1, *y0]
    });
    doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Highlight", "P" => page,
        "Rect" => bounds(regions), "QuadPoints" => numbers(quads),
        "C" => numbers(color), "CA" => 0.45_f32, "F" => 4,
        "NM" => Object::string_literal(name),
        "Contents" => Object::string_literal(format!("{name}: multi-region text highlight")),
        "T" => Object::string_literal("Kver spike"),
        "AP" => dictionary! { "N" => ap },
    })
}

fn fixture(path: &Path) -> Result<()> {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut ids = Vec::new();
    for rotation in [0, 90, 180, 270] {
        let page = doc.new_object_id();
        let mut ops = Vec::new();
        for (x, y, text) in [
            (
                60,
                510,
                format!("PDF highlight experiment - rotation {rotation}"),
            ),
            (
                60,
                470,
                "Three separate regions, never one gutter-spanning box.".into(),
            ),
            (60, 420, "LEFT alpha selection".into()),
            (60, 395, "LEFT beta selection".into()),
            (320, 420, "RIGHT alpha selection".into()),
            (320, 395, "RIGHT unselected line".into()),
            (60, 310, "EXISTING annotation stays here".into()),
            (60, 250, "LINK: go to first page".into()),
        ] {
            ops.extend([
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 14.into()]),
                Operation::new("Td", vec![x.into(), y.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ]);
        }
        let content = doc.add_object(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode()?,
        ));
        doc.objects.insert(page, dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => numbers([0.0, 0.0, 600.0, 600.0]),
            "CropBox" => numbers([20.0, 30.0, 580.0, 570.0]), "Rotate" => rotation,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            "Contents" => content,
        }.into());
        let existing = highlight(
            &mut doc,
            page,
            &[[60.0, 307.0, 290.0, 325.0]],
            [0.0, 0.8, 0.8],
            "existing-highlight",
        );
        ids.push((page, existing));
    }
    for &(page, existing) in &ids {
        let link = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Link", "Rect" => numbers([60.0, 247.0, 230.0, 268.0]),
            "Border" => numbers([0.0, 0.0, 1.0]), "Dest" => vec![ids[0].0.into(), "Fit".into()],
        });
        // Deliberately indirect Annots array: common in existing documents.
        let annots = doc.add_object(Object::Array(vec![existing.into(), link.into()]));
        doc.get_object_mut(page)?
            .as_dict_mut()?
            .set("Annots", annots);
    }
    doc.objects.insert(
        pages,
        dictionary! {
            "Type" => "Pages", "Count" => 4,
            "Kids" => ids.iter().map(|(p, _)| Object::Reference(*p)).collect::<Vec<_>>(),
        }
        .into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    doc.save(path)?;
    Ok(())
}

fn annots(doc: &Document, page: ObjectId) -> Result<Vec<Object>> {
    let dict = doc.get_dictionary(page)?;
    match dict.get(b"Annots") {
        Ok(value) => Ok(doc.dereference(value)?.1.as_array()?.clone()),
        Err(_) => Ok(Vec::new()),
    }
}

fn is_ours(doc: &Document, object: &Object) -> bool {
    let Some(dictionary) = doc
        .dereference(object)
        .ok()
        .and_then(|(_, a)| a.as_dict().ok())
    else {
        return false;
    };
    // Fixture lookup only. Preview may drop NM. A real editor must enumerate
    // current annotation handles, not identify arbitrary annotations by notes.
    dictionary.get(b"NM").ok().and_then(|n| n.as_str().ok()) == Some(NAME.as_bytes())
        || dictionary
            .get(b"Contents")
            .ok()
            .and_then(|n| n.as_str().ok())
            .is_some_and(|contents| contents.starts_with(format!("{NAME}:").as_bytes()))
}

fn stored_regions(annotation: &Dictionary) -> Result<Vec<[f32; 4]>> {
    let points = annotation.get(b"QuadPoints")?.as_array()?;
    if points.is_empty() || points.len() % 8 != 0 {
        return Err("invalid fixture QuadPoints".into());
    }
    points
        .as_chunks::<8>()
        .0
        .iter()
        .map(|quad| {
            let values = quad
                .iter()
                .map(Object::as_float)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok([
                [values[0], values[2], values[4], values[6]]
                    .into_iter()
                    .fold(f32::INFINITY, f32::min),
                [values[1], values[3], values[5], values[7]]
                    .into_iter()
                    .fold(f32::INFINITY, f32::min),
                [values[0], values[2], values[4], values[6]]
                    .into_iter()
                    .fold(f32::NEG_INFINITY, f32::max),
                [values[1], values[3], values[5], values[7]]
                    .into_iter()
                    .fold(f32::NEG_INFINITY, f32::max),
            ])
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Edit {
    Add,
    Recolor,
    Delete,
}

fn edit(input: &Path, output: &Path, edit: Edit) -> Result<()> {
    edit_regions(input, output, edit, &REGIONS)
}

fn edit_regions(input: &Path, output: &Path, edit: Edit, regions: &[[f32; 4]]) -> Result<()> {
    let mut incremental = IncrementalDocument::load(input)?;
    let pages = incremental.get_prev_documents().get_pages();
    for (_, page) in pages {
        let previous = incremental.get_prev_documents();
        let mut dictionary = previous.get_dictionary(page)?.clone();
        let mut entries = annots(previous, page)?;
        let owned = entries.iter().position(|a| is_ours(previous, a));
        match edit {
            Edit::Add => {
                assert!(owned.is_none());
                let id = highlight(&mut incremental.new_document, page, regions, YELLOW, NAME);
                entries.push(id.into());
            }
            Edit::Recolor => {
                let id = entries[owned.ok_or("missing highlight")?].as_reference()?;
                let mut annotation = incremental.get_prev_documents().get_dictionary(id)?.clone();
                let stored = stored_regions(&annotation)?;
                let ap = appearance(&mut incremental.new_document, &stored, PINK);
                // Respect an imported annotation's existing Rect even if it is
                // larger than its quads, so the appearance isn't scaled into it.
                incremental
                    .new_document
                    .objects
                    .get_mut(&ap)
                    .ok_or("missing AP")?
                    .as_stream_mut()?
                    .dict
                    .set("BBox", annotation.get(b"Rect")?.clone());
                annotation.set("C", numbers(PINK));
                annotation.set("AP", dictionary! { "N" => ap });
                incremental
                    .new_document
                    .objects
                    .insert(id, annotation.into());
            }
            Edit::Delete => {
                entries.remove(owned.ok_or("missing highlight")?);
            }
        }
        dictionary.set("Annots", Object::Array(entries));
        incremental
            .new_document
            .objects
            .insert(page, dictionary.into());
    }
    let mut bytes = Vec::new();
    incremental.save_to(&mut bytes)?;
    assert!(
        bytes.starts_with(&fs::read(input)?),
        "incremental save changed old bytes"
    );
    fs::write(output, bytes)?;
    Ok(())
}

fn snapshot(doc: &Document, page: ObjectId) -> Result<(String, Vec<u8>, Vec<String>)> {
    let mut dictionary = doc.get_dictionary(page)?.clone();
    dictionary.remove(b"Annots");
    let existing = annots(doc, page)?
        .iter()
        .filter(|a| !is_ours(doc, a))
        .map(|a| Ok(format!("{:?}", doc.dereference(a)?.1)))
        .collect::<Result<Vec<_>>>()?;
    Ok((
        format!("{dictionary:?}"),
        doc.get_page_content(page),
        existing,
    ))
}

fn verify(original: &Path, modified: &Path, expected: usize) -> Result<()> {
    let original = Document::load(original)?;
    let doc = Document::load(modified)?;
    assert_eq!(original.get_pages(), doc.get_pages());
    assert_eq!(
        format!("{:?}", original.catalog()?),
        format!("{:?}", doc.catalog()?)
    );
    let mut highlights = 0;
    for (_, page) in doc.get_pages() {
        assert_eq!(snapshot(&original, page)?, snapshot(&doc, page)?);
        let original_geometry = annots(&original, page)?
            .into_iter()
            .find(|a| is_ours(&original, a))
            .map(|a| {
                original
                    .dereference(&a)?
                    .1
                    .as_dict()?
                    .get(b"QuadPoints")
                    .cloned()
            })
            .transpose()?;
        for a in annots(&doc, page)? {
            if is_ours(&doc, &a) {
                highlights += 1;
                let a = doc.dereference(&a)?.1.as_dict()?;
                assert_eq!(a.get(b"QuadPoints")?.as_array()?.len(), 24);
                if let Some(expected) = &original_geometry {
                    assert_eq!(
                        format!("{:?}", a.get(b"QuadPoints")?),
                        format!("{expected:?}"),
                        "recoloring changed stored selection geometry"
                    );
                }
                assert!(a.get(b"AP")?.as_dict()?.get(b"N")?.as_reference().is_ok());
                let color = a.get(b"C")?.as_array()?;
                let expected_color = if modified.file_stem().unwrap() == "recolored" {
                    PINK
                } else {
                    YELLOW
                };
                assert_eq!(
                    format!("{color:?}"),
                    format!("{:?}", numbers(expected_color).as_array()?)
                );
            }
        }
    }
    assert_eq!(highlights, expected);
    Ok(())
}

fn render(path: &Path, page: usize) -> Result<Vec<u8>> {
    // Same Hayro version and 1x settings as Kver's pdf::rasterize, without
    // depending on the GUI application or changing its build configuration.
    let pdf = Pdf::new(fs::read(path)?).map_err(|e| format!("{e:?}"))?;
    let page = pdf.pages().get(page).ok_or("missing render page")?;
    Ok(hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: 1.0,
            y_scale: 1.0,
            bg_color: WHITE,
        },
    )
    .into_png()?)
}

fn main() -> Result<()> {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("hayro-control")) {
        let directory = std::env::args_os()
            .nth(2)
            .ok_or("hayro-control OUTPUT_DIRECTORY")?;
        let directory = Path::new(&directory);
        let source = directory.join("original.pdf");
        let regions = geometry::extract(&source, 0)?;
        for page in 1..4 {
            assert_eq!(geometry::extract(&source, page)?, regions);
        }
        println!("Extracted directly from Hayro: {regions:?}");
        for (name, regions) in [
            ("hayro-ink-control", regions.ink),
            ("hayro-selection-control", regions.selection),
        ] {
            let output = directory.join(format!("{name}.pdf"));
            edit_regions(&source, &output, Edit::Add, &regions)?;
            verify(&source, &output, 4)?;
            for page in 0..4 {
                let before = render(&source, page)?;
                let after = render(&output, page)?;
                assert_ne!(before, after);
                fs::write(directory.join(format!("{name}-{page}.png")), after)?;
            }
            println!(
                "PASS {}: actual glyph-derived regions at all four rotations",
                output.display()
            );
        }
        return Ok(());
    }
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("roundtrip")) {
        let input = std::env::args_os()
            .nth(2)
            .ok_or("roundtrip INPUT OUTPUT_DIRECTORY")?;
        let output = std::env::args_os()
            .nth(3)
            .ok_or("roundtrip INPUT OUTPUT_DIRECTORY")?;
        let input = Path::new(&input);
        let out = Path::new(&output);
        fs::create_dir_all(out)?;
        let recolored = out.join("recolored.pdf");
        let deleted = out.join("deleted.pdf");
        edit(input, &recolored, Edit::Recolor)?;
        edit(&recolored, &deleted, Edit::Delete)?;
        verify(input, &recolored, 4)?;
        verify(input, &deleted, 0)?;
        for page in 0..4 {
            let original = render(input, page)?;
            let updated = render(&recolored, page)?;
            assert_ne!(original, updated, "external-editor color not changed");
            fs::write(out.join(format!("hayro-recolored-{page}.png")), updated)?;
        }
        println!(
            "PASS Rust can reopen, recolor, render, and delete highlights saved by another editor"
        );
        return Ok(());
    }
    let output = std::env::args_os()
        .nth(1)
        .ok_or("Usage: highlight-spike OUTPUT_DIRECTORY")?;
    let out = Path::new(&output);
    fs::create_dir_all(out)?;
    let source = out.join("original.pdf");
    fixture(&source)?;
    let added = out.join("highlighted.pdf");
    let recolored = out.join("recolored.pdf");
    let deleted = out.join("deleted.pdf");
    edit(&source, &added, Edit::Add)?;
    edit(&added, &recolored, Edit::Recolor)?;
    edit(&recolored, &deleted, Edit::Delete)?;
    for (path, count) in [(&added, 4), (&recolored, 4), (&deleted, 0)] {
        verify(&source, path, count)?;
        println!(
            "PASS {}: {count} new highlights, original page objects/content/annotations preserved",
            path.display()
        );
    }
    for page in 0..4 {
        let original = render(&source, page)?;
        let deleted_render = render(&deleted, page)?;
        assert_eq!(
            original, deleted_render,
            "deletion did not restore baseline image"
        );
        let highlighted = render(&added, page)?;
        assert_ne!(original, highlighted, "Hayro did not render the annotation");
        let recolored_render = render(&recolored, page)?;
        assert_ne!(
            highlighted, recolored_render,
            "appearance recolor invisible"
        );
        fs::write(out.join(format!("hayro-original-{page}.png")), original)?;
        fs::write(
            out.join(format!("hayro-highlighted-{page}.png")),
            highlighted,
        )?;
        fs::write(
            out.join(format!("hayro-recolored-{page}.png")),
            recolored_render,
        )?;
    }
    println!("PASS Hayro render differences and exact deletion round-trip at 0/90/180/270 degrees");
    // Exercise the same mutation on an actual existing, linked Typst document.
    let field_guide = Path::new("examples/field-guide.pdf");
    let field_copy = out.join("field-guide-source.pdf");
    fs::copy(field_guide, &field_copy)?;
    let field_added = out.join("field-guide-highlighted.pdf");
    edit(&field_copy, &field_added, Edit::Add)?;
    verify(
        &field_copy,
        &field_added,
        Document::load(&field_copy)?.get_pages().len(),
    )?;
    assert_eq!(fs::read(field_guide)?, fs::read(&field_copy)?);
    println!(
        "PASS existing field guide: original untouched, page content/links/outlines preserved"
    );
    Ok(())
}
