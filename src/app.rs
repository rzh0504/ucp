mod status_bar;

use self::status_bar::{
    ClearHistoryButton, ClipboardMonitorButton, StatusSettingsButton, history_count_for_filter,
};
use crate::components::{AppPage, HistoryList, SettingsPage, TopBar};
use crate::i18n;
use crate::model::{
    AppLanguage, AppSettings, ClipboardContent, ClipboardFilter, ClipboardHistory,
    DEFAULT_BACKGROUND_OPACITY,
};
use crate::storage;
use dioxus::desktop::{
    self, DesktopContext, HotKeyState, LogicalSize, ShortcutHandle, ShortcutRegistryError,
    WindowCloseBehaviour, use_window,
};
use dioxus::events::MountedData;
use dioxus::html::Key;
use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedReceiver;
use futures_timer::Delay;
use global_hotkey::hotkey::HotKey;
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;

const STYLES: Asset = asset!("/assets/app.css");
const BASE_STYLES: Asset = asset!("/assets/styles/base.css");
const LAYOUT_STYLES: Asset = asset!("/assets/styles/layout.css");
const TOP_BAR_STYLES: Asset = asset!("/assets/styles/top_bar.css");
const STATUS_STYLES: Asset = asset!("/assets/styles/status.css");
const DIALOG_STYLES: Asset = asset!("/assets/styles/dialog.css");
const FILTER_TABS_STYLES: Asset = asset!("/assets/styles/filter_tabs.css");
const LIST_HEADER_STYLES: Asset = asset!("/assets/styles/list_header.css");
const HISTORY_LIST_STYLES: Asset = asset!("/assets/styles/history_list.css");
const SETTINGS_STYLES: Asset = asset!("/assets/styles/settings.css");
const RESPONSIVE_STYLES: Asset = asset!("/assets/styles/responsive.css");
const APP_STYLES: [Asset; 11] = [
    STYLES,
    BASE_STYLES,
    LAYOUT_STYLES,
    TOP_BAR_STYLES,
    STATUS_STYLES,
    DIALOG_STYLES,
    FILTER_TABS_STYLES,
    LIST_HEADER_STYLES,
    HISTORY_LIST_STYLES,
    SETTINGS_STYLES,
    RESPONSIVE_STYLES,
];
const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/icons/Ucp.png");
const TRAY_SHOW_WINDOW_ID: &str = "ucp-show-window";
const TRAY_OPEN_WIDGET_ID: &str = "ucp-open-widget";
const TRAY_QUIT_ID: &str = "ucp-quit";
const STATUS_AUTO_CLEAR_DELAY: Duration = Duration::from_secs(4);
const NORMAL_WINDOW_WIDTH: f64 = 900.0;
const NORMAL_WINDOW_HEIGHT: f64 = 660.0;
const NORMAL_WINDOW_MIN_WIDTH: f64 = 860.0;
const NORMAL_WINDOW_MIN_HEIGHT: f64 = 620.0;
const WIDGET_WINDOW_WIDTH: f64 = 420.0;
const WIDGET_WINDOW_HEIGHT: f64 = 620.0;
const SEARCH_DEBOUNCE_DELAY: Duration = Duration::from_millis(120);

#[derive(Clone)]
struct InitialStorageState {
    settings: AppSettings,
    history: ClipboardHistory,
    status: String,
}

