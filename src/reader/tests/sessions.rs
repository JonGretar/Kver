//! Shared baseline contracts exercised through production document windows.

use super::*;

fn frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

fn drag_title(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    reader: &Entity<ReaderView>,
    glyphs: std::ops::Range<usize>,
) {
    let text = pdf::page_text(Path::new("examples/readme-sample.pdf"), 0).unwrap();
    let (start, end) = reader.read_with(cx, |reader, _| {
        let viewport = reader.page_viewport(0).unwrap();
        let at = |ix: usize, fraction: f64| {
            let glyph = &text.glyphs[ix];
            let at = viewport.document_to_screen(Point {
                x: glyph.start.x + (glyph.end.x - glyph.start.x) * fraction,
                y: glyph.start.y + (glyph.top - glyph.start.y) * 0.4,
            });
            point(px(at.x as f32), px(at.y as f32))
        };
        (at(glyphs.start, 0.1), at(glyphs.end - 1, 0.9))
    });
    cx.update_window(window, |_, window, cx| {
        window.drag(start, end, cx);
        window.press("cmd-c", cx);
    })
    .unwrap();
}

fn clipboard(cx: &mut TestAppContext) -> String {
    cx.update(|cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap()
    })
}

#[gpui_kit::test]
fn repeated_opens_keep_navigation_selection_mode_zoom_and_focus_window_local(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    // Both normal opens start before either background load completes.
    let path = PathBuf::from("examples/readme-sample.pdf");
    let (first_window, first) = recent_test_window(cx, Some(path.clone()));
    let (second_window, second) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    frame(cx, first_window);
    frame(cx, second_window);

    drag_title(cx, first_window, &first, 0..4);
    assert_eq!(clipboard(cx), "KVER");
    second.read_with(cx, |reader, _| assert_eq!(reader.selected_text(), None));
    drag_title(cx, second_window, &second, 7..13);
    assert_eq!(clipboard(cx), "READER");
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_text().as_deref(), Some("KVER"))
    });

    cx.update_window(first_window, |_, window, cx| {
        window.press("cmd-f", cx);
        window.input("This is page", cx);
    })
    .unwrap();
    cx.run_until_parked();
    // Focusing and navigating in the other window cannot steal Find's focus
    // or query, and its own selection remains available for copy.
    cx.update_window(second_window, |_, window, cx| {
        window.press("cmd-c", cx);
        window.dispatch_action(Box::new(SinglePage), cx);
        window.press("right", cx);
        window.press("cmd-=", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(clipboard(cx), "READER");
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.scale(), 1.0);
        assert_eq!(reader.reading_mode(), ReadingMode::Continuous);
        assert_eq!(reader.selected_text().as_deref(), Some("KVER"));
    });
    second.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.scale(), 1.25);
        assert_eq!(reader.reading_mode(), ReadingMode::SinglePage);
    });
    cx.update_window(first_window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("find-query").focused(), Some(true));
        assert_eq!(window.find("find-query").value(), Some("This is page"));
        window.input(" 1", cx);
        assert_eq!(window.find("find-query").value(), Some("This is page 1"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn repeated_opens_use_one_baseline_until_the_last_window_closes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.pdf");
    let other = root.path().join("other.pdf");
    std::fs::copy("examples/readme-sample.pdf", &path).unwrap();
    let (first_window, first) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    frame(cx, first_window);

    // A later open must join the existing version, not mix fresh disk bytes
    // into its metadata, text, outline, links, or raster requests.
    std::fs::copy("examples/field-guide.pdf", &path).unwrap();
    std::fs::copy(&path, &other).unwrap();
    let alias = root.path().join(".").join("shared.pdf");
    let (second_window, second) = recent_test_window(cx, Some(alias));
    let (other_window, independent) = recent_test_window(cx, Some(other));
    cx.run_until_parked();
    frame(cx, second_window);
    drag_title(cx, second_window, &second, 0..4);
    assert_eq!(clipboard(cx), "KVER");
    let fresh_pages = pdf::document_pages(&path).unwrap().len();
    assert_ne!(fresh_pages, 100);
    independent.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(fresh_pages))
    });
    cx.update_window(second_window, |_, window, cx| {
        window.press("cmd-t", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second_window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("contents-empty").visible());
        window.press("escape", cx);
        window.press("cmd-f", cx);
        window.input("This is page 100 of 100", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    second.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(100));
        assert_eq!(reader.page_index(), 99);
        assert_eq!(reader.displayed_page_index(), Some(99));
    });
    first.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));

    // Close with newly requested raster/text/link work still outstanding.
    first.update(cx, |reader, cx| {
        reader.next_page(cx);
        reader.zoom_in(cx);
    });
    let closed = first.downgrade();
    drop(first);
    cx.simulate_keystrokes(first_window, "cmd-w");
    cx.run_until_parked();
    assert!(closed.upgrade().is_none());
    second.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 99));
    let closed = second.downgrade();
    drop(second);
    cx.simulate_keystrokes(second_window, "cmd-w");
    cx.run_until_parked();
    assert!(closed.upgrade().is_none());

    // The registry and completed sidebar/background work do not own membership.
    let (_, reopened) = recent_test_window(cx, Some(path));
    cx.run_until_parked();
    reopened.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(fresh_pages))
    });
    frame(cx, other_window);
    independent.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn replacement_detaches_only_its_window_and_rejects_pending_old_work(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/readme-sample.pdf");
    let (first_window, first) = recent_test_window(cx, Some(path.clone()));
    let (second_window, second) = recent_test_window(cx, Some(path.clone()));
    // Replace while the initial open is pending in both windows.
    first.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/field-guide.pdf"), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        second.read_with(cx, |reader, _| reader.page_count()),
        Some(100)
    );
    assert_eq!(
        first.read_with(cx, |reader, _| reader.page_count()),
        Some(24)
    );
    frame(cx, first_window);
    frame(cx, second_window);
    drag_title(cx, second_window, &second, 0..4);
    // Start new raster/text/link work and Find/Contents extraction, then replace
    // without draining the executor. Late replies must not restore old state.
    cx.update_window(first_window, |_, window, cx| {
        window.press("cmd-t", cx);
        window.press("cmd-f", cx);
        window.input("Landscape", cx);
    })
    .unwrap();
    first.update(cx, |reader, cx| {
        reader.next_page(cx);
        reader.zoom_in(cx);
        reader.open_path(path, cx);
    });
    cx.run_until_parked();
    frame(cx, first_window);
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(100));
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.scale(), 1.0);
        assert_eq!(reader.selected_text(), None);
        assert_eq!(reader.displayed_page_index(), Some(0));
    });
    second.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_text().as_deref(), Some("KVER"))
    });
    cx.update_window(first_window, |_, window, cx| {
        window.press("cmd-f", cx);
        assert_eq!(window.find("find-query").value(), Some(""));
        window.input("This is page 2", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    first.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    second.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));
}

