use super::{StorageError, data_directory};
use crate::model::ClipboardImage;
use std::fs;
use std::path::{Path, PathBuf};

const IMAGE_CACHE_DIR: &str = "image-cache";

pub(super) fn write_preview(
    entry_id: u64,
    image: &ClipboardImage,
) -> Result<Option<String>, StorageError> {
    let Some(bytes) = image.preview_png_bytes() else {
        return Ok(image.preview_url.clone());
    };

    let path = preview_path(entry_id)?;
    fs::write(&path, bytes)?;
    Ok(Some(path_to_file_url(&path)))
}

pub(super) fn remove_previews(urls: Vec<String>) {
    for url in urls {
        if let Some(path) = preview_path_from_url(&url) {
            let _ = fs::remove_file(path);
        }
    }
}

pub(super) fn exists(url: Option<&str>) -> bool {
    url.and_then(preview_path_from_url)
        .is_some_and(|path| path.is_file())
}

fn directory() -> Result<PathBuf, StorageError> {
    let directory = data_directory().join(IMAGE_CACHE_DIR);
    fs::create_dir_all(&directory)?;
    Ok(directory)
}

fn preview_path(entry_id: u64) -> Result<PathBuf, StorageError> {
    Ok(directory()?.join(format!("image-preview-{entry_id}.png")))
}

fn preview_path_from_url(url: &str) -> Option<PathBuf> {
    let path = file_url_to_path(url)?;
    let cache_directory = directory().ok()?;
    (path.parent() == Some(cache_directory.as_path())).then_some(path)
}

fn path_to_file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let prefix = if path.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    format!("{prefix}{}", percent_encode_file_url_path(&path))
}

fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let path = url
        .strip_prefix("file:///")
        .or_else(|| url.strip_prefix("file://"))?;
    let path = percent_decode_file_url_path(path)?;

    #[cfg(windows)]
    {
        Some(PathBuf::from(path.replace('/', "\\")))
    }

    #[cfg(not(windows))]
    {
        Some(PathBuf::from(format!("/{path}")))
    }
}

fn percent_encode_file_url_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b':' | b'/' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char);
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode_file_url_path(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied()?;
            let low = bytes.get(index + 2).copied()?;
            decoded.push(hex_value(high)? * 16 + hex_value(low)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{reset_storage_for_tests, storage_test_lock, test_data_directory};
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn file_url_round_trip_handles_platform_paths() {
        #[cfg(windows)]
        let path = PathBuf::from(r"C:\Users\Tester\UCP Clipboard\image-cache\预览 1.png");
        #[cfg(not(windows))]
        let path = PathBuf::from("/home/tester/UCP Clipboard/image-cache/预览 1.png");

        let url = path_to_file_url(&path);

        assert!(url.starts_with("file://"));
        assert!(url.contains("UCP%20Clipboard"));
        assert_eq!(file_url_to_path(&url).as_deref(), Some(path.as_path()));
    }

    #[test]
    fn preview_path_is_limited_to_image_cache_directory() {
        let _guard = storage_test_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let directory = unique_test_directory();
        reset_storage_for_tests();
        *test_data_directory().lock().unwrap() = Some(directory.clone());

        let cache_path = preview_path(7).unwrap();
        fs::write(&cache_path, b"preview").unwrap();
        let cache_url = path_to_file_url(&cache_path);
        assert_eq!(
            preview_path_from_url(&cache_url).as_deref(),
            Some(cache_path.as_path())
        );

        let outside_path = directory.join("outside.png");
        fs::write(&outside_path, b"outside").unwrap();
        let outside_url = path_to_file_url(&outside_path);
        assert!(preview_path_from_url(&outside_url).is_none());

        remove_previews(vec![outside_url, cache_url]);
        assert!(outside_path.exists());
        assert!(!cache_path.exists());

        reset_storage_for_tests();
        *test_data_directory().lock().unwrap() = None;
        let _ = fs::remove_dir_all(directory);
    }

    fn unique_test_directory() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "ucp-image-cache-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
