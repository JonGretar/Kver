use super::*;
use crate::pdf;

fn linked_window(
    cx: &mut TestAppContext,
    with_target: bool,
) -> (
    tempfile::TempDir,
    AnyWindowHandle,
    Entity<DocumentWindow>,
    PathBuf,
) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let target = root.path().join("other.pdf");
    std::fs::write(&source, pdf::linked_fixture()).unwrap();
    if with_target {
        std::fs::copy("examples/readme-sample.pdf", &target).unwrap();
    }
    setup(cx);
    let (window, document) = open(cx, Some(source));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    (root, window, document, target)
}

#[gpui_kit::test]
fn another_pdf_link_opens_independent_window_at_target_page(cx: &mut TestAppContext) {
    let (_root, source_window, document, target) = linked_window(cx, true);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    cx.update_window(source_window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(
            frame.find("pdf-link-0-9").label(),
            Some("Open other.pdf in a new window")
        );
        frame.click("pdf-link-0-9", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.document_generation(), generation);
        assert!(reader.path().unwrap().ends_with("source.pdf"));
    });
    let new_window = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != source_window)
            .unwrap()
    });
    cx.update_window(new_window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("reader-status").label(), Some("Page 2 of 100"));
        assert!(frame.find("document-window").visible());
    })
    .unwrap();
    assert!(target.exists());
}

#[gpui_kit::test]
fn missing_link_target_reports_error_without_opening_window(cx: &mut TestAppContext) {
    let (_root, source_window, document, _) = linked_window(cx, false);
    cx.update_window(source_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-9", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    document.read_with(cx, |document, cx| {
        let reader = document.reader().read(cx);
        assert_eq!(reader.page_index(), 0);
        assert!(
            reader.status().contains("Cannot open linked PDF"),
            "{}",
            reader.status()
        );
        assert!(reader.status().contains("other.pdf"), "{}", reader.status());
    });
}

#[gpui_kit::test]
fn obsolete_link_completion_cannot_open_a_window_or_change_replacement(cx: &mut TestAppContext) {
    let (_root, source_window, document, target) = linked_window(cx, true);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    document.update(cx, |document, cx| {
        document.open_linked_document(generation, target.clone(), 1, source_window, cx);
        document.open_path(PathBuf::from("examples/field-guide.pdf"), cx);
        document.finish_linked_open(
            LinkedOpen {
                request: 1,
                generation,
                path: target,
                page: 1,
                source_window,
            },
            Ok(()),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.path(),
            Some(std::path::Path::new("examples/field-guide.pdf"))
        );
        assert_eq!(reader.page_index(), 0);
    });
}

#[gpui_kit::test]
fn earlier_link_open_cannot_override_a_newer_request(cx: &mut TestAppContext) {
    let (_root, source_window, document, target) = linked_window(cx, true);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    document.update(cx, |document, cx| {
        document.linked_open_request = 2;
        let request = |request| LinkedOpen {
            request,
            generation,
            path: target.clone(),
            page: 1,
            source_window,
        };
        document.finish_linked_open(request(1), Ok(()), cx);
        assert_eq!(cx.windows().len(), 1);
        document.finish_linked_open(request(2), Ok(()), cx);
    });
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn checked_in_link_guide_opens_the_illustrated_field_guide(cx: &mut TestAppContext) {
    setup(cx);
    let (source_window, source) = open(cx, Some(PathBuf::from("examples/link-guide.pdf")));
    let source_reader = source.read_with(cx, |source, _| source.reader().clone());
    cx.run_until_parked();
    cx.update_window(source_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-1", cx);
    })
    .unwrap();
    cx.run_until_parked();
    source_reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    let field_guide = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != source_window)
            .unwrap()
    });
    cx.update_window(field_guide, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("reader-status").label(), Some("Page 8 of 24"));
    })
    .unwrap();
}
