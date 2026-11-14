//! Mounted production document windows; native pointer/key dispatch, real worker.
use super::*;
use crate::reading_history::RulerAnchor;

fn setup(cx: &mut TestAppContext) -> (tempfile::TempDir, PathBuf, RecentStore) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let path = root.path().join("columns.pdf");
    std::fs::copy("examples/reading-order.pdf", &path).unwrap();
    let path = RecentStore::normalize(&path).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    (root, path, RecentStore::new(&paths))
}

fn anchor(page: usize, line: usize) -> RulerAnchor {
    let text = pdf::page_text(Path::new("examples/reading-order.pdf"), page).unwrap();
    let [left, bottom, right, top] = crate::reader::ruler::document_rect(&text, line).unwrap();
    RulerAnchor {
        page,
        x: (left + right) / 2.,
        y: (bottom + top) / 2.,
    }
}

fn place(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    reader: &Entity<ReaderView>,
    at: RulerAnchor,
) {
    let screen = reader.read_with(cx, |reader, _| {
        reader
            .page_viewport(at.page)
            .unwrap()
            .document_to_screen(Point { x: at.x, y: at.y })
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        let id = gpui::SharedString::from(format!("pdf-page-{}", at.page));
        let bounds = frame.find(id.clone()).bounds();
        frame.click_with_options(
            id,
            ClickOptions::new()
                .with_offset(point(px(screen.x as f32), px(screen.y as f32)) - bounds.origin)
                .with_modifiers(Modifiers::alt()),
            cx,
        );
    })
    .unwrap();
    settle(cx, window);
}

#[gpui_kit::test]
fn ruler_reopen_is_off_resolves_visible_column_and_windows_never_sync_or_save_on_close(
    cx: &mut TestAppContext,
) {
    let (_root, path, store) = setup(cx);
    let (first_window, first) = open(cx, path.to_str().unwrap(), 2);
    place(cx, first_window, &first, anchor(2, 1));
    assert_eq!(target(&first, cx), (2, 1));
    let (second_window, second) = open(cx, path.to_str().unwrap(), 2);
    second.read_with(cx, |r, _| {
        assert!(!r.ruler.enabled);
        assert!(r.ruler.target.is_none());
    });
    let offset = second.read_with(cx, |r, _| r.scroll.offset());
    cx.simulate_keystrokes(second_window, "r");
    settle(cx, second_window);
    assert_eq!(target(&second, cx), (2, 1));
    assert_eq!(second.read_with(cx, |r, _| r.scroll.offset()), offset);
    place(cx, second_window, &second, anchor(2, 7));
    assert_eq!(target(&second, cx), (2, 7));
    assert_eq!(target(&first, cx), (2, 1));
    // Closing the older window must not turn its retained target into a save.
    cx.update_window(first_window, |_, window, _| window.remove_window())
        .unwrap();
    drop(first);
    cx.run_until_parked();
    assert_eq!(store.ruler_anchor(&path).unwrap(), Some(anchor(2, 7)));
    let (third_window, third) = open(cx, path.to_str().unwrap(), 2);
    cx.simulate_keystrokes(third_window, "r");
    settle(cx, third_window);
    assert_eq!(target(&third, cx), (2, 7));
}

#[gpui_kit::test]
fn ruler_nonvisible_missing_or_changed_geometry_falls_back_without_viewport_jump(
    cx: &mut TestAppContext,
) {
    let (_root, path, store) = setup(cx);
    store.record(&path).unwrap();
    // A different page must never become a ruler-driven viewport restore.
    store.save_ruler_anchor(&path, anchor(2, 7)).unwrap();
    let (window, reader) = open(cx, path.to_str().unwrap(), 0);
    let before = reader.read_with(cx, |r, _| r.scroll.offset());
    cx.simulate_keystrokes(window, "r");
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 0);
    assert_eq!(reader.read_with(cx, |r, _| r.scroll.offset()), before);
    // An old anchor on a still-present page but outside all current text is
    // rejected, not snapped to an arbitrary closest column.
    store
        .save_ruler_anchor(
            &path,
            RulerAnchor {
                page: 2,
                x: 1e9,
                y: 1e9,
            },
        )
        .unwrap();
    // A fresh worker models restart rather than the existing session cache.
    let paths = crate::settings::AppPaths::for_locations(
        _root.path().join("config.toml"),
        _root.path().join("local"),
    );
    cx.update(|cx| recent::install(cx, Some(paths)));
    let (window, changed) = open(cx, path.to_str().unwrap(), 2);
    let before = changed.read_with(cx, |r, _| r.scroll.offset());
    cx.simulate_keystrokes(window, "r");
    settle(cx, window);
    assert_eq!(target(&changed, cx).0, 2);
    assert_eq!(changed.read_with(cx, |r, _| r.scroll.offset()), before);
    changed.read_with(cx, |r, _| assert!(r.ruler_visible()));
}

