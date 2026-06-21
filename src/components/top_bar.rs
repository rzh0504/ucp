use super::AppPage;
use super::icons::{AppIcon, Icon};
use crate::i18n;
use crate::model::AppLanguage;
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
    keyboard_shortcuts: bool,
    widget_mode: bool,
    widget_topmost: bool,
    language: AppLanguage,
    on_topmost_change: EventHandler<bool>,
    on_close: EventHandler<()>,
) -> Element {
    let window = use_window();
    let drag_window = window.clone();
    let minimize_window = window.clone();
    let maximize_window = window.clone();
    let copy = i18n::tr(language);

    rsx! {
        Toolbar { class: "top-bar", aria_label: copy.toolbar_label,
            div {
                class: "title-drag-region",
                onmousedown: move |_| drag_window.drag(),
                h1 { class: "app-title", "UCP Clipboard" }
            }
            if active_page() == AppPage::History {
                SearchField { query, search_input, keyboard_shortcuts, language }
            } else if !widget_mode {
                div { class: "top-bar-context", "{copy.settings}" }
            }
            WindowControls {
                language,
                widget_mode,
                widget_topmost,
                on_minimize: move |_| {
                    if widget_mode {
                        minimize_window.set_visible(false);
                    } else {
                        minimize_window.set_minimized(true);
                    }
                },
                on_topmost_change: move |topmost| on_topmost_change.call(topmost),
                on_maximize: move |_| maximize_window.toggle_maximized(),
                on_close: move |_| on_close.call(()),
            }
        }
    }
}

#[component]
fn WindowControls(
    language: AppLanguage,
    widget_mode: bool,
    widget_topmost: bool,
    on_minimize: EventHandler<()>,
    on_topmost_change: EventHandler<bool>,
    on_maximize: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let copy = i18n::tr(language);
    let controls_class = if widget_mode {
        "window-controls is-widget"
    } else {
        "window-controls"
    };
    let topmost_class = if widget_topmost {
        "window-icon-control is-topmost is-active"
    } else {
        "window-icon-control is-topmost"
    };
    let topmost_title = if widget_topmost {
        copy.cancel_keep_on_top
    } else {
        copy.keep_on_top
    };

    rsx! {
        div { class: controls_class, aria_label: copy.window_controls,
            if widget_mode {
                button {
                    class: topmost_class,
                    title: topmost_title,
                    aria_label: topmost_title,
                    onclick: move |_| on_topmost_change.call(!widget_topmost),
                    Icon { icon: AppIcon::Topmost }
                }
            }
            button {
                class: "window-dot is-minimize",
                title: copy.minimize,
                onclick: move |_| on_minimize.call(()),
                span { "−" }
            }
            if !widget_mode {
                button {
                    class: "window-dot is-maximize",
                    title: copy.maximize_or_restore,
                    onclick: move |_| on_maximize.call(()),
                    span { "□" }
                }
            }
            button {
                class: "window-dot is-close",
                title: copy.hide_to_background,
                onclick: move |_| on_close.call(()),
                span { "×" }
            }
        }
    }
}

#[component]
fn SearchField(
    query: Signal<String>,
    search_input: Signal<Option<Rc<MountedData>>>,
    keyboard_shortcuts: bool,
    language: AppLanguage,
) -> Element {
    let copy = i18n::tr(language);
    let title = if keyboard_shortcuts {
        copy.focus_search_title
    } else {
        copy.search_history
    };

    rsx! {
        label { class: "search-field",
            svg {
                class: "search-icon",
                view_box: "0 0 24 24",
                "aria-hidden": "true",
                path {
                    d: "M10.5 5.25a5.25 5.25 0 1 0 0 10.5 5.25 5.25 0 0 0 0-10.5ZM3.75 10.5a6.75 6.75 0 1 1 12.06 4.17l4.01 4.01a.75.75 0 0 1-1.06 1.06l-4.01-4.01A6.75 6.75 0 0 1 3.75 10.5Z",
                    fill: "currentColor",
                }
            }
            input {
                r#type: "search",
                placeholder: copy.search_history,
                value: "{query}",
                title,
                onmounted: move |event| search_input.set(Some(event.data())),
                oninput: move |event| query.set(event.value()),
            }
        }
    }
}