#[gpui_kit::test]
fn closing_a_loading_participant_does_not_retain_it_or_cancel_the_other_open(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let path = PathBuf::from("examples/readme-sample.pdf");
    let (closed_window, closed) = recent_test_window(cx, Some(path.clone()));
    let (window, reader) = recent_test_window(cx, Some(path));
    let weak = closed.downgrade();
    drop(closed);
    cx.update_window(closed_window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    frame(cx, window);
    cx.simulate_keystrokes(window, "right");
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), Some(100));
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.displayed_page_index(), Some(1));
    });
}

#[gpui_kit::test]
fn a_failed_open_does_not_poison_later_opens_of_a_repaired_file(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("repaired.pdf");
    std::fs::write(&path, b"not a PDF").unwrap();
    let (_, failed) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    failed.read_with(cx, |reader, _| {
        assert_eq!(reader.page_count(), None);
        assert!(reader.status().contains("Cannot open PDF"));
    });

    std::fs::copy("examples/readme-sample.pdf", &path).unwrap();
    let (window, opened) = recent_test_window(cx, Some(path.clone()));
    cx.run_until_parked();
    frame(cx, window);
    drag_title(cx, window, &opened, 0..4);
    assert_eq!(clipboard(cx), "KVER");
    failed.read_with(cx, |reader, _| assert_eq!(reader.page_count(), None));
    failed.update(cx, |reader, cx| reader.open_path(path, cx));
    cx.run_until_parked();
    failed.read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(100)));
    opened.read_with(cx, |reader, _| {
        assert_eq!(reader.selected_text().as_deref(), Some("KVER"))
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn symlinks_join_the_baseline_but_distinct_hard_link_paths_do_not(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
    });
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    let symlink = root.path().join("alias.pdf");
    let hardlink = root.path().join("hardlink.pdf");
    std::fs::write(&source, pdf::linked_fixture()).unwrap();
    std::os::unix::fs::symlink(&source, &symlink).unwrap();
    std::fs::hard_link(&source, &hardlink).unwrap();
    let (first_window, first) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();

    // Mutate the same inode: the hard link now contains the new bytes, while
    // the canonical source session must keep its original baseline.
    std::fs::write(
        &source,
        std::fs::read("examples/readme-sample.pdf").unwrap(),
    )
    .unwrap();
    let (alias_window, alias) = recent_test_window(cx, Some(symlink));
    let (_, independent) = recent_test_window(cx, Some(hardlink));
    cx.run_until_parked();
    frame(cx, alias_window);
    alias.read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(2)));
    independent.read_with(cx, |reader, _| assert_eq!(reader.page_count(), Some(100)));
    // Link extraction also comes from the retained baseline, not the rewritten
    // file; its normal pointer action still uses window-local navigation.
    cx.update_window(alias_window, |_, window, cx| {
        window.click("pdf-link-0-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    alias.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 1);
        assert_eq!(reader.scale(), 1.5);
    });
    frame(cx, first_window);
    first.read_with(cx, |reader, _| {
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.scale(), 1.0);
    });
}
