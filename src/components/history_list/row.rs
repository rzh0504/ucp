use super::actions::{
    copy_entry, delete_entries_with_animation, hide_window_after_copy, open_file_location,
    run_quick_paste_shortcut, save_entry_with_status, save_image_file,
};
use super::file_display::FileListDisplay;
use super::selection::update_selection;
use crate::components::icons::{AppIcon, Icon};
use crate::i18n;
use crate::model::{AppLanguage, ClipboardContent, ClipboardEntry, ClipboardHistory};
use dioxus::desktop::use_window;
use dioxus::events::MountedData;
use dioxus::prelude::*;
use dioxus_primitives::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton, ToolbarSeparator};
use std::rc::Rc;

#[component]
pub(super) fn HistoryRow(
    entry: ClipboardEntry,
    entry_ids: Rc<Vec<u64>>,
    is_selected: bool,
    is_deleting: bool,
    history: Signal<ClipboardHistory>,
    ignored_clipboard_write: Signal<Option<ClipboardContent>>,
    mut selected_ids: Signal<Vec<u64>>,
    mut selection_anchor_id: Signal<Option<u64>>,
    mut focused_id: Signal<Option<u64>>,
    mut show_focus_highlight: Signal<bool>,
    deleting_ids: Signal<Vec<u64>>,
    promote_on_copy: bool,
    quick_paste: bool,
    hide_after_copy: bool,
    show_copy_time: bool,
    show_text_length: bool,
    language: AppLanguage,
    mut status: Signal<String>,
) -> Element {
    let id = entry.id;
    let mut button_ref = use_signal(|| None::<Rc<MountedData>>);
    let mut files_expanded = use_signal(|| false);
    let mut context_menu_open = use_signal(|| None::<bool>);
    let paste_window = use_window();
    let is_focus_highlighted = show_focus_highlight() && focused_id() == Some(id);
    let mut row_class = "history-row".to_string();
    if is_selected {
        row_class.push_str(" is-selected");
    }
    if is_focus_highlighted {
        row_class.push_str(" is-focused");
    }
    if is_deleting {
        row_class.push_str(" is-deleting");
    }
    let is_text = entry.is_text();
    let is_image = matches!(&entry.content, ClipboardContent::Image(_));
    let file_paths = match &entry.content {
        ClipboardContent::Files(files) => Some(files.clone()),
        _ => None,
    };
    let has_context_menu = is_image
        || file_paths
            .as_ref()
            .is_some_and(|files| files.iter().any(|file| !file.trim().is_empty()));
    let show_size = !entry.is_text() || show_text_length;
    let entry_title = (!is_image).then(|| entry.title_with_language(language));
    let entry_size = show_size.then(|| entry.size_label_with_language(language));
    let entry_age = show_copy_time.then(|| entry.age_label_with_language(language));
    let file_display = match &entry.content {
        ClipboardContent::Files(files) => Some(FileListDisplay::new(files, language)),
        _ => None,
    };
    let files_expanded_value = files_expanded();
    let image_to_save = match &entry.content {
        ClipboardContent::Image(image) => Some((id, image.clone())),
        _ => None,
    };
    let image_preview_url = match &entry.content {
        ClipboardContent::Image(image) => image.preview_url.clone(),
        _ => None,
    };
    let image_dimensions = match &entry.content {
        ClipboardContent::Image(image) => Some(format!("{} x {}", image.width, image.height)),
        _ => None,
    };
    let row_main_class = if is_image {
        "history-row-main has-preview"
    } else {
        "history-row-main"
    };
    use_effect(move || {
        if focused_id() == Some(id)
            && let Some(element) = button_ref()
        {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
    });

    rsx! {
        ContextMenu {
            tabindex: "-1",
            disabled: !has_context_menu,
            open: context_menu_open,
            on_open_change: move |open| context_menu_open.set(Some(open)),
            ContextMenuTrigger {
                article {
                    class: "{row_class}",
                    onclick: move |event| {
                        context_menu_open.set(Some(false));
                        event.stop_propagation();
                    },
                    div { class: "history-row-clip",
                        div { class: "history-row-content",
                            button {
                        class: "{row_main_class}",
                        aria_selected: if is_selected { "true" } else { "false" },
                        onmounted: move |event| button_ref.set(Some(event.data())),
                        onfocus: move |_| focused_id.set(Some(id)),
                        onclick: move |event| {
                            let modifiers = event.data.modifiers();
                            let mut selection = selected_ids.read().clone();
                            let mut anchor = selection_anchor_id();

                            update_selection(
                                &entry_ids,
                                &mut selection,
                                &mut anchor,
                                id,
                                modifiers.ctrl() || modifiers.meta(),
                                modifiers.shift(),
                            );

                            selected_ids.set(selection);
                            selection_anchor_id.set(anchor);
                            focused_id.set(Some(id));
                            show_focus_highlight.set(false);
                        },
                        ondoubleclick: move |_| {
                            if quick_paste && is_text {
                                if copy_entry(id, history, ignored_clipboard_write, promote_on_copy, status, language) {
                                    paste_window.set_minimized(true);
                                    run_quick_paste_shortcut(status, language);
                                }
                            } else if copy_entry(id, history, ignored_clipboard_write, promote_on_copy, status, language)
                                && hide_after_copy
                            {
                                hide_window_after_copy(&paste_window);
                            }
                        },
                        div { class: "entry-index", aria_hidden: "true" }
                        if is_image {
                            if let Some(preview_url) = &image_preview_url {
                                img {
                                    class: "entry-image-preview",
                                    src: "{preview_url}",
                                    alt: i18n::tr(language).image_preview_alt,
                                }
                            } else {
                                div { class: "entry-image-preview is-empty", "IMG" }
                            }
                        }
                        div { class: "entry-content",
                            div { class: "entry-kicker",
                                if is_image {
                                    if let Some(dimensions) = &image_dimensions {
                                        span { "{dimensions}" }
                                    }
                                }
                                if show_copy_time {
                                    if let Some(entry_age) = &entry_age {
                                        span { "{entry_age}" }
                                    }
                                }
                            }
                            if let Some(file_display) = &file_display {
                                div { class: "entry-file-list",
                                    for file in file_display.visible_files(files_expanded_value).iter() {
                                        div { class: if file.exists { "entry-file-row" } else { "entry-file-row is-missing" },
                                            if let Some(icon_url) = &file.icon_url {
                                                img {
                                                    class: "entry-file-app-icon",
                                                    src: "{icon_url}",
                                                    alt: "",
                                                }
                                            } else {
                                                span { class: "entry-file-app-icon is-fallback",
                                                    Icon { icon: AppIcon::File }
                                                }
                                            }
                                            p { class: if file.exists { "entry-title" } else { "entry-title is-muted" }, "{file.name}" }
                                        }
                                    }
                                    if file_display.hidden_count(files_expanded_value) > 0 {
                                        span {
                                            class: "entry-file-expand",
                                            role: "button",
                                            onclick: move |event| {
                                                event.stop_propagation();
                                                files_expanded.set(true);
                                            },
                                            "{expand_more_label(language, file_display.hidden_count(files_expanded_value))}"
                                        }
                                    } else if file_display.can_collapse(files_expanded_value) {
                                        span {
                                            class: "entry-file-expand",
                                            role: "button",
                                            onclick: move |event| {
                                                event.stop_propagation();
                                                files_expanded.set(false);
                                            },
                                            "{i18n::tr(language).collapse_file_list}"
                                        }
                                    }
                                }
                                p { class: if file_display.missing_count > 0 { "entry-size is-warning" } else { "entry-size" }, "{file_display.stats}" }
                            } else if !is_image {
                                p { class: if entry.is_text() { "entry-title is-text" } else { "entry-title is-rich" }, "{entry_title.as_deref().unwrap_or_default()}" }
                                if show_size {
                                    p { class: "entry-size", "{entry_size.as_deref().unwrap_or_default()}" }
                                }
                            }
                        }
                    }
                    Toolbar { class: "entry-actions", aria_label: i18n::tr(language).entry_actions,
                        ToolbarButton {
                            class: if entry.favorite { "ghost-action is-favorite is-on is-favorite-visible" } else { "ghost-action is-favorite" },
                            index: 0usize,
                            on_click: move |_| {
                                if let Some(entry) = history.write().toggle_favorite(id) {
                                    save_entry_with_status(&entry, status, i18n::tr(language).favorite_status_updated, language);
                                }
                            },
                            Icon { icon: if entry.favorite { AppIcon::FavoriteFilled } else { AppIcon::Favorite } }
                        }
                        ToolbarButton {
                            class: if entry.pinned { "ghost-action is-on is-pin-visible" } else { "ghost-action" },
                            index: 1usize,
                            on_click: move |_| {
                                if let Some(entry) = history.write().toggle_pin(id) {
                                    save_entry_with_status(&entry, status, i18n::tr(language).pin_status_updated, language);
                                }
                            },
                            Icon { icon: if entry.pinned { AppIcon::PinFilled } else { AppIcon::Pin } }
                        }
                        ToolbarSeparator { class: "entry-action-separator", decorative: true }
                        ToolbarButton {
                            class: "ghost-action is-danger",
                            index: 2usize,
                            on_click: move |_| {
                                delete_entries_with_animation(
                                    vec![id],
                                    deleting_ids,
                                    history,
                                    status,
                                    language,
                                    i18n::tr(language).history_deleted,
                                );
                            },
                            Icon { icon: AppIcon::Delete }
                        }
                    }
                        }
                    }
                }
            }
            ContextMenuContent { class: "entry-context-menu",
                if let Some(files) = file_paths.clone() {
                    ContextMenuItem {
                        class: "entry-context-menu-item",
                        value: "open-file-location".to_string(),
                        index: 0usize,
                        on_select: move |_| {
                            open_file_location(&files, status, language);
                        },
                        span { "{i18n::tr(language).open_file_location}" }
                    }
                }
                if let Some((image_id, image)) = image_to_save.clone() {
                    ContextMenuItem {
                        class: "entry-context-menu-item",
                        value: "save-image".to_string(),
                        index: 1usize,
                        on_select: move |_| {
                            save_image_file(
                                image_id,
                                image.clone(),
                                entry.captured_at.format("ucp-image-%Y%m%d-%H%M%S.png").to_string(),
                                status,
                                language,
                            );
                        },
                        span { "{i18n::tr(language).save_as_image_file}" }
                    }
                }
            }
        }
    }
}

fn expand_more_label(language: AppLanguage, count: usize) -> String {
    match language {
        AppLanguage::Chinese => format!("展开另外 {count} 项"),
        AppLanguage::English => format!("Show {count} more"),
    }
}
