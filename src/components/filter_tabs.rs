use super::tabs::{TabList, TabTrigger, Tabs};
use crate::model::{ClipboardFilter, HistoryCounts};
use dioxus::prelude::*;

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
