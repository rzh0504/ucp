use crate::components::{AppIcon, AppPage, Icon};
use crate::i18n;
use crate::model::{AppLanguage, ClipboardFilter, ClipboardHistory, HistoryCounts};
use crate::storage;
use dioxus::prelude::*;
use dioxus_primitives::alert_dialog::{
    AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogContent,
    AlertDialogDescription, AlertDialogRoot, AlertDialogTitle,
};
use dioxus_primitives::select::{
    Select, SelectItemIndicator, SelectList, SelectOption, SelectTrigger,
};
use futures_timer::Delay;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipboardMonitorPauseDuration {
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    UntilResume,
}

impl ClipboardMonitorPauseDuration {
    const OPTIONS: [Self; 5] = [
        Self::FiveMinutes,
        Self::FifteenMinutes,
        Self::ThirtyMinutes,
        Self::OneHour,
        Self::UntilResume,
    ];

    fn duration(self) -> Option<Duration> {
        match self {
            Self::FiveMinutes => Some(Duration::from_secs(5 * 60)),
            Self::FifteenMinutes => Some(Duration::from_secs(15 * 60)),
            Self::ThirtyMinutes => Some(Duration::from_secs(30 * 60)),
            Self::OneHour => Some(Duration::from_secs(60 * 60)),
            Self::UntilResume => None,
        }
    }
}

#[component]
pub(super) fn ClipboardMonitorButton(
    mut paused: Signal<bool>,
    language: AppLanguage,
    status: Signal<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let generation = use_signal(|| 0_u64);
    let mut selected_duration = use_signal(|| ClipboardMonitorPauseDuration::FiveMinutes);
    let selected_value = use_memo(move || Some(selected_duration()));
    let is_paused = paused();
    let copy = i18n::tr(language);
    let button_class = if is_paused {
        "status-icon-action status-monitor-action is-paused"
    } else {
        "status-icon-action status-monitor-action"
    };

    rsx! {
        button {
            class: button_class,
            type: "button",
            title: if is_paused { copy.resume_clipboard_monitor } else { copy.pause_clipboard_monitor },
            aria_label: if is_paused { copy.resume_clipboard_monitor } else { copy.pause_clipboard_monitor },
            onclick: move |_| {
                if paused() {
                    resume_clipboard_monitor(paused, status, generation, language);
                } else {
                    open.set(true);
                }
            },
            Icon { icon: if is_paused { AppIcon::Play } else { AppIcon::Pause } }
        }
        AlertDialogRoot {
            open: open(),
            on_open_change: move |value| open.set(value),
            div { class: "alert-dialog-backdrop" }
            AlertDialogContent { class: "alert-dialog-content",
                AlertDialogTitle { class: "alert-dialog-title", "{copy.pause_clipboard_monitor_title}" }
                AlertDialogDescription { class: "alert-dialog-description",
                    "{copy.pause_clipboard_monitor_description}"
                }
                AlertDialogActions { class: "alert-dialog-actions",
                    Select::<ClipboardMonitorPauseDuration> {
                        class: "alert-dialog-select",
                        value: Some(ReadSignal::from(selected_value)),
                        on_value_change: move |value: Option<ClipboardMonitorPauseDuration>| {
                            if let Some(value) = value {
                                selected_duration.set(value);
                            }
                        },
                        SelectTrigger {
                            class: "alert-dialog-select-trigger",
                            aria_label: copy.pause_duration,
                            span {
                                class: "alert-dialog-select-value",
                                "{pause_duration_label(language, selected_duration())}"
                            }
                        }
                        SelectList {
                            class: "alert-dialog-select-list",
                            aria_label: copy.pause_duration,
                            for (index, option) in ClipboardMonitorPauseDuration::OPTIONS.into_iter().enumerate() {
                                SelectOption::<ClipboardMonitorPauseDuration> {
                                    class: "alert-dialog-select-option",
                                    index,
                                    value: option,
                                    text_value: Some(pause_duration_label(language, option).to_string()),
                                    "{pause_duration_label(language, option)}"
                                    SelectItemIndicator { span { "✓" } }
                                }
                            }
                        }
                    }
                    AlertDialogCancel { class: "alert-dialog-button", "{copy.cancel}" }
                    AlertDialogAction {
                        class: "alert-dialog-button is-primary",
                        on_click: move |_| {
                            pause_clipboard_monitor(
                                selected_duration(),
                                paused,
                                status,
                                generation,
                                language,
                            );
                        },
                        "{copy.confirm_pause}"
                    }
                }
            }
        }
    }
}

fn pause_clipboard_monitor(
    pause_duration: ClipboardMonitorPauseDuration,
    mut paused: Signal<bool>,
    mut status: Signal<String>,
    generation: Signal<u64>,
    language: AppLanguage,
) {
    match pause_duration.duration() {
        Some(duration) => {
            let next_generation = pause_clipboard_monitor_now(paused, generation);
            status.set(clipboard_monitor_paused_for_duration_message(
                language,
                pause_duration,
            ));

            spawn(async move {
                Delay::new(duration).await;
                if *generation.peek() == next_generation && *paused.peek() {
                    paused.set(false);
                    status.set(i18n::tr(language).clipboard_monitor_resumed.to_string());
                }
            });
        }
        None => {
            pause_clipboard_monitor_now(paused, generation);
            status.set(
                i18n::tr(language)
                    .clipboard_monitor_paused_until_resume
                    .to_string(),
            );
        }
    }
}

