use crate::model::{ClipboardEntry, ClipboardFilter, ClipboardHistory, HistoryCounts};
use crate::platform;
use dioxus::prelude::*;

#[component]
pub fn TopBar(query: Signal<String>, status: String) -> Element {
    rsx! {
        header { class: "top-bar",
            div { class: "traffic-lights", aria_label: "窗口状态",
                span { class: "traffic-dot is-close" }
                span { class: "traffic-dot is-minimize" }
                span { class: "traffic-dot is-zoom" }
            }
            h1 { class: "app-title", "UCP Clipboard" }
            SearchField { query }
            div { class: "status-card",
                span { class: "status-dot" }
                span { class: "status-text", "{status}" }
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
        nav { class: "filter-tabs", aria_label: "剪贴板类型筛选",
            for (filter, label, count) in tabs {
                FilterTab {
                    key: "{filter.key()}",
                    filter,
                    active_filter,
                    label,
                    count,
                }
            }
        }
    }
}

#[component]
fn FilterTab(
    filter: ClipboardFilter,
    active_filter: Signal<ClipboardFilter>,
    label: &'static str,
    count: usize,
) -> Element {
    rsx! {
        button {
            class: if active_filter() == filter { "filter-tab is-active" } else { "filter-tab" },
            onclick: move |_| active_filter.set(filter),
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
) -> Element {
    rsx! {
        section { class: "history-panel",
            div { class: "list-header",
                h2 { "剪贴板历史" }
                span { "{selected_count} 项" }
            }
            if entries.is_empty() {
                EmptyState {}
            } else {
                div { class: "history-list",
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
            div { class: "entry-actions", aria_label: "条目操作",
                button {
                    class: if entry.favorite { "ghost-action is-on" } else { "ghost-action" },
                    title: "{favorite_label}",
                    onclick: move |_| history.write().toggle_favorite(id),
                    "★"
                }
                button {
                    class: if entry.pinned { "ghost-action is-on" } else { "ghost-action" },
                    title: "{pin_label}",
                    onclick: move |_| history.write().toggle_pin(id),
                    "◆"
                }
                button {
                    class: "ghost-action is-danger",
                    title: "删除",
                    onclick: move |_| history.write().remove(id),
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
