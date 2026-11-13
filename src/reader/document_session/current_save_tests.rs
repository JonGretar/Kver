use super::save_tests::region;
use super::*;
use gpui_kit::TestAppContext;

fn load(cx: &mut TestAppContext, source: &std::path::Path) -> Entity<DocumentSession> {
    let session = cx.update(|cx| DocumentSession::join(source.to_path_buf(), cx));
    cx.run_until_parked();
    session.update(cx, |session, cx| session.ensure_annotations(cx));
    cx.run_until_parked();
    session
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui_kit::test]
fn save_updates_the_baseline_without_changing_session_identity_and_keeps_newer_edits_dirty(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().canonicalize().unwrap().join("current.pdf");
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    std::fs::write(&source, &original).unwrap();
    let session = load(cx, &source);
    let epoch = session.read_with(cx, |session, _| session.epoch());
    cx.update(|cx| {
        session.update(cx, |session, cx| {
            session.create_highlight(0, region(734.), cx).unwrap();
            session.start_save(cx).unwrap();
            session.create_highlight(0, region(714.), cx).unwrap();
        });
        assert_eq!(
            DocumentSession::join(source.clone(), cx),
            session,
            "repeated opens still join during Save"
        );
    });
    cx.run_until_parked();
    let first = std::fs::read(&source).unwrap();
    assert!(first.starts_with(&original));
    assert_eq!(
        pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        1
    );
    session.read_with(cx, |session, _| {
        assert_eq!(session.path(), source);
        assert_eq!(session.epoch(), epoch);
        assert!(session.is_dirty());
        assert_eq!(session.unsaved_annotations().count(), 1);
        assert!(session.save_outcome().unwrap().1.is_ok());
    });
    session.update(cx, |session, cx| session.start_save(cx).unwrap());
    cx.run_until_parked();
    // Each coherent edit snapshot is encoded against the session's immutable
    // editing source, not accumulated against previous preview/save increments.
    assert!(std::fs::read(&source).unwrap().starts_with(&original));
    assert_eq!(
        pdf::Document::open(&source)
            .unwrap()
            .page_annotations(0, 100)
            .unwrap()
            .len(),
        2
    );
    session.read_with(cx, |session, _| {
        assert!(!session.is_dirty());
        assert_eq!(session.unsaved_annotations().count(), 0);
    });
}

#[gpui_kit::test]
fn save_refuses_both_rewritten_bytes_and_same_bytes_on_a_replaced_inode(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    for replace in [false, true] {
        let source = root.join(format!("current-{replace}.pdf"));
        std::fs::write(&source, &original).unwrap();
        let session = load(cx, &source);
        session.update(cx, |session, cx| {
            session.create_highlight(0, region(734.), cx).unwrap()
        });
        let external = if replace {
            let replacement = root.join("replacement.pdf");
            std::fs::write(&replacement, &original).unwrap();
            std::fs::rename(replacement, &source).unwrap();
            original.clone()
        } else {
            let external = [original.as_slice(), b"\n% external application change\n"].concat();
            std::fs::write(&source, &external).unwrap();
            external
        };
        session.update(cx, |session, cx| session.start_save(cx).unwrap());
        cx.run_until_parked();
        session.read_with(cx, |session, _| {
            assert!(session.is_dirty());
            assert!(matches!(
                session.save_outcome().unwrap().1,
                Err(SaveError::Conflict(_))
            ));
            assert_eq!(session.result().unwrap().unwrap().bytes(), original);
        });
        assert_eq!(std::fs::read(&source).unwrap(), external);
        let copy = root.join(format!("recovered-{replace}.pdf"));
        session.update(cx, |session, cx| {
            session
                .start_save_as(Arc::new(Destination::inspect(copy.clone()).unwrap()), cx)
                .unwrap();
        });
        cx.run_until_parked();
        assert!(std::fs::read(copy).unwrap().starts_with(&original));
        assert_eq!(std::fs::read(source).unwrap(), external);
    }
}

#[cfg(unix)]
#[gpui_kit::test]
fn readonly_save_keeps_edits_for_save_as_without_changing_the_original(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let source = root.join("readonly.pdf");
    let original = std::fs::read("examples/selection-sample.pdf").unwrap();
    std::fs::write(&source, &original).unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o444)).unwrap();
    let session = load(cx, &source);
    session.update(cx, |session, cx| {
        session.create_highlight(0, region(734.), cx).unwrap();
        session.start_save(cx).unwrap();
    });
    cx.run_until_parked();
    session.read_with(cx, |session, _| {
        assert!(session.is_dirty());
        assert!(matches!(
            session.save_outcome().unwrap().1,
            Err(SaveError::Readonly(_))
        ));
    });
    assert_eq!(std::fs::read(&source).unwrap(), original);
}
