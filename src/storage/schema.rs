use super::{
    IMAGE_FORMAT_PNG, PNG_SIGNATURE, SCHEMA_VERSION, StorageError, content_hash_from_parts,
    image_cache, image_preview_urls_for_ids, load_files,
};
use crate::model::ClipboardImage;
use rusqlite::{Connection, params};

pub(super) fn migrate(connection: &Connection) -> Result<(), StorageError> {
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
    materialize_cached_image_previews(connection)?;
    populate_content_hashes(connection)?;
    deduplicate_entries(connection)?;
    create_content_hash_index(connection)?;

    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

pub(super) fn ensure_current_schema_columns(connection: &Connection) -> Result<(), StorageError> {
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

pub(super) fn populate_content_hashes(connection: &Connection) -> Result<(), StorageError> {
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

pub(super) fn deduplicate_entries(connection: &Connection) -> Result<(), StorageError> {
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
            let preview_urls = image_preview_urls_for_ids(connection, &[id as u64])?;
            connection.execute("DELETE FROM clipboard_entries WHERE id = ?1", params![id])?;
            image_cache::remove_previews(preview_urls);
        }
    }

    Ok(())
}

pub(super) fn create_content_hash_index(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_entries_content_hash \
             ON clipboard_entries(content_hash) \
             WHERE content_hash IS NOT NULL;",
    )?;
    Ok(())
}

pub(super) fn materialize_cached_image_previews(
    connection: &Connection,
) -> Result<(), StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, image_width, image_height, image_blob, image_preview_url \
         FROM clipboard_entries \
         WHERE kind = 'image' AND image_blob IS NOT NULL",
    )?;

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, Option<i64>>(1)?.unwrap_or_default().max(0) as usize,
                row.get::<_, Option<i64>>(2)?.unwrap_or_default().max(0) as usize,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for (id, width, height, bytes, preview_url) in rows {
        if image_cache::exists(preview_url.as_deref()) {
            continue;
        }

        let Some(image) = ClipboardImage::from_stored_bytes(width, height, bytes, preview_url)
        else {
            continue;
        };
        let Some(cached_url) = image_cache::write_preview(id, &image)? else {
            continue;
        };

        connection.execute(
            "UPDATE clipboard_entries SET image_preview_url = ?2 WHERE id = ?1",
            params![id as i64, cached_url],
        )?;
    }

    Ok(())
}

pub(super) fn compress_stored_images(connection: &Connection) -> Result<(), StorageError> {
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

pub(super) fn column_exists(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, StorageError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

pub(super) fn schema_version(connection: &Connection) -> Result<i32, StorageError> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(StorageError::from)
}
