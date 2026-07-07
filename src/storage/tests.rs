use super::*;
use crate::model::{
    AppLanguage, AppSettings, AppTheme, ClipboardContent, ClipboardEntry, ClipboardImage,
    DEFAULT_BACKGROUND_OPACITY,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn storage_round_trips_settings_and_clipboard_entries() {
    let _guard = storage_test_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = unique_test_directory();
    reset_storage_for_tests();
    *test_data_directory().lock().unwrap() = Some(directory.clone());

    let settings = AppSettings {
        history_limit: 50,
        auto_cleanup_days: Some(30),
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
        hide_after_copy: true,
        show_copy_time: false,
        show_text_length: false,
        background_opacity: DEFAULT_BACKGROUND_OPACITY,
    };
    save_settings(&settings).unwrap();

    let mut text_entry = ClipboardEntry::new(10, ClipboardContent::Text("hello".to_string()));
    text_entry.favorite = true;
    save_entry(&text_entry).unwrap();

    let mut image_entry = ClipboardEntry::new(
        11,
        ClipboardContent::Image(ClipboardImage::from_rgba(1, 1, vec![10, 20, 30, 255])),
    );
    image_entry.pinned = true;
    save_entry(&image_entry).unwrap();

    let file_entry = ClipboardEntry::new(
        12,
        ClipboardContent::Files(vec!["C:\\tmp\\a.txt".to_string(), "D:\\b.png".to_string()]),
    );
    save_entry(&file_entry).unwrap();

    let loaded_settings = load_settings().unwrap();
    let loaded_history = load_history(10).unwrap();
    let database_bytes = fs::read(database_path().unwrap()).unwrap();
    assert_eq!(&database_bytes[..16], b"SQLite format 3\0");

    let schema_version = with_connection(|connection| schema_version(connection)).unwrap();
    let has_image_rgba =
        with_connection(|connection| column_exists(connection, "clipboard_entries", "image_rgba"))
            .unwrap();
    let has_image_blob =
        with_connection(|connection| column_exists(connection, "clipboard_entries", "image_blob"))
            .unwrap();
    let has_image_format = with_connection(|connection| {
        column_exists(connection, "clipboard_entries", "image_format")
    })
    .unwrap();
    let has_content_hash = with_connection(|connection| {
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
        load_image(11).unwrap(),
        Some(image) if image.rgba_bytes() == Some([10, 20, 30, 255].as_slice())
    ));
    let stored_image = with_connection(|connection| {
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

    let mut metadata_only_image = loaded_history.entry(11).unwrap().clone();
    metadata_only_image.favorite = true;
    save_entry(&metadata_only_image).unwrap();
    assert!(matches!(
        load_image(11).unwrap(),
        Some(image) if image.rgba_bytes() == Some([10, 20, 30, 255].as_slice())
    ));

    save_entry(&ClipboardEntry::new(
        13,
        ClipboardContent::Text("hello".to_string()),
    ))
    .unwrap();
    let hello_count = with_connection(|connection| {
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

    delete_entries(&[10]).unwrap();
    assert!(load_history(10).unwrap().entry(10).is_none());

    delete_entries(&[11]).unwrap();
    assert!(!image_cache::exists(Some(preview_url.as_str())));

    reset_storage_for_tests();
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
