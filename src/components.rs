use crate::model::{ClipboardEntry, ClipboardFilter, ClipboardHistory, HistoryCounts};
use crate::platform;
use dioxus::desktop::use_window;
use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton, ToolbarSeparator};

mod tabs;
use tabs::{TabList, TabTrigger, Tabs};

#[component]
pub fn TopBar(query: Signal<String>) -> Element {
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
            SearchField { query }
            WindowControls {
                on_minimize: move |_| minimize_window.set_minimized(true),
                on_maximize: move |_| maximize_window.toggle_maximized(),
                on_close: move |_| close_window.close(),
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
fn HistoryRow(entry: ClipboardEntry, index: usize, history: Signal<ClipboardHistory>) -> Element {
    let id = entry.id;
    let copy_text = entry.content.clone();
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
    let kind_label = entry.kind.label();

    rsx! {
        article { class: if index == 1 { "history-row is-current" } else { "history-row" },
            button {
                class: "history-row-main",
                onclick: move |_| {
                    if platform::clipboard::write_text(&copy_text).is_ok() {
                        history.write().promote(id);
                    }
                },
                div { class: "entry-index", "{index}" }
                div { class: "entry-content",
                    div { class: "entry-kicker",
                        span { "{kind_label}" }
                        span { "{entry.age_label()}" }
                    }
                    if entry.is_text() {
                        p { class: "entry-title", "{entry.content}" }
                    } else {
                        p { class: "entry-title is-muted", "{entry.kind.label()} 暂未启用" }
                    }
                    p { class: "entry-size", "{entry.size_label()}" }
                }
            }
            Toolbar { class: "entry-actions", aria_label: "条目操作",
                ToolbarButton {
                    class: if entry.favorite { "ghost-action is-on" } else { "ghost-action" },
                    index: 0usize,
                    title: "{favorite_label}",
                    on_click: move |_| history.write().toggle_favorite(id),
                    "★"
                }
                ToolbarButton {
                    class: if entry.pinned { "ghost-action is-on" } else { "ghost-action" },
                    index: 1usize,
                    title: "{pin_label}",
                    on_click: move |_| history.write().toggle_pin(id),
                    "◆"
                }
                ToolbarSeparator { class: "entry-action-separator", decorative: true }
                ToolbarButton {
                    class: "ghost-action is-danger",
                    index: 2usize,
                    title: "删除",
                    on_click: move |_| history.write().remove(id),
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
