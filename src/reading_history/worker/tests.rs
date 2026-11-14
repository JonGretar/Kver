use super::*;
use std::fs;

#[gpui_kit::test]
fn ruler_worker_orders_movements_not_reply_polling_and_obeys_privacy_epochs(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (root, store) = store();
    let history = install_history(cx, root.path());
    let path = root.path().join("ruler.pdf");
    complete(cx, history.set_recording(false)).unwrap();
    complete(cx, history.record(path.clone())).unwrap();
    let epoch = history.epoch();
    let first = RulerAnchor {
        page: 0,
        x: 20.,
        y: 30.,
    };
    let second = RulerAnchor { x: 320., ..first };
    let older = history.save_ruler_anchor(path.clone(), first, epoch);
    let newer = history.save_ruler_anchor(path.clone(), second, epoch);
    complete(cx, newer).unwrap();
    complete(cx, older).unwrap();
    assert_eq!(
        complete(cx, history.ruler_anchor(path.clone(), epoch)).unwrap(),
        Some(second)
    );
    // Closing a window drops its waiter, not the synchronously queued movement.
    drop(history.save_ruler_anchor(path.clone(), first, epoch));
    assert_eq!(
        complete(cx, history.ruler_anchor(path.clone(), epoch)).unwrap(),
        Some(first)
    );
    drop(history.save_ruler_anchor(path.clone(), second, epoch));
    assert!(!root.path().join("local/reading.sqlite3").exists());
    complete(cx, history.set_recording(true)).unwrap();
    assert_eq!(store.ruler_anchor(&path).unwrap(), Some(second));
    // Reads and late writes from pre-clear windows cannot resurrect saved data,
    // even after a different window explicitly reopens the same PDF.
    complete(cx, history.clear()).unwrap();
    complete(cx, history.record(path.clone())).unwrap();
    complete(cx, history.save_ruler_anchor(path.clone(), first, epoch)).unwrap();
    assert_eq!(
        complete(cx, history.ruler_anchor(path.clone(), epoch)).unwrap(),
        None
    );
    assert_eq!(store.ruler_anchor(&path).unwrap(), None);
    complete(
        cx,
        history.save_ruler_anchor(path.clone(), second, history.epoch()),
    )
    .unwrap();
    complete(cx, history.remove(path.clone())).unwrap();
    assert_eq!(
        complete(cx, history.ruler_anchor(path.clone(), history.epoch())).unwrap(),
        None
    );
    assert_eq!(store.ruler_anchor(&path).unwrap(), None);
}

fn store() -> (tempfile::TempDir, RecentStore) {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("local"));
    (root, RecentStore::new(&paths))
}

#[gpui_kit::test]
fn pending_open_from_before_clear_does_not_recreate_deleted_data(
    cx: &mut gpui_kit::TestAppContext,
) {
    use std::sync::{Arc, Mutex};
    let (root, store) = store();
    let pdf = root.path().join("pending.pdf");
    store.record(&pdf).unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("local"));
    let history = cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx, Some(paths));
        history(cx).unwrap()
    });
    let pending = history.epoch();
    let clear = history.clear();
    let observed = Arc::new(Mutex::new(None));
    let answer = observed.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| *answer.lock().unwrap() = Some(clear.await))
            .detach();
    });
    cx.run_until_parked();
    observed.lock().unwrap().take().unwrap().unwrap();
    let late = history.record_at(pdf.clone(), pending);
    let observed = Arc::new(Mutex::new(None));
    let answer = observed.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| *answer.lock().unwrap() = Some(late.await))
            .detach();
    });
    cx.run_until_parked();
    observed.lock().unwrap().take().unwrap().unwrap();
    assert!(!store.contains(&pdf).unwrap());
    let reopened = history.record(pdf.clone());
    let observed = Arc::new(Mutex::new(None));
    let answer = observed.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| *answer.lock().unwrap() = Some(reopened.await))
            .detach();
    });
    cx.run_until_parked();
    observed.lock().unwrap().take().unwrap().unwrap();
    assert!(store.contains(&pdf).unwrap());
}

