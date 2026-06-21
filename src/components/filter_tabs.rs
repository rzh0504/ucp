use super::icons::{AppIcon, Icon};
use super::tabs::{TabList, TabTrigger, Tabs};
use crate::i18n;
use crate::model::AppLanguage;
use crate::model::{ClipboardFilter, HistoryCounts};
use dioxus::prelude::*;

#[component]
pub fn FilterTabs(
    active_filter: Signal<ClipboardFilter>,
    counts: HistoryCounts,
    language: AppLanguage,
) -> Element {
    let tabs = [
        (ClipboardFilter::All, counts.total),
        (ClipboardFilter::Text, counts.text),
        (ClipboardFilter::Image, counts.image),
        (ClipboardFilter::File, counts.file),
        (ClipboardFilter::Favorite, counts.favorite),
    ];
    let copy = i18n::tr(language);

    rsx! {
        Tabs {
            class: "filter-tabs-root",
            value: Some(active_filter().key().to_string()),
            on_value_change: move |key: String| active_filter.set(ClipboardFilter::from_key(&key)),
            horizontal: true,
            TabList { class: "filter-tabs", aria_label: copy.clipboard_type_filter,
                for (index, (filter, count)) in tabs.into_iter().enumerate() {
                    FilterTab {
                        key: "{filter.key()}",
                        filter,
                        index,
                        count,
                        language,
                    }
                }
            }
        }
    }
}

#[component]
fn FilterTab(
    filter: ClipboardFilter,
    index: usize,
    count: usize,
    language: AppLanguage,
) -> Element {
    let icon = match filter {
        ClipboardFilter::Text => Some(AppIcon::Text),
        ClipboardFilter::Image => Some(AppIcon::Image),
        ClipboardFilter::File => Some(AppIcon::File),
        ClipboardFilter::Favorite => Some(AppIcon::Favorite),
        ClipboardFilter::All => None,
    };
    let label = i18n::filter_label(language, filter);

    rsx! {
        TabTrigger {
            class: "filter-tab",
            value: filter.key().to_string(),
            index,
            if let Some(icon) = icon {
                Icon { icon }
            }
            span { class: "filter-tab-label", "{label}" }
            span { class: "filter-tab-count", "{count}" }
        }
    }
}
