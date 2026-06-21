use crate::components::{AppIcon, AppPage, HistoryList, Icon, SettingsPage, TopBar};
use crate::i18n;
use crate::model::{AppLanguage, AppSettings, ClipboardFilter, ClipboardHistory};
use crate::storage;
use dioxus::desktop::{
    self, DesktopContext, HotKeyState, ShortcutRegistryError, WindowCloseBehaviour,
    use_global_shortcut, use_window,
};
use dioxus::events::MountedData;
use dioxus::html::Key;
use dioxus::prelude::*;
use dioxus_primitives::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use futures_channel::mpsc::UnboundedReceiver;
use std::rc::Rc;

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
const GLOBAL_SHOW_SHORTCUT: &str = "Ctrl+Shift+V";
const TRAY_SHOW_WINDOW_ID: &str = "ucp-show-window";
const TRAY_QUIT_ID: &str = "ucp-quit";

#[derive(Clone)]
struct InitialStorageState {
    settings: AppSettings,
    history: ClipboardHistory,
    status: String,
}

#[component]
pub fn App() -> Element {
    let initial_storage = use_hook(load_initial_storage);
    let initial_settings = initial_storage.settings;
    let initial_history = initial_storage.history.clone();
    let initial_status = initial_storage.status.clone();
    let settings = use_signal(move || initial_settings);
    let history = use_signal(move || initial_history);
    let mut query = use_signal(String::new);
    let mut active_filter = use_signal(|| ClipboardFilter::All);
    let mut active_page = use_signal(|| AppPage::History);
    let search_input = use_signal(|| None::<Rc<MountedData>>);
    let mut shell = use_signal(|| None::<Rc<MountedData>>);
    let status = use_signal(move || initial_status);
    let mut startup_cleanup_done = use_signal(|| false);
    let desktop = use_window();
    let mut shortcut_error_reported = use_signal(|| false);

    use_app_tray(desktop.clone(), status, settings.peek().language);

    let global_shortcut = use_global_shortcut(GLOBAL_SHOW_SHORTCUT, {
        let desktop = desktop.clone();
        move |state| {
            if state == HotKeyState::Pressed {
                show_desktop_window(&desktop);
            }
        }
    });

    let shortcut_error = global_shortcut
        .as_ref()
        .err()
        .map(|error| shortcut_error_message(error, settings.peek().language));
    use_effect(move || {
        if let Some(error) = shortcut_error.as_ref()
            && !shortcut_error_reported()
        {
            shortcut_error_reported.set(true);
            let mut status = status;
            status.set(error.clone());
        }
    });

    let _watcher = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        crate::clipboard_watcher::watch_clipboard(history, settings, status).await;
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

    let snapshot = use_memo(move || history.read().filtered(query().as_str(), active_filter()));
    let counts = use_memo(move || history.read().counts());
    let snapshot_entries = snapshot();
    let counts_snapshot = counts();
    let entry_count = snapshot_entries.len();
    let settings_snapshot = settings();
    let language = settings_snapshot.language;
    let query_snapshot = query();

    rsx! {
        document::Link { rel: "stylesheet", href: STYLES }
        document::Link { rel: "stylesheet", href: BASE_STYLES }
        document::Link { rel: "stylesheet", href: LAYOUT_STYLES }
        document::Link { rel: "stylesheet", href: TOP_BAR_STYLES }
        document::Link { rel: "stylesheet", href: STATUS_STYLES }
        document::Link { rel: "stylesheet", href: DIALOG_STYLES }
        document::Link { rel: "stylesheet", href: FILTER_TABS_STYLES }
        document::Link { rel: "stylesheet", href: LIST_HEADER_STYLES }
        document::Link { rel: "stylesheet", href: HISTORY_LIST_STYLES }
        document::Link { rel: "stylesheet", href: SETTINGS_STYLES }
        document::Link { rel: "stylesheet", href: RESPONSIVE_STYLES }
        main {
            class: "shell",
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
                    } else if !query.read().is_empty() {
                        event.prevent_default();
                        query.set(String::new());
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
                language,
            }
            section { class: "content-panel",
                if active_page() == AppPage::Settings {
                    SettingsPage {
                        active_page,
                        settings,
                        history,
                        status,
                    }
                } else {
                    HistoryList {
                        entries: snapshot_entries,
                        history,
                        entry_count,
                        query: query_snapshot,
                        active_filter,
                        counts: counts_snapshot,
                        keyboard_shortcuts: settings_snapshot.keyboard_shortcuts,
                        auto_focus: settings_snapshot.auto_focus_history,
                        promote_on_copy: settings_snapshot.promote_copied_entries,
                        quick_paste: settings_snapshot.quick_paste,
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
                span { class: "status-message", "{status}" }
                StatusSettingsButton { active_page, language }
                ClearHistoryButton {
                    history,
                    history_count: counts_snapshot.total,
                    language,
                    status,
                }
            }
        }
    }
}

fn shortcut_error_message(error: &ShortcutRegistryError, language: AppLanguage) -> String {
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
                        format!("全局快捷键 {GLOBAL_SHOW_SHORTCUT} 已被占用，仍可通过托盘打开窗口")
                    }
                    AppLanguage::English => format!(
                        "Global shortcut {GLOBAL_SHOW_SHORTCUT} is already in use. You can still open the window from the tray"
                    ),
                }
            } else {
                match language {
                    AppLanguage::Chinese => {
                        format!("全局快捷键 {GLOBAL_SHOW_SHORTCUT} 注册失败：{message}")
                    }
                    AppLanguage::English => format!(
                        "Failed to register global shortcut {GLOBAL_SHOW_SHORTCUT}: {message}"
                    ),
                }
            }
        }
        _ => match language {
            AppLanguage::Chinese => format!("全局快捷键 {GLOBAL_SHOW_SHORTCUT} 注册失败"),
            AppLanguage::English => {
                format!("Failed to register global shortcut {GLOBAL_SHOW_SHORTCUT}")
            }
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
            Err(error) => InitialStorageState {
                settings,
                history: ClipboardHistory::new(settings.history_limit),
                status: storage_initialization_failed(settings.language, &error.to_string()),
            },
        },
        Err(error) => {
            let settings = AppSettings::default();
            InitialStorageState {
                settings,
                history: ClipboardHistory::new(settings.history_limit),
                status: storage_initialization_failed(settings.language, &error.to_string()),
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

#[component]
fn StatusSettingsButton(mut active_page: Signal<AppPage>, language: AppLanguage) -> Element {
    let is_settings = active_page() == AppPage::Settings;
    let copy = i18n::tr(language);
    let button_class = if is_settings {
        "status-icon-action status-settings-action is-active"
    } else {
        "status-icon-action status-settings-action"
    };

    rsx! {
        button {
            class: button_class,
            type: "button",
            title: if is_settings { copy.on_settings_page } else { copy.settings },
            aria_label: if is_settings { copy.on_settings_page } else { copy.open_settings },
            onclick: move |_| active_page.set(AppPage::Settings),
            Icon { icon: AppIcon::Settings }
        }
    }
}

#[component]
fn ClearHistoryButton(
    history: Signal<ClipboardHistory>,
    history_count: usize,
    language: AppLanguage,
    mut status: Signal<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let disabled = history_count == 0;
    let copy = i18n::tr(language);

    rsx! {
        button {
            class: "status-icon-action status-clear-action",
            type: "button",
            disabled,
            title: if disabled { copy.no_history_to_clear } else { copy.clear_all_history },
            aria_label: if disabled { copy.no_history_to_clear } else { copy.clear_all_history },
            onclick: move |_| open.set(true),
            Icon { icon: AppIcon::Clear }
        }
        AlertDialogRoot {
            open: open(),
            on_open_change: move |value| open.set(value),
            div { class: "alert-dialog-backdrop" }
            AlertDialogContent { class: "alert-dialog-content",
                AlertDialogTitle { class: "alert-dialog-title", "{copy.clear_all_history_title}" }
                AlertDialogDescription { class: "alert-dialog-description",
                    "{copy.clear_all_history_description}"
                }
                AlertDialogActions { class: "alert-dialog-actions",
                    AlertDialogCancel { class: "alert-dialog-button", "{copy.cancel}" }
                    AlertDialogAction {
                        class: "alert-dialog-button is-danger",
                        on_click: move |_| {
                            match storage::clear_history() {
                                Ok(()) => {
                                    history.write().clear();
                                    status.set(i18n::tr(language).history_cleared.to_string());
                                }
                                Err(error) => status.set(match language {
                                    AppLanguage::Chinese => format!("历史清空失败：{error}"),
                                    AppLanguage::English => format!("Failed to clear history: {error}"),
                                }),
                            }
                        },
                        "{copy.confirm_clear}"
                    }
                }
            }
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
fn use_app_tray(desktop: DesktopContext, mut status: Signal<String>, language: AppLanguage) {
    let _tray = use_hook(|| {
        use desktop::trayicon::menu::{Menu, MenuItem, PredefinedMenuItem};

        let copy = i18n::tr(language);
        let menu = Menu::new();
        let show_window = MenuItem::with_id(TRAY_SHOW_WINDOW_ID, copy.show_window, true, None);
        let separator = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id(TRAY_QUIT_ID, copy.quit, true, None);
        menu.append_items(&[&show_window, &separator, &quit])
            .expect("tray menu creation failed");

        desktop::trayicon::init_tray_icon(menu, None)
    });

    desktop::use_tray_menu_event_handler(move |event| match event.id().0.as_str() {
        TRAY_SHOW_WINDOW_ID => {
            show_desktop_window(&desktop);
        }
        TRAY_QUIT_ID => {
            status.set(i18n::tr(language).exiting_app.to_string());
            desktop.set_close_behavior(WindowCloseBehaviour::WindowCloses);
            desktop.close();
        }
        _ => {}
    });
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn use_app_tray(_desktop: DesktopContext, _status: Signal<String>, _language: AppLanguage) {}

fn show_desktop_window(desktop: &DesktopContext) {
    desktop.set_visible(true);
    desktop.set_minimized(false);
    desktop.set_focus();
}

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
