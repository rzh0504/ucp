use super::*;
use crate::model::{
    AppLanguage, AppSettings, AppTheme, ClipboardContent, ClipboardEntry, ClipboardImage,
    DEFAULT_BACKGROUND_OPACITY,
};
use crate::services::persistence::StorageWriter;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn double_click_copy_is_enabled_by_default() {
    assert!(AppSettings::default().double_click_copy);
}

#[test]
fn quick_paste_is_disabled_by_default() {
    assert!(!AppSettings::default().quick_paste);
}

#[test]
fn storage_round_trips_settings_and_clipboard_entries() {
    let _guard = storage_test_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = unique_test_directory();
    let storage = StorageHandle::new_for_test(directory.clone());
    *test_data_directory().lock().unwrap() = Some(directory.clone());

    let settings = AppSettings {
        history_limit: 50,
        auto_cleanup_days: Some(30),
        preserve_favorites_on_delete: true,
        language: AppLanguage::English,
        theme: AppTheme::Dark,
        launch_at_startup: true,
        desktop_widget: false,
        desktop_widget_topmost: false,
        keyboard_shortcuts: false,
        global_show_shortcut: "Ctrl+Alt+Space".to_string(),
        auto_focus_history: false,
        promote_copied_entries: false,
        quick_paste: true,
        double_click_copy: false,
        hide_after_copy: true,
        show_copy_time: false,
        show_text_length: false,
        background_opacity: DEFAULT_BACKGROUND_OPACITY,
    };
    save_settings(&storage, &settings).unwrap();

    let mut text_entry = ClipboardEntry::new(10, ClipboardContent::Text("hello".to_string()));
    text_entry.favorite = true;
    save_entry(&storage, &text_entry).unwrap();

    let mut image_entry = ClipboardEntry::new(
        11,
        ClipboardContent::Image(ClipboardImage::from_rgba(1, 1, vec![10, 20, 30, 255])),
    );
    image_entry.pinned = true;
    save_entry(&storage, &image_entry).unwrap();

    let file_entry = ClipboardEntry::new(
        12,
        ClipboardContent::Files(vec!["C:\\tmp\\a.txt".to_string(), "D:\\b.png".to_string()]),
    );
    save_entry(&storage, &file_entry).unwrap();

    let loaded_settings = load_settings(&storage).unwrap();
    let loaded_history = load_history(&storage, 10).unwrap();
    let database_bytes = fs::read(storage.database_path().unwrap()).unwrap();
    assert_eq!(&database_bytes[..16], b"SQLite format 3\0");

    let schema_version = storage
        .with_connection(|connection| schema_version(connection))
        .unwrap();
    let has_image_rgba = storage
        .with_connection(|connection| column_exists(connection, "clipboard_entries", "image_rgba"))
        .unwrap();
    let has_image_blob = storage
        .with_connection(|connection| column_exists(connection, "clipboard_entries", "image_blob"))
        .unwrap();
    let has_image_format = storage
        .with_connection(|connection| {
            column_exists(connection, "clipboard_entries", "image_format")
        })
        .unwrap();
    let has_content_hash = storage
        .with_connection(|connection| {
            column_exists(connection, "clipboard_entries", "content_hash")
        })
        .unwrap();

    assert_eq!(loaded_settings, settings);
    assert_eq!(schema_version, SCHEMA_VERSION);
    assert!(!has_image_rgba);
    assert!(has_image_blob);
    assert!(has_image_format);
    assert!(has_content_hash);
    assert_eq!(loaded_history.counts().text, 1);
    assert_eq!(loaded_history.counts().image, 1);
    assert_eq!(loaded_history.counts().file, 1);
    assert!(loaded_history.entry(10).unwrap().favorite);
    assert!(loaded_history.entry(11).unwrap().pinned);
    assert!(matches!(
        &loaded_history.entry(10).unwrap().content,
        ClipboardContent::Text(text) if text == "hello"
    ));
    assert!(matches!(
        &loaded_history.entry(11).unwrap().content,
        ClipboardContent::Image(image) if !image.has_bytes()
    ));
    assert!(matches!(
        load_image(&storage, 11).unwrap(),
        Some(image) if image.rgba_bytes() == Some([10, 20, 30, 255].as_slice())
    ));
    let stored_image = storage
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT image_blob FROM clipboard_entries WHERE id = 11",
                    [],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(StorageError::from)
        })
        .unwrap();
    assert!(stored_image.starts_with(PNG_SIGNATURE));
    let preview_url = match &loaded_history.entry(11).unwrap().content {
        ClipboardContent::Image(image) => image.preview_url.clone().unwrap(),
        _ => unreachable!(),
    };
    assert!(preview_url.starts_with("file://"));
    assert!(image_cache::exists(Some(preview_url.as_str())));
    assert!(matches!(
        &loaded_history.entry(12).unwrap().content,
        ClipboardContent::Files(files) if files == &["C:\\tmp\\a.txt", "D:\\b.png"]
    ));

    let mut metadata_only_image = Arc::unwrap_or_clone(loaded_history.entry(11).unwrap());
    metadata_only_image.favorite = true;
    save_entry(&storage, &metadata_only_image).unwrap();
    assert!(matches!(
        load_image(&storage, 11).unwrap(),
        Some(image) if image.rgba_bytes() == Some([10, 20, 30, 255].as_slice())
    ));

    save_entry(
        &storage,
        &ClipboardEntry::new(13, ClipboardContent::Text("hello".to_string())),
    )
    .unwrap();
    let hello_count = storage
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT COUNT(*) FROM clipboard_entries WHERE text_content = 'hello'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(StorageError::from)
        })
        .unwrap();
    assert_eq!(hello_count, 1);

    delete_entries(&storage, &[10]).unwrap();
    assert!(load_history(&storage, 10).unwrap().entry(10).is_none());
    save_entry(&storage, &text_entry).unwrap();
    assert!(load_history(&storage, 10).unwrap().entry(10).is_none());

    delete_entries(&storage, &[11]).unwrap();
    assert!(!image_cache::exists(Some(preview_url.as_str())));

    *test_data_directory().lock().unwrap() = None;
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn age_cleanup_preserves_favorites_when_enabled() {
    let _guard = storage_test_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = unique_test_directory();
    let storage = StorageHandle::new_for_test(directory.clone());
    *test_data_directory().lock().unwrap() = Some(directory.clone());

    let captured_at = Local::now() - chrono::Duration::days(31);
    let mut regular = ClipboardEntry::new(20, ClipboardContent::Text("regular".to_string()));
    regular.captured_at = captured_at;
    save_entry(&storage, &regular).unwrap();

    let mut favorite = ClipboardEntry::new(21, ClipboardContent::Text("favorite".to_string()));
    favorite.captured_at = captured_at;
    favorite.favorite = true;
    save_entry(&storage, &favorite).unwrap();

    let removed =
        delete_entries_older_than(&storage, Local::now() - chrono::Duration::days(30), true)
            .unwrap();
    let history = load_history(&storage, 10).unwrap();

    assert_eq!(removed, 1);
    assert!(history.entry(20).is_none());
    assert!(history.entry(21).is_some());

    *test_data_directory().lock().unwrap() = None;
    let _ = fs::remove_dir_all(directory);
}

