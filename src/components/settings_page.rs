use super::{AppIcon, AppPage, Icon};
use crate::i18n;
use crate::model::{
    AUTO_CLEANUP_DAY_OPTIONS, AppLanguage, AppSettings, AppTheme, ClipboardFilter,
    ClipboardHistory, HISTORY_LIMIT_OPTIONS, MIN_BACKGROUND_OPACITY,
};
use crate::platform;
use crate::storage;
use crate::updater::{self, UpdateCheck, UpdateInfo};
use chrono::{Duration as ChronoDuration, Local};
use dioxus::html::{Code, Key};
use dioxus::prelude::*;
use dioxus_primitives::combobox::{
    Combobox, ComboboxInput, ComboboxItemIndicator, ComboboxList, ComboboxOption,
};
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::switch::{Switch, SwitchThumb};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY_URL: &str = "https://github.com/rzh0504/ucp";

#[component]
pub fn SettingsPage(
    mut active_page: Signal<AppPage>,
    mut active_filter: Signal<ClipboardFilter>,
    widget_mode: bool,
    settings: Signal<AppSettings>,
    history: Signal<ClipboardHistory>,
    status: Signal<String>,
) -> Element {
    let startup_pending = use_signal(|| None::<bool>);
    let update_check = use_signal(|| UpdateCheckState::Idle);
    let settings_snapshot = settings();
    let language = settings_snapshot.language;
    let copy = i18n::tr(language);
    let startup_pending_value = startup_pending();
    let update_check_snapshot = update_check();
    let startup_checked = startup_pending_value.unwrap_or(settings_snapshot.launch_at_startup);
    let startup_disabled = startup_pending_value.is_some();
    let header_class = if widget_mode {
        "list-header settings-header is-widget"
    } else {
        "list-header settings-header"
    };

    rsx! {
        div { class: header_class,
            div { class: "settings-title-copy",
                h2 { "{copy.settings}" }
                span { "{copy.app_preferences}" }
            }
            button {
                class: "settings-back-action",
                type: "button",
                title: copy.back_to_content_title,
                onclick: move |_| active_page.set(AppPage::History),
                span { aria_hidden: "true", "←" }
                "{copy.back}"
            }
        }
        Separator { class: "list-separator", decorative: true }
        ScrollArea { class: "settings-scroll", direction: ScrollDirection::Vertical, tabindex: "0",
            div { class: "settings-page",
                section { class: "settings-group",
                    h3 { "{copy.display}" }
                    div { class: "setting-row setting-row-control",
                        div { class: "setting-row-copy",
                            span { class: "setting-label", "{copy.language}" }
                            p { "{copy.language_hint}" }
                        }
                        LanguageCombobox {
                            value: settings_snapshot.language,
                            on_change: move |language| {
                                update_settings(settings, status, |next| next.language = language);
                            },
                        }
                    }
                    div { class: "setting-row setting-row-control",
                        div { class: "setting-row-copy",
                            span { class: "setting-label", "{copy.theme}" }
                            p { "{copy.theme_hint}" }
                        }
                        ThemeCombobox {
                            value: settings_snapshot.theme,
                            language,
                            on_change: move |theme| {
                                update_settings(settings, status, |next| next.theme = theme);
                            },
                        }
                    }
                    SettingSwitchRow {
                        label: copy.desktop_widget,
                        hint: copy.desktop_widget_hint,
                        checked: settings_snapshot.desktop_widget,
                        on_change: move |checked| {
                            if update_settings(settings, status, |next| next.desktop_widget = checked) && checked {
                                active_filter.set(ClipboardFilter::All);
                                active_page.set(AppPage::History);
                            }
                        },
                    }
                    OpacitySliderRow {
                        label: copy.background_opacity,
                        hint: copy.background_opacity_hint,
                        value: settings_snapshot.background_opacity,
                        on_input: move |opacity| {
                            update_settings_in_memory(settings, |next| next.background_opacity = opacity);
                        },
                        on_commit: move |opacity| {
                            update_settings(settings, status, |next| next.background_opacity = opacity);
                        },
                    }
                    SettingSwitchRow {
                        label: copy.show_copy_time,
                        hint: copy.show_copy_time_hint,
                        checked: settings_snapshot.show_copy_time,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.show_copy_time = checked);
                        },
                    }
                    SettingSwitchRow {
                        label: copy.show_text_length,
                        hint: copy.show_text_length_hint,
                        checked: settings_snapshot.show_text_length,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.show_text_length = checked);
                        },
                    }
                }

                section { class: "settings-group",
                    h3 { "{copy.system}" }
                    SettingSwitchRow {
                        label: copy.launch_at_startup,
                        hint: copy.launch_at_startup_hint,
                        checked: startup_checked,
                        disabled: startup_disabled,
                        on_change: move |checked| {
                            if startup_pending.peek().is_some() {
                                return;
                            }

                            update_startup_setting(
                                checked,
                                language,
                                settings,
                                status,
                                startup_pending,
                            );
                        },
                    }
                }

                section { class: "settings-group",
                    h3 { "{copy.history_policy}" }
                    div { class: "setting-row setting-row-control",
                        div { class: "setting-row-copy",
                            span { class: "setting-label", "{copy.history_limit}" }
                            p { "{copy.history_limit_hint}" }
                        }
                        HistoryLimitCombobox {
                            value: settings_snapshot.history_limit,
                            language,
                            on_change: move |limit| {
                                if update_settings(settings, status, |next| next.history_limit = limit) {
                                    let removed_ids = history.write().set_capacity(limit);
                                    if let Err(error) = storage::delete_entries(&removed_ids) {
                                        let mut status = status;
                                        status.set(match language {
                                            AppLanguage::Chinese => format!("历史清理失败：{error}"),
                                            AppLanguage::English => format!("Failed to clean history: {error}"),
                                        });
                                    }
                                }
                            },
                        }
                    }
                    div { class: "setting-row setting-row-control",
                        div { class: "setting-row-copy",
                            span { class: "setting-label", "{copy.auto_cleanup}" }
                            p { "{copy.auto_cleanup_hint}" }
                        }
                        AutoCleanupCombobox {
                            value: settings_snapshot.auto_cleanup_days,
                            language,
                            on_change: move |days| {
                                if update_settings(settings, status, |next| next.auto_cleanup_days = days)
                                    && let Some(days) = days
                                {
                                    match apply_auto_cleanup(
                                        history,
                                        days,
                                        settings_snapshot.preserve_favorites_on_delete,
                                    ) {
                                        Ok(removed) if removed > 0 => {
                                            let mut status = status;
                                            status.set(match language {
                                                AppLanguage::Chinese => format!("已清理 {removed} 项过期历史"),
                                                AppLanguage::English => format!("Cleaned up {removed} expired history items"),
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
                            },
                        }
                    }
                    SettingSwitchRow {
                        label: copy.preserve_favorites_on_delete,
                        hint: copy.preserve_favorites_on_delete_hint,
                        checked: settings_snapshot.preserve_favorites_on_delete,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.preserve_favorites_on_delete = checked);
                        },
                    }
                }

                section { class: "settings-group",
                    h3 { "{copy.shortcuts_interaction}" }
                    SettingSwitchRow {
                        label: copy.keyboard_shortcuts,
                        hint: copy.keyboard_shortcuts_hint,
                        checked: settings_snapshot.keyboard_shortcuts,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.keyboard_shortcuts = checked);
                        },
                    }
                    ShortcutInputRow {
                        label: copy.global_shortcut,
                        hint: copy.global_shortcut_hint,
                        placeholder: copy.global_shortcut_placeholder,
                        value: settings_snapshot.global_show_shortcut.clone(),
                        on_commit: move |shortcut| {
                            update_settings(settings, status, |next| next.global_show_shortcut = shortcut);
                        },
                    }
                    SettingSwitchRow {
                        label: copy.auto_focus_history,
                        hint: copy.auto_focus_history_hint,
                        checked: settings_snapshot.auto_focus_history,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.auto_focus_history = checked);
                        },
                    }
                }

                section { class: "settings-group",
                    h3 { "{copy.copy_behavior}" }
                    SettingSwitchRow {
                        label: copy.promote_copied_entries,
                        hint: copy.promote_copied_entries_hint,
                        checked: settings_snapshot.promote_copied_entries,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.promote_copied_entries = checked);
                        },
                    }
                    SettingSwitchRow {
                        label: copy.quick_paste,
                        hint: copy.quick_paste_hint,
                        checked: settings_snapshot.quick_paste,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.quick_paste = checked);
                        },
                    }
                    SettingSwitchRow {
                        label: copy.hide_after_copy,
                        hint: copy.hide_after_copy_hint,
                        checked: settings_snapshot.hide_after_copy,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.hide_after_copy = checked);
                        },
                    }
                }

                section { class: "settings-group",
                    h3 { "{copy.about}" }
                    VersionRow {
                        language,
                        state: update_check_snapshot,
                        on_check: move |_| {
                            start_update_check(update_check);
                        },
                    }
                    div { class: "setting-row setting-row-control",
                        div { class: "setting-row-copy",
                            span { class: "setting-label", "{copy.open_source_repository}" }
                            p { "{copy.open_source_repository_hint}" }
                        }
                        a {
                            class: "settings-link",
                            href: REPOSITORY_URL,
                            target: "_blank",
                            rel: "noopener noreferrer",
                            title: REPOSITORY_URL,
                            aria_label: copy.open_source_repository,
                            Icon { icon: AppIcon::Github }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum UpdateCheckState {
    Idle,
    Checking,
    UpToDate(String),
    Available(UpdateInfo),
    Failed(String),
}

#[component]
fn VersionRow(
    language: AppLanguage,
    state: UpdateCheckState,
    on_check: EventHandler<()>,
) -> Element {
    let copy = i18n::tr(language);
    let checking = matches!(state, UpdateCheckState::Checking);
    let subtitle = version_subtitle(language, &state);
    let button_label = if checking {
        copy.checking_updates
    } else if matches!(state, UpdateCheckState::Available(_)) {
        copy.check_updates_again
    } else {
        copy.check_updates_button
    };
    let check_button_class = if checking {
        "settings-action-button settings-icon-action is-loading"
    } else {
        "settings-action-button settings-icon-action"
    };
    let download = match &state {
        UpdateCheckState::Available(info) => {
            let label = if info.asset_name.is_some() {
                copy.download_update
            } else {
                copy.open_release_page
            };
            Some((info.download_url.clone(), label))
        }
        _ => None,
    };

    rsx! {
        div { class: "setting-row setting-row-control",
            div { class: "setting-row-copy",
                span { class: "setting-label", "{copy.app_version}" }
                if !subtitle.is_empty() {
                    p { "{subtitle}" }
                }
            }
            div { class: "settings-version-control",
                button {
                    class: check_button_class,
                    type: "button",
                    disabled: checking,
                    title: button_label,
                    aria_label: button_label,
                    onclick: move |_| on_check.call(()),
                    Icon { icon: AppIcon::CheckUpdate }
                }
                if let Some((download_url, download_label)) = download {
                    a {
                        class: "settings-action-button is-primary",
                        href: "{download_url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        title: "{download_url}",
                        aria_label: download_label,
                        "{download_label}"
                    }
                }
                strong { "{APP_VERSION}" }
            }
        }
    }
}

fn version_subtitle(language: AppLanguage, state: &UpdateCheckState) -> String {
    match state {
        UpdateCheckState::Idle => i18n::update_up_to_date(language, APP_VERSION),
        UpdateCheckState::Checking => i18n::tr(language).checking_updates.to_string(),
        UpdateCheckState::UpToDate(version) => i18n::update_up_to_date(language, version),
        UpdateCheckState::Available(info) => i18n::update_available(language, &info.version),
        UpdateCheckState::Failed(error) => i18n::update_check_failed(language, error),
    }
}

#[component]
fn AutoCleanupCombobox(
    value: Option<u16>,
    language: AppLanguage,
    on_change: EventHandler<Option<u16>>,
) -> Element {
    let selected_value = use_memo(move || Some(value));
    let copy = i18n::tr(language);
    let options = AUTO_CLEANUP_DAY_OPTIONS
        .into_iter()
        .enumerate()
        .map(|(index, days)| (index, days, i18n::auto_cleanup_label(language, days)))
        .collect::<Vec<_>>();

    rsx! {
        Combobox::<Option<u16>> {
            class: "settings-combobox",
            value: Some(ReadSignal::from(selected_value)),
            on_value_change: move |value: Option<Option<u16>>| {
                if let Some(days) = value {
                    on_change.call(days);
                }
            },
            ComboboxInput { class: "settings-combobox-input", placeholder: copy.select_cleanup_period }
            ComboboxList { class: "settings-combobox-list",
                for (index, days, label) in options {
                    ComboboxOption::<Option<u16>> {
                        class: "settings-combobox-option",
                        index,
                        value: days,
                        text_value: Some(label.clone()),
                        "{label}"
                        ComboboxItemIndicator { span { "✓" } }
                    }
                }
            }
        }
    }
}

#[component]
fn SettingSwitchRow(
    label: &'static str,
    hint: &'static str,
    checked: bool,
    #[props(default = false)] disabled: bool,
    on_change: EventHandler<bool>,
) -> Element {
    rsx! {
        div { class: "setting-row setting-row-control",
            div { class: "setting-row-copy",
                span { class: "setting-label", "{label}" }
                p { "{hint}" }
            }
            Switch {
                class: "settings-switch",
                checked,
                disabled,
                aria_label: label,
                on_checked_change: move |value| on_change.call(value),
                SwitchThumb { class: "settings-switch-thumb" }
            }
        }
    }
}

#[component]
fn ShortcutInputRow(
    label: &'static str,
    hint: &'static str,
    placeholder: &'static str,
    value: String,
    on_commit: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "setting-row setting-row-control",
            div { class: "setting-row-copy",
                span { class: "setting-label", "{label}" }
                p { "{hint}" }
            }
            input {
                class: "settings-text-input",
                r#type: "text",
                value,
                placeholder,
                aria_label: label,
                readonly: true,
                spellcheck: "false",
                onkeydown: move |event| {
                    event.prevent_default();
                    event.stop_propagation();
                    let data = event.data();
                    if let Some(shortcut) = recorded_shortcut(data.key(), data.code(), data.modifiers().ctrl(), data.modifiers().alt(), data.modifiers().shift(), data.modifiers().meta()) {
                        on_commit.call(shortcut);
                    }
                },
            }
        }
    }
}

fn recorded_shortcut(
    key: Key,
    code: Code,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
) -> Option<String> {
    if !ctrl && !alt && !shift && !meta {
        return None;
    }

    if is_modifier_key(&key) || code == Code::Unidentified {
        return None;
    }

    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl".to_string());
    }
    if alt {
        parts.push("Alt".to_string());
    }
    if shift {
        parts.push("Shift".to_string());
    }
    if meta {
        parts.push("Super".to_string());
    }
    parts.push(shortcut_key_label(&code));

    Some(parts.join("+"))
}