fn pause_clipboard_monitor_now(mut paused: Signal<bool>, mut generation: Signal<u64>) -> u64 {
    let next_generation = *generation.peek() + 1;
    generation.set(next_generation);
    paused.set(true);
    next_generation
}

fn resume_clipboard_monitor(
    mut paused: Signal<bool>,
    mut status: Signal<String>,
    mut generation: Signal<u64>,
    language: AppLanguage,
) {
    let next_generation = *generation.peek() + 1;
    generation.set(next_generation);
    paused.set(false);
    status.set(i18n::tr(language).clipboard_monitor_resumed.to_string());
}

fn pause_duration_label(
    language: AppLanguage,
    duration: ClipboardMonitorPauseDuration,
) -> &'static str {
    match (language, duration) {
        (AppLanguage::Chinese, ClipboardMonitorPauseDuration::FiveMinutes) => "5 分钟",
        (AppLanguage::Chinese, ClipboardMonitorPauseDuration::FifteenMinutes) => "15 分钟",
        (AppLanguage::Chinese, ClipboardMonitorPauseDuration::ThirtyMinutes) => "30 分钟",
        (AppLanguage::Chinese, ClipboardMonitorPauseDuration::OneHour) => "1 小时",
        (AppLanguage::Chinese, ClipboardMonitorPauseDuration::UntilResume) => "直到手动恢复",
        (AppLanguage::English, ClipboardMonitorPauseDuration::FiveMinutes) => "5 minutes",
        (AppLanguage::English, ClipboardMonitorPauseDuration::FifteenMinutes) => "15 minutes",
        (AppLanguage::English, ClipboardMonitorPauseDuration::ThirtyMinutes) => "30 minutes",
        (AppLanguage::English, ClipboardMonitorPauseDuration::OneHour) => "1 hour",
        (AppLanguage::English, ClipboardMonitorPauseDuration::UntilResume) => "Until resumed",
    }
}

fn clipboard_monitor_paused_for_duration_message(
    language: AppLanguage,
    duration: ClipboardMonitorPauseDuration,
) -> String {
    let duration = pause_duration_label(language, duration);
    match language {
        AppLanguage::Chinese => format!("已暂停剪贴板监听 {duration}"),
        AppLanguage::English => format!("Clipboard monitoring paused for {duration}"),
    }
}

#[component]
pub(super) fn StatusSettingsButton(
    mut active_page: Signal<AppPage>,
    language: AppLanguage,
) -> Element {
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
pub(super) fn ClearHistoryButton(
    history: Signal<ClipboardHistory>,
    filter: ClipboardFilter,
    history_count: usize,
    language: AppLanguage,
    mut status: Signal<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let disabled = history_count == 0;
    let copy = i18n::tr(language);
    let clear_label = i18n::clear_history_label(language, filter);
    let button_label = if disabled {
        copy.no_history_to_clear.to_string()
    } else {
        clear_label
    };
    let dialog_title = i18n::clear_history_title(language, filter);
    let dialog_description = i18n::clear_history_description(language, filter);

    rsx! {
        button {
            class: "status-icon-action status-clear-action",
            type: "button",
            disabled,
            title: button_label.clone(),
            aria_label: button_label.clone(),
            onclick: move |_| open.set(true),
            Icon { icon: AppIcon::Clear }
        }
        AlertDialogRoot {
            open: open(),
            on_open_change: move |value| open.set(value),
            div { class: "alert-dialog-backdrop" }
            AlertDialogContent { class: "alert-dialog-content",
                AlertDialogTitle { class: "alert-dialog-title", "{dialog_title}" }
                AlertDialogDescription { class: "alert-dialog-description",
                    "{dialog_description}"
                }
                AlertDialogActions { class: "alert-dialog-actions",
                    AlertDialogCancel { class: "alert-dialog-button", "{copy.cancel}" }
                    AlertDialogAction {
                        class: "alert-dialog-button is-danger",
                        on_click: move |_| {
                            match clear_history_for_filter(history, filter) {
                                Ok(()) => {
                                    status.set(i18n::history_cleared_message(language, filter));
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

pub(super) fn history_count_for_filter(counts: HistoryCounts, filter: ClipboardFilter) -> usize {
    match filter {
        ClipboardFilter::All => counts.total,
        ClipboardFilter::Text => counts.text,
        ClipboardFilter::Image => counts.image,
        ClipboardFilter::File => counts.file,
        ClipboardFilter::Favorite => counts.favorite,
    }
}

fn clear_history_for_filter(
    mut history: Signal<ClipboardHistory>,
    filter: ClipboardFilter,
) -> Result<(), storage::StorageError> {
    if filter == ClipboardFilter::All {
        storage::clear_history()?;
        history.write().clear();
        return Ok(());
    }

    let ids = history.read().ids_for_filter(filter);
    if ids.is_empty() {
        return Ok(());
    }

    storage::delete_entries(&ids)?;

    let mut history = history.write();
    for id in ids {
        history.remove(id);
    }

    Ok(())
}
