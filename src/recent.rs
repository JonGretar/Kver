//! Local document records. This table owns a PDF's identity and is the parent
//! for future marks, view overrides, and positions (with cascading foreign keys).
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::settings::AppPaths;

const SCHEMA_VERSION: i64 = 1;
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
                 PRAGMA user_version = 1;
                 COMMIT;",
                )
                .map_err(|e| format!("Cannot migrate reading history: {e}"))?;
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
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        let connection = self.connect()?;
        connection
            .execute("DELETE FROM documents WHERE path = ?1", [path])
            .map(|affected| affected != 0)
            .map_err(|e| e.to_string())
    }

    pub fn contains(&self, path: &Path) -> Result<bool, String> {
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
}

#[cfg(feature = "gui")]
mod shared {
    use super::*;
    use gpui_kit::{App, Global};
    use std::future::Future;

    /// A single background consumer orders all reader-window requests by the
    /// time they are made, not by when separately spawned workers happen to run.
    enum Request {
        Record(PathBuf, async_channel::Sender<Result<(), String>>),
        Search(
            String,
            Option<PathBuf>,
            async_channel::Sender<Result<Vec<RecentFile>, String>>,
        ),
        Remove(PathBuf, async_channel::Sender<Result<bool, String>>),
    }

    #[derive(Clone)]
    pub struct RecentHistory {
        sender: async_channel::Sender<Request>,
    }

    impl RecentHistory {
        fn new(store: RecentStore, cx: &mut App) -> Self {
            let (sender, receiver) = async_channel::unbounded();
            cx.background_executor()
                .spawn(async move {
                    while let Ok(request) = receiver.recv().await {
                        match request {
                            Request::Record(path, reply) => {
                                let _ = reply.try_send(store.record(&path));
                            }
                            Request::Search(query, excluded, reply) => {
                                let _ = reply
                                    .try_send(store.search_excluding(&query, excluded.as_deref()));
                            }
                            Request::Remove(path, reply) => {
                                let _ = reply.try_send(store.remove(&path));
                            }
                        }
                    }
                })
                .detach();
            Self { sender }
        }

        /// Enqueue on the UI thread without doing disk work there. The returned
        /// future only waits for the background consumer's answer.
        pub fn record(&self, path: PathBuf) -> impl Future<Output = Result<(), String>> + use<> {
            let (reply, answer) = async_channel::bounded(1);
            let queued = self
                .sender
                .try_send(Request::Record(path, reply))
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
    }

    pub(super) struct SharedStore(pub RecentHistory);
    impl Global for SharedStore {}

    pub fn install(cx: &mut App, paths: Option<AppPaths>) {
        if let Some(paths) = paths {
            let history = RecentHistory::new(RecentStore::new(&paths), cx);
            cx.set_global(SharedStore(history));
        }
    }

    pub fn history(cx: &App) -> Option<RecentHistory> {
        cx.try_global::<SharedStore>()
            .map(|shared| shared.0.clone())
    }
}

#[cfg(feature = "gui")]
pub use shared::{history, install};

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
    fn migration_and_unknown_version() {
        let (_root, store) = store();
        let connection = store.connect().unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        store.record(Path::new("/tmp/kept.pdf")).unwrap();
        assert!(store.contains(Path::new("/tmp/kept.pdf")).unwrap());
        connection.pragma_update(None, "user_version", 2).unwrap();
        assert!(store.search("").unwrap_err().contains("newer"));
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
