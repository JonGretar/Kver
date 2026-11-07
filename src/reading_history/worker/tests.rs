use super::*;
use std::fs;

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
