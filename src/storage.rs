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
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;

#[cfg(test)]
use std::sync::OnceLock;

const APP_DIR: &str = "UCP";
const DATABASE_FILE: &str = "history.ucp";
const SCHEMA_VERSION: i32 = 3;
const BUSY_TIMEOUT_MS: u64 = 3_000;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const IMAGE_FORMAT_PNG: &str = "png";

/// Handle to the storage layer, encapsulating database connection and pending deletes.
/// This replaces the global mutable state pattern with a per-instance handle.
#[derive(Clone)]
pub struct StorageHandle {
    connection: std::sync::Arc<Mutex<Connection>>,
    pending_deletes: std::sync::Arc<Mutex<HashSet<u64>>>,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database connection lock is poisoned")]
    ConnectionLock,
    #[error("database version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: i32, supported: i32 },
}

impl StorageHandle {
    /// Creates a new storage handle with an open database connection.
    pub fn new() -> Result<Self, StorageError> {
        let connection = open_connection()?;
        Ok(Self {
            connection: std::sync::Arc::new(Mutex::new(connection)),
            pending_deletes: std::sync::Arc::new(Mutex::new(HashSet::new())),
        })
    }

    #[cfg(test)]
    pub fn new_for_test(directory: PathBuf) -> Self {
        *test_data_directory().lock().unwrap() = Some(directory.clone());
        let connection = open_connection().expect("Failed to open test database");
        Self {
            connection: std::sync::Arc::new(Mutex::new(connection)),
            pending_deletes: std::sync::Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Marks entries as pending deletion to prevent race conditions during save.
    pub fn suppress_entry_saves(&self, ids: &[u64]) {
        let mut pending = self
            .pending_deletes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        pending.extend(ids.iter().copied());
    }

    /// Removes entries from pending deletion list.
    pub fn allow_entry_saves(&self, ids: &[u64]) {
        let mut pending = self
            .pending_deletes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for id in ids {
            pending.remove(id);
        }
    }

    /// Checks if an entry is marked for deletion.
    fn is_pending_delete(&self, id: u64) -> bool {
        self.pending_deletes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(&id)
    }

    /// Executes an operation with the database connection.
    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StorageError::ConnectionLock)?;
        operation(&mut connection)
    }

    #[cfg(test)]
    pub fn database_path(&self) -> Result<PathBuf, StorageError> {
        database_path()
    }
}

pub fn load_history(
    storage: &StorageHandle,
    capacity: usize,
) -> Result<ClipboardHistory, StorageError> {
    storage.with_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT e.id, e.kind, e.text_content, e.image_width, e.image_height, e.image_preview_url, e.content_hash, \
                    e.captured_at_millis, e.pinned, e.favorite, \
                    GROUP_CONCAT(f.path, '\x1F') as file_paths \
             FROM clipboard_entries e \
             LEFT JOIN clipboard_files f ON e.id = f.entry_id \
             GROUP BY e.id \
             ORDER BY e.pinned DESC, e.captured_at_millis DESC, e.id DESC",
        )?;

        let entries = statement
            .query_map([], |row| {
                let id = row.get::<_, i64>(0)? as u64;
                let kind = row.get::<_, String>(1)?;
                let captured_at_millis = row.get::<_, i64>(7)?;
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
                        content_hash: row.get(6)?,
                    }),
                    "file" => {
                        let file_paths = row
                            .get::<_, Option<String>>(10)?
                            .map(|paths| paths.split('\x1F').map(String::from).collect())
                            .unwrap_or_default();
                        ClipboardContent::Files(file_paths)
                    }
                    _ => ClipboardContent::Text(String::new()),
                };

                Ok(ClipboardEntry {
                    id,
                    content,
                    captured_at,
                    pinned: row.get::<_, i64>(8)? != 0,
                    favorite: row.get::<_, i64>(9)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ClipboardHistory::from_entries(capacity, entries))
    })
}

pub fn load_image(
    storage: &StorageHandle,
    entry_id: u64,
) -> Result<Option<ClipboardImage>, StorageError> {
    storage.with_connection(|connection| {
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
                            content_hash: None,
                        }))
                },
            )
            .optional()
            .map_err(StorageError::from)
    })
}

pub fn image_preview_path(preview_url: Option<&str>) -> Option<PathBuf> {
    image_cache::path(preview_url)
}