fn unique_test_directory() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!("ucp-storage-test-{}-{nanos}", std::process::id()))
}

// Holds the global directory override until all queued work has completed.
struct TestStorage {
    storage: StorageHandle,
    directory: PathBuf,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl TestStorage {
    fn new() -> Self {
        let guard = storage_test_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = unique_test_directory();
        Self {
            storage: StorageHandle::new_for_test(directory.clone()),
            directory,
            _guard: guard,
        }
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        *test_data_directory().lock().unwrap() = None;
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn text_entry(id: u64, text: &str) -> ClipboardEntry {
    ClipboardEntry::new(id, ClipboardContent::Text(text.into()))
}

#[test]
fn writer_fifo_preserves_rapid_favorites_captures_and_promotions() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    let writer = StorageWriter::new(storage.clone());
    let clone = writer.clone();
    let first = text_entry(100, "first");
    let promoted_at = Local::now() + chrono::Duration::hours(1);
    let save = writer.save(first, vec![]);
    let favorite = clone.set_favorites(vec![(100, true)]);
    let capture = writer.save(text_entry(101, "second"), vec![]);
    let unfavorite = writer.set_favorites(vec![(100, false)]);
    let promote = clone.promote(100, promoted_at);
    let refavorite = writer.set_favorites(vec![(100, true)]);
    // Await in reverse order: submission, not awaiting order, defines FIFO.
    refavorite.recv_blocking().unwrap().unwrap();
    promote.recv_blocking().unwrap().unwrap();
    unfavorite.recv_blocking().unwrap().unwrap();
    capture.recv_blocking().unwrap().unwrap();
    favorite.recv_blocking().unwrap().unwrap();
    save.recv_blocking().unwrap().unwrap();
    let entries = load_entries(storage).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, 100);
    assert!(entries[0].favorite);
    assert_eq!(
        entries[0].captured_at.timestamp_millis(),
        promoted_at.timestamp_millis()
    );

    let delete = writer.delete(vec![100]);
    let late_favorite = writer.set_favorites(vec![(100, true)]);
    let late_promote = writer.promote(100, Local::now());
    late_promote.recv_blocking().unwrap().unwrap();
    late_favorite.recv_blocking().unwrap().unwrap();
    delete.recv_blocking().unwrap().unwrap();
    assert_eq!(load_entries(storage).unwrap()[0].id, 101);
    drop(clone);
    drop(writer);
}

#[test]
fn save_merges_duplicate_under_submitted_id_and_delete_uses_that_id() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    let mut old = text_entry(1, "same content");
    old.favorite = true;
    old.pinned = true;
    save_entry(storage, &old).unwrap();
    // Exercise a collision with BOTH an existing target ID and a unique hash.
    save_entry(storage, &text_entry(2, "previous content")).unwrap();
    let writer = StorageWriter::new(storage.clone());
    writer
        .save(text_entry(2, "same content"), vec![])
        .recv_blocking()
        .unwrap()
        .unwrap();
    let entries = load_entries(storage).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, 2);
    assert!(entries[0].pinned && entries[0].favorite);
    writer.delete(vec![2]).recv_blocking().unwrap().unwrap();
    assert!(load_entries(storage).unwrap().is_empty());
}

