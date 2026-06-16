use super::AppPage;
use dioxus::desktop::use_window;
use dioxus::events::MountedData;
use dioxus::prelude::*;
use dioxus_primitives::toolbar::Toolbar;
use std::rc::Rc;

#[component]
pub fn TopBar(
    query: Signal<String>,
    active_page: Signal<AppPage>,
    search_input: Signal<Option<Rc<MountedData>>>,
) -> Element {
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
                SearchField { query, search_input }
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
fn SearchField(query: Signal<String>, search_input: Signal<Option<Rc<MountedData>>>) -> Element {
    rsx! {
        label { class: "search-field",
            span { class: "search-icon", "⌕" }
            input {
                r#type: "search",
                placeholder: "搜索剪贴板历史",
                value: "{query}",
                title: "Ctrl+F 聚焦搜索",
                onmounted: move |event| search_input.set(Some(event.data())),
                oninput: move |event| query.set(event.value()),
            }
        }
    }
}
