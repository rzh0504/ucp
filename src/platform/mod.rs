pub mod clipboard;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum OperatingSystem {
    Windows,
    Macos,
    Linux,
    Ios,
    Android,
    Unknown,
}

impl OperatingSystem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Macos => "macOS",
            Self::Linux => "Linux",
            Self::Ios => "iOS",
            Self::Android => "Android",
            Self::Unknown => "Unknown",
        }
    }
}

#[cfg(target_os = "windows")]
pub const CURRENT_OS: OperatingSystem = OperatingSystem::Windows;

#[cfg(target_os = "macos")]
pub const CURRENT_OS: OperatingSystem = OperatingSystem::Macos;

#[cfg(target_os = "linux")]
pub const CURRENT_OS: OperatingSystem = OperatingSystem::Linux;

#[cfg(target_os = "ios")]
pub const CURRENT_OS: OperatingSystem = OperatingSystem::Ios;

#[cfg(target_os = "android")]
pub const CURRENT_OS: OperatingSystem = OperatingSystem::Android;

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "ios",
    target_os = "android"
)))]
pub const CURRENT_OS: OperatingSystem = OperatingSystem::Unknown;