#[test]
fn metadata_only_duplicate_image_retains_blob_and_preview() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    let mut original = ClipboardEntry::new(
        20,
        ClipboardContent::Image(ClipboardImage::from_rgba(1, 1, vec![1, 2, 3, 255])),
    );
    original.favorite = true;
    save_entry(storage, &original).unwrap();
    let mut metadata = load_entries(storage).unwrap().remove(0);
    metadata.id = 21;
    metadata.favorite = false;
    if let ClipboardContent::Image(image) = &mut metadata.content {
        image.preview_url = None;
    }
    let preview = save_entry(storage, &metadata).unwrap().unwrap();
    let entries = load_entries(storage).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, 21);
    assert!(entries[0].favorite);
    assert!(image_cache::exists(Some(&preview)));
    assert_eq!(
        load_image(storage, 21).unwrap().unwrap().rgba_bytes(),
        Some([1, 2, 3, 255].as_slice())
    );
    assert!(load_image(storage, 20).unwrap().is_none());

    // Missing preview metadata on the same ID must also retain the cache.
    save_entry(storage, &metadata).unwrap();
    assert!(image_cache::exists(Some(&preview)));
    let loaded = load_entries(storage).unwrap();
    assert!(matches!(
        &loaded[0].content,
        ClipboardContent::Image(image) if image.preview_url.as_deref() == Some(preview.as_str())
    ));
    delete_entries(storage, &[21]).unwrap();
    assert!(!image_cache::exists(Some(&preview)));
}

#[test]
fn capacity_prunes_database_before_recapture_delete_and_reload() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    let timestamp = Local::now();
    for id in 1..=5 {
        let mut entry = text_entry(id, &format!("text {id}"));
        entry.captured_at = timestamp;
        entry.pinned = id == 1;
        entry.favorite = id == 2;
        save_entry(storage, &entry).unwrap();
    }
    let loaded = load_entries_with_capacity(storage, 3).unwrap();
    assert_eq!(
        loaded.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![1, 5, 2]
    );
    assert_eq!(load_entries(storage).unwrap().len(), 3);

    // Recapture content pruned from SQLite under a new authoritative ID.
    let writer = StorageWriter::new(storage.clone());
    let capture = writer.save(text_entry(6, "text 3"), vec![5]);
    let delete = writer.delete(vec![6]);
    delete.recv_blocking().unwrap().unwrap();
    capture.recv_blocking().unwrap().unwrap();
    let reloaded = StorageHandle::new_for_test(fixture.directory.clone());
    let loaded = load_entries(&reloaded).unwrap();
    assert_eq!(
        loaded.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    // Protected rows may exceed even zero capacity.
    assert_eq!(load_entries_with_capacity(&reloaded, 0).unwrap().len(), 2);
}