#[gpui_kit::test]
fn queued_open_then_forget_cannot_resurrect_a_record(cx: &mut gpui_kit::TestAppContext) {
    use std::sync::{Arc, Mutex};

    let (root, store) = store();
    let pdf = root.path().join("one.pdf");
    fs::write(&pdf, b"%PDF").unwrap();
    let pdf = RecentStore::normalize(&pdf).unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("local"));
    let history = cx.update(|cx| {
        gpui_kit::init(cx);
        install(cx, Some(paths));
        history(cx).unwrap()
    });

    // Enqueue from two independent callers before either completion is
    // polled; the UI-request order must win over worker scheduling.
    let record = history.record(pdf.clone());
    let forget = history.remove(pdf.clone());
    let search = history.search_excluding(String::new(), None);
    let observed = Arc::new(Mutex::new(None));
    let answer = observed.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| {
            let results = search.await.unwrap();
            let removed = forget.await.unwrap();
            record.await.unwrap();
            *answer.lock().unwrap() = Some((removed, results));
        })
        .detach();
    });
    cx.run_until_parked();
    let (removed, results) = observed.lock().unwrap().take().unwrap();
    assert!(removed);
    assert!(results.is_empty());
    assert!(!store.contains(&pdf).unwrap());
    assert!(pdf.exists());

    let reopen = history.record(pdf.clone());
    let observed = Arc::new(Mutex::new(None));
    let answer = observed.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| {
            *answer.lock().unwrap() = Some(reopen.await);
        })
        .detach();
    });
    cx.run_until_parked();
    observed.lock().unwrap().take().unwrap().unwrap();
    assert!(store.contains(&pdf).unwrap());
}

fn complete<T: 'static>(
    cx: &mut gpui_kit::TestAppContext,
    future: impl Future<Output = T> + 'static,
) -> T {
    let result = Arc::new(std::sync::Mutex::new(None));
    let output = result.clone();
    cx.update(|cx| {
        cx.spawn(async move |_| *output.lock().unwrap() = Some(future.await))
            .detach();
    });
    cx.run_until_parked();
    let mut answer = result.lock().unwrap();
    answer.take().expect("history worker replied")
}

fn install_history(cx: &mut gpui_kit::TestAppContext, root: &std::path::Path) -> RecentHistory {
    cx.update(|cx| {
        gpui_kit::init(cx);
        install(
            cx,
            Some(AppPaths::for_locations(
                root.join("config.toml"),
                root.join("local"),
            )),
        );
        history(cx).unwrap()
    })
}

#[gpui_kit::test]
fn missing_cleanup_preserves_session_state_and_stays_removed_when_recording_changes(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (root, store) = store();
    let history = install_history(cx, root.path());
    let path = root.path().join("missing.pdf");
    let position = ReadingPosition { page: 2, y: 30.0 };
    complete(cx, history.set_recording(false)).unwrap();
    complete(cx, history.record(path.clone())).unwrap();
    complete(cx, history.save_position(path.clone(), position)).unwrap();
    complete(cx, history.change_mark(path.clone(), 'a', Some(position))).unwrap();
    assert_eq!(
        complete(cx, history.search_excluding(String::new(), None))
            .unwrap()
            .len(),
        1
    );
    let epoch = history.epoch();
    history.set_remove_missing_recent_files(true).unwrap();
    assert!(
        complete(cx, history.search_excluding(String::new(), None))
            .unwrap()
            .is_empty()
    );
    assert_eq!(history.epoch(), epoch);
    assert_eq!(
        complete(cx, history.position(path.clone(), epoch)).unwrap(),
        Some(position)
    );
    assert!(
        complete(cx, history.marks(path.clone(), epoch))
            .unwrap()
            .contains_key(&'a')
    );

    // Saving session state must not resurrect automatically removed entries.
    complete(cx, history.set_recording(true)).unwrap();
    history.set_remove_missing_recent_files(false).unwrap();
    assert!(
        complete(cx, history.search_excluding(String::new(), None))
            .unwrap()
            .is_empty()
    );
    assert!(store.search("").unwrap().is_empty());
    assert_eq!(store.position(&path).unwrap(), Some(position));
    assert!(store.marks(&path).unwrap().contains_key(&'a'));

    fs::write(&path, b"%PDF").unwrap();
    complete(cx, history.record(path.clone())).unwrap();
    assert_eq!(
        complete(cx, history.search_excluding(String::new(), None)).unwrap(),
        vec![RecentFile { path }]
    );
}

