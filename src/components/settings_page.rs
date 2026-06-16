use crate::model::{AppSettings, ClipboardHistory, HISTORY_LIMIT_OPTIONS};
use crate::platform;
use crate::storage;
use dioxus::prelude::*;
use dioxus_primitives::combobox::{
    Combobox, ComboboxInput, ComboboxItemIndicator, ComboboxList, ComboboxOption,
};
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::switch::{Switch, SwitchThumb};

#[component]
pub fn SettingsPage(settings: Signal<AppSettings>, history: Signal<ClipboardHistory>) -> Element {
    let settings_snapshot = settings();

    rsx! {
        div { class: "list-header settings-header",
            h2 { "设置" }
            span { "应用偏好" }
        }
        Separator { class: "list-separator", decorative: true }
        ScrollArea { class: "settings-scroll", direction: ScrollDirection::Vertical, tabindex: "0",
            div { class: "settings-page",
                section { class: "settings-group",
                    h3 { "系统" }
                    SettingSwitchRow {
                        label: "开机启动",
                        hint: "登录 Windows 后自动启动 UCP Clipboard。",
                        checked: settings_snapshot.launch_at_startup,
                        on_change: move |checked| {
                            if platform::startup::set_enabled(checked).is_ok() {
                                update_settings(settings, |next| next.launch_at_startup = checked);
                            }
                        },
                    }
                }

                section { class: "settings-group",
                    h3 { "历史策略" }
                    div { class: "setting-row setting-row-control",
                        div { class: "setting-row-copy",
                            span { class: "setting-label", "历史保留数量" }
                            p { "超过上限时会自动清理较旧且未固定、未收藏的记录。" }
                        }
                        HistoryLimitCombobox {
                            value: settings_snapshot.history_limit,
                            on_change: move |limit| {
                                update_settings(settings, |next| next.history_limit = limit);
                                let removed_ids = history.write().set_capacity(limit);
                                let _ = storage::delete_entries(&removed_ids);
                            },
                        }
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
fn HistoryLimitCombobox(value: usize, on_change: EventHandler<usize>) -> Element {
    rsx! {
        Combobox::<usize> {
            class: "settings-combobox",
            default_value: Some(value),
            on_value_change: move |value: Option<usize>| {
                if let Some(limit) = value {
                    on_change.call(limit);
                }
            },
            ComboboxInput { class: "settings-combobox-input", placeholder: "选择保留数量" }
            ComboboxList { class: "settings-combobox-list",
                for (index, limit) in HISTORY_LIMIT_OPTIONS.into_iter().enumerate() {
                    ComboboxOption::<usize> {
                        class: "settings-combobox-option",
                        index,
                        value: limit,
                        text_value: Some(format!("{limit} 项")),
                        "{limit} 项"
                        ComboboxItemIndicator { span { "✓" } }
                    }
                }
            }
        }
    }
}

fn update_settings(mut settings: Signal<AppSettings>, update: impl FnOnce(&mut AppSettings)) {
    let mut next = settings();
    update(&mut next);
    next = next.normalized();
    settings.set(next);
    let _ = storage::save_settings(&next);
}
