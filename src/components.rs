use crate::model::{
    AppSettings, ClipboardContent, ClipboardEntry, ClipboardFilter, ClipboardHistory,
    HISTORY_LIMIT_OPTIONS, HistoryCounts,
};
use crate::platform;
use crate::storage;
use dioxus::desktop::use_window;
use dioxus::prelude::*;
use dioxus_primitives::combobox::{
    Combobox, ComboboxInput, ComboboxItemIndicator, ComboboxList, ComboboxOption,
};
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::switch::{Switch, SwitchThumb};
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton, ToolbarSeparator};

mod tabs;
use tabs::{TabList, TabTrigger, Tabs};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppPage {
    History,
    Settings,
}

#[component]
pub fn TopBar(query: Signal<String>, active_page: Signal<AppPage>) -> Element {
    let window = use_window();
    let drag_window = window.clone();
    let minimize_window = window.clone();
    let maximize_window = window.clone();
    let close_window = window;

    rsx! {
        Toolbar { class: "top-bar", aria_label: "剪贴板工具栏",
            div {
                class: "title-drag-region",
                onmousedown: move |_| drag_window.drag(),
                h1 { class: "app-title", "UCP Clipboard" }
            }
            if active_page() == AppPage::History {
                SearchField { query }
            } else {
                div { class: "top-bar-context", "设置" }
            }
            WindowControls {
                on_minimize: move |_| minimize_window.set_minimized(true),
                on_maximize: move |_| maximize_window.toggle_maximized(),
                on_close: move |_| close_window.close(),
            }
        }
    }
}

#[component]
pub fn FloatingSettingsButton(active_page: Signal<AppPage>) -> Element {
    let is_settings = active_page() == AppPage::Settings;
    let button_class = if is_settings {
        "floating-settings-action is-active"
    } else {
        "floating-settings-action"
    };
    let title = if is_settings {
        "返回历史"
    } else {
        "设置"
    };

    rsx! {
        Toolbar { class: "floating-settings", aria_label: "设置入口",
            ToolbarButton {
                class: button_class,
                index: 0usize,
                title,
                on_click: move |_| {
                    let next_page = if active_page() == AppPage::Settings {
                        AppPage::History
                    } else {
                        AppPage::Settings
                    };
                    active_page.set(next_page);
                },
                span { class: "floating-settings-icon", "⚙" }
            }
        }
    }
}

#[component]
fn WindowControls(
    on_minimize: EventHandler<()>,
    on_maximize: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "window-controls", aria_label: "窗口控制",
            button {
                class: "window-dot is-minimize",
                title: "最小化",
                onclick: move |_| on_minimize.call(()),
                span { "−" }
            }
            button {
                class: "window-dot is-maximize",
                title: "最大化或还原",
                onclick: move |_| on_maximize.call(()),
                span { "□" }
            }
            button {
                class: "window-dot is-close",
                title: "关闭",
                onclick: move |_| on_close.call(()),
                span { "×" }
            }
        }
    }
}

#[component]
fn SearchField(query: Signal<String>) -> Element {
    rsx! {
        label { class: "search-field",
            span { class: "search-icon", "⌕" }
            input {
                r#type: "search",
                placeholder: "搜索剪贴板历史",
                value: "{query}",
                oninput: move |event| query.set(event.value()),
            }
        }
    }
}

#[component]
pub fn FilterTabs(active_filter: Signal<ClipboardFilter>, counts: HistoryCounts) -> Element {
    let tabs = [
        (ClipboardFilter::All, "全部", counts.total),
        (ClipboardFilter::Text, "文本", counts.text),
        (ClipboardFilter::Image, "图像", counts.image),
        (ClipboardFilter::File, "文件", counts.file),
        (ClipboardFilter::Favorite, "收藏", counts.favorite),
    ];

    rsx! {
        Tabs {
            class: "filter-tabs-root",
            value: Some(active_filter().key().to_string()),
            on_value_change: move |key: String| active_filter.set(ClipboardFilter::from_key(&key)),
            horizontal: true,
            TabList { class: "filter-tabs", aria_label: "剪贴板类型筛选",
                for (index, (filter, label, count)) in tabs.into_iter().enumerate() {
                    FilterTab {
                        key: "{filter.key()}",
                        filter,
                        index,
                        label,
                        count,
                    }
                }
            }
        }
    }
}

#[component]
fn FilterTab(filter: ClipboardFilter, index: usize, label: &'static str, count: usize) -> Element {
    rsx! {
        TabTrigger {
            class: "filter-tab",
            value: filter.key().to_string(),
            index,
            span { class: "filter-tab-label", "{label}" }
            span { class: "filter-tab-count", "{count}" }
        }
    }
}