#[gpui_kit::test]
fn missing_cleanup_uses_startup_preference_and_handles_persisted_and_session_entries(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (root, store) = store();
    let path = root.path().join("stored.pdf");
    store.record(&path).unwrap();
    fs::write(
        root.path().join("config.toml"),
        "[persistence]\nremove_missing_recent_files = true\n",
    )
    .unwrap();
    let history = install_history(cx, root.path());
    assert!(
        complete(cx, history.search_excluding(String::new(), None))
            .unwrap()
            .is_empty()
    );
    assert!(store.contains(&path).unwrap());

    history.set_remove_missing_recent_files(false).unwrap();
    complete(cx, history.record(path.clone())).unwrap();
    assert_eq!(
        complete(cx, history.search_excluding(String::new(), None))
            .unwrap()
            .len(),
        1
    );
    history.set_remove_missing_recent_files(true).unwrap();
    assert!(
        complete(cx, history.search_excluding(String::new(), None))
            .unwrap()
            .is_empty()
    );
    assert!(store.search("").unwrap().is_empty());
    assert!(store.contains(&path).unwrap());
}

#[gpui_kit::test]
fn missing_session_membership_is_persisted_even_beyond_database_result_limit(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (root, store) = store();
    let history = install_history(cx, root.path());
    let missing = root.path().join("missing.pdf");
    complete(cx, history.record(missing.clone())).unwrap();
    for n in 0..RESULT_LIMIT {
        let path = root.path().join(format!("existing-{n}.pdf"));
        fs::write(&path, b"%PDF").unwrap();
        store.record(&path).unwrap();
    }
    history.set_remove_missing_recent_files(true).unwrap();
    assert_eq!(
        complete(cx, history.search_excluding(String::new(), None))
            .unwrap()
            .len(),
        RESULT_LIMIT as usize,
    );
    assert!(store.search("missing").unwrap().is_empty());
    assert!(store.contains(&missing).unwrap());
}

#[gpui_kit::test]
fn dropped_reply_futures_do_not_cancel_ordered_writes(cx: &mut gpui_kit::TestAppContext) {
    let (root, store) = store();
    let history = install_history(cx, root.path());
    let other_caller = history.clone();
    let pdf = root.path().join("document.pdf");
    let position = ReadingPosition { page: 3, y: 75.0 };
    // Each call must enqueue now, not when its returned future is first polled.
    drop(history.record(pdf.clone()));
    drop(other_caller.save_position(pdf.clone(), position));
    drop(history.set_reading_mode(pdf.clone(), Some(ReadingView::Page)));
    drop(other_caller.change_mark(pdf.clone(), 'a', Some(position)));
    drop(history.name_mark(pdf.clone(), 'a', Some("Chapter".into())));
    let rows = complete(cx, other_caller.search_excluding(String::new(), None)).unwrap();
    assert_eq!(rows, vec![RecentFile { path: pdf.clone() }]);
    assert_eq!(store.position(&pdf).unwrap(), Some(position));
    assert_eq!(store.reading_mode(&pdf).unwrap(), Some(ReadingView::Page));
    assert_eq!(
        store.marks(&pdf).unwrap().get(&'a'),
        Some(&LocalMark {
            position,
            label: Some("Chapter".into()),
        })
    );
}

