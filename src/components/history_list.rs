use super::filter_tabs::FilterTabs;
use super::icons::{AppIcon, Icon};
use crate::model::{
    ClipboardContent, ClipboardEntry, ClipboardFilter, ClipboardHistory, HistoryCounts,
};
use crate::platform;
use crate::storage;
use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton, ToolbarSeparator};

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
                    Icon { icon: AppIcon::Favorite }
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
                    Icon { icon: AppIcon::Pin }
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
