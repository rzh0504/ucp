use crate::model::{ClipboardContent, ClipboardImage};
use arboard::{Clipboard, Error as ArboardError, ImageData};
use std::borrow::Cow;
use std::fmt;
#[cfg(target_os = "linux")]
use std::io::Write;

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

#[cfg(windows)]
pub fn paste_shortcut() -> Result<(), ClipboardError> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, SendInput, VK_CONTROL, VK_V,
    };

    unsafe {
        let mut inputs = [INPUT::default(); 4];
        inputs[0].r#type = INPUT_KEYBOARD;
        inputs[0].Anonymous.ki.wVk = VK_CONTROL;

        inputs[1].r#type = INPUT_KEYBOARD;
        inputs[1].Anonymous.ki.wVk = VK_V;

        inputs[2].r#type = INPUT_KEYBOARD;
        inputs[2].Anonymous.ki.wVk = VK_V;
        inputs[2].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        inputs[3].r#type = INPUT_KEYBOARD;
        inputs[3].Anonymous.ki.wVk = VK_CONTROL;
        inputs[3].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;

        let sent = SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );

        if sent == inputs.len() as u32 {
            Ok(())
        } else {
            Err(ClipboardError::Unavailable(
                "发送粘贴快捷键失败".to_string(),
            ))
        }
    }
}

