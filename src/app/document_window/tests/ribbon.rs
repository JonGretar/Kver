use super::*;
use crate::app::ribbon::{Choice, Notice, present};
use std::{cell::Cell, rc::Rc};

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[gpui_kit::test]
fn linux_update_offer_and_cancelled_restart_preserve_the_document(cx: &mut TestAppContext) {
    use crate::app::linux_updates;
    setup(cx);
    let (window, document) = open(cx, Some("examples/selection-sample.pdf".into()));
    cx.update_window(window, |_, w, _| w.activate_window())
        .unwrap();
    cx.run_until_parked();
    super::saving::highlight(cx, window, &document);
    for managed in [false, true] {
        cx.update(|cx| linux_updates::test_notice(managed, false, cx));
        cx.update_window(window, |_, w, cx| {
            w.render_frame(cx);
            assert_eq!(w.try_find("notice-action").is_some(), managed);
        })
        .unwrap();
    }
    cx.update(|cx| linux_updates::test_notice(true, true, cx));
    cx.update_window(window, |_, w, cx| {
        w.render_frame(cx);
        w.click("notice-action", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, w, cx| {
        w.render_frame(cx);
        w.click("unsaved-cancel", cx);
        w.render_frame(cx);
        assert_eq!(w.find("notice-action").label(), Some("Restart to update"));
        assert!(document.read(cx).reader().read(cx).is_dirty(cx));
    })
    .unwrap();
}

#[gpui_kit::test]
fn ribbon_dispatches_only_the_selected_choice(cx: &mut TestAppContext) {
    setup(cx);
    let (window, _) = open(cx, None);
    cx.update_window(window, |_, window, _| window.activate_window())
        .unwrap();
    let replies = Rc::new(Cell::new(0));
    for (button, choice) in [
        ("dismiss-notice", Choice::Dismiss),
        ("notice-action", Choice::Activate),
    ] {
        let replies = replies.clone();
        cx.update(|cx| {
            present(
                Some(Notice {
                    message: "A new version is available.".into(),
                    link: None,
                    action: Some("Download update".into()),
                    respond: Rc::new(move |result, _| {
                        assert!(result == choice);
                        replies.set(replies.get() + 1);
                    }),
                }),
                cx,
            )
        });
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.click(button, cx);
            window.render_frame(cx);
            assert!(window.try_find("dismiss-notice").is_none());
        })
        .unwrap();
    }
    assert_eq!(replies.get(), 2);
}
