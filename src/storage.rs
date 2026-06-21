use crate::model::{
    AppLanguage, AppSettings, ClipboardContent, ClipboardEntry, ClipboardHistory, ClipboardImage,
};
use chrono::{DateTime, Local, TimeZone};
use rusqlite::{Connection, OptionalExtension, params};
use std::env;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(test)]
use std::sync::OnceLock;

const APP_DIR: &str = "UCP Clipboard";
const DATABASE_FILE: &str = "history.ucp";
const LEGACY_DATABASE_FILE: &str = "history.sqlite3";
const SCHEMA_VERSION: i32 = 1;
static DATABASE_CONNECTION: Mutex<Option<Connection>> = Mutex::new(None);

#[derive(Debug)]
pub enum StorageError {
    Io(String),
    Database(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) | Self::Database(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}

pub fn load_history(capacity: usize) -> Result<ClipboardHistory, StorageError> {
    with_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, kind, text_content, image_width, image_height, image_preview_url, \
                    captured_at_millis, pinned, favorite \
             FROM clipboard_entries \
             ORDER BY pinned DESC, captured_at_millis DESC, id DESC",
        )?;

        let entries = statement
            .query_map([], |row| {
                let id = row.get::<_, i64>(0)? as u64;
                let kind = row.get::<_, String>(1)?;
                let captured_at_millis = row.get::<_, i64>(6)?;
                let captured_at = Local
                    .timestamp_millis_opt(captured_at_millis)
                    .single()
                    .unwrap_or_else(Local::now);

                let content = match kind.as_str() {
                    "text" => {
                        ClipboardContent::Text(row.get::<_, Option<String>>(2)?.unwrap_or_default())
                    }
                    "image" => ClipboardContent::Image(ClipboardImage {
                        width: row.get::<_, Option<i64>>(3)?.unwrap_or_default().max(0) as usize,
                        height: row.get::<_, Option<i64>>(4)?.unwrap_or_default().max(0) as usize,
                        bytes: None,
                        preview_url: row.get(5)?,
                    }),
                    "file" => ClipboardContent::Files(load_files(connection, id)?),
                    _ => ClipboardContent::Text(String::new()),
                };

                Ok(ClipboardEntry {
                    id,
                    content,
                    captured_at,
                    pinned: row.get::<_, i64>(7)? != 0,
                    favorite: row.get::<_, i64>(8)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ClipboardHistory::from_entries(capacity, entries))
    })
}

pub fn load_image(entry_id: u64) -> Result<Option<ClipboardImage>, StorageError> {
    with_connection(|connection| {
        connection
            .query_row(
                "SELECT image_width, image_height, image_rgba, image_preview_url \
                 FROM clipboard_entries \
                 WHERE id = ?1 AND kind = 'image'",
                params![entry_id as i64],
                |row| {
                    let bytes = row.get::<_, Option<Vec<u8>>>(2)?.map(Arc::new);
                    Ok(ClipboardImage {
                        width: row.get::<_, Option<i64>>(0)?.unwrap_or_default().max(0) as usize,
                        height: row.get::<_, Option<i64>>(1)?.unwrap_or_default().max(0) as usize,
                        bytes,
                        preview_url: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    })
}

pub fn save_entry(entry: &ClipboardEntry) -> Result<(), StorageError> {
    with_connection(|connection| {
        let transaction = connection.transaction()?;

        let kind = entry.kind().key();
        let mut text_content: Option<&str> = None;
        let mut image_width: Option<i64> = None;
        let mut image_height: Option<i64> = None;
        let mut image_rgba: Option<&[u8]> = None;
        let mut image_preview_url: Option<&str> = None;

        match &entry.content {
            ClipboardContent::Text(text) => text_content = Some(text),
            ClipboardContent::Image(image) => {
                image_width = Some(image.width as i64);
                image_height = Some(image.height as i64);
                image_rgba = image.rgba_bytes();
                image_preview_url = image.preview_url.as_deref();
            }
            ClipboardContent::Files(_) => {}
        }

        transaction.execute(
            "INSERT INTO clipboard_entries (\
                 id, kind, text_content, image_width, image_height, image_rgba, image_preview_url, \
                 captured_at_millis, pinned, favorite\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(id) DO UPDATE SET \
                 kind = excluded.kind, \
                  text_content = excluded.text_content, \
                  image_width = excluded.image_width, \
                  image_height = excluded.image_height, \
                  image_rgba = COALESCE(excluded.image_rgba, clipboard_entries.image_rgba), \
                  image_preview_url = excluded.image_preview_url, \
                 captured_at_millis = excluded.captured_at_millis, \
                 pinned = excluded.pinned, \
                 favorite = excluded.favorite",
            params![
                entry.id as i64,
                kind,
                text_content,
                image_width,
                image_height,
                image_rgba,
                image_preview_url,
                entry.captured_at.timestamp_millis(),
                entry.pinned as i64,
                entry.favorite as i64,
            ],
        )?;

        transaction.execute(
            "DELETE FROM clipboard_files WHERE entry_id = ?1",
            params![entry.id as i64],
        )?;

        if let ClipboardContent::Files(files) = &entry.content {
            for (position, file) in files.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO clipboard_files (entry_id, position, path) VALUES (?1, ?2, ?3)",
                    params![entry.id as i64, position as i64, file],
                )?;
            }
        }

        transaction.commit()?;
        Ok(())
    })
}

pub fn delete_entry(id: u64) -> Result<(), StorageError> {
    delete_entries(&[id])
}

pub fn delete_entries(ids: &[u64]) -> Result<(), StorageError> {
    if ids.is_empty() {
        return Ok(());
    }

    with_connection(|connection| {
        let transaction = connection.transaction()?;

        for id in ids {
            transaction.execute(
                "DELETE FROM clipboard_entries WHERE id = ?1",
                params![*id as i64],
            )?;
        }

        transaction.commit()?;
        Ok(())
    })
}

pub fn clear_history() -> Result<(), StorageError> {
    with_connection(|connection| {
        connection.execute("DELETE FROM clipboard_entries", [])?;
        Ok(())
    })
}

pub fn delete_entries_older_than(cutoff: DateTime<Local>) -> Result<usize, StorageError> {
    with_connection(|connection| {
        connection
            .execute(
                "DELETE FROM clipboard_entries WHERE captured_at_millis < ?1",
                params![cutoff.timestamp_millis()],
            )
            .map_err(StorageError::from)
    })
}

pub fn load_settings() -> Result<AppSettings, StorageError> {
    with_connection(|connection| {
        let mut settings = AppSettings::default();
        let mut statement = connection.prepare("SELECT key, value FROM app_settings")?;

        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (key, value) = row?;
            match key.as_str() {
                "history_limit" => {
                    settings.history_limit = value
                        .parse::<usize>()
                        .unwrap_or(AppSettings::default().history_limit)
                }
                "auto_cleanup_days" => settings.auto_cleanup_days = parse_auto_cleanup_days(&value),
                "language" => settings.language = AppLanguage::from_key(&value),
                "launch_at_startup" => settings.launch_at_startup = parse_bool(&value),
                "desktop_widget" => settings.desktop_widget = parse_bool(&value),
                "keyboard_shortcuts" => settings.keyboard_shortcuts = parse_bool(&value),
                "auto_focus_history" => settings.auto_focus_history = parse_bool(&value),
                "promote_copied_entries" => settings.promote_copied_entries = parse_bool(&value),
                "quick_paste" => settings.quick_paste = parse_bool(&value),
                "show_copy_time" => settings.show_copy_time = parse_bool(&value),
                "show_text_length" => settings.show_text_length = parse_bool(&value),
                _ => {}
            }
        }

        Ok(settings.normalized())
    })
}

pub fn save_settings(settings: &AppSettings) -> Result<(), StorageError> {
    with_connection(|connection| {
        let transaction = connection.transaction()?;
        let values = [
            ("history_limit", settings.history_limit.to_string()),
            (
                "auto_cleanup_days",
                settings
                    .auto_cleanup_days
                    .map(|days| days.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ),
            ("language", settings.language.key().to_string()),
            ("launch_at_startup", settings.launch_at_startup.to_string()),
            ("desktop_widget", settings.desktop_widget.to_string()),
            (
                "keyboard_shortcuts",
                settings.keyboard_shortcuts.to_string(),
            ),
            (
                "auto_focus_history",
                settings.auto_focus_history.to_string(),
            ),
            (
                "promote_copied_entries",
                settings.promote_copied_entries.to_string(),
            ),
            ("quick_paste", settings.quick_paste.to_string()),
            ("show_copy_time", settings.show_copy_time.to_string()),
            ("show_text_length", settings.show_text_length.to_string()),
        ];

        for (key, value) in values {
            transaction.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }

        transaction.commit()?;
        Ok(())
    })
}

pub fn database_path() -> Result<PathBuf, StorageError> {
    let directory = data_directory();
    fs::create_dir_all(&directory)?;
    Ok(directory.join(DATABASE_FILE))
}

fn with_connection<T>(
    operation: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    let mut connection = DATABASE_CONNECTION
        .lock()
        .map_err(|_| StorageError::Database("数据库连接锁已损坏".to_string()))?;

    if connection.is_none() {
        *connection = Some(open_connection()?);
    }

    operation(
        connection
            .as_mut()
            .expect("database connection initialized"),
    )
}

fn open_connection() -> Result<Connection, StorageError> {
    let path = database_path()?;
    restore_plaintext_database_if_needed(&path)?;

    let connection = Connection::open(&path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&connection)?;
    Ok(connection)
}

fn restore_plaintext_database_if_needed(database_path: &Path) -> Result<(), StorageError> {
    if database_path.exists() && is_plaintext_sqlite(database_path)? {
        return Ok(());
    }

    if let Some(source_path) = plaintext_restore_source()? {
        if database_path.exists() {
            fs::rename(
                database_path,
                unique_backup_path(database_path, "sqlcipher-backup"),
            )?;
        }
        fs::copy(source_path, database_path)?;
        return Ok(());
    }

    if database_path.exists() {
        return Err(StorageError::Database(format!(
            "数据库文件不是普通 SQLite：{}。如果要回退 SQLCipher，请保留旧的 history.sqlite3 或 history.ucp.plaintext-backup 后再启动。",
            database_path.display()
        )));
    }

    Ok(())
}

fn plaintext_restore_source() -> Result<Option<PathBuf>, StorageError> {
    for file_name in [LEGACY_DATABASE_FILE, "history.ucp.plaintext-backup"] {
        let path = data_directory().join(file_name);
        if is_plaintext_sqlite(&path)? {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn is_plaintext_sqlite(path: &Path) -> Result<bool, StorageError> {
    if !path.exists() {
        return Ok(false);
    }

    let mut header = [0; 16];
    let mut file = fs::File::open(path)?;
    let bytes_read = file.read(&mut header)?;
    Ok(bytes_read == header.len() && header == *b"SQLite format 3\0")
}

fn unique_backup_path(path: &Path, suffix: &str) -> PathBuf {
    for index in 0.. {
        let extension = if index == 0 {
            format!("ucp.{suffix}")
        } else {
            format!("ucp.{suffix}-{index}")
        };
        let candidate = path.with_extension(extension);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!()
}

fn migrate(connection: &Connection) -> Result<(), StorageError> {
    let user_version = schema_version(connection)?;
    if user_version > SCHEMA_VERSION {
        return Err(StorageError::Database(format!(
            "数据库版本 {user_version} 高于当前程序支持的版本 {SCHEMA_VERSION}"
        )));
    }

    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS clipboard_entries (\
             id INTEGER PRIMARY KEY NOT NULL, \
             kind TEXT NOT NULL, \
             text_content TEXT, \
             image_width INTEGER, \
             image_height INTEGER, \
             image_rgba BLOB, \
             image_preview_url TEXT, \
             captured_at_millis INTEGER NOT NULL, \
             pinned INTEGER NOT NULL DEFAULT 0, \
             favorite INTEGER NOT NULL DEFAULT 0\
         );

         CREATE TABLE IF NOT EXISTS clipboard_files (\
             entry_id INTEGER NOT NULL, \
             position INTEGER NOT NULL, \
             path TEXT NOT NULL, \
             PRIMARY KEY (entry_id, position), \
             FOREIGN KEY (entry_id) REFERENCES clipboard_entries(id) ON DELETE CASCADE\
         );

         CREATE INDEX IF NOT EXISTS idx_clipboard_entries_order \
             ON clipboard_entries (pinned, captured_at_millis, id);

          CREATE INDEX IF NOT EXISTS idx_clipboard_files_entry \
              ON clipboard_files (entry_id, position);

           CREATE TABLE IF NOT EXISTS app_settings (\
               key TEXT PRIMARY KEY NOT NULL, \
               value TEXT NOT NULL\
           );",
    )?;
    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn schema_version(connection: &Connection) -> Result<i32, StorageError> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(StorageError::from)
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "true" | "1" | "yes" | "on")
}

fn parse_auto_cleanup_days(value: &str) -> Option<u16> {
    match value {
        "7" => Some(7),
        "30" => Some(30),
        "60" => Some(60),
        _ => None,
    }
}

fn load_files(connection: &Connection, entry_id: u64) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT path FROM clipboard_files WHERE entry_id = ?1 ORDER BY position ASC")?;

    statement
        .query_map(params![entry_id as i64], |row| row.get(0))?
        .collect()
}

fn data_directory() -> PathBuf {
    #[cfg(test)]
    if let Some(directory) = test_data_directory().lock().unwrap().clone() {
        return directory;
    }

    #[cfg(windows)]
    {
        env::var_os("LOCALAPPDATA")
            .or_else(|| env::var_os("APPDATA"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_DIR)
    }

    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Application Support")
            .join(APP_DIR)
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_DIR)
    }
}

#[cfg(test)]
fn test_data_directory() -> &'static Mutex<Option<PathBuf>> {
    static TEST_DATA_DIRECTORY: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    TEST_DATA_DIRECTORY.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn reset_storage_for_tests() {
    *DATABASE_CONNECTION.lock().unwrap() = None;
}

trait ClipboardKindKey {
    fn key(self) -> &'static str;
}

impl ClipboardKindKey for crate::model::ClipboardKind {
    fn key(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::File => "file",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipboardContent, ClipboardEntry, ClipboardImage};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn storage_round_trips_settings_and_clipboard_entries() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = unique_test_directory();
        reset_storage_for_tests();
        *test_data_directory().lock().unwrap() = Some(directory.clone());

        let settings = AppSettings {
            history_limit: 50,
            auto_cleanup_days: Some(30),
            language: AppLanguage::English,
            launch_at_startup: true,
            keyboard_shortcuts: false,
            auto_focus_history: false,
            promote_copied_entries: false,
            quick_paste: true,
            show_copy_time: false,
            show_text_length: false,
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

        assert_eq!(loaded_settings, settings);
        assert_eq!(schema_version, SCHEMA_VERSION);
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

        delete_entry(10).unwrap();
        assert!(load_history(10).unwrap().entry(10).is_none());

        reset_storage_for_tests();
        *test_data_directory().lock().unwrap() = None;
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn legacy_plaintext_database_restores_ucp() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = unique_test_directory();
        reset_storage_for_tests();
        fs::create_dir_all(&directory).unwrap();
        *test_data_directory().lock().unwrap() = Some(directory.clone());

        let legacy_path = directory.join(LEGACY_DATABASE_FILE);
        let legacy_connection = Connection::open(&legacy_path).unwrap();
        legacy_connection
            .execute_batch(
                "CREATE TABLE app_settings (
                    key TEXT PRIMARY KEY NOT NULL,
                    value TEXT NOT NULL
                );
                INSERT INTO app_settings (key, value) VALUES ('history_limit', '100');
                PRAGMA user_version = 1;",
            )
            .unwrap();
        legacy_connection.close().unwrap();

        let settings = load_settings().unwrap();
        let database_bytes = fs::read(database_path().unwrap()).unwrap();

        assert_eq!(settings.history_limit, 100);
        assert_eq!(&database_bytes[..16], b"SQLite format 3\0");
        assert!(legacy_path.exists());

        reset_storage_for_tests();
        *test_data_directory().lock().unwrap() = None;
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn unreadable_ucp_restores_from_legacy_plaintext_database() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = unique_test_directory();
        reset_storage_for_tests();
        fs::create_dir_all(&directory).unwrap();
        *test_data_directory().lock().unwrap() = Some(directory.clone());

        let legacy_path = directory.join(LEGACY_DATABASE_FILE);
        let legacy_connection = Connection::open(&legacy_path).unwrap();
        legacy_connection
            .execute_batch(
                "CREATE TABLE app_settings (
                    key TEXT PRIMARY KEY NOT NULL,
                    value TEXT NOT NULL
                );
                INSERT INTO app_settings (key, value) VALUES ('history_limit', '500');
                PRAGMA user_version = 1;",
            )
            .unwrap();
        legacy_connection.close().unwrap();
        fs::write(directory.join(DATABASE_FILE), b"not a sqlite database").unwrap();

        let settings = load_settings().unwrap();
        let database_bytes = fs::read(database_path().unwrap()).unwrap();

        assert_eq!(settings.history_limit, 500);
        assert_eq!(&database_bytes[..16], b"SQLite format 3\0");
        assert!(directory.join("history.ucp.sqlcipher-backup").exists());

        reset_storage_for_tests();
        *test_data_directory().lock().unwrap() = None;
        let _ = fs::remove_dir_all(directory);
    }

    fn test_lock() -> &'static Mutex<()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn unique_test_directory() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("ucp-storage-test-{}-{nanos}", std::process::id()))
    }
}
