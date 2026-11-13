use super::*;
use gpui_kit::{Focusable as _, TestAppContext, test::TestWindowExt};

#[test]
fn finder_file_urls_preserve_special_characters() {
    let path = std::env::temp_dir().join("PDF space # % æ.pdf");
    let url = url::Url::from_file_path(&path).unwrap();
    assert_eq!(local_path(url.as_str()), Some(path));
}

#[test]
fn ignore_non_file_and_invalid_urls() {
    assert_eq!(local_path("https://example.com/book.pdf"), None);
    assert_eq!(local_path("not a URL"), None);
}

#[gpui_kit::test]
fn onboarding_choices_apply_to_the_initial_pdf(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
    assert!(crate::settings::create_if_missing(paths.config()).unwrap());
    let (welcome, finished) = cx.update(|cx| {
        initialize_ui(cx);
        onboarding::open(paths.config().to_path_buf(), cx).unwrap()
    });
    assert_eq!(cx.update(|cx| cx.windows()), vec![welcome]);
    cx.update_window(welcome, |_, window, cx| {
        window.render_frame(cx);
        window.within("onboarding-reading-view").click(1usize, cx);
        window.within("onboarding-keyboard").click(1usize, cx);
        window.click("onboarding-start", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(finished.try_recv().is_ok());
    let document = cx.update(|cx| {
        // The native quit bridge requires the process main thread. Exercise
        // the production services/reader handoff here without installing it.
        initialize_services(cx, Some(paths));
        windows::open(cx, Some("examples/readme-sample.pdf".into())).unwrap()
    });
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(cx.windows().len(), 1);
        let reader = document.read(cx).reader().read(cx);
        assert_eq!(
            reader.path(),
            Some(std::path::Path::new("examples/readme-sample.pdf"))
        );
        assert_eq!(
            reader.reading_mode(),
            crate::reader::ReadingMode::SinglePage
        );
        assert_eq!(
            settings_ui::state(cx).read(cx).keyboard_preset(),
            keymap::Preset::Vim
        );
    });
}

#[gpui_kit::test]
fn startup_wires_document_and_auxiliary_window_shortcuts(cx: &mut TestAppContext) {
    // Use the production initialization order without accessing user storage.
    let (reader, document) = cx.update(|cx| {
        initialize(cx, None);
        let document = windows::open(cx, None).unwrap();
        (cx.windows()[0], document)
    });
    cx.update_window(reader, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-,", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let settings = cx.update(|cx| {
        assert_eq!(cx.windows().len(), 2);
        let active = cx.active_window().unwrap();
        assert_ne!(active, reader);
        active
    });
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("appearance-choice").visible());
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows()), vec![reader]);

    cx.update_window(reader, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            document
                .read(cx)
                .reader()
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    })
    .unwrap();

    cx.update(|cx| {
        cx.update_window(reader, |_, window, _| window.activate_window())
            .unwrap();
        cx.dispatch_action(&commands::AboutKver);
    });
    let about = cx.update(|cx| {
        assert_eq!(cx.windows().len(), 2);
        let active = cx.active_window().unwrap();
        assert_ne!(active, reader);
        active
    });
    cx.update_window(about, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-w", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows()), vec![reader]);

    cx.update_window(reader, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.press("cmd-n", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let second = cx.update(|cx| {
        assert_eq!(cx.windows().len(), 2);
        let active = cx.active_window().unwrap();
        assert_ne!(active, reader);
        active
    });
    cx.update_window(second, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-w", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows()), vec![reader]);
}
