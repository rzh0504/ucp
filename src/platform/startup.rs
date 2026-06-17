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

#[cfg(not(windows))]
fn enable() -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
fn disable() -> Result<(), String> {
    Ok(())
}
