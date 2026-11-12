use super::*;
use gpui_kit::{AnyWindowHandle, TestAppContext, test::TestWindowExt};

mod annotations;
mod chrome;
mod contents;
mod find;
mod jumplist;
mod links;
mod palette;
mod recent_history;
mod recent_picker;
mod saving;
mod sidebar;

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::keymap::install(cx);
        settings_ui::install_with_paths(cx, None);
    });
}

fn open(
    cx: &mut TestAppContext,
    path: Option<PathBuf>,
) -> (AnyWindowHandle, Entity<DocumentWindow>) {
    cx.update(|cx| {
        gpui_kit::open_window(Default::default(), cx, |window, cx| {
            cx.new(|cx| DocumentWindow::new(path, window, cx))
        })
        .unwrap()
    })
}

fn reader_window(
    cx: &mut TestAppContext,
    path: Option<PathBuf>,
) -> (AnyWindowHandle, Entity<ReaderView>) {
    let (window, document) = open(cx, path);
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    (window, reader)
}

#[gpui_kit::test]
fn picker_cancellation_and_obsolete_completion_preserve_the_current_document(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (_, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    document.update(cx, |document, cx| {
        document.finish_file_choice(generation, None, cx);
    });
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.path(),
            Some(std::path::Path::new("examples/readme-sample.pdf"))
        );
        assert_eq!(reader.page_count(), Some(100));
    });

    // A Recent open can replace the document through the child while a native
    // picker is still outstanding. Both entry points use the same generation.
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/missing.pdf"), cx)
    });
    document.update(cx, |document, cx| {
        document.finish_file_choice(
            generation,
            Some(PathBuf::from("examples/readme-sample.pdf")),
            cx,
        );
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.path(),
            Some(std::path::Path::new("examples/missing.pdf"))
        );
        assert_eq!(reader.page_count(), None);
    });
}

#[gpui_kit::test]
fn picker_completion_survives_zoom_and_uses_the_existing_load_path(cx: &mut TestAppContext) {
    setup(cx);
    let (_, document) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    cx.run_until_parked();
    let reader = document.read_with(cx, |document, _| document.reader().clone());
    let generation = reader.read_with(cx, |reader, _| reader.document_generation());
    reader.update(cx, |reader, cx| {
        reader.next_page(cx);
        reader.zoom_in(cx);
    });
    document.update(cx, |document, cx| {
        document.finish_file_choice(
            generation,
            Some(PathBuf::from("examples/readme-sample.pdf")),
            cx,
        );
    });
    cx.run_until_parked();
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.document_generation(), generation + 1);
        assert_eq!(reader.page_count(), Some(100));
        assert_eq!(reader.page_index(), 0);
        assert_eq!(reader.scale(), 1.0);
    });
}

#[gpui_kit::test]
fn window_commands_bubble_from_sidebar_and_palette_without_moving_the_other_reader(
    cx: &mut TestAppContext,
) {
    setup(cx);
    let (first_window, first) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    let (second_window, second) = open(cx, Some(PathBuf::from("examples/readme-sample.pdf")));
    let first_reader = first.read_with(cx, |document, _| document.reader().clone());
    let second_reader = second.read_with(cx, |document, _| document.reader().clone());
    cx.run_until_parked();
    cx.update_window(first_window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.press("right", cx);
        window.press("cmd-f", cx);
        window.render_frame(cx);
        window.input("next", cx);
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-`"
            } else {
                "ctrl-f6"
            },
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.active_window()), Some(second_window));
    first_reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 1));
    second_reader.read_with(cx, |reader, _| assert_eq!(reader.page_index(), 0));

    cx.update_window(second_window, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-shift-p", cx);
        window.render_frame(cx);
        window.input("Previous Window", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second_window, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.active_window()), Some(first_window));
    let weak_reader = first_reader.downgrade();
    let weak_document = first.downgrade();
    drop(first_reader);
    drop(first);
    cx.update_window(first_window, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-w", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows()), vec![second_window]);
    assert!(weak_document.upgrade().is_none());
    assert!(weak_reader.upgrade().is_none());
}
