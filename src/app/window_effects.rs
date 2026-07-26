use crate::model::{AppLanguage, AppSettings, DEFAULT_BACKGROUND_OPACITY};
use dioxus::desktop::{DesktopContext, HotKeyState, ShortcutHandle, ShortcutRegistryError};
use dioxus::prelude::*;
use global_hotkey::hotkey::HotKey;
use std::str::FromStr;

/// Hook to manage widget mode and window opacity
pub fn use_window_mode_effect(
    desktop: DesktopContext,
    settings: Signal<AppSettings>,
    mut applied_widget_mode: Signal<Option<(bool, bool)>>,
    mut applied_window_opacity: Signal<Option<u8>>,
) {
    use_effect({
        let desktop = desktop.clone();
        move || {
            let settings_snapshot = settings();
            let widget_mode = settings_snapshot.desktop_widget;
            let topmost = settings_snapshot.desktop_widget_topmost;
            if *applied_widget_mode.peek() == Some((widget_mode, topmost)) {
                return;
            }

            applied_widget_mode.set(Some((widget_mode, topmost)));
            super::apply_window_mode(&desktop, widget_mode, topmost);

            let opacity = if widget_mode {
                settings_snapshot.background_opacity
            } else {
                DEFAULT_BACKGROUND_OPACITY
            };
            applied_window_opacity.set(Some(opacity));
            super::apply_window_opacity(&desktop, opacity);
        }
    });
}

/// Hook to manage window opacity changes
pub fn use_window_opacity_effect(
    desktop: DesktopContext,
    settings: Signal<AppSettings>,
    mut applied_window_opacity: Signal<Option<u8>>,
) {
    use_effect({
        let desktop = desktop.clone();
        move || {
            let settings_snapshot = settings();
            let opacity = if settings_snapshot.desktop_widget {
                settings_snapshot.background_opacity
            } else {
                DEFAULT_BACKGROUND_OPACITY
            };

            if *applied_window_opacity.peek() == Some(opacity) {
                return;
            }

            applied_window_opacity.set(Some(opacity));
            super::apply_window_opacity(&desktop, opacity);
        }
    });
}

/// Hook to manage global shortcut registration
pub fn use_global_shortcut_effect(
    desktop: DesktopContext,
    settings: Signal<AppSettings>,
    status: Signal<String>,
    mut applied_global_shortcut: Signal<String>,
    mut global_shortcut_handle: Signal<Option<ShortcutHandle>>,
) {
    use_effect({
        let desktop = desktop.clone();
        move || {
            let settings_snapshot = settings();
            let shortcut = settings_snapshot.global_show_shortcut.trim().to_string();
            if applied_global_shortcut.peek().as_str() == shortcut.as_str() {
                return;
            }

            if let Some(handle) = global_shortcut_handle.write().take() {
                handle.remove();
            }
            applied_global_shortcut.set(shortcut.clone());

            let hotkey = match HotKey::from_str(&shortcut) {
                Ok(hotkey) => hotkey,
                Err(_) => {
                    let mut status = status;
                    status.set(super::invalid_shortcut_message(
                        &shortcut,
                        settings_snapshot.language,
                    ));
                    return;
                }
            };

            let shortcut_desktop = desktop.clone();
            match desktop.create_shortcut(hotkey, move |state| {
                if state == HotKeyState::Pressed {
                    super::show_desktop_window(&shortcut_desktop);
                }
            }) {
                Ok(handle) => {
                    global_shortcut_handle.set(Some(handle));
                }
                Err(error) => {
                    let mut status = status;
                    status.set(super::shortcut_error_message(
                        &error,
                        &shortcut,
                        settings_snapshot.language,
                    ));
                }
            }
        }
    });
}

pub fn invalid_shortcut_message(shortcut: &str, language: AppLanguage) -> String {
    match language {
        AppLanguage::Chinese => format!("无效的快捷键：{shortcut}"),
        AppLanguage::English => format!("Invalid shortcut: {shortcut}"),
    }
}

pub fn shortcut_error_message(
    error: &ShortcutRegistryError,
    shortcut: &str,
    language: AppLanguage,
) -> String {
    match error {
        ShortcutRegistryError::InvalidShortcut(shortcut) => match language {
            AppLanguage::Chinese => format!("全局快捷键配置无效：{shortcut}"),
            AppLanguage::English => format!("Invalid global shortcut configuration: {shortcut}"),
        },
        ShortcutRegistryError::Other(error) => {
            let message = error.to_string();
            let debug_message = format!("{error:?}");
            if message.to_ascii_lowercase().contains("already registered")
                || debug_message.contains("AlreadyRegistered")
            {
                match language {
                    AppLanguage::Chinese => {
                        format!("全局快捷键 {shortcut} 已被占用，仍可通过托盘或窗口按钮打开窗口")
                    }
                    AppLanguage::English => format!(
                        "Global shortcut {shortcut} is already in use. You can still open the window from the tray or window controls"
                    ),
                }
            } else {
                match language {
                    AppLanguage::Chinese => {
                        format!("全局快捷键 {shortcut} 注册失败：{message}")
                    }
                    AppLanguage::English => {
                        format!("Failed to register global shortcut {shortcut}: {message}")
                    }
                }
            }
        }
        _ => match language {
            AppLanguage::Chinese => format!("全局快捷键 {shortcut} 发生未知错误"),
            AppLanguage::English => format!("Unknown error with global shortcut {shortcut}"),
        },
    }
}
