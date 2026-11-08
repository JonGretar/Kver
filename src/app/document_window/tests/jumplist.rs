//! Semantic navigation through real document windows and representative PDFs.

use super::*;
use crate::reader::{ReadingMode, ZoomMode};

fn linked_window(
    cx: &mut TestAppContext,
) -> (tempfile::TempDir, AnyWindowHandle, Entity<DocumentWindow>) {
    setup(cx);
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("linked.pdf");
    std::fs::write(&path, crate::pdf::linked_fixture()).unwrap();
    let (window, document) = open(cx, Some(path));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
    (root, window, document)
}

#[gpui_kit::test]
fn link_records_once_and_restores_logical_zoom_and_reading_mode(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.fit_width(cx);
    });
    let before = reader.read_with(cx, |reader, _| reader.jump_location().unwrap());
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2);
        assert_eq!(document.jumplist.cursor, 1);
        assert!(document.jumplist.entries[0].equivalent(&before));
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
        assert_eq!(reader.scale(), 1.5);
    });
    let (origin, destination) = document.read_with(cx, |document, _| {
        (
            document.jumplist.entries[0].clone(),
            document.jumplist.entries[1].clone(),
        )
    });
    reader.update(cx, |reader, cx| assert!(reader.restore_jump(&origin, cx)));
    reader.read_with(cx, |reader, _| {
        assert!(reader.jump_location().unwrap().equivalent(&before));
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
        assert_eq!(reader.selected_text(), None);
    });
    reader.update(cx, |reader, cx| {
        assert!(reader.restore_jump(&destination, cx))
    });
    reader.read_with(cx, |reader, _| {
        assert!(reader.jump_location().unwrap().equivalent(&destination));
        assert!((reader.reading_position().1 - 500.0).abs() < 1.0);
        assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
        assert_eq!(reader.scale(), 1.5);
    });
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2)
    });
}

#[gpui_kit::test]
fn ordinary_motion_does_not_record_and_repeated_destination_coalesces(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    reader.update(cx, |reader, cx| {
        reader.next_page(cx);
        reader.previous_page(cx);
        reader.zoom_in(cx);
        reader.fit_page(cx);
        reader.scroll_by(50.0, cx);
        reader.select_page(0, cx);
    });
    document.read_with(cx, |document, _| {
        assert!(document.jumplist.entries.is_empty())
    });
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    let destination = document.read_with(cx, |document, _| document.jumplist.entries[1].clone());
    // The same PDF destination, activated again, is not another history entry.
    let path = reader.read_with(cx, |reader, _| reader.path().unwrap().to_path_buf());
    let link = crate::pdf::page_links(&path, 0).unwrap().remove(0);
    reader.update(cx, |reader, cx| {
        reader.navigate_outline(link.destination().unwrap(), cx)
    });
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2);
        assert!(document.jumplist.entries[1].equivalent(&destination));
    });
}

#[gpui_kit::test]
fn later_jump_captures_departure_without_losing_previous_destination(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    let (first_origin, first_destination) = document.read_with(cx, |document, _| {
        (
            document.jumplist.entries[0].clone(),
            document.jumplist.entries[1].clone(),
        )
    });
    // A later jump from a page-turn departure captures that place at jump
    // time, without rewriting the earlier semantic destination.
    document.update(cx, |document, _| {
        document.jumplist.record(&JumpEvent {
            from: first_origin.clone(),
            to: first_destination.clone(),
        });
    });
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 4);
        assert_eq!(document.jumplist.cursor, 3);
        assert!(document.jumplist.entries[1].equivalent(&first_destination));
        assert!(document.jumplist.entries[2].equivalent(&first_origin));
    });
}

#[gpui_kit::test]
fn contents_and_find_record_in_only_their_window(cx: &mut TestAppContext) {
    setup(cx);
    let (window, document) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    let (_, independent) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-t", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click(("contents-entry", 0usize), cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2)
    });
    independent.read_with(cx, |document, _| {
        assert!(document.jumplist.entries.is_empty())
    });

    let (find_window, found) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    cx.update_window(find_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-f", cx);
        frame.render_frame(cx);
        frame.input("This is page", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(find_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click(("find-result", 2usize), cx);
    })
    .unwrap();
    found.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2)
    });
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2)
    });
    independent.read_with(cx, |document, _| {
        assert!(document.jumplist.entries.is_empty())
    });
}

