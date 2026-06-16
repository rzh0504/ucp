use super::AppPage;
use dioxus::prelude::*;
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton};

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
