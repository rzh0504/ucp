pub mod clipboard;
pub mod single_instance;
pub mod startup;
pub mod tray;
#[cfg(windows)]
pub mod windows;

pub fn hide_window(window: &gpui::Window) {
    #[cfg(windows)]
    if let Some(handle) = windows::window_handle(window) {
        windows::hide_window(handle);
    }

    #[cfg(not(windows))]
    let _ = window;
}