fn shortcut_key_label(code: &Code) -> String {
    let label = match code {
        Code::Backquote => "`",
        Code::Backslash => "\\",
        Code::BracketLeft => "[",
        Code::BracketRight => "]",
        Code::Comma => ",",
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::Equal => "=",
        Code::KeyA => "A",
        Code::KeyB => "B",
        Code::KeyC => "C",
        Code::KeyD => "D",
        Code::KeyE => "E",
        Code::KeyF => "F",
        Code::KeyG => "G",
        Code::KeyH => "H",
        Code::KeyI => "I",
        Code::KeyJ => "J",
        Code::KeyK => "K",
        Code::KeyL => "L",
        Code::KeyM => "M",
        Code::KeyN => "N",
        Code::KeyO => "O",
        Code::KeyP => "P",
        Code::KeyQ => "Q",
        Code::KeyR => "R",
        Code::KeyS => "S",
        Code::KeyT => "T",
        Code::KeyU => "U",
        Code::KeyV => "V",
        Code::KeyW => "W",
        Code::KeyX => "X",
        Code::KeyY => "Y",
        Code::KeyZ => "Z",
        Code::Minus => "-",
        Code::Period => ".",
        Code::Quote => "'",
        Code::Semicolon => ";",
        Code::Slash => "/",
        _ => return code.to_string(),
    };

    label.to_string()
}

