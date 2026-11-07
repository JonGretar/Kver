//! Modal/request lifetimes without a reader or PDF renderer.

use super::*;
use crate::{
    reading_history::{self, RecentStore},
    settings::AppPaths,
};
use gpui_kit::{AnyWindowHandle, TestAppContext, test::TestWindowExt};

fn show_picker(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<RecentPicker>) {
    cx.update(|cx| {
        let (handle, _) =
            gpui_kit::open_window(Default::default(), cx, |_, cx| cx.new(|_| gpui_kit::Empty))
                .unwrap();
        let picker = cx
            .update_window(handle, |_, window, cx| {
                let picker = cx.new(|cx| RecentPicker::new(None, window, cx));
                picker.update(cx, |picker, cx| picker.show(window, cx));
                picker
            })
            .unwrap();
        (handle, picker)
    })
}

fn setup(cx: &mut TestAppContext) -> (tempfile::TempDir, RecentStore, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
    let store = RecentStore::new(&paths);
    let first = root.path().join("first.pdf");
    let second = root.path().join("second.pdf");
    store.record(&first).unwrap();
    store.record(&second).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        reading_history::install(cx, Some(paths));
    });
    (root, store, first, second)
}

#[gpui_kit::test]
fn late_open_reply_cannot_close_a_reopened_picker(cx: &mut TestAppContext) {
    let (_root, _store, _, _) = setup(cx);
    let (handle, old) = show_picker(cx);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    let request = old.read_with(cx, |picker, _| picker.pending_open.unwrap());
    cx.update_window(handle, |_, window, cx| window.press("escape", cx))
        .unwrap();
    assert!(!old.read_with(cx, |picker, _| picker.open));

    let new = cx
        .update_window(handle, |_, window, cx| {
            let new = cx.new(|cx| RecentPicker::new(None, window, cx));
            new.update(cx, |picker, cx| picker.show(window, cx));
            new
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.input("second", cx);
        old.update(cx, |picker, cx| {
            picker.finish_open(request, Ok(()), window, cx)
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert!(new.read_with(cx, |picker, _| picker.open));
    assert_eq!(new.read_with(cx, |picker, cx| picker.query(cx)), "second");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("recent-panel").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn search_refresh_cannot_finish_or_replace_an_open_request(cx: &mut TestAppContext) {
    let (_root, store, _, second) = setup(cx);
    let (handle, picker) = show_picker(cx);
    cx.run_until_parked();
    picker.update(cx, |picker, cx| {
        picker.request_open(0, cx);
        picker.search("second".into(), cx);
    });
    let first_request = picker.read_with(cx, |picker, _| picker.pending_open.unwrap());
    cx.run_until_parked();
    picker.update(cx, |picker, cx| {
        assert!(!picker.searching);
        assert!(picker.is_loading());
        assert_eq!(picker.files().len(), 1);
        picker.remove(second.clone(), cx);
    });
    assert!(
        store.contains(&second).unwrap(),
        "open should block removal"
    );
    cx.update_window(handle, |_, window, cx| {
        picker.update(cx, |picker, cx| {
            picker.finish_open(first_request, Err("first attempt".into()), window, cx);
            assert!(!picker.is_loading());
            picker.request_open(0, cx);
            let second_request = picker.pending_open.unwrap();
            assert_ne!(first_request, second_request);
            picker.finish_open(first_request, Ok(()), window, cx);
            assert!(picker.open);
            assert_eq!(picker.pending_open, Some(second_request));
            picker.finish_open(second_request, Err("second attempt".into()), window, cx);
            assert!(picker.error().unwrap().contains("second attempt"));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn confirmed_forget_notifies_other_windows_after_origin_closes(cx: &mut TestAppContext) {
    let (_root, store, first, _) = setup(cx);
    let (origin, picker) = show_picker(cx);
    let (_, other) = show_picker(cx);
    cx.run_until_parked();
    picker.update(cx, |picker, cx| picker.remove(first.clone(), cx));
    cx.update_window(origin, |_, window, cx| {
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .unwrap();
    drop(picker);
    cx.run_until_parked();
    assert!(!store.contains(&first).unwrap());
    other.read_with(cx, |picker, _| {
        assert!(!picker.files().iter().any(|file| file.path == first));
        assert!(!picker.is_loading());
        assert!(picker.error().is_none());
    });
}

#[gpui_kit::test]
fn replacing_search_discards_obsolete_rows(cx: &mut TestAppContext) {
    let (_root, _store, _, second) = setup(cx);
    let (handle, picker) = show_picker(cx);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.input("first", cx);
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.press("cmd-a", cx);
        window.input("second", cx);
    })
    .unwrap();
    cx.run_until_parked();
    picker.read_with(cx, |picker, cx| {
        assert_eq!(picker.query(cx), "second");
        assert_eq!(picker.files(), &[RecentFile { path: second }]);
        assert!(!picker.is_loading());
    });
}
