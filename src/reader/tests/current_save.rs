#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::authoring::draw;
use super::authoring::select_title;
use super::*;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn command_save_updates_the_current_file_without_a_toolbar_button_for_both_windows(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().canonicalize().unwrap().join("current.pdf");
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    std::fs::write(&source, &original).unwrap();
    let (first, reader) = recent_test_window(cx, Some(source.clone()));
    let (second, other) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    draw(cx, first);
    draw(cx, second);
    select_title(cx, first, &reader, &source);
    cx.update_window(first, |_, window, cx| {
        window.click("highlight-selection", cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, cx| assert!(reader.is_dirty(cx)));
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("save-as").is_none(),
            "Save As is not a routine toolbar operation"
        );
        assert!(
            window.try_find("save").is_none(),
            "Save is a menu/keyboard command, not a toolbar button"
        );
        window.press("cmd-s", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(
        !cx.did_prompt_for_new_path(),
        "writable originals save without a filename prompt"
    );
    let saved = std::fs::read(&source).unwrap();
    assert!(saved.starts_with(&original));
    assert_eq!(
        pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
    for view in [&reader, &other] {
        view.read_with(cx, |reader, cx| {
            assert_eq!(reader.path(), Some(source.as_path()));
            assert!(!reader.is_dirty(cx));
            assert!(!reader.save_available(cx));
            assert!(reader.save_as_available(cx));
            assert_eq!(
                reader
                    .session()
                    .unwrap()
                    .read(cx)
                    .unsaved_annotations()
                    .count(),
                0
            );
        });
    }
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_text().as_deref(), Some("Alpha"))
    });
    other.read_with(cx, |reader, _| assert!(reader.selected_text().is_none()));
    cx.update_window(first, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read(source).unwrap(), saved);
}

#[gpui_kit::test]
fn save_conflict_keeps_external_version_and_offers_a_copy_of_kvers_loaded_version(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let source = root.join("current.pdf");
    let copy = root.join("recovered-annotated.pdf");
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    std::fs::write(&source, &original).unwrap();
    let (handle, reader) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &source);
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    let external = [original.as_slice(), b"\n% external version\n"].concat();
    std::fs::write(&source, &external).unwrap();
    cx.update_window(handle, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(std::fs::read(&source).unwrap(), external);
    reader.read_with(cx, |reader, cx| assert!(reader.is_dirty(cx)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("save-conflict-copy").visible());
        window.click("save-conflict-copy", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(copy.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read(source).unwrap(), external);
    let recovered = std::fs::read(&copy).unwrap();
    assert!(recovered.starts_with(&original));
    assert!(!recovered.starts_with(&external));
    assert_eq!(
        pdf::Document::open(&copy)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
    reader.read_with(cx, |reader, cx| {
        assert!(!reader.is_dirty(cx));
        assert_eq!(reader.path(), Some(copy.as_path()));
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn save_routes_a_newly_readonly_original_to_save_as_and_cancel_keeps_edits(
    cx: &mut TestAppContext,
) {
    use std::os::unix::fs::PermissionsExt;
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("readonly.pdf");
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    std::fs::write(&source, &original).unwrap();
    let (handle, reader) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &source);
    cx.update_window(handle, |_, window, cx| window.press("cmd-shift-h", cx))
        .unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o444)).unwrap();
    cx.update_window(handle, |_, window, cx| window.press("cmd-s", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert_eq!(std::fs::read(source).unwrap(), original);
    reader.read_with(cx, |reader, cx| assert!(reader.is_dirty(cx)));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn default_save_from_last_close_writes_the_current_pdf_without_a_save_as_picker(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().canonicalize().unwrap().join("current.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let (handle, reader) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    select_title(cx, handle, &reader, &source);
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-shift-h", cx);
        window.dispatch_action(Box::new(crate::app::windows::CloseWindow), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("unsaved-save").label(), Some("Save"));
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!cx.did_prompt_for_new_path());
    assert!(cx.update(|cx| cx.windows().is_empty()));
    assert_eq!(
        pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
    let (reopened, reopened_reader) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    draw(cx, reopened);
    reopened_reader.read_with(cx, |reader, cx| {
        assert!(!reader.is_dirty(cx));
        let session = reader.session().unwrap();
        assert_eq!(session.read(cx).annotations().unwrap().unwrap().len(), 1);
        assert_eq!(session.read(cx).unsaved_annotations().count(), 0);
    });
}