fn is_modifier_key(key: &Key) -> bool {
    matches!(
        key,
        Key::Alt
            | Key::AltGraph
            | Key::Control
            | Key::Fn
            | Key::FnLock
            | Key::Meta
            | Key::Shift
            | Key::Super
            | Key::Hyper
            | Key::CapsLock
            | Key::NumLock
            | Key::ScrollLock
    )
}

#[component]
fn HistoryLimitCombobox(
    value: usize,
    language: AppLanguage,
    on_change: EventHandler<usize>,
) -> Element {
    let selected_value = use_memo(move || Some(value));
    let copy = i18n::tr(language);
    let options = HISTORY_LIMIT_OPTIONS
        .into_iter()
        .enumerate()
        .map(|(index, limit)| (index, limit, i18n::item_count(language, limit)))
        .collect::<Vec<_>>();

    rsx! {
        Combobox::<usize> {
            class: "settings-combobox",
            value: Some(ReadSignal::from(selected_value)),
            on_value_change: move |value: Option<usize>| {
                if let Some(limit) = value {
                    on_change.call(limit);
                }
            },
            ComboboxInput { class: "settings-combobox-input", placeholder: copy.select_history_limit }
            ComboboxList { class: "settings-combobox-list",
                for (index, limit, label) in options {
                    ComboboxOption::<usize> {
                        class: "settings-combobox-option",
                        index,
                        value: limit,
                        text_value: Some(label.clone()),
                        "{label}"
                        ComboboxItemIndicator { span { "✓" } }
                    }
                }
            }
        }
    }
}

