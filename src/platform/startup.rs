const APP_RUN_VALUE: &str = "UCP Clipboard";

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "dev.ucp.clipboard";
#[cfg(target_os = "linux")]
const AUTOSTART_DESKTOP_FILE: &str = "dev.ucp.clipboard.desktop";

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled { enable() } else { disable() }
}

#[cfg(windows)]
fn enable() -> Result<(), String> {
    use std::process::Command;

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let command = format!("\"{}\"", executable.display());
    let output = Command::new("reg")
        .args(["add", RUN_KEY, "/v", APP_RUN_VALUE, "/t", "REG_SZ", "/d"])
        .arg(command)
        .arg("/f")
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(windows)]
fn disable() -> Result<(), String> {
    use std::process::Command;

    let output = Command::new("reg")
        .args(["delete", RUN_KEY, "/v", APP_RUN_VALUE, "/f"])
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() || registry_value_was_missing(&output) {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

#[cfg(windows)]
fn registry_value_was_missing(output: &std::process::Output) -> bool {
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    stderr.contains("unable to find")
        || stderr.contains("cannot find")
        || stdout.contains("unable to find")
        || stdout.contains("cannot find")
}

#[cfg(windows)]
fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    }
}

#[cfg(target_os = "macos")]
fn enable() -> Result<(), String> {
    let path = launch_agent_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "LaunchAgents directory is unavailable".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
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
    );

    std::fs::write(path, plist).map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn disable() -> Result<(), String> {
    remove_file_if_exists(launch_agent_path()?)
}

#[cfg(target_os = "linux")]
fn enable() -> Result<(), String> {
    let path = autostart_desktop_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "autostart directory is unavailable".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let desktop_file = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={APP_RUN_VALUE}\n\
         Comment=Desktop clipboard history manager\n\
         Exec={}\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n",
        desktop_exec_quote(&executable.to_string_lossy()),
    );

    std::fs::write(path, desktop_file).map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn disable() -> Result<(), String> {
    remove_file_if_exists(autostart_desktop_path()?)
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
fn enable() -> Result<(), String> {
    Err("当前平台暂不支持开机启动".to_string())
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
fn disable() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<std::path::PathBuf, String> {
    home_dir()
        .map(|home| {
            home.join("Library")
                .join("LaunchAgents")
                .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
        })
        .ok_or_else(|| "HOME is not set".to_string())
}

#[cfg(target_os = "linux")]
fn autostart_desktop_path() -> Result<std::path::PathBuf, String> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .ok_or_else(|| "HOME is not set".to_string())?;

    Ok(config_home.join("autostart").join(AUTOSTART_DESKTOP_FILE))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn remove_file_if_exists(path: std::path::PathBuf) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
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
