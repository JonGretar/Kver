//! Local document records. This table owns a PDF's identity and is the parent
//! for future marks, view overrides, and positions (with cascading foreign keys).
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::settings::{AppPaths, ReadingView};

const SCHEMA_VERSION: i64 = 2;
const RESULT_LIMIT: i64 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFile {
    pub path: PathBuf,
}

#[derive(Clone)]
pub struct RecentStore {
    database: PathBuf,
}

impl RecentStore {
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            database: paths.local_data().join("reading.sqlite3"),
        }
    }

    fn database_exists(&self) -> Result<bool, String> {
        self.database.try_exists().map_err(|error| {
            format!(
                "Cannot access reading history at {}: {error}",
                self.database.display()
            )
        })
    }

    fn connect(&self) -> Result<Connection, String> {
        if let Some(parent) = self.database.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create local data directory: {e}"))?;
        }
        let connection = Connection::open(&self.database)
            .map_err(|e| format!("Cannot open reading history: {e}"))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| e.to_string())?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if version > SCHEMA_VERSION {
            return Err(format!(
                "Reading history schema {version} is newer than this Kver version"
            ));
        }
        if version == 0 {
            connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS documents (
                     path TEXT PRIMARY KEY NOT NULL,
                     last_opened INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS documents_recent ON documents(last_opened DESC);
                 COMMIT;",
                )
                .map_err(|e| format!("Cannot migrate reading history: {e}"))?;
        }
        if version < 2 {
            connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     CREATE TABLE IF NOT EXISTS reading_modes (
                         path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE,
                         mode TEXT NOT NULL CHECK(mode IN ('continuous', 'page'))
                     );
                     PRAGMA user_version = 2;
                     COMMIT;",
                )
                .map_err(|e| format!("Cannot migrate reading modes: {e}"))?;
        }
        Ok(connection)
    }

    /// Normalize only after an actual successful open. Canonicalization also
    /// resolves relative paths, `..`, and symlinks to one local identity.
    pub fn normalize(path: &Path) -> Result<PathBuf, String> {
        fs::canonicalize(path).map_err(|e| format!("Cannot resolve {}: {e}", path.display()))
    }

    pub fn record(&self, path: &Path) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO documents(path, last_opened)
             VALUES (?1, (SELECT COALESCE(MAX(last_opened), 0) + 1 FROM documents))
             ON CONFLICT(path) DO UPDATE SET last_opened = excluded.last_opened",
                [path],
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())
    }

    /// Query the database, not a truncated in-memory initial list. Wildcards
    /// typed by the reader are literal characters, not SQL patterns.
    pub fn search(&self, query: &str) -> Result<Vec<RecentFile>, String> {
        self.search_excluding(query, None)
    }

    /// Omit the active reader's PDF before applying the result limit.
    pub fn search_excluding(
        &self,
        query: &str,
        excluded: Option<&Path>,
    ) -> Result<Vec<RecentFile>, String> {
        if !self.database_exists()? {
            return Ok(Vec::new());
        }
        let connection = self.connect()?;
        let excluded = excluded
            .map(|path| {
                path.to_str()
                    .ok_or("Cannot search a non-UTF-8 document path")
            })
            .transpose()?;
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let mut statement = connection
            .prepare(
                "SELECT path FROM documents
                 WHERE path LIKE ?1 ESCAPE '\\' AND (?2 IS NULL OR path <> ?2)
             ORDER BY last_opened DESC, path ASC LIMIT ?3",
            )
            .map_err(|e| e.to_string())?;
        statement
            .query_map(params![pattern, excluded, RESULT_LIMIT], |row| {
                Ok(RecentFile {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// Deleting the parent cascades to *all* future per-PDF child tables.
    /// Never touches the PDF on disk.
    pub fn remove(&self, path: &Path) -> Result<bool, String> {
        if !self.database_exists()? {
            return Ok(false);
        }
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        let connection = self.connect()?;
        connection
            .execute("DELETE FROM documents WHERE path = ?1", [path])
            .map(|affected| affected != 0)
            .map_err(|e| e.to_string())
    }

    /// The parent table is the ownership boundary for all per-PDF child data.
    pub fn clear(&self) -> Result<(), String> {
        if !self.database_exists()? {
            return Ok(());
        }
        self.connect()?
            .execute("DELETE FROM documents", [])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn contains(&self, path: &Path) -> Result<bool, String> {
        if !self.database_exists()? {
            return Ok(false);
        }
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        self.connect()?
            .query_row(
                "SELECT 1 FROM documents WHERE path = ?1",
                [path],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
            .map_err(|e| e.to_string())
    }

    /// None follows the application default rather than copying its value.
    pub fn reading_mode(&self, path: &Path) -> Result<Option<ReadingView>, String> {
        if !self.database_exists()? {
            return Ok(None);
        }
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        let mode: Option<String> = self
            .connect()?
            .query_row(
                "SELECT mode FROM reading_modes WHERE path = ?1",
                [path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        mode.map(|mode| ReadingView::parse(&mode)).transpose()
    }

    pub fn set_reading_mode(&self, path: &Path, mode: Option<ReadingView>) -> Result<(), String> {
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        if mode.is_none() && !self.database_exists()? {
            return Ok(());
        }
        let connection = self.connect()?;
        if let Some(mode) = mode {
            connection
                .execute(
                    "INSERT INTO reading_modes(path, mode) VALUES (?1, ?2)
                     ON CONFLICT(path) DO UPDATE SET mode = excluded.mode",
                    params![path, mode.key()],
                )
                .map_err(|e| e.to_string())?;
        } else {
            connection
                .execute("DELETE FROM reading_modes WHERE path = ?1", [path])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(feature = "gui")]
mod shared {
    use super::*;
    use gpui_kit::{App, AppContext as _, Entity, Global};
    use std::{
        collections::HashMap,
        future::Future,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };

    /// A single background consumer orders all reader-window requests by the
    /// time they are made, not by when separately spawned workers happen to run.
    enum Request {
        Record(PathBuf, u64, async_channel::Sender<Result<(), String>>),
        Search(
            String,
            Option<PathBuf>,
            async_channel::Sender<Result<Vec<RecentFile>, String>>,
        ),
        Remove(PathBuf, async_channel::Sender<Result<bool, String>>),
        Clear(async_channel::Sender<Result<(), String>>),
        SetRecording(bool, async_channel::Sender<Result<(), String>>),
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
    }

    #[derive(Clone)]
    pub struct RecentHistory {
        sender: async_channel::Sender<Request>,
        epoch: Arc<AtomicU64>,
    }

    impl RecentHistory {
        fn new(store: RecentStore, config: PathBuf, cx: &mut App) -> Self {
            let (sender, receiver) = async_channel::unbounded();
            let epoch = Arc::new(AtomicU64::new(0));
            let worker_epoch = epoch.clone();
            cx.background_executor()
                .spawn(async move {
                    // Read before the first queued open: an off preference must
                    // never briefly write history during startup.
                    let mut recording = crate::settings::load(&config)
                        .map(|settings| settings.remember_reading_history())
                        .unwrap_or(true);
                    let mut session = Vec::<PathBuf>::new();
                    let mut modes = HashMap::<PathBuf, Option<ReadingView>>::new();
                    while let Ok(request) = receiver.recv().await {
                        match request {
                            Request::Record(path, token, reply) => {
                                let result = if token != worker_epoch.load(Ordering::SeqCst) {
                                    Ok(())
                                } else if recording {
                                    store.record(&path)
                                } else {
                                    Ok(())
                                };
                                if result.is_ok() && token == worker_epoch.load(Ordering::SeqCst) {
                                    session.retain(|entry| entry != &path);
                                    session.insert(0, path);
                                }
                                let _ = reply.try_send(result);
                            }
                            Request::Search(query, excluded, reply) => {
                                let result = store
                                    .search_excluding(&query, excluded.as_deref())
                                    .map(|files| {
                                        let mut merged = session
                                            .iter()
                                            .filter(|path| {
                                                excluded.as_ref() != Some(path)
                                                    && path
                                                        .to_string_lossy()
                                                        .to_lowercase()
                                                        .contains(&query.to_lowercase())
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
                                    modes.remove(&path);
                                    worker_epoch.fetch_add(1, Ordering::SeqCst);
                                }
                                let _ = reply.try_send(result);
                            }
                            Request::Clear(reply) => {
                                let result = store.clear();
                                if result.is_ok() {
                                    session.clear();
                                    modes.clear();
                                    worker_epoch.fetch_add(1, Ordering::SeqCst);
                                }
                                let _ = reply.try_send(result);
                            }
                            Request::SetRecording(enabled, reply) => {
                                let result = if enabled {
                                    session
                                        .iter()
                                        .rev()
                                        .try_for_each(|path| store.record(path))
                                        .and_then(|()| {
                                            modes.iter().try_for_each(|(path, mode)| {
                                                store.set_reading_mode(path, *mode)
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
                        }
                    }
                })
                .detach();
            Self { sender, epoch }
        }

        pub fn epoch(&self) -> u64 {
            self.epoch.load(Ordering::SeqCst)
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

        pub fn set_recording(
            &self,
            enabled: bool,
        ) -> impl Future<Output = Result<(), String>> + use<> {
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
    }

    pub(super) struct SharedStore(pub RecentHistory);
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
}

#[cfg(feature = "gui")]
pub use shared::{history, install, notify_changed, revision};

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, RecentStore) {
        let root = tempfile::tempdir().unwrap();
        let paths =
            AppPaths::for_locations(root.path().join("config.toml"), root.path().join("local"));
        (root, RecentStore::new(&paths))
    }

    #[test]
    fn order_search_all_records_and_reopen() {
        let (_root, store) = store();
        for n in 0..140 {
            store
                .record(&PathBuf::from(format!("/tmp/book-{n:03}.pdf")))
                .unwrap();
        }
        assert_eq!(store.search("").unwrap().len(), 100);
        let without_active = store
            .search_excluding("", Some(Path::new("/tmp/book-139.pdf")))
            .unwrap();
        assert_eq!(without_active.len(), 100);
        assert_eq!(without_active[0].path, PathBuf::from("/tmp/book-138.pdf"));
        assert!(
            without_active
                .iter()
                .all(|file| file.path != Path::new("/tmp/book-139.pdf"))
        );
        assert_eq!(
            store.search("").unwrap()[0].path,
            PathBuf::from("/tmp/book-139.pdf")
        );
        assert_eq!(
            store.search("book-000").unwrap()[0].path,
            PathBuf::from("/tmp/book-000.pdf")
        );
        store.record(Path::new("/tmp/book-000.pdf")).unwrap();
        assert_eq!(
            store.search("").unwrap()[0].path,
            PathBuf::from("/tmp/book-000.pdf")
        );
        assert_eq!(store.search("book-%").unwrap().len(), 0);
    }

    #[test]
    fn remove_cascades_but_leaves_pdf_and_allows_reopen() {
        let (root, store) = store();
        let pdf = root.path().join("document.pdf");
        fs::write(&pdf, b"%PDF").unwrap();
        let path = RecentStore::normalize(&pdf).unwrap();
        store.record(&path).unwrap();
        let connection = store.connect().unwrap();
        connection.execute_batch(
            "CREATE TABLE marks (path TEXT REFERENCES documents(path) ON DELETE CASCADE, name TEXT);
             INSERT INTO marks VALUES ((SELECT path FROM documents), 'a');"
        ).unwrap();
        assert!(store.remove(&path).unwrap());
        assert!(pdf.exists());
        let marks: i64 = connection
            .query_row("SELECT count(*) FROM marks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(marks, 0);
        assert!(!store.contains(&path).unwrap());
        store.record(&path).unwrap();
        assert!(store.contains(&path).unwrap());
    }

    #[test]
    fn clear_cascades_for_missing_paths_without_changing_preferences() {
        let (root, store) = store();
        let config = root.path().join("config.toml");
        crate::settings::save(
            &config,
            crate::settings::SettingsChange::RememberReadingHistory(false),
        )
        .unwrap();
        let existing = root.path().join("exists.pdf");
        fs::write(&existing, b"%PDF").unwrap();
        let missing = root.path().join("offline.pdf");
        store.record(&existing).unwrap();
        store.record(&missing).unwrap();
        let connection = store.connect().unwrap();
        connection.execute_batch(
            "CREATE TABLE positions (path TEXT REFERENCES documents(path) ON DELETE CASCADE, page INTEGER);
             INSERT INTO positions VALUES ((SELECT path FROM documents LIMIT 1), 2);"
        ).unwrap();
        store.clear().unwrap();
        assert!(store.search("").unwrap().is_empty());
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM positions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(existing.exists());
        assert!(
            !crate::settings::load(&config)
                .unwrap()
                .remember_reading_history()
        );
    }

    #[test]
    fn migration_and_unknown_version() {
        let (_root, store) = store();
        let connection = store.connect().unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        store.record(Path::new("/tmp/kept.pdf")).unwrap();
        assert!(store.contains(Path::new("/tmp/kept.pdf")).unwrap());
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(store.search("").unwrap_err().contains("newer"));
    }

    #[test]
    fn existing_history_migrates_and_mode_is_cascaded_without_copying_a_default() {
        let (_root, store) = store();
        std::fs::create_dir_all(store.database.parent().unwrap()).unwrap();
        let connection = Connection::open(&store.database).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE documents (path TEXT PRIMARY KEY NOT NULL, last_opened INTEGER NOT NULL);
                 INSERT INTO documents VALUES ('/tmp/old.pdf', 1);
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        drop(connection);
        let path = Path::new("/tmp/old.pdf");
        assert_eq!(store.reading_mode(path).unwrap(), None);
        store
            .set_reading_mode(path, Some(ReadingView::Page))
            .unwrap();
        assert_eq!(store.reading_mode(path).unwrap(), Some(ReadingView::Page));
        store.set_reading_mode(path, None).unwrap();
        assert_eq!(store.reading_mode(path).unwrap(), None);
        store
            .set_reading_mode(path, Some(ReadingView::Continuous))
            .unwrap();
        store.remove(path).unwrap();
        assert_eq!(store.reading_mode(path).unwrap(), None);
    }

    #[test]
    fn normalization_unifies_symlinks_and_relative_segments() {
        let (root, _store) = store();
        let pdf = root.path().join("one.pdf");
        fs::write(&pdf, b"%PDF").unwrap();
        fs::create_dir(root.path().join("sub")).unwrap();
        assert_eq!(
            RecentStore::normalize(&root.path().join("sub/../one.pdf")).unwrap(),
            fs::canonicalize(&pdf).unwrap()
        );
        assert!(RecentStore::normalize(&root.path().join("missing.pdf")).is_err());
    }

    #[cfg(feature = "gui")]
    #[gpui_kit::test]
    fn pending_open_from_before_clear_does_not_recreate_deleted_data(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use std::sync::{Arc, Mutex};
        let (root, store) = store();
        let pdf = root.path().join("pending.pdf");
        store.record(&pdf).unwrap();
        let paths =
            AppPaths::for_locations(root.path().join("config.toml"), root.path().join("local"));
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

    #[cfg(feature = "gui")]
    #[gpui_kit::test]
    fn queued_open_then_forget_cannot_resurrect_a_record(cx: &mut gpui_kit::TestAppContext) {
        use std::sync::{Arc, Mutex};

        let (root, store) = store();
        let pdf = root.path().join("one.pdf");
        fs::write(&pdf, b"%PDF").unwrap();
        let pdf = RecentStore::normalize(&pdf).unwrap();
        let paths =
            AppPaths::for_locations(root.path().join("config.toml"), root.path().join("local"));
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
}
