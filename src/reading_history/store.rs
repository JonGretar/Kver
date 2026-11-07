//! Local document records. The documents table owns a PDF's identity and is the
//! parent for marks, view overrides, and positions (with cascading foreign keys).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::settings::{AppPaths, ReadingView};

// Version 2 holds reading modes; version 3 added positions in an independent
// branch. Version 4 reconciles databases created by either branch alone;
// version 5 adds document-local letter marks.
const SCHEMA_VERSION: i64 = 5;
pub(super) const RESULT_LIMIT: i64 = 100;

/// Page-local distance in PDF points from the top of the displayed page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadingPosition {
    pub page: usize,
    pub y: f32,
}

/// A letter's destination and optional reader-facing label. The letter remains
/// its identity; labels do not create independent named bookmarks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalMark {
    pub(crate) position: ReadingPosition,
    pub(crate) label: Option<String>,
}

pub(super) fn valid_mark_label(label: &str) -> bool {
    !label.trim().is_empty() && label.chars().count() <= 80 && !label.chars().any(char::is_control)
}

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
        if version < 3 {
            connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     CREATE TABLE IF NOT EXISTS reading_positions (
                         path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE,
                         page INTEGER NOT NULL,
                         y REAL NOT NULL
                     );
                     PRAGMA user_version = 3;
                     COMMIT;",
                )
                .map_err(|e| format!("Cannot migrate reading positions: {e}"))?;
        }
        if version < 4 {
            connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     CREATE TABLE IF NOT EXISTS reading_modes (
                         path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE,
                         mode TEXT NOT NULL CHECK(mode IN ('continuous', 'page'))
                     );
                     CREATE TABLE IF NOT EXISTS reading_positions (
                         path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE,
                         page INTEGER NOT NULL,
                         y REAL NOT NULL
                     );
                     PRAGMA user_version = 4;
                     COMMIT;",
                )
                .map_err(|e| format!("Cannot reconcile reading data: {e}"))?;
        }
        if version < 5 {
            connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     CREATE TABLE IF NOT EXISTS local_marks (
                         path TEXT NOT NULL REFERENCES documents(path) ON DELETE CASCADE,
                         letter TEXT NOT NULL CHECK(length(letter) = 1 AND letter BETWEEN 'a' AND 'z'),
                         page INTEGER NOT NULL,
                         y REAL NOT NULL,
                         label TEXT,
                         PRIMARY KEY(path, letter)
                     );
                     PRAGMA user_version = 5;
                     COMMIT;",
                )
                .map_err(|e| format!("Cannot migrate local marks: {e}"))?;
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

    pub fn position(&self, path: &Path) -> Result<Option<ReadingPosition>, String> {
        if !self.database_exists()? {
            return Ok(None);
        }
        let path = path
            .to_str()
            .ok_or("Cannot read a non-UTF-8 document path")?;
        let stored: Option<(i64, f64)> = self
            .connect()?
            .query_row(
                "SELECT page, y FROM reading_positions WHERE path = ?1",
                [path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(stored.and_then(|(page, y)| {
            if !y.is_finite() || y < 0.0 || y > f32::MAX as f64 {
                return None;
            }
            Some(ReadingPosition {
                page: usize::try_from(page).ok()?,
                y: y as f32,
            })
        }))
    }

    pub fn save_position(&self, path: &Path, position: ReadingPosition) -> Result<(), String> {
        if !position.y.is_finite() || position.y < 0.0 {
            return Err("Invalid reading position".into());
        }
        let page = i64::try_from(position.page).map_err(|e| e.to_string())?;
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        // A removed PDF stays removed until it is explicitly opened again.
        transaction
            .execute(
                "INSERT INTO reading_positions(path, page, y)
             SELECT path, ?2, ?3 FROM documents WHERE path = ?1
             ON CONFLICT(path) DO UPDATE SET page = excluded.page, y = excluded.y",
                params![path, page, position.y],
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())
    }

    pub(crate) fn marks(&self, path: &Path) -> Result<BTreeMap<char, LocalMark>, String> {
        if !self.database_exists()? {
            return Ok(BTreeMap::new());
        }
        let path = path
            .to_str()
            .ok_or("Cannot read a non-UTF-8 document path")?;
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT letter, page, y, label FROM local_marks WHERE path = ?1")
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([path], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut marks = BTreeMap::new();
        for row in rows {
            let (letter, page, y, label) = row.map_err(|e| e.to_string())?;
            if let (Some(letter), Ok(page)) = (letter.chars().next(), usize::try_from(page))
                && letter.is_ascii_lowercase()
                && y.is_finite()
                && (0.0..=f32::MAX as f64).contains(&y)
            {
                marks.insert(
                    letter,
                    LocalMark {
                        position: ReadingPosition { page, y: y as f32 },
                        label,
                    },
                );
            }
        }
        Ok(marks)
    }

    /// Replace the whole set atomically, including session-only deletions.
    pub(crate) fn replace_marks(
        &self,
        path: &Path,
        marks: &BTreeMap<char, LocalMark>,
    ) -> Result<(), String> {
        if marks.is_empty() && !self.database_exists()? {
            return Ok(());
        }
        let path = path
            .to_str()
            .ok_or("Cannot store a non-UTF-8 document path")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction().map_err(|e| e.to_string())?;
        if transaction
            .query_row(
                "SELECT 1 FROM documents WHERE path = ?1",
                [path],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Ok(());
        }
        transaction
            .execute("DELETE FROM local_marks WHERE path = ?1", [path])
            .map_err(|e| e.to_string())?;
        for (&letter, mark) in marks {
            let position = mark.position;
            if !letter.is_ascii_lowercase()
                || !position.y.is_finite()
                || position.y < 0.0
                || mark
                    .label
                    .as_deref()
                    .is_some_and(|label| !valid_mark_label(label))
            {
                return Err("Invalid local mark".into());
            }
            let page = i64::try_from(position.page).map_err(|e| e.to_string())?;
            transaction
                .execute(
                    "INSERT INTO local_marks(path, letter, page, y, label) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![path, letter.to_string(), page, position.y, mark.label.as_deref()],
                )
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