#[test]
fn snapshot_delete_does_not_clear_a_later_capture() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    let writer = StorageWriter::new(storage.clone());
    let first = writer.save(text_entry(1, "before clear"), vec![]);
    let clear = writer.delete(vec![1]);
    let later = writer.save(text_entry(2, "after clear"), vec![]);
    later.recv_blocking().unwrap().unwrap();
    clear.recv_blocking().unwrap().unwrap();
    first.recv_blocking().unwrap().unwrap();
    let loaded = load_entries(storage).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, 2);
}

#[test]
fn failed_delete_does_not_cancel_an_earlier_queued_save() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    storage
        .with_connection(|connection| {
            connection.execute_batch(
                "CREATE TRIGGER reject_delete BEFORE DELETE ON clipboard_entries
             BEGIN SELECT RAISE(ABORT, 'delete rejected'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    let writer = StorageWriter::new(storage.clone());
    let save = writer.save(text_entry(1, "must be durable"), vec![]);
    let delete = writer.delete(vec![1]);
    assert!(delete.recv_blocking().unwrap().is_err());
    save.recv_blocking().unwrap().unwrap();
    storage.allow_entry_saves(&[1]);
    let loaded = load_entries(storage).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, 1);
}

#[test]
fn snapshot_delete_retains_same_content_recaptured_under_a_new_id() {
    let fixture = TestStorage::new();
    let writer = StorageWriter::new(fixture.storage.clone());
    let first = writer.save(text_entry(1, "same content"), vec![]);
    let clear = writer.delete(vec![1]);
    let recapture = writer.save(text_entry(2, "same content"), vec![]);
    recapture.recv_blocking().unwrap().unwrap();
    clear.recv_blocking().unwrap().unwrap();
    first.recv_blocking().unwrap().unwrap();
    let loaded = load_entries(&fixture.storage).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, 2);
}

#[test]
fn worker_drains_after_last_writer_drops_and_ignores_abandoned_replies() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    let writer = StorageWriter::new(storage.clone());
    drop(writer.save(text_entry(1, "abandoned reply"), vec![]));
    let settings = AppSettings {
        history_limit: 123,
        ..AppSettings::default()
    }
    .normalized();
    let saved_settings = writer.save_settings(settings.clone());
    let last = writer.save(text_entry(2, "last queued capture"), vec![]);
    drop(writer);
    last.recv_blocking().unwrap().unwrap();
    saved_settings.recv_blocking().unwrap().unwrap();
    assert_eq!(load_entries(storage).unwrap().len(), 2);
    assert_eq!(load_settings(storage).unwrap(), settings);
}

#[test]
fn writer_propagates_database_errors_and_continues_after_failure() {
    let fixture = TestStorage::new();
    let storage = &fixture.storage;
    save_entry(
        storage,
        &text_entry(9, "must not be evicted on save failure"),
    )
    .unwrap();
    storage
        .with_connection(|connection| {
            connection.execute_batch(
                "CREATE TRIGGER reject_capture BEFORE INSERT ON clipboard_entries \
                 BEGIN SELECT RAISE(ABORT, 'test rejection'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    let writer = StorageWriter::new(storage.clone());
    assert!(matches!(
        writer
            .save(text_entry(1, "fails"), vec![9])
            .recv_blocking()
            .unwrap(),
        Err(StorageError::Database(_))
    ));
    assert_eq!(load_entries(storage).unwrap()[0].id, 9);
    storage
        .with_connection(|connection| {
            connection.execute_batch(
                "DROP TRIGGER reject_capture; \
                 CREATE TRIGGER reject_delete BEFORE DELETE ON clipboard_entries \
                 BEGIN SELECT RAISE(ABORT, 'test eviction rejection'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        writer
            .save(text_entry(2, "saved before eviction fails"), vec![9])
            .recv_blocking()
            .unwrap(),
        Err(StorageError::Database(_))
    ));
    assert_eq!(load_entries(storage).unwrap().len(), 2);
    storage
        .with_connection(|connection| {
            connection.execute_batch("DROP TRIGGER reject_delete;")?;
            Ok(())
        })
        .unwrap();
    writer
        .save(text_entry(3, "works"), vec![9, 2])
        .recv_blocking()
        .unwrap()
        .unwrap();
    let loaded = load_entries(storage).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, 3);
}
