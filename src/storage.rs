use crate::model::{
    AppLanguage, AppSettings, AppTheme, ClipboardContent, ClipboardEntry, ClipboardHistory,
    ClipboardImage,
};
use chrono::{DateTime, Local, TimeZone};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(test)]
use std::sync::OnceLock;

const APP_DIR: &str = "UCP Clipboard";
const DATABASE_FILE: &str = "history.ucp";
const SCHEMA_VERSION: i32 = 3;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const IMAGE_FORMAT_PNG: &str = "png";
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
                "SELECT image_width, image_height, image_blob, image_preview_url \
                 FROM clipboard_entries \
                 WHERE id = ?1 AND kind = 'image'",
                params![entry_id as i64],
                |row| {
                    let width = row.get::<_, Option<i64>>(0)?.unwrap_or_default().max(0) as usize;
                    let height = row.get::<_, Option<i64>>(1)?.unwrap_or_default().max(0) as usize;
                    let preview_url = row.get(3)?;

                    Ok(row
                        .get::<_, Option<Vec<u8>>>(2)?
                        .and_then(|bytes| {
                            ClipboardImage::from_stored_bytes(width, height, bytes, preview_url)
                        })
                        .unwrap_or(ClipboardImage {
                            width,
                            height,
                            bytes: None,
                            preview_url: None,
                        }))
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
        let mut image_blob: Option<Vec<u8>> = None;
        let mut image_format: Option<&str> = None;
        let mut image_preview_url: Option<&str> = None;

        match &entry.content {
            ClipboardContent::Text(text) => text_content = Some(text),
            ClipboardContent::Image(image) => {
                image_width = Some(image.width as i64);
                image_height = Some(image.height as i64);
                image_blob = image.stored_bytes();
                image_format = image_blob.as_ref().map(|_| IMAGE_FORMAT_PNG);
                image_preview_url = image.preview_url.as_deref();
            }
            ClipboardContent::Files(_) => {}
        }
        let content_hash = content_hash_for_entry(entry, image_blob.as_deref());
        let database_id = if let Some(hash) = content_hash.as_deref() {
            transaction
                .query_row(
                    "SELECT id FROM clipboard_entries WHERE content_hash = ?1 AND id <> ?2",
                    params![hash, entry.id as i64],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(entry.id as i64)
        } else {
            entry.id as i64
        };
        let merge_duplicate_metadata = (database_id != entry.id as i64) as i64;

        transaction.execute(
            "INSERT INTO clipboard_entries (\
                 id, kind, text_content, image_width, image_height, image_blob, image_format, \
                 image_preview_url, captured_at_millis, pinned, favorite, content_hash\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
             ON CONFLICT(id) DO UPDATE SET \
                 kind = excluded.kind, \
                   text_content = excluded.text_content, \
                   image_width = excluded.image_width, \
                   image_height = excluded.image_height, \
                   image_blob = COALESCE(excluded.image_blob, clipboard_entries.image_blob), \
                   image_format = COALESCE(excluded.image_format, clipboard_entries.image_format), \
                   image_preview_url = excluded.image_preview_url, \
                  captured_at_millis = excluded.captured_at_millis, \
                  pinned = CASE \
                      WHEN ?13 != 0 THEN clipboard_entries.pinned OR excluded.pinned \
                      ELSE excluded.pinned \
                  END, \
                  favorite = CASE \
                      WHEN ?13 != 0 THEN clipboard_entries.favorite OR excluded.favorite \
                      ELSE excluded.favorite \
                  END, \
                  content_hash = COALESCE(excluded.content_hash, clipboard_entries.content_hash)",
            params![
                database_id,
                kind,
                text_content,
                image_width,
                image_height,
                image_blob.as_deref(),
                image_format,
                image_preview_url,
                entry.captured_at.timestamp_millis(),
                entry.pinned as i64,
                entry.favorite as i64,
                content_hash,
                merge_duplicate_metadata,
            ],
        )?;

        transaction.execute(
            "DELETE FROM clipboard_files WHERE entry_id = ?1",
            params![database_id],
        )?;

        if let ClipboardContent::Files(files) = &entry.content {
            for (position, file) in files.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO clipboard_files (entry_id, position, path) VALUES (?1, ?2, ?3)",
                    params![database_id, position as i64, file],
                )?;
            }
        }

        if database_id != entry.id as i64 {
            transaction.execute(
                "DELETE FROM clipboard_entries WHERE id = ?1",
                params![entry.id as i64],
            )?;
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

pub fn compact_database() -> Result<(), StorageError> {
    with_connection(|connection| {
        ensure_current_schema_columns(connection)?;
        compress_stored_images(connection)?;
        populate_content_hashes(connection)?;
        deduplicate_entries(connection)?;
        create_content_hash_index(connection)?;
        connection.execute_batch("VACUUM;")?;
        Ok(())
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
                "theme" => settings.theme = AppTheme::from_key(&value),
                "launch_at_startup" => settings.launch_at_startup = parse_bool(&value),
                "desktop_widget" => settings.desktop_widget = parse_bool(&value),
                "desktop_widget_topmost" => settings.desktop_widget_topmost = parse_bool(&value),
                "keyboard_shortcuts" => settings.keyboard_shortcuts = parse_bool(&value),
                "global_show_shortcut" => settings.global_show_shortcut = value,
                "auto_focus_history" => settings.auto_focus_history = parse_bool(&value),
                "promote_copied_entries" => settings.promote_copied_entries = parse_bool(&value),
                "quick_paste" => settings.quick_paste = parse_bool(&value),
                "show_copy_time" => settings.show_copy_time = parse_bool(&value),
                "show_text_length" => settings.show_text_length = parse_bool(&value),
                "background_opacity" => {
                    settings.background_opacity = value
                        .parse::<u8>()
                        .unwrap_or(AppSettings::default().background_opacity)
                }
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
            ("theme", settings.theme.key().to_string()),
            ("launch_at_startup", settings.launch_at_startup.to_string()),
            ("desktop_widget", settings.desktop_widget.to_string()),
            (
                "desktop_widget_topmost",
                settings.desktop_widget_topmost.to_string(),
            ),
            (
                "keyboard_shortcuts",
                settings.keyboard_shortcuts.to_string(),
            ),
            (
                "global_show_shortcut",
                settings.global_show_shortcut.clone(),
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
            (
                "background_opacity",
                settings.background_opacity.to_string(),
            ),
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
    let connection = Connection::open(&path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&connection)?;
    Ok(connection)
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
             image_blob BLOB, \
             image_format TEXT, \
             image_preview_url TEXT, \
             captured_at_millis INTEGER NOT NULL, \
             pinned INTEGER NOT NULL DEFAULT 0, \
             favorite INTEGER NOT NULL DEFAULT 0, \
             content_hash TEXT\
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

    ensure_current_schema_columns(connection)?;
    connection.execute(
        "UPDATE clipboard_entries \
         SET image_format = ?1 \
         WHERE kind = 'image' AND image_blob IS NOT NULL AND image_format IS NULL",
        params![IMAGE_FORMAT_PNG],
    )?;
    compress_stored_images(connection)?;
    populate_content_hashes(connection)?;
    deduplicate_entries(connection)?;
    create_content_hash_index(connection)?;

    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn ensure_current_schema_columns(connection: &Connection) -> Result<(), StorageError> {
    if column_exists(connection, "clipboard_entries", "image_rgba")?
        && !column_exists(connection, "clipboard_entries", "image_blob")?
    {
        connection.execute_batch(
            "ALTER TABLE clipboard_entries RENAME COLUMN image_rgba TO image_blob;",
        )?;
    }

    if !column_exists(connection, "clipboard_entries", "image_format")? {
        connection.execute_batch("ALTER TABLE clipboard_entries ADD COLUMN image_format TEXT;")?;
    }

    if !column_exists(connection, "clipboard_entries", "content_hash")? {
        connection.execute_batch("ALTER TABLE clipboard_entries ADD COLUMN content_hash TEXT;")?;
    }

    Ok(())
}

fn populate_content_hashes(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, kind, text_content, image_blob \
         FROM clipboard_entries \
         WHERE content_hash IS NULL",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for (id, kind, text, image_blob) in rows {
        let files = if kind == "file" {
            load_files(connection, id as u64)?
        } else {
            Vec::new()
        };
        let Some(hash) = content_hash_from_parts(
            &kind,
            text.as_deref(),
            image_blob.as_deref(),
            files.as_slice(),
        ) else {
            continue;
        };

        connection.execute(
            "UPDATE clipboard_entries SET content_hash = ?2 WHERE id = ?1",
            params![id, hash],
        )?;
    }

    Ok(())
}

fn deduplicate_entries(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare(
        "SELECT content_hash \
         FROM clipboard_entries \
         WHERE content_hash IS NOT NULL \
         GROUP BY content_hash \
         HAVING COUNT(*) > 1",
    )?;
    let hashes = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for hash in hashes {
        let mut rows_statement = connection.prepare(
            "SELECT id, pinned, favorite, captured_at_millis \
             FROM clipboard_entries \
             WHERE content_hash = ?1 \
             ORDER BY pinned DESC, favorite DESC, captured_at_millis DESC, id DESC",
        )?;
        let rows = rows_statement
            .query_map(params![hash], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(rows_statement);

        let Some((keep_id, _, _, _)) = rows.first().copied() else {
            continue;
        };
        let pinned = rows.iter().any(|(_, pinned, _, _)| *pinned != 0) as i64;
        let favorite = rows.iter().any(|(_, _, favorite, _)| *favorite != 0) as i64;
        let captured_at_millis = rows
            .iter()
            .map(|(_, _, _, captured_at_millis)| *captured_at_millis)
            .max()
            .unwrap_or_default();

        connection.execute(
            "UPDATE clipboard_entries \
             SET pinned = ?2, favorite = ?3, captured_at_millis = ?4 \
             WHERE id = ?1",
            params![keep_id, pinned, favorite, captured_at_millis],
        )?;

        for (id, _, _, _) in rows.into_iter().skip(1) {
            connection.execute("DELETE FROM clipboard_entries WHERE id = ?1", params![id])?;
        }
    }

    Ok(())
}

fn create_content_hash_index(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_entries_content_hash \
             ON clipboard_entries(content_hash) \
             WHERE content_hash IS NOT NULL;",
    )?;
    Ok(())
}

fn compress_stored_images(connection: &Connection) -> Result<(), StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, image_width, image_height, image_blob, image_preview_url \
         FROM clipboard_entries \
         WHERE kind = 'image' AND image_blob IS NOT NULL",
    )?;

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or_default().max(0) as usize,
                row.get::<_, Option<i64>>(2)?.unwrap_or_default().max(0) as usize,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for (id, width, height, bytes, preview_url) in rows {
        if bytes.starts_with(PNG_SIGNATURE) {
            continue;
        }

        let Some(image) = ClipboardImage::from_stored_bytes(width, height, bytes, preview_url)
        else {
            continue;
        };
        let Some(compressed) = image.to_png_bytes() else {
            continue;
        };

        connection.execute(
            "UPDATE clipboard_entries \
             SET image_width = ?2, image_height = ?3, image_blob = ?4, image_format = ?5 \
             WHERE id = ?1",
            params![
                id,
                image.width as i64,
                image.height as i64,
                compressed,
                IMAGE_FORMAT_PNG,
            ],
        )?;
    }

    Ok(())
}

fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool, StorageError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

fn content_hash_for_entry(entry: &ClipboardEntry, image_blob: Option<&[u8]>) -> Option<String> {
    match &entry.content {
        ClipboardContent::Text(text) => content_hash_from_parts("text", Some(text), None, &[]),
        ClipboardContent::Image(_) => content_hash_from_parts("image", None, image_blob, &[]),
        ClipboardContent::Files(files) => content_hash_from_parts("file", None, None, files),
    }
}

fn content_hash_from_parts(
    kind: &str,
    text: Option<&str>,
    image_blob: Option<&[u8]>,
    files: &[String],
) -> Option<String> {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, kind.as_bytes());

    match kind {
        "text" => hash_part(&mut hasher, text?.as_bytes()),
        "image" => hash_part(&mut hasher, image_blob?),
        "file" => {
            if files.is_empty() {
                return None;
            }
            for file in files {
                hash_part(&mut hasher, file.as_bytes());
            }
        }
        _ => return None,
    }

    Some(format!("{:x}", hasher.finalize()))
}

fn hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(bytes.len().to_le_bytes());
    hasher.update(bytes);
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
    use crate::model::{
        AppTheme, ClipboardContent, ClipboardEntry, ClipboardImage, DEFAULT_BACKGROUND_OPACITY,
    };
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
            theme: AppTheme::Dark,
            launch_at_startup: true,
            desktop_widget: false,
            desktop_widget_topmost: false,
            keyboard_shortcuts: false,
            global_show_shortcut: "Ctrl+Alt+Space".to_string(),
            auto_focus_history: false,
            promote_copied_entries: false,
            quick_paste: true,
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
        let has_image_rgba = with_connection(|connection| {
            column_exists(connection, "clipboard_entries", "image_rgba")
        })
        .unwrap();
        let has_image_blob = with_connection(|connection| {
            column_exists(connection, "clipboard_entries", "image_blob")
        })
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

        delete_entry(10).unwrap();
        assert!(load_history(10).unwrap().entry(10).is_none());

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
