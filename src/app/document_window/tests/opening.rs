use super::*;
use crate::settings::{AppPaths, OpenFiles, SettingsChange};

fn configure(cx: &mut TestAppContext, behavior: OpenFiles) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
    crate::settings::save(paths.config(), SettingsChange::OpenFiles(behavior)).unwrap();
    setup(cx);
    cx.update(|cx| settings_ui::install_with_paths(cx, Some(paths)));
    cx.run_until_parked();
    root
}

#[gpui_kit::test]
fn file_picker_reuse_targets_the_invoking_reader_not_the_active_one(cx: &mut TestAppContext) {
    let _root = configure(cx, OpenFiles::Reuse);
    let (_, origin) = open(cx, Some("examples/readme-sample.pdf".into()));
    let (_, other) = open(cx, Some("examples/field-guide.pdf".into()));
    cx.run_until_parked();
    let target = PathBuf::from("examples/selection-sample.pdf");
    origin.update(cx, |document, cx| {
        let generation = document.reader.read(cx).document_generation();
        document.finish_file_choice(generation, Some(target.clone()), cx);
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(cx.windows().len(), 2);
        assert_eq!(
            origin.read(cx).reader.read(cx).path(),
            Some(target.as_path())
        );
        assert_eq!(
            other.read(cx).reader.read(cx).path(),
            Some(PathBuf::from("examples/field-guide.pdf").as_path())
        );
    });
}

#[gpui_kit::test]
fn open_url_dialog_accepts_a_pdf_by_keyboard(cx: &mut TestAppContext) {
    let root = configure(cx, OpenFiles::Reuse);
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
    cx.update(|cx| crate::reading_history::install(cx, Some(paths.clone())));
    let url = crate::app::remote_pdf::tests::serve(
        std::fs::read("examples/readme-sample.pdf").unwrap(),
        1,
    );
    let (window, document) = open(cx, None);
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(OpenUrl), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(frame.find("url-open").visible());
    })
    .unwrap();
    cx.simulate_input(window, url.as_str());
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        assert_eq!(reader.page_count(), Some(100));
        assert!(reader.record_path().is_none());
        assert!(reader.is_preview(cx));
        assert!(reader.save_available(cx));
    });
    assert!(!paths.local_data().join("Downloads").exists());
    let store = crate::reading_history::RecentStore::new(&paths);
    assert!(store.search("").unwrap().is_empty());
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(Save), cx)
    })
    .unwrap();
    cx.run_until_parked();
    let saved = root.path().join("saved.pdf");
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read(&saved).unwrap(),
        std::fs::read("examples/readme-sample.pdf").unwrap()
    );
    document.read_with(cx, |document, cx| {
        let reader = document.reader.read(cx);
        assert!(!reader.is_preview(cx));
        assert_eq!(
            reader.record_path(),
            Some(saved.canonicalize().unwrap().as_path())
        );
    });
    let recent = store.search("").unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].path, saved.canonicalize().unwrap());
}

#[gpui_kit::test]
fn file_picker_new_window_preserves_both_existing_readers(cx: &mut TestAppContext) {
    let _root = configure(cx, OpenFiles::NewWindow);
    let original = PathBuf::from("examples/readme-sample.pdf");
    let (_, origin) = open(cx, Some(original.clone()));
    let (_, other) = open(cx, Some("examples/field-guide.pdf".into()));
    cx.run_until_parked();
    origin.update(cx, |document, cx| {
        let generation = document.reader.read(cx).document_generation();
        document.finish_file_choice(generation, Some("examples/selection-sample.pdf".into()), cx);
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(cx.windows().len(), 3);
        assert_eq!(
            origin.read(cx).reader.read(cx).path(),
            Some(original.as_path())
        );
        assert_eq!(
            other.read(cx).reader.read(cx).path(),
            Some(PathBuf::from("examples/field-guide.pdf").as_path())
        );
    });
}

fn recent_new_window(
    cx: &mut TestAppContext,
    valid: bool,
) -> (tempfile::TempDir, AnyWindowHandle, Entity<DocumentWindow>) {
    let root = configure(cx, OpenFiles::NewWindow);
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
    let target = if valid {
        crate::reading_history::RecentStore::normalize(std::path::Path::new(
            "examples/field-guide.pdf",
        ))
        .unwrap()
    } else {
        root.path().join("missing.pdf")
    };
    crate::reading_history::RecentStore::new(&paths)
        .record(&target)
        .unwrap();
    cx.update(|cx| crate::reading_history::install(cx, Some(paths)));
    let (handle, document) = open(cx, Some("examples/readme-sample.pdf".into()));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.press("enter", cx))
        .unwrap();
    (root, handle, document)
}

#[gpui_kit::test]
fn recent_new_window_finishes_the_originating_picker_without_replacing_its_pdf(
    cx: &mut TestAppContext,
) {
    let (_root, handle, document) = recent_new_window(cx, true);
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(
            document.reader.read(cx).path(),
            Some(PathBuf::from("examples/readme-sample.pdf").as_path())
        );
        assert!(document.recent.is_none());
    });
    cx.update(|cx| {
        assert_eq!(cx.windows().len(), 2);
        assert_ne!(cx.active_window(), Some(handle));
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("recent-panel").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn recent_new_window_failure_leaves_an_actionable_error_in_the_originating_picker(
    cx: &mut TestAppContext,
) {
    let (_root, handle, document) = recent_new_window(cx, false);
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        let picker = document.recent.as_ref().unwrap().read(cx);
        assert!(picker.error().unwrap().contains("Could not open this PDF"));
        assert!(!picker.is_loading());
        assert_eq!(document.reader.read(cx).page_count(), Some(100));
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("recent-error").visible());
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(document.read_with(cx, |document, _| document.recent.is_none()));
}

#[gpui_kit::test]
fn recent_new_window_completion_cannot_dismiss_a_reopened_picker(cx: &mut TestAppContext) {
    let (_root, handle, document) = recent_new_window(cx, true);
    cx.update_window(handle, |_, window, cx| {
        window.press("escape", cx);
        window.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    let new = document.read_with(cx, |document, _| {
        document.recent.as_ref().unwrap().entity_id()
    });
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        let picker = document.recent.as_ref().unwrap();
        assert_eq!(picker.entity_id(), new);
        assert!(!picker.read(cx).is_loading());
    });
}