#[component]
fn LanguageCombobox(value: AppLanguage, on_change: EventHandler<AppLanguage>) -> Element {
    let selected_value = use_memo(move || Some(value));

    rsx! {
        Combobox::<AppLanguage> {
            class: "settings-combobox",
            value: Some(ReadSignal::from(selected_value)),
            on_value_change: move |value: Option<AppLanguage>| {
                if let Some(language) = value {
                    on_change.call(language);
                }
            },
            ComboboxInput { class: "settings-combobox-input", placeholder: "Language" }
            ComboboxList { class: "settings-combobox-list",
                for (index, language) in AppLanguage::OPTIONS.into_iter().enumerate() {
                    ComboboxOption::<AppLanguage> {
                        class: "settings-combobox-option",
                        index,
                        value: language,
                        text_value: Some(language.label().to_string()),
                        "{language.label()}"
                        ComboboxItemIndicator { span { "✓" } }
                    }
                }
            }
        }
    }
}

#[component]
fn ThemeCombobox(
    value: AppTheme,
    language: AppLanguage,
    on_change: EventHandler<AppTheme>,
) -> Element {
    let selected_value = use_memo(move || Some(value));
    let copy = i18n::tr(language);

    rsx! {
        Combobox::<AppTheme> {
            class: "settings-combobox",
            value: Some(ReadSignal::from(selected_value)),
            on_value_change: move |value: Option<AppTheme>| {
                if let Some(theme) = value {
                    on_change.call(theme);
                }
            },
            ComboboxInput { class: "settings-combobox-input", placeholder: copy.select_theme }
            ComboboxList { class: "settings-combobox-list",
                for (index, theme) in AppTheme::OPTIONS.into_iter().enumerate() {
                    ComboboxOption::<AppTheme> {
                        class: "settings-combobox-option",
                        index,
                        value: theme,
                        text_value: Some(theme.label(language).to_string()),
                        "{theme.label(language)}"
                        ComboboxItemIndicator { span { "✓" } }
                    }
                }
            }
        }
    }
}

