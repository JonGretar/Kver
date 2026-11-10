use super::*;
use gpui_kit::{AppContext as _, Focusable as _, TestAppContext, test::TestWindowExt};

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