#[gpui_kit::test]
fn local_mark_jump_records_only_a_valid_destination(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let path = root.path().join("book.pdf");
    std::fs::write(&path, crate::pdf::linked_fixture()).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::keymap::install(cx);
        crate::recent::install(cx, Some(paths.clone()));
        settings_ui::install_with_paths(cx, Some(paths));
    });
    let (window, document) = open(cx, Some(path));
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(SetLocalMark('a')), cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(JumpLocalMark('a')), cx);
    })
    .unwrap();
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2);
    });
}

#[gpui_kit::test]
fn replacement_clears_history_and_rejects_obsolete_jump(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    let old = document.read_with(cx, |document, _| document.jumplist.entries[0].clone());
    let obsolete = document.read_with(cx, |document, _| JumpEvent {
        from: document.jumplist.entries[0].clone(),
        to: document.jumplist.entries[1].clone(),
    });
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
    });
    reader.update(cx, |_, cx| cx.emit(obsolete));
    document.read_with(cx, |document, _| {
        assert!(document.jumplist.entries.is_empty())
    });
    cx.run_until_parked();
    reader.update(cx, |reader, cx| assert!(!reader.restore_jump(&old, cx)));
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn keyboard_traversal_restores_both_locations_without_appending(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    reader.update(cx, |reader, cx| {
        reader.set_reading_mode(ReadingMode::SinglePage, cx);
        reader.fit_width(cx);
    });
    let origin = reader.read_with(cx, |reader, _| reader.jump_location().unwrap());
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    let destination = reader.read_with(cx, |reader, _| reader.jump_location().unwrap());
    reader.update(cx, |reader, cx| reader.select_page(0, cx));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("ctrl-o", cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert!(reader.jump_location().unwrap().equivalent(&origin));
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
        assert_eq!(reader.zoom_mode(), ZoomMode::FitWidth);
    });
    document.read_with(cx, |document, _| {
        assert!(document.jumplist.can_forward());
        assert!(!document.jumplist.can_back());
        assert_eq!(document.jumplist.entries.len(), 2);
    });
    cx.update_window(window, |_, frame, cx| frame.press("ctrl-i", cx))
        .unwrap();
    reader.read_with(cx, |reader, _| {
        assert!(reader.jump_location().unwrap().equivalent(&destination));
        assert_eq!(reader.zoom_mode(), ZoomMode::Fixed);
        assert_eq!(reader.scale(), 1.5);
    });
    document.read_with(cx, |document, _| {
        assert!(!document.jumplist.can_forward());
        assert!(document.jumplist.can_back());
        assert_eq!(document.jumplist.entries.len(), 2);
    });
}

#[gpui_kit::test]
fn toolbar_back_and_forward_use_jumplist_actions_and_disable_at_boundaries(
    cx: &mut TestAppContext,
) {
    let (_root, window, document) = linked_window(cx);
    cx.simulate_window_resize(
        window,
        gpui_kit::size(gpui_kit::px(360.), gpui_kit::px(480.)),
    );
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert!(frame.find("navigate-back").visible());
        assert!(frame.find("navigate-forward").visible());
        assert!(frame.find("settings-button").visible());
        assert!(
            frame.find("navigate-forward").bounds().right()
                <= frame.find("previous-page").bounds().left()
        );
        assert!(
            frame.find("settings-button").bounds().right()
                <= frame.find("document-window").bounds().right()
        );
    })
    .unwrap();
    cx.simulate_window_resize(
        window,
        gpui_kit::size(gpui_kit::px(920.), gpui_kit::px(800.)),
    );
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("navigate-back").disabled(), Some(true));
        assert_eq!(frame.find("navigate-forward").disabled(), Some(true));
        frame.click("navigate-back", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert!(document.jumplist.entries.is_empty())
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("navigate-forward").disabled(), Some(true));
        frame.click("navigate-back", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.jumplist.cursor, 0);
        assert_eq!(document.reader().read(cx).page_index(), 0);
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("navigate-back").disabled(), Some(true));
        frame.click("navigate-back", cx);
        frame.click("navigate-forward", cx);
    })
    .unwrap();
    document.read_with(cx, |document, cx| {
        assert_eq!(document.jumplist.cursor, 1);
        assert_eq!(document.reader().read(cx).page_index(), 1);
        assert_eq!(document.jumplist.entries.len(), 2);
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("navigate-forward").disabled(), Some(true));
        frame.click("navigate-forward", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| assert_eq!(document.jumplist.cursor, 1));
}