#[component]
fn OpacitySliderRow(
    label: &'static str,
    hint: &'static str,
    value: u8,
    on_input: EventHandler<u8>,
    on_commit: EventHandler<u8>,
) -> Element {
    rsx! {
        div { class: "setting-row setting-row-control",
            div { class: "setting-row-copy",
                span { class: "setting-label", "{label}" }
                p { "{hint}" }
            }
            div { class: "settings-range-control",
                input {
                    class: "settings-range",
                    r#type: "range",
                    min: "{MIN_BACKGROUND_OPACITY}",
                    max: "100",
                    step: "5",
                    value: "{value}",
                    aria_label: label,
                    oninput: move |event| {
                        if let Ok(value) = event.value().parse::<u8>() {
                            on_input.call(value);
                        }
                    },
                    onchange: move |event| {
                        if let Ok(value) = event.value().parse::<u8>() {
                            on_commit.call(value);
                        }
                    },
                }
                strong { "{value}%" }
            }
        }
    }
}

fn update_startup_setting(
    checked: bool,
    language: AppLanguage,
    settings: Signal<AppSettings>,
    mut status: Signal<String>,
    mut startup_pending: Signal<Option<bool>>,
) {
    startup_pending.set(Some(checked));
    status.set(match language {
        AppLanguage::Chinese => "正在更新开机启动设置...".to_string(),
        AppLanguage::English => "Updating startup setting...".to_string(),
    });

    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(platform::startup::set_enabled(checked));
    });

    spawn(async move {
        let result = receiver
            .await
            .unwrap_or_else(|_| Err("startup setting task was cancelled".to_string()));
        startup_pending.set(None);

        match result {
            Ok(()) => {
                update_settings(settings, status, |next| next.launch_at_startup = checked);
            }
            Err(error) => {
                status.set(match language {
                    AppLanguage::Chinese => format!("开机启动设置失败：{error}"),
                    AppLanguage::English => format!("Failed to update startup setting: {error}"),
                });
            }
        }
    });
}

