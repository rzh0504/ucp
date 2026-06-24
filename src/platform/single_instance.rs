#[cfg(windows)]
const MUTEX_NAME: &str = r"Local\dev.ucp.clipboard.single-instance";
#[cfg(windows)]
const ACTIVATION_ENDPOINT: &str = "127.0.0.1:49731";

#[cfg(windows)]
static ACTIVATION_REQUESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    std::thread::spawn(|| {
        let Ok(listener) = std::net::TcpListener::bind(ACTIVATION_ENDPOINT) else {
            return;
        };

        for stream in listener.incoming() {
            if stream.is_ok() {
                ACTIVATION_REQUESTS.fetch_add(1, std::sync::atomic::Ordering::Release);
            }
        }
    });
}

#[cfg(windows)]
pub fn notify_existing_instance() {
    use std::io::Write as _;
    use std::time::Duration;

    for _ in 0..5 {
        if let Ok(mut stream) = std::net::TcpStream::connect(ACTIVATION_ENDPOINT) {
            let _ = stream.write_all(&[1]);
            return;
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
pub fn activation_count() -> u64 {
    ACTIVATION_REQUESTS.load(std::sync::atomic::Ordering::Acquire)
}

#[cfg(windows)]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