pub(crate) fn style_head() -> String {
    use std::fmt::Write as _;

    let mut head = String::new();
    for style in APP_STYLES {
        let _ = writeln!(head, r#"<link rel="stylesheet" href="{style}">"#);
    }
    head
}

#[component]
pub fn App() -> Element {
    let initial_storage = use_hook(load_initial_storage);
    let initial_settings = initial_storage.settings.clone();
    let initial_history = initial_storage.history.clone();
    let initial_status = initial_storage.status.clone();
    let settings = use_signal(move || initial_settings);
    let history = use_signal(move || initial_history);
    let mut query = use_signal(String::new);
    let mut debounced_query = use_signal(String::new);
    let mut active_filter = use_signal(|| ClipboardFilter::All);
    let mut active_page = use_signal(|| AppPage::History);
    let search_input = use_signal(|| None::<Rc<MountedData>>);
    let mut shell = use_signal(|| None::<Rc<MountedData>>);
    let status = use_signal(move || initial_status);
    let ignored_clipboard_write = use_signal(|| None::<ClipboardContent>);
    let clipboard_monitor_paused = use_signal(|| false);
    let mut status_clear_generation = use_signal(|| 0_u64);
    let mut search_generation = use_signal(|| 0_u64);
    let mut startup_cleanup_done = use_signal(|| false);
    let mut suppress_window_control_hover = use_signal(|| false);
    let desktop = use_window();
    let mut global_shortcut_handle = use_signal(|| None::<ShortcutHandle>);
    let mut applied_global_shortcut = use_signal(String::new);
    let mut applied_widget_mode = use_signal(|| None::<(bool, bool)>);
    let mut applied_window_opacity = use_signal(|| None::<u8>);

    let _startup_command_sync = use_hook({
        let settings = settings;
        move || {
            if settings.peek().launch_at_startup {
                std::thread::spawn(|| {
                    let _ = crate::platform::startup::set_enabled(true);
                });
            }
        }
    });

    #[cfg(windows)]
    let _activation_task = use_hook({
        let desktop = desktop.clone();
        move || {
            spawn(async move {
                let mut last_activation_count =
                    crate::platform::single_instance::activation_count();
                let last_quit_count = crate::platform::single_instance::quit_count();
                loop {
                    Delay::new(Duration::from_millis(200)).await;
                    let quit_count = crate::platform::single_instance::quit_count();
                    if quit_count != last_quit_count {
                        desktop.set_close_behavior(WindowCloseBehaviour::WindowCloses);
                        desktop.close();
                        return;
                    }

                    let activation_count = crate::platform::single_instance::activation_count();
                    if activation_count != last_activation_count {
                        last_activation_count = activation_count;
                        show_desktop_window(&desktop);
                    }
                }
            })
        }
    });

    use_app_tray(
        desktop.clone(),
        settings,
        status,
        active_page,
        active_filter,
        settings.peek().language,
    );

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
            apply_window_mode(&desktop, widget_mode, topmost);

            let opacity = if widget_mode {
                settings_snapshot.background_opacity
            } else {
                DEFAULT_BACKGROUND_OPACITY
            };
            applied_window_opacity.set(Some(opacity));
            apply_window_opacity(&desktop, opacity);
        }
    });

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
            apply_window_opacity(&desktop, opacity);
        }
    });

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
                    status.set(invalid_shortcut_message(
                        &shortcut,
                        settings_snapshot.language,
                    ));
                    return;
                }
            };

            let shortcut_desktop = desktop.clone();
            match desktop.create_shortcut(hotkey, move |state| {
                if state == HotKeyState::Pressed {
                    show_desktop_window(&shortcut_desktop);
                }
            }) {
                Ok(handle) => {
                    global_shortcut_handle.set(Some(handle));
                }
                Err(error) => {
                    let mut status = status;
                    status.set(shortcut_error_message(
                        &error,
                        &shortcut,
                        settings_snapshot.language,
                    ));
                }
            }
        }
    });

    use_effect(move || {
        let message = status();
        if message.is_empty() {
            return;
        }

        let generation = *status_clear_generation.peek() + 1;
        status_clear_generation.set(generation);
        spawn(async move {
            Delay::new(STATUS_AUTO_CLEAR_DELAY).await;
            if *status_clear_generation.peek() == generation
                && status.peek().as_str() == message.as_str()
            {
                let mut status = status;
                status.set(String::new());
            }
        });
    });

    use_effect(move || {
        let next_query = query();
        let generation = *search_generation.peek() + 1;
        search_generation.set(generation);

        if next_query.is_empty() {
            debounced_query.set(String::new());
            return;
        }

        spawn(async move {
            Delay::new(SEARCH_DEBOUNCE_DELAY).await;
            if *search_generation.peek() == generation {
                debounced_query.set(next_query);
            }
        });
    });

    let _watcher = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        crate::clipboard_watcher::watch_clipboard(
            history,
            settings,
            status,
            ignored_clipboard_write,
            clipboard_monitor_paused,
        )
        .await;
    });

    use_effect(move || {
        if startup_cleanup_done() {
            return;
        }

        startup_cleanup_done.set(true);
        if let Some(days) = settings.peek().auto_cleanup_days {
            let language = settings.peek().language;
            match crate::clipboard_watcher::prune_history_by_age(history, days) {
                Ok(removed) if removed > 0 => {
                    let mut status = status;
                    status.set(match language {
                        AppLanguage::Chinese => format!("已自动清理 {removed} 项过期历史"),
                        AppLanguage::English => {
                            format!("Automatically cleaned up {removed} expired history items")
                        }
                    });
                }
                Err(error) => {
                    let mut status = status;
                    status.set(match language {
                        AppLanguage::Chinese => format!("自动清理历史失败：{error}"),
                        AppLanguage::English => format!("Failed to auto-clean history: {error}"),
                    });
                }
                _ => {}
            }
        }
    });

    let snapshot = use_memo(move || {
        history
            .read()
            .filtered(debounced_query().as_str(), active_filter())
    });
    let counts = use_memo(move || history.read().counts());
    let snapshot_entries = snapshot();
    let counts_snapshot = counts();
    let entry_count = snapshot_entries.len();
    let active_filter_snapshot = active_filter();
    let clear_history_count = history_count_for_filter(counts_snapshot, active_filter_snapshot);
    let settings_snapshot = settings();
    let language = settings_snapshot.language;
    let query_snapshot = debounced_query();
    let background_opacity = if settings_snapshot.desktop_widget {
        settings_snapshot.background_opacity
    } else {
        100
    };
    let shell_style = format!("--app-bg-alpha: {:.2};", background_opacity as f32 / 100.0);
    let theme = settings_snapshot.theme.key();
    let status_snapshot = status();
    let status_message = if status_snapshot.is_empty() {
        if clipboard_monitor_paused() {
            i18n::tr(language).clipboard_monitor_paused.to_string()
        } else {
            i18n::item_count(language, entry_count)
        }
    } else {
        status_snapshot
    };
    let close_desktop = desktop.clone();
    let topmost_desktop = desktop.clone();

    rsx! {
        main {
            class: "shell",
            "data-theme": theme,
            "data-suppress-window-hover": if suppress_window_control_hover() { "true" } else { "false" },
            style: "{shell_style}",
            tabindex: "-1",
            onmounted: move |event| {
                let element = event.data();
                shell.set(Some(element.clone()));
                spawn(async move {
                    let _ = element.set_focus(true).await;
                });
            },
            onkeydown: move |event| {
                let data = event.data();
                let modifiers = data.modifiers();
                let primary = modifiers.ctrl() || modifiers.meta();

                if !settings.read().keyboard_shortcuts {
                    return;
                }

                if primary && matches!(data.key(), Key::Character(key) if key.eq_ignore_ascii_case("f")) {
                    event.prevent_default();
                    active_page.set(AppPage::History);
                    if let Some(input) = search_input.read().clone() {
                        spawn(async move {
                            let _ = input.set_focus(true).await;
                        });
                    }
                    return;
                }

                if primary && matches!(data.key(), Key::Character(key) if key == ",") {
                    event.prevent_default();
                    active_page.set(if active_page() == AppPage::Settings {
                        AppPage::History
                    } else {
                        AppPage::Settings
                    });
                    return;
                }

                if primary && let Some(filter) = filter_shortcut(&data.key()) {
                    event.prevent_default();
                    active_page.set(AppPage::History);
                    active_filter.set(filter);
                    return;
                }

                if data.key() == Key::Escape {
                    if active_page() == AppPage::Settings {
                        event.prevent_default();
                        active_page.set(AppPage::History);
                    } else if !query.read().is_empty() || !debounced_query.read().is_empty() {
                        event.prevent_default();
                        query.set(String::new());
                        debounced_query.set(String::new());
                        if let Some(element) = shell.read().clone() {
                            spawn(async move {
                                let _ = element.set_focus(true).await;
                            });
                        }
                    }
                }
            },
            TopBar {
                query,
                active_page,
                search_input,
                keyboard_shortcuts: settings_snapshot.keyboard_shortcuts,
                widget_mode: settings_snapshot.desktop_widget,
                widget_topmost: settings_snapshot.desktop_widget_topmost,
                language,
                on_topmost_change: move |topmost| {
                    handle_widget_topmost_change(&topmost_desktop, settings, status, topmost);
                },
                on_window_controls_mouseleave: move |_| {
                    if suppress_window_control_hover() {
                        suppress_window_control_hover.set(false);
                    }
                },
                on_close: move |_| {
                    suppress_window_control_hover.set(true);
                    handle_window_close(&close_desktop, settings, status);
                },
            }
            section { class: "content-panel",
                if active_page() == AppPage::Settings {
                    SettingsPage {
                        active_page,
                        active_filter,
                        widget_mode: settings_snapshot.desktop_widget,
                        settings,
                        history,
                        status,
                    }
                } else {
                    HistoryList {
                        entries: snapshot_entries,
                        history,
                        ignored_clipboard_write,
                        query: query_snapshot,
                        active_filter,
                        counts: counts_snapshot,
                        keyboard_shortcuts: settings_snapshot.keyboard_shortcuts,
                        auto_focus: settings_snapshot.auto_focus_history,
                        promote_on_copy: settings_snapshot.promote_copied_entries,
                        quick_paste: settings_snapshot.quick_paste,
                        hide_after_copy: settings_snapshot.hide_after_copy && !settings_snapshot.desktop_widget,
                        show_copy_time: settings_snapshot.show_copy_time,
                        show_text_length: settings_snapshot.show_text_length,
                        language,
                        status,
                    }
                }
            }
            div {
                class: "status-bar",
                "aria-live": "polite",
                role: "status",
                span { class: "status-message", "{status_message}" }
                ClipboardMonitorButton {
                    paused: clipboard_monitor_paused,
                    language,
                    status,
                }
                StatusSettingsButton { active_page, language }
                ClearHistoryButton {
                    history,
                    filter: active_filter_snapshot,
                    history_count: clear_history_count,
                    language,
                    status,
                }
            }
        }
    }
}

