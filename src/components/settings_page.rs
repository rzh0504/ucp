use super::AppPage;
use crate::i18n;
use crate::model::{
    AUTO_CLEANUP_DAY_OPTIONS, AppLanguage, AppSettings, AppTheme, ClipboardFilter,
    ClipboardHistory, HISTORY_LIMIT_OPTIONS, MIN_BACKGROUND_OPACITY,
};
use crate::platform;
use crate::storage;
use chrono::{Duration as ChronoDuration, Local};
use dioxus::prelude::*;
use dioxus_primitives::combobox::{
    Combobox, ComboboxInput, ComboboxItemIndicator, ComboboxList, ComboboxOption,
};
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::switch::{Switch, SwitchThumb};

#[component]
pub fn SettingsPage(
    mut active_page: Signal<AppPage>,
    mut active_filter: Signal<ClipboardFilter>,
    widget_mode: bool,
    settings: Signal<AppSettings>,
    history: Signal<ClipboardHistory>,
    status: Signal<String>,
) -> Element {
    let settings_snapshot = settings();
    let language = settings_snapshot.language;
    let copy = i18n::tr(language);
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
                        checked: settings_snapshot.launch_at_startup,
                        on_change: move |checked| {
                            match platform::startup::set_enabled(checked) {
                                Ok(()) => {
                                    update_settings(settings, status, |next| next.launch_at_startup = checked);
                                }
                                Err(error) => {
                                    let mut status = status;
                                    status.set(match language {
                                        AppLanguage::Chinese => format!("开机启动设置失败：{error}"),
                                        AppLanguage::English => format!("Failed to update startup setting: {error}"),
                                    });
                                }
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
                                    match apply_auto_cleanup(history, days) {
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
                    SettingSwitchRow {
                        label: copy.auto_focus_history,
                        hint: copy.auto_focus_history_hint,
                        checked: settings_snapshot.auto_focus_history,
                        on_change: move |checked| {
                            update_settings(settings, status, |next| next.auto_focus_history = checked);
                        },
                    }
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
                }
            }
        }
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
                aria_label: label,
                on_checked_change: move |value| on_change.call(value),
                SwitchThumb { class: "settings-switch-thumb" }
            }
        }
    }
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
            settings.set(next);
            status.set(i18n::tr(next.language).settings_saved.to_string());
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
) -> Result<usize, storage::StorageError> {
    let cutoff = Local::now() - ChronoDuration::days(i64::from(days));
    storage::delete_entries_older_than(cutoff)?;
    Ok(history.write().remove_older_than_days(days))
}