/// Saves the entry and returns the cached `file://` preview URL when an image
/// preview was written to the on-disk cache.
pub fn save_entry(
    storage: &StorageHandle,
    entry: &ClipboardEntry,
) -> Result<Option<String>, StorageError> {
    if storage.is_pending_delete(entry.id) {
        return Ok(None);
    }

    let kind = entry.kind().key();
    let mut text_content: Option<&str> = None;
    let mut image_width: Option<i64> = None;
    let mut image_height: Option<i64> = None;
    let mut image_blob: Option<Vec<u8>> = None;
    let mut image_format: Option<&str> = None;
    let mut image_preview_url: Option<String> = None;

    match &entry.content {
        ClipboardContent::Text(text) => text_content = Some(text),
        ClipboardContent::Image(image) => {
            image_width = Some(image.width as i64);
            image_height = Some(image.height as i64);
            image_blob = image.stored_bytes();
            image_format = image_blob.as_ref().map(|_| IMAGE_FORMAT_PNG);
            image_preview_url = image.preview_url.clone();
        }
        ClipboardContent::Files(_) => {}
    }
    let content_hash = content_hash_for_entry(entry, image_blob.as_deref());

    storage.with_connection(|connection| {
        if storage.is_pending_delete(entry.id) {
            return Ok(None);
        }

        let database_id = if let Some(hash) = content_hash.as_deref() {
            connection
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
        if let Some(url) = cached_image_preview_url.clone() {
            image_preview_url = Some(url);
        }

        let transaction = connection.transaction()?;
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
                image_preview_url.as_deref(),
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
        Ok(cached_image_preview_url)
    })
}

pub fn delete_entries(storage: &StorageHandle, ids: &[u64]) -> Result<(), StorageError> {
    if ids.is_empty() {
        return Ok(());
    }

    storage.suppress_entry_saves(ids);
    storage.with_connection(|connection| {
        let transaction = connection.transaction()?;
        let preview_urls = image_preview_urls_for_ids(&transaction, ids)?;

        let database_ids = database_ids(ids);
        let placeholders = sql_placeholders(database_ids.len());
        transaction.execute(
            &format!("DELETE FROM clipboard_entries WHERE id IN ({placeholders})"),
            params_from_iter(database_ids),
        )?;

        transaction.commit()?;
        image_cache::remove_previews(preview_urls);
        Ok(())
    })
}

pub fn clear_history(storage: &StorageHandle) -> Result<(), StorageError> {
    storage.with_connection(|connection| {
        let preview_urls = all_image_preview_urls(connection)?;
        connection.execute("DELETE FROM clipboard_entries", [])?;
        image_cache::remove_previews(preview_urls);
        Ok(())
    })
}

pub fn delete_entries_older_than(
    storage: &StorageHandle,
    cutoff: DateTime<Local>,
    preserve_favorites: bool,
) -> Result<usize, StorageError> {
    storage.with_connection(|connection| {
        let preview_urls = image_preview_urls_older_than(connection, cutoff, preserve_favorites)?;
        let removed = connection.execute(
            "DELETE FROM clipboard_entries \
             WHERE captured_at_millis < ?1 AND (?2 = 0 OR favorite = 0)",
            params![cutoff.timestamp_millis(), preserve_favorites as i64],
        )?;
        image_cache::remove_previews(preview_urls);
        Ok(removed)
    })
}

pub fn compact_database(storage: &StorageHandle) -> Result<(), StorageError> {
    storage.with_connection(|connection| {
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
    let db_path = directory.join(DATABASE_FILE);

    // Set restrictive permissions on the database file (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if db_path.exists() {
            let permissions = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&db_path, permissions)?;
        }
    }

    Ok(db_path)
}

fn image_preview_urls_for_ids(
    connection: &Connection,
    ids: &[u64],
) -> Result<Vec<String>, StorageError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let database_ids = database_ids(ids);
    let placeholders = sql_placeholders(database_ids.len());
    image_preview_urls_matching(
        connection,
        &format!(
            "SELECT image_preview_url \
             FROM clipboard_entries \
             WHERE kind = 'image' \
               AND image_preview_url IS NOT NULL \
               AND id IN ({placeholders})"
        ),
        params_from_iter(database_ids),
    )
}

fn database_ids(ids: &[u64]) -> Vec<i64> {
    ids.iter().map(|id| *id as i64).collect()
}

fn sql_placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
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
    preserve_favorites: bool,
) -> Result<Vec<String>, StorageError> {
    image_preview_urls_matching(
        connection,
        "SELECT image_preview_url FROM clipboard_entries \
         WHERE kind = 'image' AND image_preview_url IS NOT NULL \
           AND captured_at_millis < ?1 AND (?2 = 0 OR favorite = 0)",
        params![cutoff.timestamp_millis(), preserve_favorites as i64],
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

fn open_connection() -> Result<Connection, StorageError> {
    let path = database_path()?;
    let connection = Connection::open(&path)?;
    connection.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    schema::migrate(&connection)?;
    Ok(connection)
}

fn content_hash_for_entry(entry: &ClipboardEntry, image_blob: Option<&[u8]>) -> Option<String> {
    match &entry.content {
        ClipboardContent::Text(text) => content_hash_from_parts("text", Some(text), None, &[]),
        ClipboardContent::Image(image) => image
            .content_hash
            .clone()
            .or_else(|| content_hash_from_parts("image", None, image_blob, &[])),
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

// Deprecated: N+1 query pattern, use JOIN in load_history instead
#[allow(dead_code)]
fn load_all_files(connection: &Connection) -> rusqlite::Result<HashMap<u64, Vec<String>>> {
    let mut statement = connection.prepare(
        "SELECT entry_id, path FROM clipboard_files ORDER BY entry_id ASC, position ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?))
    })?;
    let mut files_by_entry = HashMap::new();

    for row in rows {
        let (entry_id, path) = row?;
        files_by_entry
            .entry(entry_id)
            .or_insert_with(Vec::new)
            .push(path);
    }

    Ok(files_by_entry)
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