fn invalid_shortcut_message(shortcut: &str, language: AppLanguage) -> String {
    match language {
        AppLanguage::Chinese => format!("全局快捷键配置无效：{shortcut}"),
        AppLanguage::English => format!("Invalid global shortcut configuration: {shortcut}"),
    }
}

fn shortcut_error_message(
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
            AppLanguage::Chinese => format!("全局快捷键 {shortcut} 注册失败"),
            AppLanguage::English => format!("Failed to register global shortcut {shortcut}"),
        },
    }
}

fn load_initial_storage() -> InitialStorageState {
    match storage::load_settings() {
        Ok(settings) => match storage::load_history(settings.history_limit) {
            Ok(history) => InitialStorageState {
                settings,
                history,
                status: String::new(),
            },
            Err(error) => {
                let history_limit = settings.history_limit;
                let language = settings.language;
                InitialStorageState {
                    settings,
                    history: ClipboardHistory::new(history_limit),
                    status: storage_initialization_failed(language, &error.to_string()),
                }
            }
        },
        Err(error) => {
            let settings = AppSettings::default();
            let history_limit = settings.history_limit;
            let language = settings.language;
            InitialStorageState {
                settings,
                history: ClipboardHistory::new(history_limit),
                status: storage_initialization_failed(language, &error.to_string()),
            }
        }
    }
}

