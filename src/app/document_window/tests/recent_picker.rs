//! Open Recent through its document-window owner, including palette routing.

use super::*;
use crate::{
    app::keymap::install as bind_reader_keys,
    reader::ShowCommandPalette,
    reading_history::{self as recent, RecentStore},
};
use std::path::Path;

#[gpui_kit::test]
fn command_palette_opens_recent_with_search_focus(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (window, document) = open(cx, None);
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowCommandPalette), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.input("Open Recent", cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.palette_query(cx).as_deref(), Some("Open Recent"));
        assert!(document.palette.as_ref().unwrap().has_selection(cx));
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
            .unwrap();
    });
    cx.run_until_parked();
    document.read_with(cx, |document, _| {
        assert!(!document.palette_open());
        assert!(document.recent.is_some());
    });
    cx.update(|cx| {
        cx.update_window(window, |_, frame, cx| {
            assert_eq!(frame.find("recent-panel").label(), Some("Open Recent"));
            frame.input("book", cx);
        })
        .unwrap();
    });
    document.read_with(cx, |document, cx| {
        assert_eq!(document.recent.as_ref().unwrap().read(cx).query(cx), "book");
    });
}

#[gpui_kit::test]
fn successful_open_records_normal_path_and_recent_reopens_in_active_window(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (_first, first) = open(cx, None);
    let (second, second_document) = open(cx, None);
    first.update(cx, |document, cx| {
        document.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
    });
    cx.run_until_parked();
    let normalized = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    assert!(store.contains(&normalized).unwrap());
    second_document.read_with(cx, |document, cx| {
        assert!(document.reader().read(cx).path().is_none())
    });
    cx.update(|cx| {
        cx.update_window(second, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_action(Box::new(ShowRecent), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(second, |_, window, cx| window.press("enter", cx))
            .unwrap();
    });
    cx.run_until_parked();
    second_document.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_count(), Some(100));
        assert!(document.recent.is_none());
    });
    first.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_count(), Some(100))
    });
}

#[gpui_kit::test]
fn recent_dialog_arrows_do_not_scroll_the_pdf(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    for name in ["field-guide", "selection-sample"] {
        store
            .record(
                &RecentStore::normalize(&PathBuf::from(format!("examples/{name}.pdf"))).unwrap(),
            )
            .unwrap();
    }
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (handle, document) = open(
        cx,
        Some(RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap()),
    );
    cx.run_until_parked();
    let before = document.read_with(cx, |document, cx| {
        document.reader().read(cx).reading_position()
    });
    cx.update(|cx| {
        cx.update_window(handle, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowRecent), cx);
        })
        .unwrap();
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.update_window(handle, |_, frame, cx| frame.press("down", cx))
            .unwrap();
    });
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).reading_position(), before);
        assert_eq!(
            document
                .recent
                .as_ref()
                .unwrap()
                .read(cx)
                .selected_row(cx)
                .unwrap(),
            1
        );
    });
}

#[gpui_kit::test]
fn an_old_document_open_cannot_dismiss_a_new_recent_dialog(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    RecentStore::new(&paths).record(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
    });
    let (window, document) = open(cx, None);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let old = document.read_with(cx, |document, _| {
        document.recent.as_ref().unwrap().entity_id()
    });
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.update_window(window, |_, frame, cx| frame.press("escape", cx))
        .unwrap();
    assert!(document.read_with(cx, |document, _| document.recent.is_none()));
    cx.update_window(window, |_, frame, cx| {
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    let new = document.read_with(cx, |document, _| {
        document.recent.as_ref().unwrap().entity_id()
    });
    assert_ne!(old, new);
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).page_count(), Some(100));
        let picker = document
            .recent
            .as_ref()
            .expect("new picker was dismissed by old open");
        assert_eq!(picker.entity_id(), new);
        assert!(!picker.read(cx).files().iter().any(|file| file.path == pdf));
        assert!(!picker.read(cx).is_loading());
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(frame.find("recent-panel").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn replacing_a_recent_open_cancels_its_request_and_ignores_old_completion(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    let replacement = RecentStore::normalize(Path::new("examples/field-guide.pdf")).unwrap();
    RecentStore::new(&paths).record(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (window, document) = open(cx, None);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    let generation = document.read_with(cx, |document, _| {
        document
            .recent_open
            .as_ref()
            .expect("recent open pending")
            .generation
    });
    // Simulate another open entry point, before the first load can finish.
    reader.update(cx, |reader, cx| reader.open_path(replacement.clone(), cx));
    cx.run_until_parked();
    reader.update(cx, |_, cx| {
        cx.emit(DocumentEvent::Opened {
            generation,
            result: Err("obsolete load error".into()),
        });
    });
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert!(document.recent_open.is_none());
        assert_eq!(
            document.reader().read(cx).path(),
            Some(replacement.as_path())
        );
        let picker = document
            .recent
            .as_ref()
            .expect("unrelated open must not close this picker")
            .read(cx);
        assert!(!picker.is_loading());
        assert!(picker.error().is_none());
    });
}

#[gpui_kit::test]
fn closing_window_during_recent_open_releases_both_owners(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    RecentStore::new(&paths).record(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (window, document) = open(cx, None);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let picker = document.read_with(cx, |document, _| {
        document.recent.as_ref().unwrap().downgrade()
    });
    let reader = document.read_with(cx, |document, _| document.reader().downgrade());
    let owner = document.downgrade();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    drop(document);
    cx.update_window(window, |_, frame, _| frame.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(owner.upgrade().is_none());
    assert!(reader.upgrade().is_none());
    assert!(picker.upgrade().is_none());
}
