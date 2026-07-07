mod image_cache;
mod schema;
mod settings;
#[cfg(test)]
mod tests;

#[cfg(test)]
use schema::{column_exists, schema_version};
pub use settings::{load_settings, save_settings};

use crate::model::{ClipboardContent, ClipboardEntry, ClipboardHistory, ClipboardImage};
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
        let cached_image_preview_url = match &entry.content {
            ClipboardContent::Image(image) if image.has_bytes() => {
                image_cache::write_preview(database_id as u64, image)?
            }
            _ => None,
        };
        if let Some(url) = cached_image_preview_url.as_deref() {
            image_preview_url = Some(url);
        }

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

        let removed_preview_urls = if database_id != entry.id as i64 {
            image_preview_urls_for_ids(&transaction, &[entry.id])?
        } else {
            Vec::new()
        };

        if database_id != entry.id as i64 {
            transaction.execute(
                "DELETE FROM clipboard_entries WHERE id = ?1",
                params![entry.id as i64],
            )?;
        }

        transaction.commit()?;
        image_cache::remove_previews(removed_preview_urls);
        Ok(())
    })
}

pub fn delete_entries(ids: &[u64]) -> Result<(), StorageError> {
    if ids.is_empty() {
        return Ok(());
    }

    with_connection(|connection| {
        let transaction = connection.transaction()?;
        let preview_urls = image_preview_urls_for_ids(&transaction, ids)?;

        for id in ids {
            transaction.execute(
                "DELETE FROM clipboard_entries WHERE id = ?1",
                params![*id as i64],
            )?;
        }

        transaction.commit()?;
        image_cache::remove_previews(preview_urls);
        Ok(())
    })
}

pub fn clear_history() -> Result<(), StorageError> {
    with_connection(|connection| {
        let preview_urls = all_image_preview_urls(connection)?;
        connection.execute("DELETE FROM clipboard_entries", [])?;
        image_cache::remove_previews(preview_urls);
        Ok(())
    })
}

pub fn delete_entries_older_than(cutoff: DateTime<Local>) -> Result<usize, StorageError> {
    with_connection(|connection| {
        let preview_urls = image_preview_urls_older_than(connection, cutoff)?;
        let removed = connection.execute(
            "DELETE FROM clipboard_entries WHERE captured_at_millis < ?1",
            params![cutoff.timestamp_millis()],
        )?;
        image_cache::remove_previews(preview_urls);
        Ok(removed)
    })
}

pub fn compact_database() -> Result<(), StorageError> {
    with_connection(|connection| {
        schema::ensure_current_schema_columns(connection)?;
        schema::compress_stored_images(connection)?;
        schema::materialize_cached_image_previews(connection)?;
        schema::populate_content_hashes(connection)?;
        schema::deduplicate_entries(connection)?;
        schema::create_content_hash_index(connection)?;
        connection.execute_batch("VACUUM;")?;
        Ok(())
    })
}

pub fn database_path() -> Result<PathBuf, StorageError> {
    let directory = data_directory();
    fs::create_dir_all(&directory)?;
    Ok(directory.join(DATABASE_FILE))
}

fn image_preview_urls_for_ids(
    connection: &Connection,
    ids: &[u64],
) -> Result<Vec<String>, StorageError> {
    let mut urls = Vec::new();
    for id in ids {
        if let Some(url) = connection
            .query_row(
                "SELECT image_preview_url FROM clipboard_entries WHERE id = ?1 AND kind = 'image'",
                params![*id as i64],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
        {
            urls.push(url);
        }
    }

    Ok(urls)
}

fn all_image_preview_urls(connection: &Connection) -> Result<Vec<String>, StorageError> {
    image_preview_urls_matching(
        connection,
        "SELECT image_preview_url FROM clipboard_entries WHERE kind = 'image' AND image_preview_url IS NOT NULL",
        [],
    )
}

fn image_preview_urls_older_than(
    connection: &Connection,
    cutoff: DateTime<Local>,
) -> Result<Vec<String>, StorageError> {
    image_preview_urls_matching(
        connection,
        "SELECT image_preview_url FROM clipboard_entries WHERE kind = 'image' AND image_preview_url IS NOT NULL AND captured_at_millis < ?1",
        params![cutoff.timestamp_millis()],
    )
}

fn image_preview_urls_matching<P>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<String>, StorageError>
where
    P: rusqlite::Params,
{
    let mut statement = connection.prepare(sql)?;
    statement
        .query_map(params, |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
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
    schema::migrate(&connection)?;
    Ok(connection)
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

#[cfg(test)]
fn storage_test_lock() -> &'static Mutex<()> {
    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    TEST_LOCK.get_or_init(|| Mutex::new(()))
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
