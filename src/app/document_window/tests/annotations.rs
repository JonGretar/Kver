//! Readonly text-markup browsing through the production document window.
use super::*;

fn row(epoch: u64, serial: u64) -> String {
    format!("annotation-{epoch}-{serial}")
}

#[gpui_kit::test]
fn imported_annotations_browse_copy_and_identify_all_four_types(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let expected = [
        (3, "Highlight passage; Highlight"),
        (1, "Underline passage; Underline"),
        (2, "Strikeout passage; Strikethrough"),
        (0, "Squiggly passage; Squiggly underline"),
    ];
    let mut previous_bottom = gpui_kit::px(0.);
    for (serial, prefix) in expected {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let target = window.find(row(epoch, serial));
            assert!(
                target.label().unwrap().starts_with(prefix),
                "{:?}",
                target.label()
            );
            assert!(target.bounds().top() >= previous_bottom);
            previous_bottom = target.bounds().bottom();
            window.click(row(epoch, serial), cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("annotation-identification").visible());
            window.press("cmd-c", cx);
            let text = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(!text.contains("Attached comment"));
            assert_eq!(text, prefix.split(';').next().unwrap());
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        window.click(row(epoch, 5), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(row(epoch, 5))
                .label()
                .unwrap()
                .starts_with("No marked text available")
        );
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("untouched".into()));
        window.press("cmd-c", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("untouched")
        );
        window.press("down", cx); // malformed quad, usable Rect
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find(row(epoch, 6))
                .label()
                .unwrap()
                .contains("Malformed")
        );
        assert!(window.find("annotation-identification").visible());
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |r, _| r.page_index()), 0);
}

#[gpui_kit::test]
fn replacing_document_while_annotations_load_never_restores_old_rows_or_copy(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    reader.update(cx, |r, cx| {
        r.open_path(PathBuf::from("examples/readme-sample.pdf"), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("annotations-status").label(),
            Some("No text markup in this PDF")
        );
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |r, _| r.page_index()), 0);
}

#[gpui_kit::test]
fn a_growing_annotation_list_virtualizes_rows_and_navigates_offscreen_entries(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations-many.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(row(epoch, 0)).visible());
        assert!(window.try_find(row(epoch, 239)).is_none());
        window.scroll(
            "annotations-panel",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                gpui_kit::px(0.),
                gpui_kit::px(-30_000.),
            )),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(row(epoch, 0)).is_none());
        window.click(row(epoch, 239), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("copy-annotation", cx);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("Highlight passage")
        );
        assert!(window.find("annotation-identification").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn rotated_annotation_navigation_and_copy_remain_window_local(cx: &mut TestAppContext) {
    setup(cx);
    let path = PathBuf::from("examples/annotations.pdf");
    let (handle, document) = open(cx, Some(path.clone()));
    let (_, other) = open(cx, Some(path));
    cx.run_until_parked();
    let reader = document.read_with(cx, |d, _| d.reader().clone());
    let epoch = reader.read_with(cx, |r, cx| r.session().unwrap().read(cx).epoch());
    cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(row(epoch, 1 << 32), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let before = cx
        .update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("annotation-identification").visible());
            let before = window.find("annotation-identification").bounds();
            window.press("cmd-c", cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("Rotated passage")
            );
            window.dispatch_action(Box::new(ZoomIn), cx);
            before
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let after = window.find("annotation-identification").bounds();
        assert!(after.size.height > before.size.height);
        assert!(after.size.width > before.size.width);
    })
    .unwrap();
    assert_eq!(reader.read_with(cx, |r, _| r.page_index()), 1);
    other.read_with(cx, |d, cx| assert_eq!(d.reader().read(cx).page_index(), 0));
}

#[gpui_kit::test]
fn replacing_a_document_with_an_unreadable_pdf_does_not_leave_annotations_loading(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/annotations.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.update(cx, |d, cx| {
        d.open_path(PathBuf::from("examples/missing.pdf"), cx)
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .find("annotations-status")
                .label()
                .unwrap()
                .starts_with("Couldn’t load")
        );
        assert_eq!(window.find("copy-annotation").disabled(), Some(true));
    })
    .unwrap();
}
