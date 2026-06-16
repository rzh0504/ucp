const APP_RUN_VALUE: &str = "UCP Clipboard";

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

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

    let _ = Command::new("reg")
        .args(["delete", RUN_KEY, "/v", APP_RUN_VALUE, "/f"])
        .output()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[cfg(not(windows))]
fn enable() -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
fn disable() -> Result<(), String> {
    Ok(())
}
