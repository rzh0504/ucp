use thiserror::Error;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("failed to create update client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("failed to fetch latest release: {0}")]
    Fetch(#[source] reqwest::Error),
    #[error("failed to read latest release: {0}")]
    Decode(#[source] reqwest::Error),
    #[error("latest release page did not resolve to a tag: {url}")]
    MissingTag { url: String },
    #[error("invalid release version {version}: {source}")]
    InvalidVersion {
        version: String,
        #[source]
        source: semver::Error,
    },
    #[error("API check failed: {api}; fallback release check failed: {fallback}")]
    ApiAndFallback { api: Box<Self>, fallback: Box<Self> },
}

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to resolve executable path: {0}")]
    CurrentExe(#[source] std::io::Error),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error("failed to access startup directory")]
    DirectoryUnavailable,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error("failed to access startup configuration")]
    HomeUnavailable,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error("failed to create startup directory: {0}")]
    CreateDirectory(#[source] std::io::Error),
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[error("failed to write startup configuration: {0}")]
    Write(#[source] std::io::Error),
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    #[error("startup is not supported on this platform")]
    Unsupported,
    #[cfg(windows)]
    #[error("Windows registry operation failed with code {0}")]
    Win32(windows_sys::Win32::Foundation::WIN32_ERROR),
}

#[derive(Debug, Error)]
#[cfg(windows)]
pub enum TrayError {
    #[error("failed to create tray menu: {0}")]
    Menu(String),
    #[error("failed to load tray icon: {0}")]
    Image(#[source] image::ImageError),
    #[error("failed to create tray icon: {0}")]
    Icon(String),
}
