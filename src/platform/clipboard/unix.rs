use super::ClipboardError;
#[cfg(target_os = "linux")]
use std::io::Write;

pub(super) fn normalized_existing_files(files: &[String]) -> Result<Vec<String>, ClipboardError> {
    let mut normalized = Vec::new();

    for file in files.iter().map(|file| file.trim()) {
        if file.is_empty() {
            continue;
        }

        match std::path::Path::new(file).try_exists() {
            Ok(true) => normalized.push(file.to_string()),
            Ok(false) => {
                return Err(ClipboardError::Unavailable(format!("文件已不存在：{file}")));
            }
            Err(error) => {
                return Err(ClipboardError::Unavailable(format!(
                    "无法访问文件：{file}（{error}）"
                )));
            }
        }
    }

    if normalized.is_empty() {
        Err(ClipboardError::Unavailable("文件列表为空".to_string()))
    } else {
        Ok(normalized)
    }
}

pub(super) fn command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn command_status(
    command: &str,
    args: &[&str],
    extra_args: &[String],
) -> Result<(), String> {
    let output = std::process::Command::new(command)
        .args(args)
        .args(extra_args)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

#[cfg(target_os = "linux")]
pub(super) fn command_with_stdin(command: &str, args: &[&str], stdin: &str) -> Result<(), String> {
    let mut child = std::process::Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| "stdin unavailable".to_string())?
        .write_all(stdin.as_bytes())
        .map_err(|error| error.to_string())?;

    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

#[cfg(target_os = "linux")]
pub(super) fn linux_file_clipboard_readers() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        (
            "wl-paste",
            &["--no-newline", "--type", "x-special/gnome-copied-files"],
        ),
        ("wl-paste", &["--no-newline", "--type", "text/uri-list"]),
        (
            "xclip",
            &[
                "-selection",
                "clipboard",
                "-target",
                "x-special/gnome-copied-files",
                "-out",
            ],
        ),
        (
            "xclip",
            &[
                "-selection",
                "clipboard",
                "-target",
                "text/uri-list",
                "-out",
            ],
        ),
    ]
}

#[cfg(target_os = "linux")]
pub(super) fn linux_file_clipboard_payload(files: &[String]) -> String {
    let uri_list = files
        .iter()
        .map(|file| path_to_file_uri(file))
        .collect::<Vec<_>>()
        .join("\n");

    format!("copy\n{uri_list}\n")
}

#[cfg(target_os = "linux")]
pub(super) fn linux_uri_list_payload(files: &[String]) -> String {
    files
        .iter()
        .map(|file| path_to_file_uri(file))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[cfg(target_os = "linux")]
pub(super) fn files_from_uri_list(output: &str) -> Option<Vec<String>> {
    let files = output
        .lines()
        .map(|line| line.trim_end_matches('\r').trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| *line != "copy" && *line != "cut")
        .filter_map(file_uri_to_path)
        .collect::<Vec<_>>();

    (!files.is_empty()).then_some(files)
}

#[cfg(target_os = "macos")]
pub(super) fn paths_from_lines(output: &str) -> Option<Vec<String>> {
    let files = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    (!files.is_empty()).then_some(files)
}

#[cfg(target_os = "linux")]
fn path_to_file_uri(path: &str) -> String {
    format!("file://{}", percent_encode_path(path))
}

#[cfg(target_os = "linux")]
fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest
        .strip_prefix("localhost/")
        .map(|path| format!("/{path}"))
        .or_else(|| rest.strip_prefix('/').map(|path| format!("/{path}")))?;

    percent_decode(&path)
}

#[cfg(target_os = "linux")]
fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(*byte as char);
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(target_os = "linux")]
fn percent_decode(value: &str) -> Option<String> {
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

#[cfg(target_os = "linux")]
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            output.status.to_string()
        } else {
            stdout
        }
    } else {
        stderr
    }
}
