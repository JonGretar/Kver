//! Preset-specific markup commands through the production document window.
use super::*;
use crate::{
    app::keymap::{self, Preset},
    pdf::annotations::{Annotation, MarkupKind},
};

fn entries(document: &Entity<DocumentWindow>, cx: &TestAppContext) -> Vec<Annotation> {
    document.read_with(cx, |d, cx| {
        d.reader
            .read(cx)
            .session()
            .unwrap()
            .read(cx)
            .annotations()
            .unwrap()
            .unwrap()
            .as_ref()
            .clone()
    })
}

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, keys: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for key in keys.split_whitespace() {
            window.press(key, cx);
        }
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn both_presets_create_and_repeat_markup_without_leaking_into_inputs(cx: &mut TestAppContext) {
    setup(cx);
    for (preset, keys, apply, annotations) in [
        (
            Preset::Standard,
            [
                "secondary-shift-h",
                "secondary-shift-u",
                "secondary-shift-x",
                "secondary-alt-u",
            ],
            "secondary-enter",
            "secondary-shift-a",
        ),
        (Preset::Vim, ["a h", "a u", "a s", "a w"], "a a", "g a"),
    ] {
        cx.update(|cx| keymap::reload(cx, None, preset).unwrap());
        let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
        cx.run_until_parked();
        super::saving::select_text(cx, handle, &document);
        press(cx, handle, apply);
        assert_eq!(entries(&document, cx)[0].kind(), MarkupKind::Highlight);
        press(cx, handle, "cmd-z");
        assert!(entries(&document, cx).is_empty());
        for (kind, key) in MarkupKind::ALL.into_iter().zip(keys) {
            press(cx, handle, key);
            let catalog = entries(&document, cx);
            assert_eq!(catalog.iter().filter(|a| a.kind() == kind).count(), 1);
            document.read_with(cx, |d, cx| {
                assert_eq!(d.reader.read(cx).active_markup(), kind);
                assert!(d.reader.read(cx).selected_text().is_some());
            });
            press(cx, handle, apply);
            assert_eq!(entries(&document, cx).len(), catalog.len());
        }
        press(cx, handle, annotations);
        document.read_with(cx, |d, cx| {
            assert_eq!(d.sidebar_panel(cx), Some(SidebarPanel::Annotations));
        });
        press(cx, handle, "escape");
        // Modified creation keys belong only to Reader, never an input.
        press(
            cx,
            handle,
            if preset == Preset::Vim { "/" } else { "cmd-f" },
        );
        cx.update_window(handle, |_, window, cx| window.input("aahuux", cx))
            .unwrap();
        press(cx, handle, "secondary-shift-u");
        assert_eq!(entries(&document, cx).len(), 4);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("find-query").value(), Some("aahuux"));
        })
        .unwrap();
        // A sequence prefix is cancelled before typing in a modal.
        press(cx, handle, "escape");
        if preset == Preset::Vim {
            press(cx, handle, "a");
            press(cx, handle, "escape");
            assert_eq!(entries(&document, cx).len(), 4);
        }
        press(cx, handle, "cmd-shift-p");
        cx.update_window(handle, |_, window, cx| window.input("aahux", cx))
            .unwrap();
        document.read_with(cx, |d, cx| {
            assert_eq!(d.palette_query(cx).unwrap().as_ref(), "aahux")
        });
        assert_eq!(entries(&document, cx).len(), 4);
        cx.update_window(handle, |_, window, _| window.remove_window())
            .unwrap();
        drop(document);
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn list_shortcuts_delete_the_list_target_and_keep_focus_after_the_final_row(
    cx: &mut TestAppContext,
) {
    setup(cx);
    for (preset, highlight, underline, show, delete, undo, redo) in [
        (
            Preset::Standard,
            "secondary-shift-h",
            "secondary-shift-u",
            "secondary-shift-a",
            "backspace",
            "cmd-z",
            "cmd-shift-z",
        ),
        (Preset::Vim, "a h", "a u", "g a", "x", "u", "ctrl-r"),
    ] {
        cx.update(|cx| keymap::reload(cx, None, preset).unwrap());
        let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
        cx.run_until_parked();
        super::saving::select_text(cx, handle, &document);
        press(cx, handle, highlight);
        press(cx, handle, underline);
        let catalog = entries(&document, cx);
        let highlight = catalog
            .iter()
            .find(|a| a.kind() == MarkupKind::Highlight)
            .unwrap()
            .id();
        let underline = catalog
            .iter()
            .find(|a| a.kind() == MarkupKind::Underline)
            .unwrap()
            .id();
        let row = |id: crate::pdf::annotations::AnnotationId| {
            format!("annotation-{}-{}", id.epoch(), id.serial())
        };
        press(cx, handle, show);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(row(highlight), cx);
        })
        .unwrap();
        cx.run_until_parked();
        // Page target is still Underline. Deletion must instead remove the list's Highlight.
        press(cx, handle, delete);
        assert_eq!(
            entries(&document, cx)
                .iter()
                .map(Annotation::id)
                .collect::<Vec<_>>(),
            [underline]
        );
        press(cx, handle, undo);
        assert_eq!(entries(&document, cx).len(), 2);
        press(cx, handle, redo);
        assert_eq!(
            entries(&document, cx)
                .iter()
                .map(Annotation::id)
                .collect::<Vec<_>>(),
            [underline]
        );
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(row(underline), cx);
        })
        .unwrap();
        cx.run_until_parked();
        press(cx, handle, delete);
        assert!(entries(&document, cx).is_empty());
        press(cx, handle, delete); // No target must not fall through to Reader.
        press(cx, handle, undo);
        assert_eq!(
            entries(&document, cx)
                .iter()
                .map(Annotation::id)
                .collect::<Vec<_>>(),
            [underline]
        );
        cx.update_window(handle, |_, window, _| window.remove_window())
            .unwrap();
        drop(document);
        cx.run_until_parked();
    }
}