#[gpui_kit::test]
fn deletion_epoch_rejects_queued_child_data_and_reopen_starts_empty(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (root, store) = store();
    let history = install_history(cx, root.path());
    let pdf = root.path().join("forgotten.pdf");
    let position = ReadingPosition { page: 2, y: 40.0 };
    complete(cx, history.record(pdf.clone())).unwrap();
    complete(cx, history.save_position(pdf.clone(), position)).unwrap();
    complete(
        cx,
        history.set_reading_mode(pdf.clone(), Some(ReadingView::Page)),
    )
    .unwrap();
    complete(cx, history.change_mark(pdf.clone(), 'a', Some(position))).unwrap();
    let epoch = history.epoch();
    let forget = history.remove(pdf.clone());
    // Capture the old epoch before the queued deletion runs.
    let save = history.save_position(pdf.clone(), position);
    let mode = history.set_reading_mode(pdf.clone(), Some(ReadingView::Continuous));
    let mark = history.change_mark(pdf.clone(), 'b', Some(position));
    let read = history.position(pdf.clone(), epoch);
    assert!(complete(cx, forget).unwrap());
    complete(cx, save).unwrap();
    complete(cx, mode).unwrap();
    assert!(complete(cx, mark).unwrap_err().contains("cleared"));
    assert_eq!(complete(cx, read).unwrap(), None);
    assert!(!store.contains(&pdf).unwrap());
    assert!(history.epoch() > epoch);

    complete(cx, history.record(pdf.clone())).unwrap();
    assert_eq!(
        complete(cx, history.position(pdf.clone(), history.epoch())).unwrap(),
        None
    );
    assert_eq!(
        complete(cx, history.reading_mode(pdf.clone(), history.epoch())).unwrap(),
        None
    );
    assert!(
        complete(cx, history.marks(pdf.clone(), history.epoch()))
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.position(&pdf).unwrap(), None);
    assert_eq!(store.reading_mode(&pdf).unwrap(), None);
    assert!(store.marks(&pdf).unwrap().is_empty());
}

#[gpui_kit::test]
fn failed_flush_stays_private_and_can_be_retried(cx: &mut gpui_kit::TestAppContext) {
    let (root, store) = store();
    crate::settings::save(
        &root.path().join("config.toml"),
        crate::settings::SettingsChange::RememberReadingHistory(false),
    )
    .unwrap();
    let history = install_history(cx, root.path());
    let first = root.path().join("private.pdf");
    let second = root.path().join("later.pdf");
    let position = ReadingPosition { page: 4, y: 120.0 };
    complete(cx, history.record(first.clone())).unwrap();
    complete(cx, history.save_position(first.clone(), position)).unwrap();
    complete(
        cx,
        history.set_reading_mode(first.clone(), Some(ReadingView::Page)),
    )
    .unwrap();
    complete(cx, history.change_mark(first.clone(), 'a', Some(position))).unwrap();
    assert!(
        !root.path().join("local").exists(),
        "startup must honor recording off before the first request"
    );

    // A file where the data directory belongs makes the first flush fail.
    fs::write(root.path().join("local"), b"blocked data directory").unwrap();
    assert!(complete(cx, history.set_recording(true)).is_err());
    complete(cx, history.record(second.clone())).expect("failed flush must not enable disk writes");
    assert_eq!(
        complete(cx, history.position(first.clone(), history.epoch())).unwrap(),
        Some(position)
    );
    fs::remove_file(root.path().join("local")).unwrap();
    complete(cx, history.set_recording(true)).unwrap();
    assert!(store.contains(&first).unwrap());
    assert!(store.contains(&second).unwrap());
    assert_eq!(store.position(&first).unwrap(), Some(position));
    assert_eq!(store.reading_mode(&first).unwrap(), Some(ReadingView::Page));
    assert_eq!(
        store.marks(&first).unwrap().get(&'a').unwrap().position,
        position
    );
}