#[component]
pub fn HistoryList(
    entries: Vec<ClipboardEntry>,
    history: Signal<ClipboardHistory>,
    selected_count: usize,
    active_filter: Signal<ClipboardFilter>,
    counts: HistoryCounts,
) -> Element {
    rsx! {
        div { class: "list-header",
            h2 { "剪贴板历史" }
            FilterTabs { active_filter, counts }
            span { "{selected_count} 项" }
        }
        Separator { class: "list-separator", decorative: true }
        if entries.is_empty() {
            EmptyState {}
        } else {
            ScrollArea { class: "history-list", direction: ScrollDirection::Vertical, tabindex: "0",
                for (index, entry) in entries.iter().enumerate() {
                    HistoryRow {
                        key: "{entry.id}",
                        entry: entry.clone(),
                        index: index + 1,
                        history,
                    }
                }
            }
        }
    }
}

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

#[component]
fn HistoryRow(entry: ClipboardEntry, index: usize, history: Signal<ClipboardHistory>) -> Element {
    let id = entry.id;
    let copy_content = entry.content.clone();
    let row_class = match (index == 1, entry.favorite) {
        (true, true) => "history-row is-current is-favorite",
        (true, false) => "history-row is-current",
        (false, true) => "history-row is-favorite",
        (false, false) => "history-row",
    };
    let favorite_label = if entry.favorite {
        "取消收藏"
    } else {
        "收藏"
    };
    let pin_label = if entry.pinned {
        "取消固定"
    } else {
        "固定"
    };
    let kind_label = entry.kind().label();
    let entry_title = entry.title();
    let row_main_class = if matches!(&entry.content, ClipboardContent::Image(_)) {
        "history-row-main has-preview"
    } else {
        "history-row-main"
    };

    rsx! {
        article { class: "{row_class}",
            button {
                class: "{row_main_class}",
                onclick: move |_| {
                    if platform::clipboard::write_content(&copy_content).is_ok() {
                        if let Some(entry) = history.write().promote(id) {
                            let _ = storage::save_entry(&entry);
                        }
                    }
                },
                div { class: "entry-index", "{index}" }
                if let ClipboardContent::Image(image) = &entry.content {
                    if let Some(preview_url) = &image.preview_url {
                        img {
                            class: "entry-image-preview",
                            src: "{preview_url}",
                            alt: "剪贴板图像预览",
                        }
                    } else {
                        div { class: "entry-image-preview is-empty", "IMG" }
                    }
                }
                div { class: "entry-content",
                    div { class: "entry-kicker",
                        span { "{kind_label}" }
                        if entry.favorite {
                            span { class: "entry-favorite-badge", "已收藏" }
                        }
                        span { "{entry.age_label()}" }
                    }
                    p { class: if entry.is_text() { "entry-title" } else { "entry-title is-rich" }, "{entry_title}" }
                    p { class: "entry-size", "{entry.size_label()}" }
                }
            }
            Toolbar { class: "entry-actions", aria_label: "条目操作",
                ToolbarButton {
                    class: if entry.favorite { "ghost-action is-on" } else { "ghost-action" },
                    index: 0usize,
                    title: "{favorite_label}",
                    on_click: move |_| {
                        if let Some(entry) = history.write().toggle_favorite(id) {
                            let _ = storage::save_entry(&entry);
                        }
                    },
                    "★"
                }
                ToolbarButton {
                    class: if entry.pinned { "ghost-action is-on" } else { "ghost-action" },
                    index: 1usize,
                    title: "{pin_label}",
                    on_click: move |_| {
                        if let Some(entry) = history.write().toggle_pin(id) {
                            let _ = storage::save_entry(&entry);
                        }
                    },
                    "◆"
                }
                ToolbarSeparator { class: "entry-action-separator", decorative: true }
                ToolbarButton {
                    class: "ghost-action is-danger",
                    index: 2usize,
                    title: "删除",
                    on_click: move |_| {
                        if history.write().remove(id) {
                            let _ = storage::delete_entry(id);
                        }
                    },
                    "×"
                }
            }
        }
    }
}

#[component]
fn EmptyState() -> Element {
    rsx! {
        div { class: "empty-state",
            div { class: "empty-glyph", "⌘C" }
            h2 { "复制任意文本开始" }
            p { "UCP 会在后台监听剪贴板，并把新的文本内容整理成可搜索历史。" }
        }
    }
}
