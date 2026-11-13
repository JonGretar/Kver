use super::*;
use crate::coordinates::Point;
use gpui_kit::TestAppContext;

pub(super) fn region(top: f64) -> Vec<Quad> {
    vec![
        Quad::new([
            Point { x: 48., y: top },
            Point { x: 170., y: top },
            Point {
                x: 48.,
                y: top - 14.,
            },
            Point {
                x: 170.,
                y: top - 14.,
            },
        ])
        .unwrap(),
    ]
}

#[gpui_kit::test]
fn mutation_during_catalog_loading_is_rejected_without_losing_or_inventing_edits(
    cx: &mut TestAppContext,
) {
    let path = std::fs::canonicalize("examples/selection-sample.pdf").unwrap();
    let session = cx.update(|cx| DocumentSession::join(path, cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| {
        session.ensure_annotations(cx);
        assert!(session.create_highlight(0, region(734.), cx).is_err());
        assert!(!session.is_dirty());
    });
    cx.run_until_parked();
    session.update(cx, |session, cx| {
        assert!(session.annotations().unwrap().is_ok());
        session.create_highlight(0, region(734.), cx).unwrap();
        assert!(session.is_dirty());
        assert_eq!(session.unsaved_annotations().count(), 1);
    });
}

#[gpui_kit::test]
fn edits_during_save_remain_dirty_and_next_save_does_not_duplicate_saved_annotations(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let source = root.join("source.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let first = root.join("first.pdf");
    let second = root.join("second.pdf");
    let session = cx.update(|cx| DocumentSession::join(source.clone(), cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| session.ensure_annotations(cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| {
        session.create_highlight(0, region(734.), cx).unwrap();
        session
            .start_save_as(Arc::new(Destination::inspect(first.clone()).unwrap()), cx)
            .unwrap();
        session.create_highlight(0, region(714.), cx).unwrap();
    });
    cx.run_until_parked();
    session.read_with(cx, |session, _| {
        assert_eq!(session.path(), first);
        assert!(session.is_dirty());
        assert_eq!(session.unsaved_annotations().count(), 1);
        assert!(session.save_outcome().unwrap().1.is_ok());
    });
    assert_eq!(
        pdf::Document::open(&first)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
    session.update(cx, |session, cx| {
        session
            .start_save_as(Arc::new(Destination::inspect(second.clone()).unwrap()), cx)
            .unwrap();
    });
    cx.run_until_parked();
    session.read_with(cx, |session, _| {
        assert!(!session.is_dirty());
        assert_eq!(session.unsaved_annotations().count(), 0);
    });
    assert_eq!(
        pdf::Document::open(&second)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        2
    );
    assert!(
        std::fs::read(second)
            .unwrap()
            .starts_with(&std::fs::read(first).unwrap())
    );
}

#[gpui_kit::test]
fn save_as_refuses_an_open_destination_even_when_that_session_is_clean(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let source = root.join("source.pdf");
    let target = root.join("target.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    std::fs::copy("examples/selection-sample.pdf", &target).unwrap();
    let original = std::fs::read(&target).unwrap();
    let (session, target_session) = cx.update(|cx| {
        (
            DocumentSession::join(source.clone(), cx),
            DocumentSession::join(target.clone(), cx),
        )
    });
    cx.run_until_parked();
    session.update(cx, |session, cx| session.ensure_annotations(cx));
    cx.run_until_parked();
    assert!(!target_session.read_with(cx, |session, _| session.is_dirty()));
    session.update(cx, |session, cx| {
        session.create_highlight(0, region(734.), cx).unwrap();
        assert!(
            session
                .start_save_as(Arc::new(Destination::inspect(target.clone()).unwrap()), cx)
                .is_err()
        );
        assert_eq!(session.path(), source);
        assert!(session.is_dirty());
    });
    assert_eq!(std::fs::read(target).unwrap(), original);
}

#[gpui_kit::test]
fn failed_background_commit_keeps_baseline_path_edits_and_retry_available(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(directory.path()).unwrap();
    let source = root.join("source.pdf");
    std::fs::copy("examples/selection-sample.pdf", &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let destination_directory = root.join("destination");
    std::fs::create_dir(&destination_directory).unwrap();
    let destination =
        Arc::new(Destination::inspect(destination_directory.join("copy.pdf")).unwrap());
    std::fs::remove_dir(destination_directory).unwrap();
    let session = cx.update(|cx| DocumentSession::join(source.clone(), cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| session.ensure_annotations(cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| {
        session.create_highlight(0, region(734.), cx).unwrap();
        session.start_save_as(destination, cx).unwrap();
    });
    cx.run_until_parked();
    session.read_with(cx, |session, _| {
        assert_eq!(session.path(), source);
        assert!(session.is_dirty());
        assert!(!session.is_saving());
        assert!(session.save_outcome().unwrap().1.is_err());
        assert_eq!(session.unsaved_annotations().count(), 1);
    });
    assert_eq!(std::fs::read(source).unwrap(), original);
    let retry = root.join("retry.pdf");
    session.update(cx, |session, cx| {
        session
            .start_save_as(Arc::new(Destination::inspect(retry.clone()).unwrap()), cx)
            .unwrap();
    });
    cx.run_until_parked();
    session.read_with(cx, |session, _| {
        assert!(!session.is_dirty());
        assert_eq!(session.path(), retry);
    });
    assert_eq!(
        pdf::Document::open(&retry)
            .unwrap()
            .page_annotations(0, 10)
            .unwrap()
            .len(),
        1
    );
}

#[gpui_kit::test]
fn identical_imported_highlight_keeps_identity_color_metadata_and_clean_baseline(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("data"),
    );
    std::fs::write(
        paths.config(),
        "[application]\nannotation_author = 'Do not claim imported work'\n",
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::settings_ui::install_with_paths(cx, Some(paths));
    });
    cx.run_until_parked();
    let path = std::fs::canonicalize("examples/annotations.pdf").unwrap();
    let session = cx.update(|cx| DocumentSession::join(path, cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| session.ensure_annotations(cx));
    cx.run_until_parked();
    let imported = session.read_with(cx, |session, _| {
        session
            .annotations()
            .unwrap()
            .unwrap()
            .iter()
            .find(|annotation| {
                annotation.kind() == MarkupKind::Highlight && !annotation.is_readonly()
            })
            .unwrap()
            .clone()
    });
    session.update(cx, |session, cx| {
        let id = session
            .create_highlight(imported.page(), imported.quads().to_vec(), cx)
            .unwrap();
        assert_eq!(id, imported.id());
        assert_eq!(session.revision(), 0);
        assert!(!session.is_dirty());
        let catalog = session.annotations().unwrap().unwrap();
        let existing = catalog
            .iter()
            .find(|annotation| annotation.id() == id)
            .unwrap();
        assert_eq!(existing.color(), imported.color());
        assert_eq!(existing.opacity(), imported.opacity());
        assert_eq!(existing.author(), imported.author());
        assert_eq!(existing.comment(), imported.comment());
        assert_eq!(existing.name(), imported.name());
        assert_eq!(existing.quads(), imported.quads());
    });
}