#[cfg(not(windows))]
pub fn paste_shortcut() -> Result<(), ClipboardError> {
    platform_paste_shortcut()
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
        Err(error) if image_data_is_unreadable(&error) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

pub fn write_image(image: &ClipboardImage) -> Result<(), ClipboardError> {
    let Some(bytes) = image.rgba_bytes() else {
        return Err(ClipboardError::Unavailable(
            "剪贴板图像原始数据尚未加载".to_string(),
        ));
    };

    Clipboard::new()
        .map_err(map_error)?
        .set_image(ImageData {
            width: image.width,
            height: image.height,
            bytes: Cow::Borrowed(bytes),
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

#[cfg(target_os = "macos")]
pub fn read_files() -> Result<Option<Vec<String>>, ClipboardError> {
    let script = r#"
ObjC.import('AppKit');
function run() {
    const pasteboard = $.NSPasteboard.generalPasteboard;
    const classes = $.NSArray.arrayWithObject($.NSURL.class);
    const urls = pasteboard.readObjectsForClassesOptions(classes, $.NSDictionary.dictionary);
    if (!urls) {
        return '';
    }

    const paths = [];
    for (let index = 0; index < urls.count; index++) {
        const url = urls.objectAtIndex(index);
        if (url.isFileURL) {
            paths.push(ObjC.unwrap(url.path));
        }
    }
    return paths.join('\n');
}
"#;

    let Some(output) = command_stdout("osascript", &["-l", "JavaScript", "-e", script]) else {
        return Ok(None);
    };

    Ok(paths_from_lines(&output))
}

#[cfg(target_os = "linux")]
pub fn read_files() -> Result<Option<Vec<String>>, ClipboardError> {
    for (command, args) in linux_file_clipboard_readers() {
        let Some(output) = command_stdout(command, args) else {
            continue;
        };

        if let Some(files) = files_from_uri_list(&output) {
            return Ok(Some(files));
        }
    }

    Ok(None)
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
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

#[cfg(target_os = "macos")]
pub fn write_files(files: &[String]) -> Result<(), ClipboardError> {
    let files = normalized_existing_files(files)?;
    let script = r#"
ObjC.import('AppKit');
function run(argv) {
    const pasteboard = $.NSPasteboard.generalPasteboard;
    const urls = $.NSMutableArray.array;
    argv.forEach(path => urls.addObject($.NSURL.fileURLWithPath(path)));
    pasteboard.clearContents;
    if (!pasteboard.writeObjects(urls)) {
        throw new Error('NSPasteboard rejected the file URLs');
    }
}
"#;

    command_status(
        "osascript",
        &["-l", "JavaScript", "-e", script, "--"],
        &files,
    )
    .map_err(|message| ClipboardError::Unavailable(format!("写入 macOS 文件剪贴板失败：{message}")))
}

#[cfg(target_os = "linux")]
pub fn write_files(files: &[String]) -> Result<(), ClipboardError> {
    let files = normalized_existing_files(files)?;
    let gnome_payload = linux_file_clipboard_payload(&files);
    let uri_payload = linux_uri_list_payload(&files);
    let attempts: [(&str, &[&str], &str); 4] = [
        (
            "wl-copy",
            &["--type", "x-special/gnome-copied-files"],
            &gnome_payload,
        ),
        ("wl-copy", &["--type", "text/uri-list"], &uri_payload),
        (
            "xclip",
            &[
                "-selection",
                "clipboard",
                "-target",
                "x-special/gnome-copied-files",
                "-in",
            ],
            &gnome_payload,
        ),
        (
            "xclip",
            &["-selection", "clipboard", "-target", "text/uri-list", "-in"],
            &uri_payload,
        ),
    ];
    let mut errors = Vec::new();

    for (command, args, payload) in attempts {
        match command_with_stdin(command, args, payload) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{command}: {error}")),
        }
    }

    Err(ClipboardError::Unavailable(format!(
        "写入 Linux 文件剪贴板失败，请安装 wl-clipboard 或 xclip：{}",
        errors.join("; ")
    )))
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
pub fn write_files(_files: &[String]) -> Result<(), ClipboardError> {
    Err(ClipboardError::Unavailable(
        "当前平台暂不支持文件剪贴板".to_string(),
    ))
}

#[cfg(windows)]
pub struct ClipboardUpdateListener {
    _shutdown: clipboard_win::monitor::Shutdown,
}

#[cfg(windows)]
pub fn listen_for_updates(
    mut on_update: impl FnMut() + Send + 'static,
) -> Result<ClipboardUpdateListener, ClipboardError> {
    let (setup_tx, setup_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let mut monitor = match clipboard_win::Monitor::new() {
            Ok(monitor) => monitor,
            Err(error) => {
                let _ = setup_tx.send(Err(map_clipboard_win_error(error)));
                return;
            }
        };

        let shutdown = monitor.shutdown_channel();
        if setup_tx.send(Ok(shutdown)).is_err() {
            return;
        }

        while let Ok(true) = monitor.recv() {
            on_update();
        }
    });

    let shutdown = setup_rx
        .recv()
        .map_err(|error| ClipboardError::Unavailable(format!("启动剪贴板监听失败：{error}")))??;

    Ok(ClipboardUpdateListener {
        _shutdown: shutdown,
    })
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
    let message = match error {
        ArboardError::ContentNotAvailable => "剪贴板中没有可读取的内容".to_string(),
        ArboardError::ClipboardNotSupported => "当前系统剪贴板不可用".to_string(),
        ArboardError::ClipboardOccupied => "剪贴板正被其他程序占用，请稍后重试".to_string(),
        ArboardError::ConversionFailure => "剪贴板内容格式暂不支持".to_string(),
        ArboardError::Unknown { description } => match description.as_str() {
            "failed to read clipboard image data" => "剪贴板图片数据暂不可读取".to_string(),
            _ => format!("剪贴板操作失败：{description}"),
        },
        _ => "剪贴板操作失败".to_string(),
    };

    ClipboardError::Unavailable(message)
}

fn image_data_is_unreadable(error: &ArboardError) -> bool {
    matches!(
        error,
        ArboardError::Unknown { description } if description == "failed to read clipboard image data"
    )
}

#[cfg(target_os = "macos")]
fn platform_paste_shortcut() -> Result<(), ClipboardError> {
    command_status(
        "osascript",
        &[
            "-e",
            "tell application \"System Events\" to keystroke \"v\" using command down",
        ],
        &[],
    )
    .map_err(|message| ClipboardError::Unavailable(format!("发送 macOS 粘贴快捷键失败：{message}")))
}

#[cfg(target_os = "linux")]
fn platform_paste_shortcut() -> Result<(), ClipboardError> {
    let attempts: [(&str, &[&str]); 2] = [
        ("wtype", &["-M", "ctrl", "v", "-m", "ctrl"]),
        ("xdotool", &["key", "--clearmodifiers", "ctrl+v"]),
    ];
    let mut errors = Vec::new();

    for (command, args) in attempts {
        match command_status(command, args, &[]) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{command}: {error}")),
        }
    }

    Err(ClipboardError::Unavailable(format!(
        "发送 Linux 粘贴快捷键失败，请安装 wtype 或 xdotool：{}",
        errors.join("; ")
    )))
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
fn platform_paste_shortcut() -> Result<(), ClipboardError> {
    Err(ClipboardError::Unavailable(
        "当前平台暂不支持快捷粘贴".to_string(),
    ))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn normalized_existing_files(files: &[String]) -> Result<Vec<String>, ClipboardError> {
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(command)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn command_status(command: &str, args: &[&str], extra_args: &[String]) -> Result<(), String> {
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
fn command_with_stdin(command: &str, args: &[&str], stdin: &str) -> Result<(), String> {
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
fn linux_file_clipboard_readers() -> &'static [(&'static str, &'static [&'static str])] {
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
fn linux_file_clipboard_payload(files: &[String]) -> String {
    let uri_list = files
        .iter()
        .map(|file| path_to_file_uri(file))
        .collect::<Vec<_>>()
        .join("\n");

    format!("copy\n{uri_list}\n")
}

#[cfg(target_os = "linux")]
fn linux_uri_list_payload(files: &[String]) -> String {
    files
        .iter()
        .map(|file| path_to_file_uri(file))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[cfg(target_os = "linux")]
fn files_from_uri_list(output: &str) -> Option<Vec<String>> {
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
fn paths_from_lines(output: &str) -> Option<Vec<String>> {
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
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

#[cfg(windows)]
fn map_clipboard_win_error(error: clipboard_win::ErrorCode) -> ClipboardError {
    ClipboardError::Unavailable(error.to_string())
}
