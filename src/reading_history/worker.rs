//! One serialized history worker shared by all document windows.
//!
//! Request methods enqueue synchronously; returned futures only await replies.
//! The consumer owns recording policy, session caches, and deletion epochs.
//! Its lifetime follows the shared request senders, not an individual panel.

use super::store::{RESULT_LIMIT, valid_mark_label};
use super::{LocalMark, ReadingPosition, RecentFile, RecentStore, RulerAnchor};
use crate::settings::{AppPaths, ReadingView};
use gpui_kit::{App, AppContext as _, Entity, Global};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

enum MarkEdit {
    Position(Option<ReadingPosition>),
    Label(Option<String>),
}

enum Request {
    Ruler(
        PathBuf,
        u64,
        async_channel::Sender<Result<Option<RulerAnchor>, String>>,
    ),
    SaveRuler(
        PathBuf,
        RulerAnchor,
        u64,
        async_channel::Sender<Result<(), String>>,
    ),
    Record(PathBuf, u64, async_channel::Sender<Result<(), String>>),
    Search(
        String,
        Option<PathBuf>,
        async_channel::Sender<Result<Vec<RecentFile>, String>>,
    ),
    Remove(PathBuf, async_channel::Sender<Result<bool, String>>),
    Clear(async_channel::Sender<Result<(), String>>),
    SetRecording(bool, async_channel::Sender<Result<(), String>>),
    SetRemoveMissingRecentFiles(bool),
    Position(
        PathBuf,
        u64,
        async_channel::Sender<Result<Option<ReadingPosition>, String>>,
    ),
    SavePosition(
        PathBuf,
        ReadingPosition,
        u64,
        async_channel::Sender<Result<(), String>>,
    ),
    ReadMode(
        PathBuf,
        u64,
        async_channel::Sender<Result<Option<ReadingView>, String>>,
    ),
    WriteMode(
        PathBuf,
        Option<ReadingView>,
        u64,
        async_channel::Sender<Result<(), String>>,
    ),
    Marks(
        PathBuf,
        u64,
        async_channel::Sender<Result<BTreeMap<char, LocalMark>, String>>,
    ),
    EditMark(
        PathBuf,
        char,
        MarkEdit,
        u64,
        async_channel::Sender<Result<BTreeMap<char, LocalMark>, String>>,
    ),
}

#[derive(Clone)]
pub struct RecentHistory {
    sender: async_channel::Sender<Request>,
    epoch: Arc<AtomicU64>,
}

