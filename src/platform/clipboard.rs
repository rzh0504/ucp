use arboard::{Clipboard, Error as ArboardError};
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

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    Clipboard::new()
        .map_err(map_error)?
        .set_text(text.to_string())
        .map_err(map_error)
}

fn map_error(error: ArboardError) -> ClipboardError {
    ClipboardError::Unavailable(error.to_string())
}
