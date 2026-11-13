use super::*;

fn store() -> (tempfile::TempDir, RecentStore) {
    let root = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_locations(root.path().join("config.toml"), root.path().join("local"));
    (root, RecentStore::new(&paths))
}

#[test]
fn missing_recent_cleanup_preserves_saved_data_and_reopen_restores_membership() {
    let (root, store) = store();
    let path = root.path().join("missing.pdf");
    store.record(&path).unwrap();
    let position = ReadingPosition { page: 2, y: 48.5 };
    store.save_position(&path, position).unwrap();
    store
        .set_reading_mode(&path, Some(ReadingView::Page))
        .unwrap();
    let marks = BTreeMap::from([(
        'a',
        LocalMark {
            position,
            label: Some("Chapter".into()),
        },
    )]);
    store.replace_marks(&path, &marks).unwrap();
    assert_eq!(store.search("").unwrap().len(), 1);
    assert!(
        store
            .search_excluding_with_cleanup("", None, true)
            .unwrap()
            .is_empty()
    );
    assert!(store.contains(&path).unwrap());
    assert_eq!(store.position(&path).unwrap(), Some(position));
    assert_eq!(store.reading_mode(&path).unwrap(), Some(ReadingView::Page));
    assert_eq!(store.marks(&path).unwrap(), marks);

    // Cleanup persists across restart and disabling the preference.
    fs::write(&path, b"%PDF").unwrap();
    let restarted = RecentStore::new(&AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    ));
    assert!(restarted.search("").unwrap().is_empty());
    restarted.record(&path).unwrap();
    assert_eq!(
        restarted.search("").unwrap(),
        vec![RecentFile { path: path.clone() }]
    );
    assert_eq!(restarted.marks(&path).unwrap(), marks);
    restarted.remove(&path).unwrap();
    assert!(!restarted.contains(&path).unwrap());
    assert!(restarted.marks(&path).unwrap().is_empty());
}

#[test]
fn cleanup_fills_results_past_missing_files_and_keeps_excluded_and_inaccessible_paths() {
    let (root, store) = store();
    let existing = root.path().join("existing.pdf");
    fs::write(&existing, b"%PDF").unwrap();
    store.record(&existing).unwrap();
    for n in 0..105 {
        store
            .record(&root.path().join(format!("missing-{n}.pdf")))
            .unwrap();
    }
    assert_eq!(
        store.search_excluding_with_cleanup("", None, true).unwrap(),
        vec![RecentFile { path: existing }],
    );
    let excluded = root.path().join("active.pdf");
    let inaccessible = root.path().join("permission-denied.pdf");
    store.record(&inaccessible).unwrap();
    store.record(&excluded).unwrap();
    let files = store
        .search_checked("", Some(&excluded), |_| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        })
        .unwrap();
    assert!(files.iter().any(|file| file.path == inaccessible));
    assert_eq!(store.search("").unwrap().len(), 3);
}

#[test]
fn recent_membership_migrates_existing_documents_without_losing_positions() {
    let (root, store) = store();
    fs::create_dir_all(root.path().join("local")).unwrap();
    let connection = Connection::open(&store.database).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE documents (path TEXT PRIMARY KEY NOT NULL, last_opened INTEGER NOT NULL);
         INSERT INTO documents VALUES ('/old.pdf', 1);
         PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(connection);
    assert_eq!(store.search("").unwrap()[0].path, Path::new("/old.pdf"));
    let position = ReadingPosition { page: 1, y: 20.0 };
    store
        .save_position(Path::new("/old.pdf"), position)
        .unwrap();
    assert!(
        store
            .search_excluding_with_cleanup("", None, true)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.position(Path::new("/old.pdf")).unwrap(),
        Some(position)
    );
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
fn position_migrates_from_v1_and_is_removed_with_its_document() {
    let (root, store) = store();
    fs::create_dir_all(root.path().join("local")).unwrap();
    let connection = Connection::open(&store.database).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE documents (path TEXT PRIMARY KEY NOT NULL, last_opened INTEGER NOT NULL);
         INSERT INTO documents VALUES ('/old.pdf', 1);
         PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(connection);
    assert_eq!(store.position(Path::new("/old.pdf")).unwrap(), None);
    let position = ReadingPosition { page: 2, y: 48.5 };
    store
        .save_position(Path::new("/old.pdf"), position)
        .unwrap();
    assert_eq!(
        store.position(Path::new("/old.pdf")).unwrap(),
        Some(position)
    );
    store.remove(Path::new("/old.pdf")).unwrap();
    assert_eq!(store.position(Path::new("/old.pdf")).unwrap(), None);
    store
        .save_position(Path::new("/old.pdf"), position)
        .unwrap();
    assert_eq!(store.position(Path::new("/old.pdf")).unwrap(), None);
    store.record(Path::new("/other.pdf")).unwrap();
    store
        .save_position(Path::new("/other.pdf"), position)
        .unwrap();
    store.clear().unwrap();
    assert_eq!(store.position(Path::new("/other.pdf")).unwrap(), None);
}

#[test]
fn mode_schema_v2_upgrades_without_losing_mode_data() {
    let (root, store) = store();
    fs::create_dir_all(root.path().join("local")).unwrap();
    let connection = Connection::open(&store.database).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE documents (path TEXT PRIMARY KEY NOT NULL, last_opened INTEGER NOT NULL);
         CREATE TABLE reading_modes (
             path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE,
             mode TEXT NOT NULL CHECK(mode IN ('continuous', 'page'))
         );
         INSERT INTO documents VALUES ('/book.pdf', 1);
         INSERT INTO reading_modes VALUES ('/book.pdf', 'page');
         PRAGMA user_version = 2;",
        )
        .unwrap();
    drop(connection);
    assert_eq!(store.position(Path::new("/book.pdf")).unwrap(), None);
    store
        .save_position(Path::new("/book.pdf"), ReadingPosition { page: 1, y: 25.0 })
        .unwrap();
    let connection = Connection::open(&store.database).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT mode FROM reading_modes WHERE path = '/book.pdf'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "page"
    );
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
}

