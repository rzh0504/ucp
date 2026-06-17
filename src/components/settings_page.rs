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

                section { class: "settings-group",
                    h3 { "快捷与交互" }
                    SettingSwitchRow {
                        label: "键盘快捷键",
                        hint: "启用 Ctrl+F 搜索、Ctrl+, 切换设置、数字过滤和列表快捷操作。",
                        checked: settings_snapshot.keyboard_shortcuts,
                        on_change: move |checked| {
                            update_settings(settings, |next| next.keyboard_shortcuts = checked);
                        },
                    }
                    SettingSwitchRow {
                        label: "自动聚焦历史列表",
                        hint: "打开历史页后自动聚焦列表，方便直接使用方向键浏览。",
                        checked: settings_snapshot.auto_focus_history,
                        on_change: move |checked| {
                            update_settings(settings, |next| next.auto_focus_history = checked);
                        },
                    }
                    SettingSwitchRow {
                        label: "复制后置顶",
                        hint: "从历史中复制记录后，将该记录更新时间并移动到列表顶部。",
                        checked: settings_snapshot.promote_copied_entries,
                        on_change: move |checked| {
                            update_settings(settings, |next| next.promote_copied_entries = checked);
                        },
                    }
                    SettingSwitchRow {
                        label: "快捷粘贴",
                        hint: "右键历史项选择快捷粘贴后，将该内容粘贴到当前光标位置。",
                        checked: settings_snapshot.quick_paste,
                        on_change: move |checked| {
                            update_settings(settings, |next| next.quick_paste = checked);
                        },
                    }
                }

                section { class: "settings-group",
                    h3 { "显示" }
                    SettingSwitchRow {
                        label: "显示复制时间",
                        hint: "在历史记录中显示每项的复制时间。",
                        checked: settings_snapshot.show_copy_time,
                        on_change: move |checked| {
                            update_settings(settings, |next| next.show_copy_time = checked);
                        },
                    }
                    SettingSwitchRow {
                        label: "显示文本字符长度",
                        hint: "在文本记录中显示字符数量。",
                        checked: settings_snapshot.show_text_length,
                        on_change: move |checked| {
                            update_settings(settings, |next| next.show_text_length = checked);
                        },
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
    let selected_value = use_memo(move || Some(value));

    rsx! {
        Combobox::<usize> {
            class: "settings-combobox",
            value: Some(ReadSignal::from(selected_value)),
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
