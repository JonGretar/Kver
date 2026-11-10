//! History persistence and dialog workflows through real document windows.

use super::*;
use crate::{
    app::keymap::install as bind_reader_keys,
    reading_history::{self as recent, RecentStore},
    settings_ui::ShowSettings,
};
use gpui_kit::{point, px, test::TestEventExt as _};
use std::path::Path;

#[gpui_kit::test]
fn fresh_private_session_never_creates_reading_database(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::RememberReadingHistory(false),
    )
    .unwrap();
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
    });
    let (first, _) = open(cx, Some(pdf.clone()));
    let (second, document) = open(cx, None);
    cx.run_until_parked();
    cx.update_window(second, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert_eq!(
            document.recent.as_ref().unwrap().read(cx).files()[0].path,
            pdf
        );
    });
    assert!(!paths.local_data().join("reading.sqlite3").exists());
    cx.update_window(first, |_, window, _| window.remove_window())
        .unwrap();
}

#[gpui_kit::test]
fn history_off_is_session_only_across_reader_windows_and_reenable_saves_it(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    let missing = root.path().join("offline-drive.pdf");
    store.record(&missing).unwrap();
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::RememberReadingHistory(false),
    )
    .unwrap();
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (_, first) = open(cx, Some(pdf.clone()));
    let (second_window, second) = open(cx, None);
    cx.run_until_parked();
    assert!(!store.contains(&pdf).unwrap(), "off wrote to SQLite");
    assert!(store.contains(&missing).unwrap(), "off erased old data");
    cx.update_window(second_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    second.read_with(cx, |document, cx| {
        let files = document.recent.as_ref().unwrap().read(cx).files();
        assert_eq!(files[0].path, pdf);
        assert_eq!(files[1].path, missing);
    });
    // Restart while recording is off: only previously saved paths survive.
    cx.update(|cx| recent::install(cx, Some(paths.clone())));
    cx.run_until_parked();
    assert_eq!(store.search("").unwrap().len(), 1);
    assert_eq!(store.search("").unwrap()[0].path, missing);
    first.update(cx, |document, cx| document.open_path(pdf.clone(), cx));
    cx.run_until_parked();
    cx.update(|cx| cx.dispatch_action(&ShowSettings));
    cx.run_until_parked();
    let settings_window = cx.update(|cx| *cx.windows().last().unwrap());
    cx.update_window(settings_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.within("settings-sidebar").click("0-2", cx);
        frame.click("remember-reading-history", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(
        crate::settings::load(paths.config())
            .unwrap()
            .remember_reading_history()
    );
    assert!(
        store.contains(&pdf).unwrap(),
        "re-enabling did not flush the session"
    );
    first.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).path(), Some(pdf.as_path()))
    });
    second.read_with(cx, |document, cx| {
        assert_eq!(document.reader().read(cx).path(), None)
    });
    // Reinitializing the same locations represents another process.
    cx.update(|cx| recent::install(cx, Some(paths.clone())));
    cx.run_until_parked();
    assert_eq!(store.search("").unwrap()[0].path, pdf);
}

#[gpui_kit::test]
fn forgetting_one_pdf_invalidates_open_recent_in_other_windows_and_does_not_resurrect(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    let missing = root.path().join("offline.pdf");
    store.record(&pdf).unwrap();
    store.record(&missing).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths.clone()));
    });
    let (first_window, first) = open(cx, None);
    let (second_window, second) = open(cx, Some(pdf.clone()));
    cx.run_until_parked();
    for window in [first_window, second_window] {
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.dispatch_action(Box::new(ShowRecent), cx);
        })
        .unwrap();
    }
    cx.run_until_parked();
    first
        .read_with(cx, |document, _| document.recent.as_ref().unwrap().clone())
        .update(cx, |picker, cx| picker.remove(missing.clone(), cx));
    cx.run_until_parked();
    assert!(!store.contains(&missing).unwrap());
    assert!(store.contains(&pdf).unwrap());
    second.read_with(cx, |document, cx| {
        assert!(
            !document
                .recent
                .as_ref()
                .unwrap()
                .read(cx)
                .files()
                .iter()
                .any(|file| file.path == missing)
        );
        assert_eq!(document.reader().read(cx).path(), Some(pdf.as_path()));
    });
    cx.update(|cx| recent::install(cx, Some(paths.clone())));
    cx.run_until_parked();
    assert!(!store.contains(&missing).unwrap());
}

