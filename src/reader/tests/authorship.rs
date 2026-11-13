//! Authorship and history privacy through production Settings, reader and save actions.
use super::authoring::{draw, select_glyphs, select_title};
use super::*;
use crate::app::document_window::{Save, SaveAs};
use crate::settings::AppPaths;
use std::time::Duration;

#[gpui_kit::test]
fn new_annotations_capture_live_author_and_history_clear_keeps_saved_and_unsaved_markup(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
    let source = root.path().join("source.pdf");
    let explicit = root.path().join("explicit.pdf");
    let output = root.path().join("saved.pdf");
    for path in [&source, &explicit] {
        std::fs::copy("examples/selection-sample.pdf", path).unwrap();
    }
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        crate::recent::install(cx, Some(paths.clone()));
        crate::settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (first, blank_reader) = recent_test_window(cx, Some(source.clone()));
    cx.run_until_parked();
    select_title(cx, first, &blank_reader, &source);
    cx.update_window(first, |_, window, cx| {
        window.click("highlight-selection", cx)
    })
    .unwrap();
    cx.run_until_parked();
    blank_reader.read_with(cx, |reader, cx| {
        let session = reader.session().unwrap();
        assert_eq!(
            session.read(cx).annotations().unwrap().unwrap()[0].author(),
            None
        );
    });

    let (second, explicit_reader) = recent_test_window(cx, Some(explicit.clone()));
    cx.run_until_parked();
    // A Settings edit applies to an already open session, not its existing annotation.
    cx.update(|cx| cx.dispatch_action(&ShowSettings));
    cx.run_until_parked();
    let settings = cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find(|handle| *handle != first && *handle != second)
            .unwrap()
    });
    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.click("annotation-author", cx);
        window.input("Jón 中文", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(settings, |_, window, cx| {
        window.within("settings-sidebar").click("0-2", cx);
        window.click("remember-reading-history", cx);
    })
    .unwrap();
    cx.run_until_parked();
    select_title(cx, second, &explicit_reader, &explicit);
    cx.update_window(second, |_, window, cx| {
        window.click("highlight-selection", cx)
    })
    .unwrap();
    cx.run_until_parked();
    for (reader, expected) in [(&blank_reader, None), (&explicit_reader, Some("Jón 中文"))] {
        reader.read_with(cx, |reader, cx| {
            assert!(reader.is_dirty(cx));
            assert_eq!(
                reader
                    .session()
                    .unwrap()
                    .read(cx)
                    .annotations()
                    .unwrap()
                    .unwrap()[0]
                    .author(),
                expected
            );
        });
    }
    for (author, range) in [("Second author", 6..=10), ("   ", 12..=16)] {
        std::fs::write(
            paths.config(),
            format!(
                "[application]\nannotation_author = '{author}'\n[persistence]\nremember = false\n"
            ),
        )
        .unwrap();
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        select_glyphs(cx, second, &explicit_reader, &explicit, range);
        cx.update_window(second, |_, window, cx| {
            window.click("highlight-selection", cx)
        })
        .unwrap();
        cx.run_until_parked();
    }
    // List labels use each annotation's captured author, never the current setting.
    for (handle, reader) in [(first, &blank_reader), (second, &explicit_reader)] {
        cx.update_window(handle, |_, window, cx| {
            window.dispatch_action(Box::new(ShowAnnotations), cx);
        })
        .unwrap();
        cx.run_until_parked();
        let annotations = reader.read_with(cx, |reader, cx| {
            let session = reader.session().unwrap();
            session.read(cx).annotations().unwrap().unwrap().to_vec()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            for annotation in annotations {
                let target = window.find(format!(
                    "annotation-{}-{}",
                    annotation.id().epoch(),
                    annotation.id().serial()
                ));
                match annotation.author() {
                    Some(author) => {
                        assert!(target.label().unwrap().ends_with(&format!("; By {author}")))
                    }
                    None => assert!(!target.label().unwrap().contains("; By ")),
                }
            }
        })
        .unwrap();
    }
    // Reapplying after changing identity must select the original without claiming it.
    cx.update_window(first, |_, window, cx| {
        window.click("highlight-selection", cx);
        window.dispatch_action(Box::new(SaveAs), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    let saved_bytes = std::fs::read(&output).unwrap();
    assert_eq!(
        pdf::Document::open(&output)
            .unwrap()
            .page_annotations(0, 50)
            .unwrap()[0]
            .author(),
        None
    );

    cx.update_window(settings, |_, window, cx| {
        window.render_frame(cx);
        window.within("settings-sidebar").click("0-2", cx);
        assert_eq!(
            window.find("remember-reading-history").checked(),
            Some(false)
        );
        window.scroll(
            "remember-reading-history",
            gpui::ScrollDelta::Pixels(point(px(0.), px(-800.))),
            cx,
        );
        window.click("clear-reading-history", cx);
        window.click("ok", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(RecentStore::new(&paths).search("").unwrap().is_empty());
    assert_eq!(std::fs::read(&output).unwrap(), saved_bytes);
    explicit_reader.read_with(cx, |reader, cx| {
        assert!(reader.is_dirty(cx), "history clear discarded unsaved edits");
        assert_eq!(
            reader
                .session()
                .unwrap()
                .read(cx)
                .annotations()
                .unwrap()
                .unwrap()[0]
                .author(),
            Some("Jón 中文")
        );
        let session = reader.session().unwrap();
        let annotations = session.read(cx).annotations().unwrap().unwrap();
        assert_eq!(annotations.len(), 3);
        let mut authors: Vec<_> = annotations
            .iter()
            .map(|annotation| annotation.author())
            .collect();
        authors.sort();
        assert_eq!(
            authors,
            [None, Some("Jón 中文"), Some("Second author")],
            "whitespace is blank and previous authors stay unchanged"
        );
    });
    // Both current-file Save and Save As remain available with recording off.
    draw(cx, second);
    cx.update_window(second, |_, window, cx| {
        window.dispatch_action(Box::new(Save), cx)
    })
    .unwrap();
    cx.run_until_parked();
    let saved = pdf::Document::open(&explicit)
        .unwrap()
        .page_annotations(0, 51)
        .unwrap();
    assert_eq!(saved.len(), 3);
    let mut authors: Vec<_> = saved.iter().map(|annotation| annotation.author()).collect();
    authors.sort();
    assert_eq!(authors, [None, Some("Jón 中文"), Some("Second author")]);
    explicit_reader.read_with(cx, |reader, cx| assert!(!reader.is_dirty(cx)));
}