fn storage_initialization_failed(language: AppLanguage, error: &str) -> String {
    match language {
        AppLanguage::Chinese => format!("存储初始化失败：{error}"),
        AppLanguage::English => format!("Failed to initialize storage: {error}"),
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
fn use_app_tray(
    desktop: DesktopContext,
    settings: Signal<AppSettings>,
    status: Signal<String>,
    active_page: Signal<AppPage>,
    active_filter: Signal<ClipboardFilter>,
    language: AppLanguage,
) {
    let mut tray_status = status;
    let _tray = use_hook(move || {
        use desktop::trayicon::TrayIconBuilder;
        use desktop::trayicon::menu::{Menu, MenuItem, PredefinedMenuItem};

        let copy = i18n::tr(language);
        let menu = Menu::new();
        let show_window = MenuItem::with_id(TRAY_SHOW_WINDOW_ID, copy.show_window, true, None);
        let open_widget =
            MenuItem::with_id(TRAY_OPEN_WIDGET_ID, copy.open_desktop_widget, true, None);
        let separator = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id(TRAY_QUIT_ID, copy.quit, true, None);
        menu.append_items(&[&show_window, &open_widget, &separator, &quit])
            .expect("tray menu creation failed");

        let tray_icon: Option<desktop::trayicon::DioxusTrayIcon> =
            desktop::icon_from_memory(APP_ICON_BYTES).ok();

        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false);
        let builder = if let Some(icon) = tray_icon {
            builder.with_icon(icon)
        } else {
            builder
        };

        match builder.build() {
            Ok(tray) => Some(tray),
            Err(error) => {
                tray_status.set(match language {
                    AppLanguage::Chinese => format!("系统托盘初始化失败：{error}"),
                    AppLanguage::English => format!("Failed to initialize system tray: {error}"),
                });
                None
            }
        }
    });

    let tray_desktop = desktop.clone();
    desktop::use_tray_menu_event_handler(move |event| {
        handle_app_tray_event(
            event.id().0.as_str(),
            &tray_desktop,
            settings,
            status,
            active_page,
            active_filter,
            language,
        );
    });

    desktop::use_muda_event_handler(move |event| {
        handle_app_tray_event(
            event.id().0.as_str(),
            &desktop,
            settings,
            status,
            active_page,
            active_filter,
            language,
        );
    });
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn use_app_tray(
    _desktop: DesktopContext,
    _settings: Signal<AppSettings>,
    _status: Signal<String>,
    _active_page: Signal<AppPage>,
    _active_filter: Signal<ClipboardFilter>,
    _language: AppLanguage,
) {
}

fn show_desktop_window(desktop: &DesktopContext) {
    desktop.set_visible(true);
    restore_desktop_window(desktop);
    desktop.set_focus();
}

fn handle_app_tray_event(
    event_id: &str,
    desktop: &DesktopContext,
    settings: Signal<AppSettings>,
    mut status: Signal<String>,
    active_page: Signal<AppPage>,
    active_filter: Signal<ClipboardFilter>,
    language: AppLanguage,
) {
    match event_id {
        TRAY_SHOW_WINDOW_ID => {
            show_desktop_window(desktop);
        }
        TRAY_OPEN_WIDGET_ID => {
            open_desktop_widget(desktop, settings, status, active_page, active_filter);
        }
        TRAY_QUIT_ID => {
            status.set(i18n::tr(language).exiting_app.to_string());
            desktop.set_close_behavior(WindowCloseBehaviour::WindowCloses);
            desktop.close();
        }
        _ => {}
    }
}

fn handle_window_close(
    desktop: &DesktopContext,
    mut settings: Signal<AppSettings>,
    mut status: Signal<String>,
) {
    let mut next = settings();
    if !next.desktop_widget {
        close_normal_window(desktop, status, next.language);
        return;
    }

    next.desktop_widget = false;
    next = next.normalized();
    match storage::save_settings(&next) {
        Ok(()) => {
            let language = next.language;
            let widget_topmost = next.desktop_widget_topmost;
            settings.set(next);
            apply_window_mode(desktop, false, widget_topmost);
            apply_window_opacity(desktop, DEFAULT_BACKGROUND_OPACITY);
            show_desktop_window(desktop);
            status.set(i18n::tr(language).settings_saved.to_string());
        }
        Err(error) => status.set(match next.language {
            AppLanguage::Chinese => format!("设置保存失败：{error}"),
            AppLanguage::English => format!("Failed to save settings: {error}"),
        }),
    }
}

#[cfg(windows)]
fn close_normal_window(desktop: &DesktopContext, _status: Signal<String>, _language: AppLanguage) {
    desktop.close();
}

#[cfg(not(windows))]
fn close_normal_window(
    desktop: &DesktopContext,
    mut status: Signal<String>,
    language: AppLanguage,
) {
    desktop.set_minimized(true);
    status.set(match language {
        AppLanguage::Chinese => "窗口已最小化；如果托盘可用，也可从托盘恢复".to_string(),
        AppLanguage::English => {
            "Window minimized; restore it from the dock/taskbar or tray if available".to_string()
        }
    });
}

fn open_desktop_widget(
    desktop: &DesktopContext,
    mut settings: Signal<AppSettings>,
    mut status: Signal<String>,
    mut active_page: Signal<AppPage>,
    mut active_filter: Signal<ClipboardFilter>,
) {
    let mut next = settings();
    let language = next.language;

    status.set(match language {
        AppLanguage::Chinese => "正在打开桌面小组件...".to_string(),
        AppLanguage::English => "Opening desktop widget...".to_string(),
    });

    active_filter.set(ClipboardFilter::All);
    active_page.set(AppPage::History);

    if next.desktop_widget {
        apply_window_mode(desktop, true, next.desktop_widget_topmost);
        apply_window_opacity(desktop, next.background_opacity);
        show_desktop_window(desktop);
        return;
    }

    next.desktop_widget = true;
    next = next.normalized();
    match storage::save_settings(&next) {
        Ok(()) => {
            let language = next.language;
            let widget_topmost = next.desktop_widget_topmost;
            let background_opacity = next.background_opacity;
            settings.set(next);
            apply_window_mode(desktop, true, widget_topmost);
            apply_window_opacity(desktop, background_opacity);
            show_desktop_window(desktop);
            status.set(i18n::tr(language).settings_saved.to_string());
        }
        Err(error) => {
            show_desktop_window(desktop);
            status.set(match next.language {
                AppLanguage::Chinese => format!("设置保存失败：{error}"),
                AppLanguage::English => format!("Failed to save settings: {error}"),
            });
        }
    }
}

fn handle_widget_topmost_change(
    desktop: &DesktopContext,
    mut settings: Signal<AppSettings>,
    mut status: Signal<String>,
    topmost: bool,
) {
    let mut next = settings();
    next.desktop_widget_topmost = topmost;
    next = next.normalized();

    match storage::save_settings(&next) {
        Ok(()) => {
            let widget_mode = next.desktop_widget;
            let widget_topmost = next.desktop_widget_topmost;
            let background_opacity = next.background_opacity;
            settings.set(next);
            apply_window_mode(desktop, widget_mode, widget_topmost);
            let opacity = if widget_mode {
                background_opacity
            } else {
                DEFAULT_BACKGROUND_OPACITY
            };
            apply_window_opacity(desktop, opacity);
        }
        Err(error) => status.set(match next.language {
            AppLanguage::Chinese => format!("设置保存失败：{error}"),
            AppLanguage::English => format!("Failed to save settings: {error}"),
        }),
    }
}

#[cfg(windows)]
fn restore_desktop_window(desktop: &DesktopContext) {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };

    let hwnd = desktop.window.hwnd() as _;
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

#[cfg(not(windows))]
fn restore_desktop_window(desktop: &DesktopContext) {
    desktop.set_minimized(false);
}

fn apply_window_mode(desktop: &DesktopContext, widget_mode: bool, widget_topmost: bool) {
    desktop.set_maximized(false);
    desktop.set_always_on_top(widget_mode && widget_topmost);
    desktop.set_visible_on_all_workspaces(widget_mode);
    desktop.set_resizable(!widget_mode);
    desktop.set_maximizable(!widget_mode);
    set_skip_taskbar(desktop, widget_mode);

    if widget_mode {
        desktop.set_title("UCP Clipboard Widget");
        desktop.set_min_inner_size(None::<LogicalSize<f64>>);
        desktop.set_inner_size(LogicalSize::new(WIDGET_WINDOW_WIDTH, WIDGET_WINDOW_HEIGHT));
    } else {
        desktop.set_title("UCP Clipboard");
        desktop.set_min_inner_size(Some(LogicalSize::new(
            NORMAL_WINDOW_MIN_WIDTH,
            NORMAL_WINDOW_MIN_HEIGHT,
        )));
        desktop.set_inner_size(LogicalSize::new(NORMAL_WINDOW_WIDTH, NORMAL_WINDOW_HEIGHT));
    }
}

#[cfg(windows)]
fn apply_window_opacity(desktop: &DesktopContext, opacity: u8) {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongW,
        WS_EX_LAYERED,
    };

    let hwnd = desktop.window.hwnd() as _;
    let alpha = ((opacity as u16 * u8::MAX as u16) / DEFAULT_BACKGROUND_OPACITY as u16) as u8;

    unsafe {
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if style & WS_EX_LAYERED as i32 == 0 {
            SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED as i32);
        }

        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
    }
}

#[cfg(not(windows))]
fn apply_window_opacity(_desktop: &DesktopContext, _opacity: u8) {}

#[cfg(windows)]
fn set_skip_taskbar(desktop: &DesktopContext, skip: bool) {
    use dioxus::desktop::tao::platform::windows::WindowExtWindows;

    let _ = desktop.window.set_skip_taskbar(skip);
}

#[cfg(target_os = "linux")]
fn set_skip_taskbar(desktop: &DesktopContext, skip: bool) {
    use dioxus::desktop::tao::platform::unix::WindowExtUnix;

    let _ = desktop.window.set_skip_taskbar(skip);
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn set_skip_taskbar(_desktop: &DesktopContext, _skip: bool) {}

fn filter_shortcut(key: &Key) -> Option<ClipboardFilter> {
    match key {
        Key::Character(key) if key == "1" => Some(ClipboardFilter::All),
        Key::Character(key) if key == "2" => Some(ClipboardFilter::Text),
        Key::Character(key) if key == "3" => Some(ClipboardFilter::Image),
        Key::Character(key) if key == "4" => Some(ClipboardFilter::File),
        Key::Character(key) if key == "5" => Some(ClipboardFilter::Favorite),
        _ => None,
    }
}