impl RecentHistory {
    pub(crate) fn ruler_anchor(
        &self,
        path: PathBuf,
        epoch: u64,
    ) -> impl Future<Output = Result<Option<RulerAnchor>, String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::Ruler(path, epoch, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    /// Called synchronously when the target moves, never from a close or a
    /// delayed save task. The shared queue is the order across live windows.
    pub(crate) fn save_ruler_anchor(
        &self,
        path: PathBuf,
        anchor: RulerAnchor,
        epoch: u64,
    ) -> impl Future<Output = Result<(), String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::SaveRuler(path, anchor, epoch, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    fn new(store: RecentStore, config: PathBuf, cx: &mut App) -> Self {
        let (sender, receiver) = async_channel::unbounded();
        let epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = epoch.clone();
        cx.background_executor()
            .spawn(async move {
                // Read before the first queued open: an off preference must
                // never briefly write history during startup.
                let settings = crate::settings::load(&config).unwrap_or_default();
                let mut recording = settings.remember_reading_history();
                let mut remove_missing = settings.remove_missing_recent_files();
                let mut session = Vec::<PathBuf>::new();
                let mut hidden_recent = HashSet::<PathBuf>::new();
                let mut positions = HashMap::<PathBuf, ReadingPosition>::new();
                let mut modes = HashMap::<PathBuf, Option<ReadingView>>::new();
                let mut marks = HashMap::<PathBuf, BTreeMap<char, LocalMark>>::new();
                let mut rulers = HashMap::<PathBuf, RulerAnchor>::new();
                while let Ok(request) = receiver.recv().await {
                    match request {
                        Request::Ruler(path, token, reply) => {
                            let result = if token != worker_epoch.load(Ordering::SeqCst) {
                                Ok(None)
                            } else if let Some(anchor) = rulers.get(&path) {
                                Ok(Some(*anchor))
                            } else {
                                store.ruler_anchor(&path)
                            };
                            let _ = reply.try_send(result);
                        }
                        Request::SaveRuler(path, anchor, token, reply) => {
                            let valid = token == worker_epoch.load(Ordering::SeqCst)
                                && session.contains(&path);
                            let result = if !anchor.is_valid() {
                                Err("Invalid reading ruler location".into())
                            } else if valid && recording {
                                store.save_ruler_anchor(&path, anchor)
                            } else {
                                Ok(())
                            };
                            if valid && result.is_ok() {
                                rulers.insert(path, anchor);
                            }
                            let _ = reply.try_send(result);
                        }
                        Request::Record(path, token, reply) => {
                            let result = if token != worker_epoch.load(Ordering::SeqCst) {
                                Ok(())
                            } else if recording {
                                store.record(&path)
                            } else {
                                Ok(())
                            };
                            if result.is_ok() && token == worker_epoch.load(Ordering::SeqCst) {
                                hidden_recent.remove(&path);
                                session.retain(|entry| entry != &path);
                                session.insert(0, path);
                            }
                            let _ = reply.try_send(result);
                        }
                        Request::Search(query, excluded, reply) => {
                            let query_lowercase = query.to_lowercase();
                            let cleanup: Result<(), String> = if remove_missing {
                                session.iter().try_for_each(|path| {
                                    if excluded.as_ref() != Some(path)
                                        && !hidden_recent.contains(path)
                                        && path.to_string_lossy().to_lowercase().contains(&query_lowercase)
                                        && matches!(path.try_exists(), Ok(false))
                                    {
                                        store.hide_recent(path)?;
                                        hidden_recent.insert(path.clone());
                                    }
                                    Ok(())
                                })
                            } else {
                                Ok(())
                            };
                            let result = cleanup
                                .and_then(|()| store.search_excluding_with_cleanup(&query, excluded.as_deref(), remove_missing))
                                .map(|files| {
                                    let mut merged = session
                                        .iter()
                                        .filter(|path| {
                                            excluded.as_ref() != Some(path)
                                                && !hidden_recent.contains(*path)
                                                && path
                                                    .to_string_lossy()
                                                    .to_lowercase()
                                                    .contains(&query_lowercase)
                                        })
                                        .map(|path| RecentFile { path: path.clone() })
                                        .collect::<Vec<_>>();
                                    merged.extend(
                                        files
                                            .into_iter()
                                            .filter(|file| !session.contains(&file.path)),
                                    );
                                    merged.truncate(RESULT_LIMIT as usize);
                                    merged
                                });
                            let _ = reply.try_send(result);
                        }
                        Request::Remove(path, reply) => {
                            let result = store.remove(&path);
                            if result.is_ok() {
                                session.retain(|entry| entry != &path);
                                hidden_recent.remove(&path);
                                positions.remove(&path);
                                modes.remove(&path);
                                marks.remove(&path);
                                rulers.remove(&path);
                                worker_epoch.fetch_add(1, Ordering::SeqCst);
                            }
                            let _ = reply.try_send(result);
                        }
                        Request::Clear(reply) => {
                            let result = store.clear();
                            if result.is_ok() {
                                session.clear();
                                hidden_recent.clear();
                                positions.clear();
                                modes.clear();
                                marks.clear();
                                rulers.clear();
                                worker_epoch.fetch_add(1, Ordering::SeqCst);
                            }
                            let _ = reply.try_send(result);
                        }
                        Request::SetRecording(enabled, reply) => {
                            let result = if enabled {
                                session
                                    .iter()
                                    .rev()
                                    .try_for_each(|path| {
                                        store.record(path)?;
                                        if hidden_recent.contains(path) {
                                            store.hide_recent(path)?;
                                        }
                                        if let Some(position) = positions.get(path) {
                                            store.save_position(path, *position)?;
                                        }
                                        if let Some(anchor) = rulers.get(path) {
                                            store.save_ruler_anchor(path, *anchor)?;
                                        }
                                        Ok(())
                                    })
                                    .and_then(|()| {
                                        modes.iter().try_for_each(|(path, mode)| {
                                            store.set_reading_mode(path, *mode)
                                        })
                                    })
                                    .and_then(|()| {
                                        marks.iter().try_for_each(|(path, letters)| {
                                            store.replace_marks(path, letters)
                                        })
                                    })
                            } else {
                                Ok(())
                            };
                            // A failed flush remains retryable; the worker
                            // must not start writing new records on failure.
                            if result.is_ok() {
                                recording = enabled;
                            }
                            let _ = reply.try_send(result);
                        }
                        Request::SetRemoveMissingRecentFiles(enabled) => {
                            remove_missing = enabled;
                        }
                        Request::Position(path, token, reply) => {
                            let result = if token != worker_epoch.load(Ordering::SeqCst) {
                                Ok(None)
                            } else if let Some(position) = positions.get(&path) {
                                Ok(Some(*position))
                            } else {
                                store.position(&path)
                            };
                            let _ = reply.try_send(result);
                        }
                        Request::SavePosition(path, position, token, reply) => {
                            let valid = token == worker_epoch.load(Ordering::SeqCst)
                                && session.contains(&path);
                            let result = if valid && recording {
                                store.save_position(&path, position)
                            } else {
                                Ok(())
                            };
                            if result.is_ok() && valid {
                                positions.insert(path, position);
                            }
                            let _ = reply.try_send(result);
                        }
                        Request::ReadMode(path, token, reply) => {
                            let result = if token != worker_epoch.load(Ordering::SeqCst) {
                                Ok(None)
                            } else if let Some(mode) = modes.get(&path) {
                                Ok(*mode)
                            } else {
                                store.reading_mode(&path)
                            };
                            let _ = reply.try_send(result);
                        }
                        Request::WriteMode(path, mode, token, reply) => {
                            let result = if token != worker_epoch.load(Ordering::SeqCst) {
                                Ok(())
                            } else if recording {
                                store.set_reading_mode(&path, mode)
                            } else {
                                Ok(())
                            };
                            if result.is_ok() && token == worker_epoch.load(Ordering::SeqCst) {
                                modes.insert(path, mode);
                            }
                            let _ = reply.try_send(result);
                        }
                        Request::Marks(path, token, reply) => {
                            let result = if token != worker_epoch.load(Ordering::SeqCst) {
                                Ok(BTreeMap::new())
                            } else if let Some(cached) = marks.get(&path) {
                                Ok(cached.clone())
                            } else {
                                store.marks(&path)
                            };
                            let _ = reply.try_send(result);
                        }
                        Request::EditMark(path, letter, edit, token, reply) => {
                            let result = (|| {
                                if token != worker_epoch.load(Ordering::SeqCst)
                                    || !session.contains(&path)
                                {
                                    return Err("This PDF's saved data was cleared; reopen it before changing marks".into());
                                }
                                if !letter.is_ascii_lowercase() {
                                    return Err("Invalid local mark letter".into());
                                }
                                let mut updated = match marks.get(&path) {
                                    Some(cached) => cached.clone(),
                                    None => store.marks(&path)?,
                                };
                                match edit {
                                    MarkEdit::Position(Some(position)) => {
                                        let label = updated.get(&letter).and_then(|mark| mark.label.clone());
                                        updated.insert(letter, LocalMark { position, label });
                                    }
                                    MarkEdit::Position(None) => {
                                        updated.remove(&letter);
                                    }
                                    MarkEdit::Label(label) => {
                                        if label.as_deref().is_some_and(|label| !valid_mark_label(label)) {
                                            return Err("Invalid local mark name".into());
                                        }
                                        let mark = updated.get_mut(&letter)
                                            .ok_or("This local mark is no longer set")?;
                                        mark.label = label;
                                    }
                                }
                                if recording {
                                    store.replace_marks(&path, &updated)?;
                                }
                                marks.insert(path, updated.clone());
                                Ok(updated)
                            })();
                            let _ = reply.try_send(result);
                        }
                    }
                }
            })
            .detach();
        Self { sender, epoch }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub fn set_remove_missing_recent_files(&self, enabled: bool) -> Result<(), String> {
        self.sender
            .try_send(Request::SetRemoveMissingRecentFiles(enabled))
            .map_err(|_| "Reading history worker is unavailable".to_owned())
    }

    /// Enqueue on the UI thread without doing disk work there. The returned
    /// future only waits for the background consumer's answer.
    pub fn record(&self, path: PathBuf) -> impl Future<Output = Result<(), String>> + use<> {
        self.record_at(path, self.epoch())
    }

    pub fn record_at(
        &self,
        path: PathBuf,
        epoch: u64,
    ) -> impl Future<Output = Result<(), String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::Record(path, epoch, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn search_excluding(
        &self,
        query: String,
        excluded: Option<PathBuf>,
    ) -> impl Future<Output = Result<Vec<RecentFile>, String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::Search(query, excluded, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn remove(&self, path: PathBuf) -> impl Future<Output = Result<bool, String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::Remove(path, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn clear(&self) -> impl Future<Output = Result<(), String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::Clear(reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn set_recording(&self, enabled: bool) -> impl Future<Output = Result<(), String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::SetRecording(enabled, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn position(
        &self,
        path: PathBuf,
        epoch: u64,
    ) -> impl Future<Output = Result<Option<ReadingPosition>, String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::Position(path, epoch, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn reading_mode(
        &self,
        path: PathBuf,
        epoch: u64,
    ) -> impl Future<Output = Result<Option<ReadingView>, String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::ReadMode(path, epoch, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn save_position(
        &self,
        path: PathBuf,
        position: ReadingPosition,
    ) -> impl Future<Output = Result<(), String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::SavePosition(path, position, self.epoch(), reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub fn set_reading_mode(
        &self,
        path: PathBuf,
        mode: Option<ReadingView>,
    ) -> impl Future<Output = Result<(), String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::WriteMode(path, mode, self.epoch(), reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub(crate) fn marks(
        &self,
        path: PathBuf,
        epoch: u64,
    ) -> impl Future<Output = Result<BTreeMap<char, LocalMark>, String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::Marks(path, epoch, reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }

    pub(crate) fn change_mark(
        &self,
        path: PathBuf,
        letter: char,
        position: Option<ReadingPosition>,
    ) -> impl Future<Output = Result<BTreeMap<char, LocalMark>, String>> + use<> {
        self.edit_mark(path, letter, MarkEdit::Position(position))
    }

    pub(crate) fn name_mark(
        &self,
        path: PathBuf,
        letter: char,
        label: Option<String>,
    ) -> impl Future<Output = Result<BTreeMap<char, LocalMark>, String>> + use<> {
        self.edit_mark(path, letter, MarkEdit::Label(label))
    }

    fn edit_mark(
        &self,
        path: PathBuf,
        letter: char,
        edit: MarkEdit,
    ) -> impl Future<Output = Result<BTreeMap<char, LocalMark>, String>> + use<> {
        let (reply, answer) = async_channel::bounded(1);
        let queued = self
            .sender
            .try_send(Request::EditMark(path, letter, edit, self.epoch(), reply))
            .map_err(|_| "Reading history worker is unavailable".to_owned());
        async move {
            queued?;
            answer
                .recv()
                .await
                .map_err(|_| "Reading history worker stopped".to_owned())?
        }
    }
}

struct SharedStore(RecentHistory);
impl Global for SharedStore {}

pub struct HistoryRevision;
struct SharedRevision(Entity<HistoryRevision>);
impl Global for SharedRevision {}

pub fn install(cx: &mut App, paths: Option<AppPaths>) {
    let revision = cx.new(|_| HistoryRevision);
    cx.set_global(SharedRevision(revision));
    if let Some(paths) = paths {
        let history =
            RecentHistory::new(RecentStore::new(&paths), paths.config().to_path_buf(), cx);
        cx.set_global(SharedStore(history));
    }
}

pub fn history(cx: &App) -> Option<RecentHistory> {
    cx.try_global::<SharedStore>()
        .map(|shared| shared.0.clone())
}

pub fn revision(cx: &App) -> Option<Entity<HistoryRevision>> {
    cx.try_global::<SharedRevision>()
        .map(|shared| shared.0.clone())
}

pub fn notify_changed(cx: &mut App) {
    if let Some(revision) = revision(cx) {
        revision.update(cx, |_, cx| cx.notify());
    }
}

#[cfg(test)]
mod tests;