#[gpui_kit::test]
fn open_recent_excludes_the_active_pdf_without_changing_stored_history(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    let active = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    let older = root.path().join("older.pdf");
    store.record(&older).unwrap();
    store.record(&active).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (first_window, first) = open(cx, Some(active.clone()));
    let (second_window, second) = open(cx, None);
    cx.run_until_parked();
    cx.update_window(first_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |document, cx| {
        let files = document.recent.as_ref().unwrap().read(cx).files();
        assert_eq!(
            files.iter().map(|file| &file.path).collect::<Vec<_>>(),
            vec![&older]
        );
    });
    cx.update_window(first_window, |_, frame, cx| {
        frame.input("readme-sample", cx)
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |document, cx| {
        assert!(
            document
                .recent
                .as_ref()
                .unwrap()
                .read(cx)
                .files()
                .is_empty()
        )
    });
    assert!(store.contains(&active).unwrap());
    cx.update_window(second_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    second.read_with(cx, |document, cx| {
        assert_eq!(
            document.recent.as_ref().unwrap().read(cx).files()[0].path,
            active
        );
    });
}

#[gpui_kit::test]
fn clicking_outside_recent_closes_it_and_restores_reader_focus(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    let store = RecentStore::new(&paths);
    store.record(&pdf).unwrap();
    store
        .record(&RecentStore::normalize(Path::new("examples/field-guide.pdf")).unwrap())
        .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (handle, document) = open(cx, None);
    cx.update_window(handle, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, frame, cx| {
        frame.click("input-state", cx);
        let focus = document
            .read(cx)
            .recent
            .as_ref()
            .unwrap()
            .read(cx)
            .focus_handle(cx);
        assert!(
            focus.is_focused(frame),
            "clicking fresh search moved focus to the reader"
        );
    })
    .unwrap();
    let before = document.read_with(cx, |document, cx| {
        document.recent.as_ref().unwrap().read(cx).selected_row(cx)
    });
    cx.update_window(handle, |_, frame, cx| {
        frame.click("recent-header", cx);
        frame.press("down", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_ne!(
            document.recent.as_ref().unwrap().read(cx).selected_row(cx),
            before,
            "arrow keys must navigate recent rows after clicking the header"
        );
    });
    cx.update_window(handle, |_, frame, cx| frame.input("read", cx))
        .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(
            document.recent.as_ref().unwrap().read(cx).query(cx),
            "read",
            "clicking the panel header must not strand keyboard search"
        );
    });
    cx.update_window(handle, |_, frame, cx| {
        let outside = point(px(20.), px(200.));
        frame.pointer_down(outside, gpui_kit::MouseButton::Left, cx);
        frame.pointer_up(outside, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert!(
            document.recent.is_none(),
            "outside click should dismiss Open Recent"
        )
    });
    cx.update_window(handle, |_, frame, cx| {
        assert!(
            document
                .read(cx)
                .reader()
                .read(cx)
                .focus_handle(cx)
                .is_focused(frame),
            "outside click should return focus to the reader"
        );
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("escape", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert!(
            document.recent.is_none(),
            "Escape should dismiss Open Recent"
        )
    });
}

#[gpui_kit::test]
fn row_x_confirms_the_specific_pdf_without_opening_it(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    store.record(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (handle, document) = open(cx, None);
    cx.update_window(handle, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let forget_id = format!("forget-pdf:{}", pdf.display());
    cx.update_window(handle, |_, frame, cx| {
        assert_eq!(
            frame.find(forget_id.clone()).label(),
            Some("Forget readme-sample.pdf")
        );
        assert!(
            frame
                .find("recent-panel")
                .bounds()
                .contains(&frame.find(forget_id.clone()).bounds().center()),
            "X is outside the recent overlay"
        );
        frame.click(forget_id.clone(), cx);
        assert!(
            document.read(cx).recent.is_some(),
            "X click dismissed Recent"
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("ok").label(), Some("Forget PDF"));
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert!(
            document.reader().read(cx).path().is_none(),
            "X must not open the PDF row"
        )
    });
    assert!(
        store.contains(&pdf).unwrap(),
        "X alone must not delete data"
    );
    cx.update_window(handle, |_, frame, cx| frame.click("cancel", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(
        store.contains(&pdf).unwrap(),
        "Cancel must preserve the record"
    );
    cx.update_window(handle, |_, frame, cx| {
        frame.simulate_next_frame(cx);
        let focus = document
            .read(cx)
            .recent
            .as_ref()
            .unwrap()
            .read(cx)
            .focus_handle(cx);
        assert!(
            focus.is_focused(frame),
            "search not focused after confirmation"
        );
        frame.click("input-state", cx);
        assert!(
            document.read(cx).recent.is_some(),
            "search click dismissed recent"
        );
        assert!(focus.is_focused(frame), "clicking search changed focus");
        frame.input("readme", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(
            document.recent.as_ref().unwrap().read(cx).query(cx),
            "readme",
            "cancelling Forget must return focus to recent-file search"
        );
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, frame, cx| frame.click(forget_id.clone(), cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, frame, cx| frame.click("ok", cx))
        .unwrap();
    cx.run_until_parked();
    assert!(!store.contains(&pdf).unwrap());
    assert!(pdf.exists(), "forgetting must not delete the PDF");
    cx.update_window(handle, |_, frame, cx| {
        frame.simulate_next_frame(cx);
        let focus = document
            .read(cx)
            .recent
            .as_ref()
            .unwrap()
            .read(cx)
            .focus_handle(cx);
        assert!(
            focus.is_focused(frame),
            "search lost focus after forgetting a row"
        );
        frame.input("x", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(
            document.recent.as_ref().unwrap().read(cx).query(cx),
            "readmex",
            "confirming Forget must return focus even when its row disappears"
        );
    });
}

#[gpui_kit::test]
fn recent_action_search_and_missing_removal_work_across_windows(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let store = RecentStore::new(&paths);
    let missing = root.path().join("missing-older.pdf");
    store.record(&missing).unwrap();
    let pdf = RecentStore::normalize(Path::new("examples/readme-sample.pdf")).unwrap();
    store.record(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        recent::install(cx, Some(paths));
    });
    let (first_window, first) = open(cx, None);
    let (_, second) = open(cx, None);
    cx.update_window(first_window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ShowRecent), cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |document, cx| {
        let panel = document.recent.as_ref().unwrap().read(cx);
        assert_eq!(panel.files().len(), 2);
        assert_eq!(panel.files()[0].path, pdf);
    });
    second.read_with(cx, |document, cx| {
        assert!(document.recent.is_none());
        assert!(document.reader().read(cx).path().is_none());
    });
    cx.update_window(first_window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("recent-panel").label(), Some("Open Recent"));
        frame.input("missing-older", cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |document, cx| {
        assert_eq!(
            document.recent.as_ref().unwrap().read(cx).files()[0].path,
            missing
        )
    });
    cx.update_window(first_window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |document, cx| {
        let panel = document.recent.as_ref().unwrap().read(cx);
        assert!(panel.error().unwrap().contains("Could not open"));
        assert_eq!(document.reader().read(cx).page_count(), None);
    });
    first
        .read_with(cx, |document, _| document.recent.as_ref().unwrap().clone())
        .update(cx, |picker, cx| picker.remove(missing.clone(), cx));
    cx.run_until_parked();
    first.read_with(cx, |document, cx| {
        let panel = document.recent.as_ref().unwrap().read(cx);
        assert!(!panel.is_loading(), "removal still pending");
        assert!(
            panel.error().is_none(),
            "removal failed: {:?}",
            panel.error()
        );
    });
    assert!(!store.contains(&missing).unwrap());
    assert!(store.contains(&pdf).unwrap());
    second.read_with(cx, |document, cx| {
        assert!(document.reader().read(cx).path().is_none())
    });
}
