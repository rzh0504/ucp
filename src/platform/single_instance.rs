#[cfg(windows)]
const MUTEX_NAME: &str = r"Local\dev.ucp.clipboard.single-instance";
#[cfg(windows)]
const ACTIVATION_ENDPOINT: &str = "127.0.0.1:49731";

#[cfg(windows)]
static ACTIVATION_REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(windows)]
static QUIT_REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(windows)]
static HOTKEY_COMMANDS: std::sync::OnceLock<std::sync::mpsc::Sender<String>> =
    std::sync::OnceLock::new();
#[cfg(windows)]
static PENDING_HOTKEY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[cfg(windows)]
const SHOW_REQUEST: u8 = 1;
#[cfg(windows)]
const QUIT_REQUEST: u8 = 2;

#[cfg(windows)]
pub enum SingleInstance {
    Primary(SingleInstanceGuard),
    AlreadyRunning,
    Unavailable,
}

#[cfg(windows)]
pub struct SingleInstanceGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
pub fn acquire() -> SingleInstance {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let name = wide_null(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        return SingleInstance::Unavailable;
    }

    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
        SingleInstance::AlreadyRunning
    } else {
        SingleInstance::Primary(SingleInstanceGuard { handle })
    }
}

#[cfg(windows)]
pub fn start_activation_listener() {
    use std::io::Read as _;

    std::thread::spawn(|| {
        start_hotkey_listener();
        let Ok(listener) = std::net::TcpListener::bind(ACTIVATION_ENDPOINT) else {
            return;
        };

        for mut stream in listener.incoming().flatten() {
            let mut request = [SHOW_REQUEST];
            let _ = stream.read(&mut request);
            if request[0] == QUIT_REQUEST {
                QUIT_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Release);
            } else {
                ACTIVATION_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Release);
            }
        }
    });
}

#[cfg(windows)]
fn start_hotkey_listener() {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let _ = HOTKEY_COMMANDS.set(command_tx);
    let initial_shortcut = PENDING_HOTKEY
        .lock()
        .ok()
        .and_then(|mut shortcut| shortcut.take());
    std::thread::spawn(move || unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MSG, PM_REMOVE, PeekMessageW, WM_HOTKEY,
        };
        let mut message = std::mem::zeroed::<MSG>();
        let mut registered = false;
        let mut initial_shortcut = initial_shortcut;
        loop {
            let shortcut = command_rx
                .try_recv()
                .ok()
                .or_else(|| initial_shortcut.take());
            if let Some(shortcut) = shortcut {
                if registered {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(
                        std::ptr::null_mut(),
                        1,
                    );
                    registered = false;
                }
                if let Some((modifiers, key)) = parse_hotkey(&shortcut) {
                    registered = windows_sys::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey(
                        std::ptr::null_mut(),
                        1,
                        modifiers,
                        key,
                    ) != 0;
                }
            }
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if message.message == WM_HOTKEY {
                    ACTIVATION_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Release);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
}

#[cfg(windows)]
pub fn configure_global_hotkey(shortcut: &str) {
    if let Some(sender) = HOTKEY_COMMANDS.get() {
        let _ = sender.send(shortcut.to_string());
    } else if let Ok(mut pending) = PENDING_HOTKEY.lock() {
        *pending = Some(shortcut.to_string());
    }
}

#[cfg(windows)]
fn parse_hotkey(shortcut: &str) -> Option<(u32, u32)> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
    };
    let mut modifiers = 0;
    let mut key = None;
    for part in shortcut
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "alt" => modifiers |= MOD_ALT,
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "meta" | "super" => modifiers |= MOD_WIN,
            value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphanumeric() => {
                key = Some(value.as_bytes()[0].to_ascii_uppercase() as u32)
            }
            value if value.starts_with('f') => {
                let n = value[1..].parse::<u32>().ok()?;
                if !(1..=24).contains(&n) {
                    return None;
                }
                key = Some(0x70 + n - 1);
            }
            _ => return None,
        }
    }
    key.map(|key| (modifiers, key))
}

#[cfg(windows)]
pub fn notify_existing_instance() {
    send_activation_request(SHOW_REQUEST);
}

#[cfg(windows)]
pub fn notify_existing_instance_to_quit() {
    send_activation_request(QUIT_REQUEST);
}

#[cfg(windows)]
pub fn prepare_for_update() -> i32 {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{OpenMutexW, WaitForSingleObject};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDOK, MB_ICONERROR, MB_ICONQUESTION, MB_OK, MB_OKCANCEL, MB_SETFOREGROUND, MessageBoxW,
    };

    let mutex_name = wide_null(MUTEX_NAME);
    const SYNCHRONIZE: u32 = 0x0010_0000;
    let handle = unsafe { OpenMutexW(SYNCHRONIZE, 0, mutex_name.as_ptr()) };
    if handle.is_null() {
        return 0;
    }

    let prompt = wide_null(
        "UCP is currently running. Close UCP and continue the installation?\n\nClick Cancel to stop the installation.",
    );
    let title = wide_null("UCP Setup");
    let response = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            prompt.as_ptr(),
            title.as_ptr(),
            MB_OKCANCEL | MB_ICONQUESTION | MB_SETFOREGROUND,
        )
    };
    if response != IDOK {
        unsafe { CloseHandle(handle) };
        return 1602;
    }

    notify_existing_instance_to_quit();
    let wait_result = unsafe { WaitForSingleObject(handle, 10_000) };
    unsafe { CloseHandle(handle) };
    if wait_result == WAIT_OBJECT_0 || wait_result == WAIT_ABANDONED {
        return 0;
    }

    let message = wide_null(
        "UCP could not be closed. End the remaining UCP process in Task Manager, then run the installer again.",
    );
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
    1
}

#[cfg(windows)]
pub fn take_activation_request() -> bool {
    ACTIVATION_REQUESTS.swap(0, std::sync::atomic::Ordering::AcqRel) != 0
}

#[cfg(windows)]
pub fn take_quit_request() -> bool {
    QUIT_REQUESTS.swap(0, std::sync::atomic::Ordering::AcqRel) != 0
}

#[cfg(windows)]
fn send_activation_request(request: u8) {
    use std::io::Write as _;
    use std::time::Duration;

    for _ in 0..5 {
        if let Ok(mut stream) = std::net::TcpStream::connect(ACTIVATION_ENDPOINT) {
            let _ = stream.write_all(&[request]);
            return;
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
