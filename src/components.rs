use crate::model::{ClipboardEntry, ClipboardFilter, ClipboardHistory, HistoryCounts};
use crate::platform;
use dioxus::prelude::*;

#[component]
pub fn TopBar(query: Signal<String>, status: String) -> Element {
    rsx! {
        header { class: "top-bar",
            BrandBlock {}
            SearchField { query }
            div { class: "status-card",
                span { class: "status-dot" }
                span { class: "status-text", "{status}" }
            }
        }
    }
}

#[component]
fn BrandBlock() -> Element {
    rsx! {
        div { class: "brand-block",
            div { class: "brand-mark", "U" }
            div {
                p { class: "eyebrow", "Universal Clipboard" }
                h1 { "UCP" }
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
                placeholder: "搜索文本、片段或命令",
                value: "{query}",
                oninput: move |event| query.set(event.value()),
            }
        }
    }
}

#[component]
pub fn FilterTabs(active_filter: Signal<ClipboardFilter>, counts: HistoryCounts) -> Element {
    let tabs = [
        (ClipboardFilter::All, "All", "全部", counts.total),
        (ClipboardFilter::Text, "Txt", "文本", counts.text),
        (ClipboardFilter::Image, "Img", "图像", counts.image),
        (ClipboardFilter::File, "File", "文件", counts.file),
        (ClipboardFilter::Favorite, "Star", "收藏", counts.favorite),
    ];

    rsx! {
        nav { class: "filter-tabs", aria_label: "剪贴板类型筛选",
            for (filter, icon, label, count) in tabs {
                FilterTab {
                    key: "{filter.key()}",
                    filter,
                    active_filter,
                    icon,
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
    icon: &'static str,
    label: &'static str,
    count: usize,
) -> Element {
    rsx! {
        button {
            class: if active_filter() == filter { "filter-tab is-active" } else { "filter-tab" },
            onclick: move |_| active_filter.set(filter),
            span { class: "filter-tab-icon", "{icon}" }
            span { class: "filter-tab-label", "{label}" }
            span { class: "filter-tab-count", "{count}" }
        }
    }
}

#[component]
pub fn HistoryList(entries: Vec<ClipboardEntry>, history: Signal<ClipboardHistory>) -> Element {
    let entry_count = entries.len();

    rsx! {
        section { class: "history-panel",
            div { class: "panel-heading",
                div {
                    p { class: "eyebrow", "History" }
                    h2 { "剪贴板历史" }
                }
                span { class: "result-count", "{entry_count} 条" }
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
        article { class: if index == 1 { "history-card is-current" } else { "history-card" },
            button {
                class: "history-card-main",
                onclick: move |_| {
                    if platform::clipboard::write_text(&copy_text).is_ok() {
                        history.write().promote(id);
                    }
                },
                div { class: "entry-badge", "{index}" }
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
            div { class: "empty-glyph", "⌘" }
            h2 { "复制任意文本开始" }
            p { "UCP 会在后台监听剪贴板，并把新的文本内容整理成可搜索历史。" }
        }
    }
}

#[component]
pub fn ActionRail(
    platform_label: &'static str,
    strategy_label: &'static str,
    selected_count: usize,
) -> Element {
    rsx! {
        aside { class: "action-rail",
            div { class: "rail-section rail-hero",
                span { class: "rail-label", "Session" }
                strong { "{selected_count}" }
                span { "当前结果" }
            }
            div { class: "rail-section",
                p { class: "rail-label", "Quick Actions" }
                button { class: "rail-command is-primary", title: "复制最近一条", "复制最近" }
                button { class: "rail-command", title: "仅查看收藏", "查看收藏" }
                button { class: "rail-command", title: "清理历史", "清理历史" }
            }
            div { class: "rail-spacer" }
            div { class: "rail-section rail-system",
                p { class: "rail-label", "System" }
                dl {
                    div {
                        dt { "平台" }
                        dd { "{platform_label}" }
                    }
                    div {
                        dt { "监听" }
                        dd { "{strategy_label}" }
                    }
                }
            }
        }
    }
}