#[gpui_kit::test]
fn new_jump_after_back_discards_forward_branch(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("ctrl-o", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| assert!(document.jumplist.can_forward()));
    let path = reader.read_with(cx, |reader, _| reader.path().unwrap().to_path_buf());
    let link = crate::pdf::page_links(&path, 0).unwrap().remove(0);
    // A different departure state produces a new branch even if the target
    // is the same page as the abandoned destination.
    reader.update(cx, |reader, cx| {
        reader.zoom_in(cx);
        reader.navigate_outline(link.destination().unwrap(), cx);
    });
    document.read_with(cx, |document, _| {
        assert!(!document.jumplist.can_forward());
        assert_eq!(
            document.jumplist.cursor,
            document.jumplist.entries.len() - 1
        );
    });
    cx.update_window(window, |_, frame, cx| frame.press("ctrl-i", cx))
        .unwrap();
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
}

#[gpui_kit::test]
fn invalid_entries_are_removed_and_obsolete_generation_cannot_restore(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    let old = document.read_with(cx, |document, _| document.jumplist.entries[0].clone());
    document.update(cx, |document, _| {
        document.jumplist.entries[0] = old.clone().with_page(usize::MAX);
    });
    cx.update_window(window, |_, frame, cx| frame.press("ctrl-o", cx))
        .unwrap();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 1);
        assert!(!document.jumplist.can_back());
    });
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));

    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
    });
    cx.run_until_parked();
    document.update(cx, |document, cx| {
        document.jumplist.entries = vec![old];
        document
            .jumplist
            .entries
            .push(reader.read(cx).jump_location().unwrap());
        document.jumplist.cursor = 1;
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(NavigateBack), cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 1);
    });
    reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn traversal_of_a_long_document_keeps_window_histories_independent(cx: &mut TestAppContext) {
    setup(cx);
    let path = PathBuf::from("examples/readme-sample.pdf");
    let (window, document) = open(cx, Some(path.clone()));
    let (_, independent) = open(cx, Some(path));
    cx.run_until_parked();
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let origin = reader.read_with(cx, |reader, _| reader.jump_location().unwrap());
    reader.update(cx, |reader, cx| reader.select_page(99, cx));
    let destination = reader.read_with(cx, |reader, _| reader.jump_location().unwrap());
    reader.update(cx, |_, cx| {
        cx.emit(JumpEvent {
            from: origin.clone(),
            to: destination.clone(),
        });
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.dispatch_action(Box::new(NavigateBack), cx);
        frame.dispatch_action(Box::new(NavigateForward), cx);
    })
    .unwrap();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 99);
        assert!(reader.jump_location().unwrap().equivalent(&destination));
    });
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2)
    });
    independent.read_with(cx, |document, _| {
        assert!(document.jumplist.entries.is_empty())
    });
}

