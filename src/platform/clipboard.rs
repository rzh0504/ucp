use crate::model::{ClipboardContent, ClipboardImage};
use arboard::{Clipboard, Error as ArboardError, ImageData};
use std::borrow::Cow;
use std::fmt;

#[derive(Debug)]
pub enum ClipboardError {
    Unavailable(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ClipboardError {}

pub fn read_text() -> Result<Option<String>, ClipboardError> {
    let mut clipboard = Clipboard::new().map_err(map_error)?;

    match clipboard.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(ArboardError::ContentNotAvailable) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

pub fn read_content() -> Result<Option<ClipboardContent>, ClipboardError> {
    if let Some(files) = read_files()? {
        return Ok(Some(ClipboardContent::Files(files)));
    }

    if let Some(image) = read_image()? {
        return Ok(Some(ClipboardContent::Image(image)));
    }

    read_text().map(|text| text.map(ClipboardContent::Text))
}

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    Clipboard::new()
        .map_err(map_error)?
        .set_text(text.to_string())
        .map_err(map_error)
}

pub fn write_content(content: &ClipboardContent) -> Result<(), ClipboardError> {
    match content {
        ClipboardContent::Text(text) => write_text(text),
        ClipboardContent::Image(image) => write_image(image),
        ClipboardContent::Files(files) => write_files(files),
    }
}

pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError> {
    let mut clipboard = Clipboard::new().map_err(map_error)?;

    match clipboard.get_image() {
        Ok(image) => Ok(Some(ClipboardImage::from_rgba(
            image.width,
            image.height,
            image.bytes.into_owned(),
        ))),
        Err(ArboardError::ContentNotAvailable) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

pub fn write_image(image: &ClipboardImage) -> Result<(), ClipboardError> {
    Clipboard::new()
        .map_err(map_error)?
        .set_image(ImageData {
            width: image.width,
            height: image.height,
            bytes: Cow::Borrowed(&image.bytes),
        })
        .map_err(map_error)
}

#[cfg(windows)]
pub fn read_files() -> Result<Option<Vec<String>>, ClipboardError> {
    use clipboard_win::{Clipboard as WindowsClipboard, Format, Getter, formats};

    if !formats::FileList.is_format_avail() {
        return Ok(None);
    }

    let _clipboard = WindowsClipboard::new_attempts(5).map_err(map_clipboard_win_error)?;
    let mut files = Vec::new();
    formats::FileList
        .read_clipboard(&mut files)
        .map_err(map_clipboard_win_error)?;

    if files.is_empty() {
        Ok(None)
    } else {
        Ok(Some(files))
    }
}

#[cfg(not(windows))]
pub fn read_files() -> Result<Option<Vec<String>>, ClipboardError> {
    Ok(None)
}

#[cfg(windows)]
pub fn write_files(files: &[String]) -> Result<(), ClipboardError> {
    use clipboard_win::{Clipboard as WindowsClipboard, Setter, formats};

    let _clipboard = WindowsClipboard::new_attempts(5).map_err(map_clipboard_win_error)?;
    formats::FileList
        .write_clipboard(files)
        .map_err(map_clipboard_win_error)
}

#[cfg(not(windows))]
pub fn write_files(_files: &[String]) -> Result<(), ClipboardError> {
    Err(ClipboardError::Unavailable(
        "当前平台暂不支持文件剪贴板".to_string(),
    ))
}

#[cfg(windows)]
pub fn sequence_number() -> Option<u32> {
    clipboard_win::raw::seq_num().map(|sequence| sequence.get())
}

#[cfg(not(windows))]
pub fn sequence_number() -> Option<u32> {
    None
}

fn map_error(error: ArboardError) -> ClipboardError {
    ClipboardError::Unavailable(error.to_string())
}

#[cfg(windows)]
fn map_clipboard_win_error(error: clipboard_win::ErrorCode) -> ClipboardError {
    ClipboardError::Unavailable(error.to_string())
}
