#![cfg(windows)]

use super::clipboard::ClipboardError;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_NOTOPMOST, HWND_TOPMOST, SW_HIDE, SW_SHOW,
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow, WS_EX_TOPMOST,
};

pub fn window_handle(window: &gpui::Window) -> Option<isize> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.hwnd.get())
}

pub fn hide_window(handle: isize) {
    unsafe { ShowWindow(handle as _, SW_HIDE) };
}

pub fn show_window(handle: isize) {
    unsafe { ShowWindow(handle as _, SW_SHOW) };
}

pub fn set_always_on_top(window: &gpui::Window, always_on_top: bool) -> bool {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return false;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return false;
    };
    let insert_after = if always_on_top {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    unsafe {
        let changed = SetWindowPos(
            handle.hwnd.get() as _,
            insert_after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
        ) != 0;
        let is_topmost =
            GetWindowLongPtrW(handle.hwnd.get() as _, GWL_EXSTYLE) & WS_EX_TOPMOST as isize != 0;
        changed && is_topmost == always_on_top
    }
}

pub struct ClipboardUpdateListener {
    _shutdown: clipboard_win::monitor::Shutdown,
}

pub fn read_files() -> Result<Option<Vec<String>>, ClipboardError> {
    use clipboard_win::{Clipboard as WindowsClipboard, Format, Getter, formats};
    if !formats::FileList.is_format_avail() {
        return Ok(None);
    }
    let _clipboard =
        WindowsClipboard::new_attempts(5).map_err(super::clipboard::map_clipboard_win_error)?;
    let mut files = Vec::new();
    formats::FileList
        .read_clipboard(&mut files)
        .map_err(super::clipboard::map_clipboard_win_error)?;
    Ok((!files.is_empty()).then_some(files))
}

pub fn write_files(files: &[String]) -> Result<(), ClipboardError> {
    use clipboard_win::{Clipboard as WindowsClipboard, Setter, formats};
    let _clipboard =
        WindowsClipboard::new_attempts(5).map_err(super::clipboard::map_clipboard_win_error)?;
    formats::FileList
        .write_clipboard(files)
        .map_err(super::clipboard::map_clipboard_win_error)
}

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
        if SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        ) == inputs.len() as u32
        {
            Ok(())
        } else {
            Err(ClipboardError::Unavailable(
                "发送粘贴快捷键失败".to_string(),
            ))
        }
    }
}

pub fn listen_for_updates(
    mut on_update: impl FnMut() + Send + 'static,
) -> Result<ClipboardUpdateListener, ClipboardError> {
    let (setup_tx, setup_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut monitor = match clipboard_win::Monitor::new() {
            Ok(monitor) => monitor,
            Err(error) => {
                let _ = setup_tx.send(Err(super::clipboard::map_clipboard_win_error(error)));
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