#[gpui_kit::test]
fn back_works_after_a_contents_jump_while_sidebar_keeps_focus(cx: &mut TestAppContext) {
    setup(cx);
    let (window, document) = open(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-t", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click(("contents-entry", 0usize), cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2)
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("ctrl-o", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| assert_eq!(document.jumplist.cursor, 0));
    cx.update_window(window, |_, frame, cx| frame.press("ctrl-i", cx))
        .unwrap();
    document.read_with(cx, |document, _| assert_eq!(document.jumplist.cursor, 1));
}

#[gpui_kit::test]
fn forward_shortcut_does_not_steal_find_input_focus(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("ctrl-o", cx);
        frame.press("cmd-f", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, _| assert_eq!(document.jumplist.cursor, 0));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("ctrl-i", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| assert_eq!(document.jumplist.cursor, 0));
}

#[gpui_kit::test]
fn palette_traversal_tracks_boundaries_and_dispatches_window_actions(cx: &mut TestAppContext) {
    let (_root, window, document) = linked_window(cx);
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-shift-p", cx);
        frame.render_frame(cx);
        frame.input("Back", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    document.read_with(cx, |document, cx| {
        assert!(document.palette_open());
        assert_eq!(document.jumplist.cursor, 0);
        assert!(!document.palette.as_ref().unwrap().has_selection(cx));
    });
    cx.update_window(window, |_, frame, cx| {
        frame.press("escape", cx);
        frame.press("escape", cx);
    })
    .unwrap();
    assert!(!document.read_with(cx, |document, _| document.palette_open()));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.entries.len(), 2)
    });
    cx.update_window(window, |_, frame, cx| {
        frame.press("cmd-shift-p", cx);
        frame.render_frame(cx);
        frame.input("Back", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.cursor, 0);
        assert_eq!(document.jumplist.entries.len(), 2);
        assert!(!document.palette_open());
    });
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("cmd-shift-p", cx);
        frame.render_frame(cx);
        frame.input("Forward", cx);
    })
    .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, cx| {
        assert!(document.jumplist.can_forward());
        assert_eq!(document.palette_query(cx).as_deref(), Some("Forward"));
        assert!(document.palette.as_ref().unwrap().has_selection(cx));
    });
    cx.update_window(window, |_, frame, cx| frame.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    document.read_with(cx, |document, _| {
        assert_eq!(document.jumplist.cursor, 1);
        assert!(!document.palette_open());
        assert!(!document.jumplist.can_forward());
    });
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn native_navigate_menu_follows_active_window_and_find_focus(cx: &mut TestAppContext) {
    use gpui_kit::OwnedMenuItem;
    fn navigation(cx: &TestAppContext) -> (bool, bool) {
        cx.update(|cx| {
            let menus = cx.get_menus().unwrap();
            let navigate = menus.iter().find(|menu| menu.name == "Navigate").unwrap();
            let disabled = |name| {
                let OwnedMenuItem::Action { disabled, action, .. } = navigate
                    .items
                    .iter()
                    .find(|item| matches!(item, OwnedMenuItem::Action { name: label, .. } if label == name))
                    .unwrap()
                else { unreachable!() };
                assert_eq!(action.name(), format!("kver::Navigate{name}"));
                *disabled
            };
            (disabled("Back"), disabled("Forward"))
        })
    }
    let (_root, window, document) = linked_window(cx);
    cx.update(crate::app::menu::install);
    cx.update_window(window, |_, frame, _| frame.activate_window())
        .unwrap();
    assert_eq!(navigation(cx), (true, true));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.click("pdf-link-0-0", cx);
    })
    .unwrap();
    assert_eq!(navigation(cx), (false, true));
    cx.update_window(window, |_, frame, cx| frame.press("ctrl-o", cx))
        .unwrap();
    assert_eq!(navigation(cx), (true, false));
    cx.update_window(window, |_, frame, cx| frame.press("cmd-f", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        assert_eq!(frame.find("find-query").focused(), Some(true));
        frame.press("ctrl-i", cx);
        assert_eq!(frame.find("find-query").focused(), Some(true));
    })
    .unwrap();
    assert_eq!(navigation(cx), (true, true));
    document.read_with(cx, |document, _| assert_eq!(document.jumplist.cursor, 0));
    cx.update_window(window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.press("escape", cx);
    })
    .unwrap();
    assert_eq!(navigation(cx), (true, false));
    let (other_window, other) = open(cx, None);
    cx.update_window(other_window, |_, frame, cx| {
        frame.render_frame(cx);
        frame.activate_window();
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.active_window()), Some(other_window));
    assert_eq!(navigation(cx), (true, true));
    cx.update_window(window, |_, frame, _| frame.activate_window())
        .unwrap();
    cx.run_until_parked();
    assert_eq!(navigation(cx), (true, false));
    other.read_with(cx, |document, _| {
        assert!(document.jumplist.entries.is_empty())
    });
}
