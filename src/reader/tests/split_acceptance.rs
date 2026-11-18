//! Integrated production-window checks across document kinds and retained work.

use super::*;
use crate::app::document_window::{
    ClosePane, DocumentWindow, KeepOnlyPane, SplitSideBySide, SplitStacked, SwitchPane,
};
use gpui_kit::component::{Theme, ThemeMode};

fn settle(cx: &mut TestAppContext, handle: AnyWindowHandle) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
}

fn action(cx: &mut TestAppContext, handle: AnyWindowHandle, action: impl gpui::Action) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(action), cx);
    })
    .unwrap();
    settle(cx, handle);
}

fn open(
    cx: &mut TestAppContext,
    path: PathBuf,
) -> (AnyWindowHandle, Entity<DocumentWindow>, Entity<ReaderView>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        settings_ui::install_with_paths(cx, None);
    });
    let (handle, document) = cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| DocumentWindow::new(Some(path), window, cx))
        })
        .unwrap()
    });
    cx.simulate_window_resize(handle, size(px(1200.), px(900.)));
    settle(cx, handle);
    let first = document.read_with(cx, |d, _| d.reader().clone());
    (handle, document, first)
}

#[gpui_kit::test]
fn long_textless_split_keeps_both_working_sets_bounded_and_releases_removed_owner(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("textless-500.pdf");
    std::fs::write(&path, pdf_with_boxes(&[(612, 792); 500], &[""; 500])).unwrap();
    let (handle, document, first) = open(cx, path);
    action(cx, handle, SinglePage);
    action(cx, handle, FitWidth);
    action(cx, handle, SplitSideBySide);
    let second = document.read_with(cx, |d, _| d.reader().clone());
    action(cx, handle, ToggleReadingRuler);
    action(cx, handle, SwitchPane);
    action(cx, handle, ToggleReadingRuler);
    for page in [10, 250, 499] {
        first.update(cx, |r, cx| r.select_page(page, cx));
        second.update(cx, |r, cx| r.select_page(499 - page, cx));
        settle(cx, handle);
        let bytes = [&first, &second].map(|reader| {
            reader.read_with(cx, |r, _| {
                assert_eq!(r.page_count(), Some(500));
                assert_eq!(r.displayed_page_index(), Some(r.page_index()));
                assert!(r.rasters.cached_pages().count() <= 1);
                assert!(r.page_text.cached_pages().count() <= 3);
                assert!(r.retained_bitmap_bytes() <= BITMAP_BUDGET);
                r.retained_bitmap_bytes()
            })
        });
        assert!(bytes.into_iter().sum::<usize>() <= 2 * BITMAP_BUDGET);
    }
    // Rapid requests coalesce to the final viewport instead of queuing every page.
    for page in 100..120 {
        first.update(cx, |r, cx| r.select_page(page, cx));
        second.update(cx, |r, cx| r.select_page(499 - page, cx));
    }
    settle(cx, handle);
    for reader in [&first, &second] {
        reader.read_with(cx, |r, _| {
            assert_eq!(r.displayed_page_index(), Some(r.page_index()));
            assert_eq!(r.rasters.cached_pages().count(), 1);
            assert!(r.page_text.cached_pages().count() <= 3);
        });
    }
    action(cx, handle, KeepOnlyPane);
    second.read_with(cx, |r, _| {
        assert_eq!(r.retained_bitmap_bytes(), 0);
        assert_eq!(r.page_text.cached_pages().count(), 0);
        assert!(r.session().is_none());
    });
    let removed = second.downgrade();
    drop(second);
    settle(cx, handle);
    assert!(
        removed.upgrade().is_none(),
        "removed pane retained by pending work"
    );
}

#[gpui_kit::test]
fn rotated_cropped_and_multicolumn_split_survive_find_resize_theme_and_rapid_removal(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    for rotation in [0, 90, 180, 270] {
        let path = root.path().join(format!("columns-{rotation}.pdf"));
        let mut fixture = lopdf::Document::load("examples/reading-order.pdf").unwrap();
        let page = fixture.get_pages()[&1];
        let dictionary = fixture.get_dictionary_mut(page).unwrap();
        dictionary.set(
            "CropBox",
            vec![20.into(), 30.into(), 590.into(), 780.into()],
        );
        dictionary.set("Rotate", rotation);
        fixture.save(&path).unwrap();
        let (handle, document, first) = open(cx, path);
        action(cx, handle, FitPage);
        action(cx, handle, SplitSideBySide);
        let second = document.read_with(cx, |d, _| d.reader().clone());
        let before = [&first, &second].map(|r| r.read_with(cx, |r, _| r.reading_position()));
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            cx.update(|cx| Theme::change(mode, None, cx));
            for orientation in [false, true] {
                if orientation {
                    action(cx, handle, SplitStacked);
                } else {
                    action(cx, handle, SplitSideBySide);
                }
                for font in [16., 24.] {
                    cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font)));
                    for dimensions in [(650., 500.), (1250., 950.)] {
                        cx.simulate_window_resize(handle, size(px(dimensions.0), px(dimensions.1)));
                        settle(cx, handle);
                        for (reader, position) in [&first, &second].into_iter().zip(before) {
                            reader.read_with(cx, |r, _| {
                                assert_eq!(r.reading_position().0, position.0);
                                assert_eq!(r.zoom_mode(), ZoomMode::FitPage);
                                assert!(r.retained_bitmap_bytes() <= BITMAP_BUDGET);
                                assert!(r.rasters.cached_pages().count() <= 3);
                            });
                        }
                        cx.update_window(handle, |_, window, _| {
                            let a = window.find(("reading-pane", first.entity_id())).bounds();
                            let b = window.find(("reading-pane", second.entity_id())).bounds();
                            assert!(a.size.width > px(0.) && a.size.height > px(0.));
                            assert!(b.size.width > px(0.) && b.size.height > px(0.));
                            if orientation {
                                assert_eq!(a.bottom(), b.top());
                            } else {
                                assert_eq!(a.right(), b.left());
                            }
                        })
                        .unwrap();
                    }
                    assert_eq!(document.read_with(cx, |d, _| d.reader().clone()), second);
                }
            }
        }
        action(cx, handle, ShowFind);
        cx.update_window(handle, |_, window, cx| window.input("columns", cx))
            .unwrap();
        settle(cx, handle);
        cx.update_window(handle, |_, window, cx| {
            assert!(window.find(("find-result", 0usize)).visible());
            window.press("enter", cx);
        })
        .unwrap();
        settle(cx, handle);
        assert_eq!(first.read_with(cx, |r, _| r.reading_position()), before[0]);
        action(cx, handle, ToggleSidebar);
        // Stress high logical zoom and change the layout before its rasters settle.
        second.update(cx, |r, cx| {
            for _ in 0..7 {
                r.zoom_in(cx);
            }
        });
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(ClosePane), cx);
        })
        .unwrap();
        settle(cx, handle);
        assert_eq!(document.read_with(cx, |d, _| d.reader().clone()), first);
        assert_eq!(second.read_with(cx, |r, _| r.retained_bitmap_bytes()), 0);
        first.read_with(cx, |r, cx| assert!(!r.is_dirty(cx)));
        cx.update_window(handle, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
    }
}
