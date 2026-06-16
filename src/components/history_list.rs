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
    entry_count: usize,
    active_filter: Signal<ClipboardFilter>,
    counts: HistoryCounts,
) -> Element {
    let mut selected_ids = use_signal(Vec::<u64>::new);
    let mut selection_anchor_id = use_signal(|| None::<u64>);
    let entry_ids = entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
    let visible_selected_count = selected_ids
        .read()
        .iter()
        .filter(|id| entry_ids.contains(id))
        .count();

    rsx! {
        div { class: "list-header",
            h2 { "剪贴板历史" }
            FilterTabs { active_filter, counts }
            span {
                if visible_selected_count == 0 {
                    "{entry_count} 项"
                } else {
                    "已选 {visible_selected_count} / {entry_count} 项"
                }
            }
        }
        Separator { class: "list-separator", decorative: true }
        if entries.is_empty() {
            EmptyState {}
        } else {
            div {
                class: "history-list-click-target",
                onclick: move |_| {
                    selected_ids.set(Vec::new());
                    selection_anchor_id.set(None);
                },
                ScrollArea { class: "history-list", direction: ScrollDirection::Vertical, tabindex: "0",
                    for (index, entry) in entries.iter().enumerate() {
                        HistoryRow {
                            key: "{entry.id}",
                            entry: entry.clone(),
                            index: index + 1,
                            entry_ids: entry_ids.clone(),
                            history,
                            selected_ids,
                            selection_anchor_id,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryRow(
    entry: ClipboardEntry,
    index: usize,
    entry_ids: Vec<u64>,
    history: Signal<ClipboardHistory>,
    mut selected_ids: Signal<Vec<u64>>,
    mut selection_anchor_id: Signal<Option<u64>>,
) -> Element {
    let id = entry.id;
    let copy_content = entry.content.clone();
    let row_class = if selected_ids.read().contains(&id) {
        "history-row is-selected"
    } else {
        "history-row"
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
        article {
            class: "{row_class}",
            onclick: move |event| event.stop_propagation(),
            button {
                class: "{row_main_class}",
                onclick: move |event| {
                    let modifiers = event.data.modifiers();
                    let mut selection = selected_ids.read().clone();
                    let mut anchor = selection_anchor_id();

                    update_selection(
                        &entry_ids,
                        &mut selection,
                        &mut anchor,
                        id,
                        modifiers.ctrl(),
                        modifiers.shift(),
                    );

                    selected_ids.set(selection);
                    selection_anchor_id.set(anchor);
                },
                ondoubleclick: move |_| {
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
                            FavoriteBadge {}
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

fn update_selection(
    entry_ids: &[u64],
    selected_ids: &mut Vec<u64>,
    anchor_id: &mut Option<u64>,
    id: u64,
    ctrl: bool,
    shift: bool,
) {
    if shift {
        let Some(anchor) = *anchor_id else {
            selected_ids.clear();
            selected_ids.push(id);
            *anchor_id = Some(id);
            return;
        };

        let Some(anchor_index) = entry_ids.iter().position(|entry_id| *entry_id == anchor) else {
            selected_ids.clear();
            selected_ids.push(id);
            *anchor_id = Some(id);
            return;
        };

        let Some(target_index) = entry_ids.iter().position(|entry_id| *entry_id == id) else {
            return;
        };

        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let range_ids = &entry_ids[start..=end];

        if ctrl {
            for range_id in range_ids {
                if !selected_ids.contains(range_id) {
                    selected_ids.push(*range_id);
                }
            }
        } else {
            selected_ids.clear();
            selected_ids.extend_from_slice(range_ids);
        }

        return;
    }

    if ctrl {
        if let Some(index) = selected_ids
            .iter()
            .position(|selected_id| *selected_id == id)
        {
            selected_ids.remove(index);
        } else {
            selected_ids.push(id);
        }
    } else {
        selected_ids.clear();
        selected_ids.push(id);
    }

    *anchor_id = Some(id);
}

#[component]
fn FavoriteBadge() -> Element {
    rsx! {
        span { class: "entry-favorite-badge", "收藏" }
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