fn start_update_check(mut update_check: Signal<UpdateCheckState>) {
    if matches!(&*update_check.peek(), UpdateCheckState::Checking) {
        return;
    }

    update_check.set(UpdateCheckState::Checking);

    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(updater::check_for_updates());
    });

    spawn(async move {
        let result = receiver
            .await
            .unwrap_or_else(|_| Err("update check task was cancelled".to_string()));

        match result {
            Ok(UpdateCheck::Available(info)) => {
                update_check.set(UpdateCheckState::Available(info));
            }
            Ok(UpdateCheck::UpToDate { latest_version }) => {
                update_check.set(UpdateCheckState::UpToDate(latest_version));
            }
            Err(error) => {
                update_check.set(UpdateCheckState::Failed(error));
            }
        }
    });
}

fn update_settings_in_memory(
    mut settings: Signal<AppSettings>,
    update: impl FnOnce(&mut AppSettings),
) {
    let mut next = settings();
    update(&mut next);
    settings.set(next.normalized());
}

fn update_settings(
    mut settings: Signal<AppSettings>,
    mut status: Signal<String>,
    update: impl FnOnce(&mut AppSettings),
) -> bool {
    let mut next = settings();
    update(&mut next);
    next = next.normalized();

    match storage::save_settings(&next) {
        Ok(()) => {
            let language = next.language;
            settings.set(next);
            status.set(i18n::tr(language).settings_saved.to_string());
            true
        }
        Err(error) => {
            status.set(match next.language {
                AppLanguage::Chinese => format!("设置保存失败：{error}"),
                AppLanguage::English => format!("Failed to save settings: {error}"),
            });
            false
        }
    }
}

fn apply_auto_cleanup(
    mut history: Signal<ClipboardHistory>,
    days: u16,
    preserve_favorites: bool,
) -> Result<usize, storage::StorageError> {
    let cutoff = Local::now() - ChronoDuration::days(i64::from(days));
    storage::delete_entries_older_than(cutoff, preserve_favorites)?;
    Ok(history
        .write()
        .remove_older_than_days(days, preserve_favorites))
}