#[test]
fn position_schema_v3_upgrades_without_losing_position_data() {
    let (root, store) = store();
    fs::create_dir_all(root.path().join("local")).unwrap();
    let connection = Connection::open(&store.database).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE documents (path TEXT PRIMARY KEY NOT NULL, last_opened INTEGER NOT NULL);
             CREATE TABLE reading_positions (
                 path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE,
                 page INTEGER NOT NULL,
                 y REAL NOT NULL
             );
             INSERT INTO documents VALUES ('/book.pdf', 1);
             INSERT INTO reading_positions VALUES ('/book.pdf', 2, 45.0);
             PRAGMA user_version = 3;",
        )
        .unwrap();
    drop(connection);
    let path = Path::new("/book.pdf");
    assert_eq!(
        store.position(path).unwrap(),
        Some(ReadingPosition { page: 2, y: 45.0 })
    );
    assert_eq!(store.reading_mode(path).unwrap(), None);
    store
        .set_reading_mode(path, Some(ReadingView::Page))
        .unwrap();
    assert_eq!(store.reading_mode(path).unwrap(), Some(ReadingView::Page));
}

#[test]
fn version_four_adds_marks_without_losing_modes_or_positions() {
    let (root, store) = store();
    fs::create_dir_all(root.path().join("local")).unwrap();
    let connection = Connection::open(&store.database).unwrap();
    connection.execute_batch(
        "CREATE TABLE documents (path TEXT PRIMARY KEY NOT NULL, last_opened INTEGER NOT NULL);
         CREATE TABLE reading_modes (path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE, mode TEXT NOT NULL);
         CREATE TABLE reading_positions (path TEXT PRIMARY KEY NOT NULL REFERENCES documents(path) ON DELETE CASCADE, page INTEGER NOT NULL, y REAL NOT NULL);
         INSERT INTO documents VALUES ('/book.pdf', 1);
         INSERT INTO reading_modes VALUES ('/book.pdf', 'page');
         INSERT INTO reading_positions VALUES ('/book.pdf', 2, 45.0);
         PRAGMA user_version = 4;",
    ).unwrap();
    drop(connection);
    let path = Path::new("/book.pdf");
    assert_eq!(store.reading_mode(path).unwrap(), Some(ReadingView::Page));
    assert_eq!(
        store.position(path).unwrap(),
        Some(ReadingPosition { page: 2, y: 45.0 })
    );
    let mut marks = BTreeMap::new();
    marks.insert(
        'a',
        LocalMark {
            position: ReadingPosition { page: 1, y: 55.0 },
            label: Some("Introduction".into()),
        },
    );
    store.replace_marks(path, &marks).unwrap();
    assert_eq!(store.marks(path).unwrap(), marks);
    marks.clear();
    marks.insert(
        'b',
        LocalMark {
            position: ReadingPosition { page: 0, y: 100.0 },
            label: None,
        },
    );
    store.replace_marks(path, &marks).unwrap();
    assert_eq!(store.marks(path).unwrap(), marks);
    store.remove(path).unwrap();
    assert!(store.marks(path).unwrap().is_empty());
    store.replace_marks(path, &marks).unwrap();
    assert!(store.marks(path).unwrap().is_empty());
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

#[test]
fn invalid_mark_replacement_rolls_back_the_entire_set() {
    let (_root, store) = store();
    let path = Path::new("/book.pdf");
    store.record(path).unwrap();
    let original = BTreeMap::from([(
        'a',
        LocalMark {
            position: ReadingPosition { page: 1, y: 50.0 },
            label: Some("Keep this".into()),
        },
    )]);
    store.replace_marks(path, &original).unwrap();
    let replacement = BTreeMap::from([
        (
            'b',
            LocalMark {
                position: ReadingPosition { page: 2, y: 80.0 },
                label: None,
            },
        ),
        (
            'z',
            LocalMark {
                position: ReadingPosition {
                    page: 3,
                    y: f32::NAN,
                },
                label: None,
            },
        ),
    ]);
    assert_eq!(
        store.replace_marks(path, &replacement).unwrap_err(),
        "Invalid local mark"
    );
    assert_eq!(
        store.marks(path).unwrap(),
        original,
        "deletion and earlier inserts must roll back together"
    );
}
