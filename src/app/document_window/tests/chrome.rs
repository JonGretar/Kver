use super::*;

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn edit_menu_groups_annotation_commands_and_routes_selected_edits(cx: &mut TestAppContext) {
    use gpui_kit::{OwnedMenu, OwnedMenuItem};

    fn annotation_menu(cx: &mut TestAppContext) -> OwnedMenu {
        cx.update(|cx| {
            let edit = cx
                .get_menus()
                .unwrap()
                .into_iter()
                .find(|m| m.name == "Edit")
                .unwrap();
            for name in ["Undo", "Redo", "Copy", "Find"] {
                assert!(edit.items.iter().any(|item| {
                    matches!(item, OwnedMenuItem::Action { name: actual, .. } if actual == name)
                }));
            }
            assert!(!edit.items.iter().any(|item| {
                matches!(item, OwnedMenuItem::Action { name, .. } if name == "Highlight")
            }));
            edit.items
                .into_iter()
                .find_map(|item| match item {
                    OwnedMenuItem::Submenu(menu) if menu.name == "Annotation" => Some(menu),
                    _ => None,
                })
                .unwrap()
        })
    }

    setup(cx);
    cx.update(menu::install);
    let (handle, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let initial = annotation_menu(cx);
    for name in [
        "Highlight",
        "Underline",
        "Strikethrough",
        "Squiggly Underline",
        "Delete Annotation",
    ] {
        assert!(initial.items.iter().any(|item| {
            matches!(item, OwnedMenuItem::Action { name: actual, disabled: true, .. } if actual == name)
        }));
    }
    super::saving::select_text(cx, handle, &document);
    let creation = annotation_menu(cx)
        .items
        .into_iter()
        .find_map(|item| match item {
            OwnedMenuItem::Action {
                name,
                action,
                disabled: false,
                ..
            } if name == "Underline" => Some(action),
            _ => None,
        })
        .expect("creation becomes available for selected text");
    cx.update_window(handle, |_, window, cx| window.dispatch_action(creation, cx))
        .unwrap();
    cx.run_until_parked();
    let editing = annotation_menu(cx);
    assert!(editing.items.iter().any(|item| {
        matches!(item, OwnedMenuItem::Submenu(menu) if menu.name == "Color" && menu.items.len() == 5)
    }));
    let delete = editing
        .items
        .into_iter()
        .find_map(|item| match item {
            OwnedMenuItem::Action {
                name,
                action,
                disabled: false,
                ..
            } if name == "Delete Annotation" => Some(action),
            _ => None,
        })
        .expect("selected annotation can be deleted");
    cx.update_window(handle, |_, window, cx| window.dispatch_action(delete, cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert!(!d.reader.read(cx).is_dirty(cx));
        assert!(d.reader.read(cx).selected_page_annotation(cx).is_none());
    });
}

#[gpui_kit::test]
fn annotation_split_applies_immediately_and_remembers_window_local_type(cx: &mut TestAppContext) {
    use crate::pdf::annotations::{HighlightColor, MarkupKind};
    use gpui_kit::test::{TestEventExt as _, TestQueryExt as _};

    setup(cx);
    let (first, document) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    let (second, other) = open(cx, Some(PathBuf::from("examples/selection-sample.pdf")));
    cx.run_until_parked();
    let click_label = |cx: &mut TestAppContext, label: &str| {
        cx.update_window(first, |_, window, cx| {
            window.render_frame(cx);
            let at = if label == "Apply another annotation type" {
                window.find_by_label(label).bounds().center()
            } else {
                window
                    .within("popup-menu")
                    .find_by_label(label)
                    .bounds()
                    .center()
            };
            window.pointer_down(at, gpui_kit::MouseButton::Left, cx);
            window.pointer_up(at, gpui_kit::MouseButton::Left, cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    cx.update_window(first, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("highlight-selection").disabled(), Some(true));
        window.click("annotation-color", cx);
    })
    .unwrap();
    cx.run_until_parked();
    click_label(cx, "Blue");
    for participant in [&document, &other] {
        participant.read_with(cx, |d, cx| {
            let reader = d.reader.read(cx);
            assert_eq!(reader.markup_color(cx), HighlightColor::Blue);
            assert!(!reader.is_dirty(cx));
            assert!(!reader.has_annotation_undo(cx));
        });
    }
    super::saving::select_text(cx, first, &document);
    for kind in MarkupKind::ALL {
        click_label(cx, "Apply another annotation type");
        click_label(cx, kind.label());
        document.read_with(cx, |d, cx| {
            let reader = d.reader.read(cx);
            assert_eq!(reader.active_markup(), kind);
            assert!(reader.selected_text().is_some());
            let entries = reader
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap();
            assert_eq!(entries.iter().filter(|a| a.kind() == kind).count(), 1);
        });
        cx.update_window(first, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window.find("highlight-selection").label(),
                Some(kind.label())
            );
            // Applying again selects the duplicate without another edit.
            window.click("highlight-selection", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    other.read_with(cx, |d, cx| {
        let reader = d.reader.read(cx);
        assert_eq!(reader.active_markup(), MarkupKind::Highlight);
        assert!(reader.selected_text().is_none());
        assert_eq!(
            reader
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()
                .len(),
            4
        );
    });
    // The color picker changes future creation, not the selected Squiggly.
    cx.update_window(first, |_, window, cx| window.click("annotation-color", cx))
        .unwrap();
    cx.run_until_parked();
    click_label(cx, "Pink");
    document.read_with(cx, |d, cx| {
        let reader = d.reader.read(cx);
        assert_eq!(reader.markup_color(cx), HighlightColor::Pink);
        assert_eq!(
            reader.selected_page_annotation(cx).unwrap().color(),
            HighlightColor::Red.color()
        );
    });
    // Shared colors do not switch another window's active type or dirty the PDF.
    cx.update_window(second, |_, window, cx| {
        window.dispatch_action(Box::new(crate::reader::Squiggly), cx)
    })
    .unwrap();
    cx.run_until_parked();
    other.read_with(cx, |d, cx| {
        assert_eq!(d.reader.read(cx).markup_color(cx), HighlightColor::Pink);
    });
    cx.update_window(first, |_, window, cx| {
        window.dispatch_action(Box::new(crate::reader::UndoAnnotation), cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |d, cx| {
        assert_eq!(
            d.reader
                .read(cx)
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()
                .len(),
            3
        );
    });
}

#[gpui_kit::test]
fn sidebar_destinations_and_annotation_controls_fit_at_interface_scales(cx: &mut TestAppContext) {
    use gpui_kit::{component::theme::Theme, px, size};

    setup(cx);
    let (handle, _) = open(cx, Some(PathBuf::from("examples/annotations-clean.pdf")));
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.dispatch_action(Box::new(ShowAnnotations), cx);
    })
    .unwrap();
    cx.run_until_parked();
    for font_size in [12., 16., 20.] {
        cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
        for width in [360., 600., 920., 1200.] {
            cx.simulate_window_resize(handle, size(px(width), px(800.)));
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let sidebar = window.find("reader-sidebar").bounds();
                if let Some(button) = window.try_find("highlight-selection") {
                    let color = window.find("annotation-color").bounds();
                    assert!(button.bounds().left() >= sidebar.right());
                    assert!(button.bounds().right() <= color.left());
                    assert!(color.right() <= px(width));
                } else {
                    assert!(window.find("reader-overflow").visible());
                }
                let mut right = sidebar.left();
                for (id, name) in [
                    ("sidebar-contents", "Contents"),
                    ("sidebar-annotations", "Annotations"),
                    ("sidebar-find", "Find"),
                ] {
                    let button = window.find(id);
                    assert_eq!(button.label(), Some(name));
                    assert!(button.bounds().left() >= right);
                    assert!(button.bounds().right() <= sidebar.right());
                    right = button.bounds().right();
                }
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn filename_stays_centered_without_overlapping_titlebar_controls(cx: &mut TestAppContext) {
    use gpui_kit::{component::theme::Theme, px, size};

    setup(cx);
    for path in [
        None,
        Some(PathBuf::from("examples/readme-sample.pdf")),
        Some(PathBuf::from(format!(
            "{}.pdf",
            "long-document-name-".repeat(12)
        ))),
    ] {
        let expected = path
            .as_ref()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .unwrap_or_else(|| "A quiet place to read".into());
        let (handle, _) = open(cx, path);
        cx.run_until_parked();
        for font_size in [12., 16., 20.] {
            cx.update(|cx| Theme::update(cx, |theme| theme.font_size = px(font_size)));
            for width in [360., 600., 920., 1200.] {
                cx.simulate_window_resize(handle, size(px(width), px(800.)));
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    let title = window.find("document-name");
                    let open = window.find("open-pdf");
                    assert!(window.try_find("reader-brand").is_none());
                    assert!(window.try_find("document-unsaved").is_none());
                    assert_eq!(title.label(), Some(expected.as_str()));
                    assert!((title.bounds().center().x - px(width / 2.)).abs() <= px(0.5));
                    assert!(title.bounds().size.width > px(0.));
                    assert!(title.bounds().left() >= px(0.));
                    assert!(title.bounds().right() < open.bounds().left());
                    assert!(open.visible());
                    assert!(open.bounds().right() <= px(width));
                })
                .unwrap();
            }
        }
        cx.update_window(handle, |_, window, _| window.remove_window())
            .unwrap();
    }
}

#[gpui_kit::test]
fn page_count_and_zoom_keep_their_anchors_as_page_numbers_grow(cx: &mut TestAppContext) {
    setup(cx);
    let (handle, reader) = reader_window(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    let mut anchors = None;
    for page in [0, 8, 9, 99] {
        reader.update(cx, |reader, cx| reader.select_page(page, cx));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let status = window.find("reader-status");
            let zoom = window.find("reader-zoom");
            assert!(window.try_find("document-unsaved").is_none());
            assert_eq!(
                status.label(),
                Some(format!("Page {} of 100", page + 1).as_str())
            );
            assert_eq!(zoom.label(), Some("100%"));
            let current = (status.bounds().left(), zoom.bounds().right());
            assert_eq!(*anchors.get_or_insert(current), current);
            assert!(status.bounds().right() < zoom.bounds().left());
        })
        .unwrap();
    }
}
