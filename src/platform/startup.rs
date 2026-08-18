use crate::error::StartupError;

const APP_RUN_VALUE: &str = "UCP Clipboard";
pub const SILENT_STARTUP_ARG: &str = "--silent-startup";

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "dev.ucp.clipboard";
#[cfg(target_os = "linux")]
const AUTOSTART_DESKTOP_FILE: &str = "dev.ucp.clipboard.desktop";

pub fn set_enabled(enabled: bool) -> Result<(), StartupError> {
    if enabled { enable() } else { disable() }
}

#[cfg(windows)]
fn enable() -> Result<(), StartupError> {
    use windows_sys::Win32::System::Registry::{REG_SZ, RegSetValueExW};

    let executable = std::env::current_exe().map_err(StartupError::CurrentExe)?;
    let command = format!("\"{}\" {SILENT_STARTUP_ARG}", executable.display());
    let value_name = wide_null(APP_RUN_VALUE);
    let command = wide_null(&command);
    let key = open_run_key()?;

    let result = unsafe {
        RegSetValueExW(
            key.0,
            value_name.as_ptr(),
            0,
            REG_SZ,
            command.as_ptr().cast::<u8>(),
            (command.len() * std::mem::size_of::<u16>()) as u32,
        )
    };

    win32_result(result)
}

#[cfg(windows)]
fn disable() -> Result<(), StartupError> {
    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::RegDeleteValueW;

    let value_name = wide_null(APP_RUN_VALUE);
    let key = open_run_key()?;
    let result = unsafe { RegDeleteValueW(key.0, value_name.as_ptr()) };

    if result == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        win32_result(result)
    }
}

#[cfg(windows)]
struct RegistryKey(windows_sys::Win32::System::Registry::HKEY);

#[cfg(windows)]
impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Registry::RegCloseKey(self.0);
        }
    }
}

#[cfg(windows)]
fn open_run_key() -> Result<RegistryKey, StartupError> {
    use std::ptr::null_mut;
    use windows_sys::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, RegCreateKeyW};

    let run_key = wide_null(RUN_KEY);
    let mut key: HKEY = null_mut();
    let result = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, run_key.as_ptr(), &mut key) };
    win32_result(result).map(|()| RegistryKey(key))
}

#[cfg(windows)]
fn win32_result(code: windows_sys::Win32::Foundation::WIN32_ERROR) -> Result<(), StartupError> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;

    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(StartupError::Win32(code))
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "macos")]
fn enable() -> Result<(), StartupError> {
    let path = launch_agent_path()?;
    let parent = path.parent().ok_or(StartupError::DirectoryUnavailable)?;
    std::fs::create_dir_all(parent).map_err(StartupError::CreateDirectory)?;

    let executable = std::env::current_exe().map_err(StartupError::CurrentExe)?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
"#,
        xml_escape(LAUNCH_AGENT_LABEL),
        xml_escape(&executable.to_string_lossy()),
        xml_escape(SILENT_STARTUP_ARG),
    );

    std::fs::write(path, plist).map_err(StartupError::Write)
}

#[cfg(target_os = "macos")]
fn disable() -> Result<(), StartupError> {
    remove_file_if_exists(launch_agent_path()?)
}

#[cfg(target_os = "linux")]
fn enable() -> Result<(), StartupError> {
    let path = autostart_desktop_path()?;
    let parent = path.parent().ok_or(StartupError::DirectoryUnavailable)?;
    std::fs::create_dir_all(parent).map_err(StartupError::CreateDirectory)?;

    let executable = std::env::current_exe().map_err(StartupError::CurrentExe)?;
    let desktop_file = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={APP_RUN_VALUE}\n\
         Comment=Desktop clipboard history manager\n\
         Exec={} {SILENT_STARTUP_ARG}\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n",
        desktop_exec_quote(&executable.to_string_lossy()),
    );

    std::fs::write(path, desktop_file).map_err(StartupError::Write)
}

#[cfg(target_os = "linux")]
fn disable() -> Result<(), StartupError> {
    remove_file_if_exists(autostart_desktop_path()?)
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
fn enable() -> Result<(), StartupError> {
    Err(StartupError::Unsupported)
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
fn disable() -> Result<(), StartupError> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<std::path::PathBuf, StartupError> {
    home_dir()
        .map(|home| {
            home.join("Library")
                .join("LaunchAgents")
                .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
        })
        .ok_or(StartupError::HomeUnavailable)
}

#[cfg(target_os = "linux")]
fn autostart_desktop_path() -> Result<std::path::PathBuf, StartupError> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .ok_or(StartupError::HomeUnavailable)?;

    Ok(config_home.join("autostart").join(AUTOSTART_DESKTOP_FILE))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn remove_file_if_exists(path: std::path::PathBuf) -> Result<(), StartupError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StartupError::Write(error)),
    }
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "linux")]
fn desktop_exec_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{escaped}\"")
}