#[gpui_kit::test]
fn ruler_reload_does_not_overwrite_a_newer_window_and_changed_pdf_has_no_stale_target(
    cx: &mut TestAppContext,
) {
    let (_root, path, store) = setup(cx);
    let (a, first) = open(cx, path.to_str().unwrap(), 2);
    place(cx, a, &first, anchor(2, 1));
    let (b, second) = open(cx, path.to_str().unwrap(), 2);
    place(cx, b, &second, anchor(2, 7));
    let generation = first.read_with(cx, |r, _| r.document_generation);
    let mut pdf = lopdf::Document::load(&path).unwrap();
    pdf.renumber_objects_with(500);
    pdf.save(&path).unwrap();
    cx.executor().advance_clock(Duration::from_secs(2));
    settle(cx, a);
    settle(cx, b);
    assert!(first.read_with(cx, |r, _| r.document_generation) > generation);
    assert_eq!(target(&first, cx), (2, 1));
    assert_eq!(target(&second, cx), (2, 7));
    assert_eq!(store.ruler_anchor(&path).unwrap(), Some(anchor(2, 7)));
    // Replace the contents at the same canonical identity with textless pages.
    // Neither old Arc<PageText> nor old line identities may survive the reload.
    std::fs::write(&path, three_page_pdf([(600, 800); 3])).unwrap();
    cx.executor().advance_clock(Duration::from_secs(2));
    settle(cx, a);
    settle(cx, b);
    for reader in [&first, &second] {
        reader.read_with(cx, |r, cx| {
            assert!(r.ruler.enabled);
            assert!(r.ruler.target.is_none());
            assert!(!r.is_dirty(cx));
        });
    }
    assert_eq!(store.ruler_anchor(&path).unwrap(), Some(anchor(2, 7)));
}

#[gpui_kit::test]
fn ruler_delayed_restore_cannot_replace_placement_or_cross_document_generation(
    cx: &mut TestAppContext,
) {
    let (_root, path, _store) = setup(cx);
    let (window, reader) = open(cx, path.to_str().unwrap(), 2);
    let epoch = cx.update(|cx| recent::history(cx).unwrap().epoch());
    let generation = reader.read_with(cx, |r, _| r.document_generation);
    place(cx, window, &reader, anchor(2, 7));
    reader.update(cx, |r, cx| {
        // Deliver the actual restore completion seam after a newer pointer event.
        r.finish_ruler_load(generation, epoch, Ok(Some(anchor(2, 1))), cx);
    });
    cx.simulate_keystrokes(window, "r r");
    settle(cx, window);
    assert_eq!(target(&reader, cx), (2, 7));
    reader.update(cx, |r, cx| {
        r.open_path(PathBuf::from("examples/readme-sample.pdf"), cx)
    });
    settle(cx, window);
    reader.update(cx, |r, cx| {
        r.finish_ruler_load(generation, epoch, Ok(Some(anchor(2, 1))), cx);
    });
    cx.simulate_keystrokes(window, "r");
    settle(cx, window);
    assert_eq!(target(&reader, cx).0, 0);
}

#[gpui_kit::test]
fn ruler_is_useful_between_windows_when_history_recording_is_disabled(cx: &mut TestAppContext) {
    let (root, path, _store) = setup(cx);
    cx.update(|cx| {
        let work = recent::history(cx).unwrap().set_recording(false);
        cx.spawn(async move |_| work.await.unwrap()).detach();
    });
    cx.run_until_parked();
    let (window, first) = open(cx, path.to_str().unwrap(), 2);
    place(cx, window, &first, anchor(2, 7));
    let (window, second) = open(cx, path.to_str().unwrap(), 2);
    cx.simulate_keystrokes(window, "r");
    settle(cx, window);
    assert_eq!(target(&second, cx), (2, 7));
    assert!(!root.path().join("local/reading.sqlite3").exists());
}
