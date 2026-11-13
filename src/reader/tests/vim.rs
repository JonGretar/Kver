//! Vim binding workflow through the resolved policy and the composed window.
use super::*;
use crate::app::keymap::{self, Preset};

#[gpui_kit::test]
fn vim_reader_navigation_and_marks_share_one_focused_workflow(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("book.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        keymap::install_preset(cx, Preset::Vim);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();

    cx.simulate_keystrokes(window, "shift-j");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    cx.simulate_keystrokes(window, "m a");
    cx.run_until_parked();
    let saved = RecentStore::new(&paths).marks(&pdf).unwrap()[&'a'].position;
    assert_eq!(saved.page, 1);

    cx.simulate_keystrokes(window, "shift-j");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));
    cx.simulate_keystrokes(window, "' a");
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));

    cx.simulate_keystrokes(window, "ctrl-o");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));
    cx.simulate_keystrokes(window, "ctrl-i");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));

    let before_scroll = reader.read_with(cx, |reader, _| reader.reading_position());
    cx.simulate_keystrokes(window, "j");
    reader.read_with(cx, |reader, _| {
        assert!(reader.reading_position().1 > before_scroll.1);
    });
    cx.simulate_keystrokes(window, "k");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_position(), before_scroll)
    });
    let before_zoom = reader.read_with(cx, |reader, _| reader.scale());
    cx.simulate_keystrokes(window, "=");
    reader.read_with(cx, |reader, _| assert!(reader.scale() > before_zoom));
    cx.simulate_keystrokes(window, "-");
    reader.read_with(cx, |reader, _| assert_eq!(reader.scale(), before_zoom));
    cx.simulate_keystrokes(window, "w");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth)
    });
    cx.simulate_keystrokes(window, "f");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.zoom_mode(), ZoomMode::FitPage)
    });
    cx.simulate_keystrokes(window, "s");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage)
    });
    cx.simulate_keystrokes(window, "c");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
        assert_eq!(reader.zoom_mode(), ZoomMode::FitPage);
    });
    cx.simulate_keystrokes(window, "d m a");
    cx.run_until_parked();
    assert!(
        !RecentStore::new(&paths)
            .marks(&pdf)
            .unwrap()
            .contains_key(&'a')
    );
    cx.simulate_keystrokes(window, "shift-k");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn vim_pending_prefix_cancels_on_escape_and_focus_transfer(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let pdf = root.path().join("book.pdf");
    std::fs::write(&pdf, three_page_pdf([(600, 1400); 3])).unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        keymap::install_preset(cx, Preset::Vim);
        recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, reader) = recent_test_window(cx, Some(pdf.clone()));
    cx.run_until_parked();
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    let store = RecentStore::new(&paths);

    for prefix in ["m", "'", "d", "d m", "g", "a"] {
        cx.simulate_keystrokes(window, prefix);
        cx.update_window(window, |_, frame, _| {
            assert!(frame.has_pending_keystrokes())
        })
        .unwrap();
        cx.simulate_keystrokes(window, "escape");
        cx.update_window(window, |_, frame, _| {
            assert!(!frame.has_pending_keystrokes())
        })
        .unwrap();
    }
    cx.simulate_keystrokes(window, "m");
    cx.simulate_keystrokes(window, "/");
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(!frame.has_pending_keystrokes());
        frame.input("a", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!store.marks(&pdf).unwrap().contains_key(&'a'));
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    // Dismissing Find returns the reader's bare-key ownership.
    cx.simulate_keystrokes(window, "escape shift-j");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 2));
}

#[gpui_kit::test]
fn vim_native_window_command_survives_focused_find_input(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        keymap::install_preset(cx, Preset::Vim);
        crate::app::windows::install(cx);
    });
    let (window, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    let (other, other_reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.activate_window();
        frame.render_frame(cx);
        frame.press("m", cx);
        assert!(frame.has_pending_keystrokes());
        frame.press("/", cx);
        frame.render_frame(cx);
        assert!(!frame.has_pending_keystrokes());
        frame.input("n", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    cx.simulate_keystrokes(window, "cmd-w");
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows()), vec![other]);
    other_reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn vim_horizontal_half_screen_and_document_edges_use_reader_actions(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        keymap::install_preset(cx, Preset::Vim);
    });
    let root = tempfile::tempdir().unwrap();
    let pdf = root.path().join("wide.pdf");
    std::fs::write(&pdf, three_page_pdf([(2000, 2200); 3])).unwrap();
    let (window, reader) = recent_test_window(cx, Some(pdf));
    cx.run_until_parked();
    let height = cx
        .update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            reader.read(cx).viewport().2
        })
        .unwrap();
    reader.update(cx, |reader, cx| {
        reader.zoom_in(cx);
        reader.zoom_in(cx);
    });
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();

    cx.simulate_keystrokes(window, "l");
    reader.read_with(cx, |reader, _| {
        let right = -f32::from(reader.scroll.offset().x);
        assert_eq!(right, 100.0);
    });
    cx.simulate_keystrokes(window, "right");
    reader.read_with(cx, |reader, _| {
        assert_eq!(-f32::from(reader.scroll.offset().x), 200.0);
        assert_eq!(reader.page_index(), 0);
    });
    cx.simulate_keystrokes(window, "h left");
    reader.read_with(cx, |reader, _| {
        assert_eq!(-f32::from(reader.scroll.offset().x), 0.0)
    });

    cx.simulate_keystrokes(window, "space");
    reader.read_with(cx, |reader, _| {
        assert!((reader.viewport().0 - height * 0.5).abs() < 2.0);
    });
    cx.simulate_keystrokes(window, "shift-space");
    reader.read_with(cx, |reader, _| assert_eq!(reader.viewport().0, 0.0));
    cx.simulate_keystrokes(window, "shift-g");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 2);
        let max = reader
            .layout()
            .clamp_scroll(0.0, f32::MAX, reader.viewport().2)
            .1;
        assert_eq!(reader.viewport().0, max);
    });
    cx.simulate_keystrokes(window, "g g");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.reading_position(), (0, 0.0));
    });
    cx.simulate_keystrokes(window, "w l right");
    reader.read_with(cx, |reader, _| {
        assert_eq!(-f32::from(reader.scroll.offset().x), 0.0);
        assert_eq!(reader.page_index(), 0);
    });
}

#[gpui_kit::test]
fn vim_find_slash_and_wide_keys_do_not_intercept_its_input(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        keymap::install_preset(cx, Preset::Vim);
    });
    let (window, reader) =
        recent_test_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "/");
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.input("h/l n p g", cx);
        frame.press("space", cx);
        frame.press("left", cx);
        frame.press("right", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.reading_position(), (0, 0.0));
    });
    cx.simulate_keystrokes(window, "escape shift-j");
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
}
