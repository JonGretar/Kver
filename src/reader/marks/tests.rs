//! Marks own their popup and worker lifecycle without a PDF renderer.

use super::*;
use crate::{reading_history::RecentStore, settings::AppPaths};
use gpui_kit::{AnyWindowHandle, TestAppContext, test::TestWindowExt};

fn setup(cx: &mut TestAppContext) -> (tempfile::TempDir, RecentStore, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
    let store = RecentStore::new(&paths);
    let path = root.path().join("book.pdf");
    store.record(&path).unwrap();
    store
        .replace_marks(
            &path,
            &BTreeMap::from([(
                'a',
                LocalMark {
                    position: ReadingPosition { page: 1, y: 80.0 },
                    label: None,
                },
            )]),
        )
        .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        history::install(cx, Some(paths));
        let work = history::history(cx).unwrap().record(path.clone());
        cx.spawn(async move |_| work.await.unwrap()).detach();
    });
    cx.run_until_parked();
    (root, store, path)
}

fn show(cx: &mut TestAppContext, path: PathBuf) -> (AnyWindowHandle, Entity<LocalMarks>) {
    cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            let restore_focus = cx.focus_handle();
            cx.new(|cx| {
                let mut marks = LocalMarks::new(path, 1, cx);
                marks.show(MarksView::List, restore_focus, window, cx);
                marks
            })
        })
        .unwrap()
    })
}

#[gpui_kit::test]
fn name_reply_cannot_finish_a_reopened_editor(cx: &mut TestAppContext) {
    let (_root, store, path) = setup(cx);
    let (window, marks) = show(cx, path.clone());
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("name-mark:a", cx);
        window.render_frame(cx);
        window.input("Original editor", cx);
        marks.update(cx, |marks, cx| {
            // Queue the real write, then replace its editor before polling the
            // reply. Seed the reopened list with the last loaded presentation.
            let listed = marks.listed_marks().clone();
            marks.save_name('a', cx);
            assert!(marks.panel.as_ref().unwrap().saving);
            let focus = marks.panel.as_ref().unwrap().restore_focus.clone();
            marks.show(MarksView::List, focus, window, cx);
            marks.panel.as_mut().unwrap().marks = listed;
            marks.edit_name('a', window, cx);
            marks
                .name_input()
                .update(cx, |input, cx| input.set_value("New draft", window, cx));
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        store.marks(&path).unwrap()[&'a'].label.as_deref(),
        Some("Original editor")
    );
    marks.read_with(cx, |marks, cx| {
        assert_eq!(marks.view(), Some(MarksView::Name('a')));
        assert_eq!(marks.name_input().read(cx).value(), "New draft");
    });
}

#[gpui_kit::test]
fn refresh_preserves_pending_name_save_and_validation(cx: &mut TestAppContext) {
    let (_root, store, path) = setup(cx);
    let (window, marks) = show(cx, path.clone());
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("name-mark:a", cx);
        window.render_frame(cx);
        window.input(&"x".repeat(81), cx);
        window.press("enter", cx);
    })
    .unwrap();
    marks.read_with(cx, |marks, _| {
        let panel = marks.panel.as_ref().unwrap();
        assert!(panel.error.as_ref().unwrap().contains("80 characters"));
        assert!(!panel.saving);
    });
    assert_eq!(store.marks(&path).unwrap()[&'a'].label, None);
    // Independently completing refreshes must not clear the write's busy flag.
    marks.update(cx, |marks, cx| {
        marks.panel.as_mut().unwrap().saving = true;
        marks.refresh(cx);
    });
    cx.run_until_parked();
    marks.read_with(cx, |marks, _| {
        assert!(!marks.panel.as_ref().unwrap().loading);
        assert!(marks.panel.as_ref().unwrap().saving);
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press("escape", cx);
    })
    .unwrap();
    assert_eq!(
        marks.read_with(cx, |marks, _| marks.view()),
        Some(MarksView::Name('a'))
    );
}

#[gpui_kit::test]
fn edits_refresh_other_windows_even_after_origin_is_dropped(cx: &mut TestAppContext) {
    let (_root, store, path) = setup(cx);
    let (_, other) = show(cx, path.clone());
    cx.run_until_parked();
    let origin = cx.new(|cx| LocalMarks::new(path.clone(), 1, cx));
    let weak = origin.downgrade();
    origin.update(cx, |marks, cx| marks.change('a', None, cx));
    drop(origin);
    cx.run_until_parked();
    assert!(
        weak.upgrade().is_none(),
        "a write must not retain its window owner"
    );
    assert!(store.marks(&path).unwrap().is_empty());
    other.read_with(cx, |marks, _| assert!(marks.listed_marks().is_empty()));
}

#[gpui_kit::test]
fn forget_invalidates_visible_marks_and_pending_navigation(cx: &mut TestAppContext) {
    let (_root, store, path) = setup(cx);
    let (_, marks) = show(cx, path.clone());
    cx.run_until_parked();
    let jumps = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = jumps.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&marks, move |_, event, _| {
            if matches!(event, MarksEvent::Jump { .. }) {
                count.set(count.get() + 1);
            }
        })
    });
    marks.update(cx, |marks, cx| marks.jump('a', cx));
    cx.update(|cx| {
        let history = history::history(cx).unwrap();
        let work = history.remove(path.clone());
        cx.spawn(async move |cx| {
            work.await.unwrap();
            cx.update(history::notify_changed);
        })
        .detach();
    });
    cx.run_until_parked();
    assert_eq!(jumps.get(), 0);
    assert!(!store.contains(&path).unwrap());
    marks.read_with(cx, |marks, _| assert!(marks.listed_marks().is_empty()));
}
