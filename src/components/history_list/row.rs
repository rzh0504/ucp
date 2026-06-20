use super::actions::{
    copy_entry, delete_entry_with_status, save_entry_with_status, save_image_file,
};
use super::file_display::FileListDisplay;
use super::selection::update_selection;
use crate::components::icons::{AppIcon, Icon};
use crate::model::{ClipboardContent, ClipboardEntry, ClipboardHistory};
use crate::platform;
use dioxus::desktop::use_window;
use dioxus::events::MountedData;
use dioxus::prelude::*;
use dioxus_primitives::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton, ToolbarSeparator};
use futures_timer::Delay;
use std::rc::Rc;
use std::time::Duration;

const QUICK_PASTE_DELAY: Duration = Duration::from_millis(260);

#[component]
pub(super) fn HistoryRow(
    entry: ClipboardEntry,
    index: usize,
    entry_ids: Vec<u64>,
    history: Signal<ClipboardHistory>,
    mut selected_ids: Signal<Vec<u64>>,
    mut selection_anchor_id: Signal<Option<u64>>,
    mut focused_id: Signal<Option<u64>>,
    promote_on_copy: bool,
    quick_paste: bool,
    show_copy_time: bool,
    show_text_length: bool,
    mut status: Signal<String>,
) -> Element {
    let id = entry.id;
    let mut button_ref = use_signal(|| None::<Rc<MountedData>>);
    let mut files_expanded = use_signal(|| false);
    let paste_window = use_window();
    let is_selected = selected_ids.read().contains(&id);
    let row_class = match (is_selected, focused_id() == Some(id)) {
        (true, true) => "history-row is-selected is-focused",
        (true, false) => "history-row is-selected",
        (false, true) => "history-row is-focused",
        (false, false) => "history-row",
    };
    let is_image = matches!(&entry.content, ClipboardContent::Image(_));
    let entry_title = entry.title();
    let entry_size = entry.size_label();
    let entry_age = entry.age_label();
    let show_size = !entry.is_text() || show_text_length;
    let file_display = match &entry.content {
        ClipboardContent::Files(files) => Some(FileListDisplay::new(files)),
        _ => None,
    };
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
        ContextMenu { tabindex: "-1", disabled: !quick_paste && !is_image,
            ContextMenuTrigger {
                article {
                    class: "{row_class}",
                    onclick: move |event| event.stop_propagation(),
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
                },
                ondoubleclick: move |_| {
                    copy_entry(id, history, promote_on_copy, status);
                },
                div { class: "entry-index", "{index}" }
                if is_image {
                    if let Some(preview_url) = &image_preview_url {
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
                        if is_image {
                            if let Some(dimensions) = &image_dimensions {
                                span { "{dimensions}" }
                            }
                        }
                        if show_copy_time {
                            span { "{entry_age}" }
                        }
                    }
                    if let Some(file_display) = &file_display {
                        div { class: "entry-file-list",
                            for file in file_display.visible_files(files_expanded()).iter() {
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
                            if file_display.hidden_count(files_expanded()) > 0 {
                                span {
                                    class: "entry-file-expand",
                                    role: "button",
                                    onclick: move |event| {
                                        event.stop_propagation();
                                        files_expanded.set(true);
                                    },
                                    "展开另外 {file_display.hidden_count(files_expanded())} 项"
                                }
                            } else if file_display.can_collapse(files_expanded()) {
                                span {
                                    class: "entry-file-expand",
                                    role: "button",
                                    onclick: move |event| {
                                        event.stop_propagation();
                                        files_expanded.set(false);
                                    },
                                    "收起文件列表"
                                }
                            }
                        }
                        p { class: if file_display.missing_count > 0 { "entry-size is-warning" } else { "entry-size" }, "{file_display.stats}" }
                    } else if !is_image {
                        p { class: if entry.is_text() { "entry-title" } else { "entry-title is-rich" }, "{entry_title}" }
                        if show_size {
                            p { class: "entry-size", "{entry_size}" }
                        }
                    }
                }
            }
            Toolbar { class: "entry-actions", aria_label: "条目操作",
                ToolbarButton {
                    class: if entry.favorite { "ghost-action is-favorite is-on is-favorite-visible" } else { "ghost-action is-favorite" },
                    index: 0usize,
                    on_click: move |_| {
                        if let Some(entry) = history.write().toggle_favorite(id) {
                            save_entry_with_status(&entry, status, "收藏状态已更新");
                        }
                    },
                    Icon { icon: if entry.favorite { AppIcon::FavoriteFilled } else { AppIcon::Favorite } }
                }
                ToolbarButton {
                    class: if entry.pinned { "ghost-action is-on is-pin-visible" } else { "ghost-action" },
                    index: 1usize,
                    on_click: move |_| {
                        if let Some(entry) = history.write().toggle_pin(id) {
                            save_entry_with_status(&entry, status, "置顶状态已更新");
                        }
                    },
                    Icon { icon: if entry.pinned { AppIcon::PinFilled } else { AppIcon::Pin } }
                }
                ToolbarSeparator { class: "entry-action-separator", decorative: true }
                ToolbarButton {
                    class: "ghost-action is-danger",
                    index: 2usize,
                    on_click: move |_| {
                        if history.write().remove(id) {
                            delete_entry_with_status(id, status);
                        }
                    },
                    Icon { icon: AppIcon::Delete }
                }
            }
                }
            }
            ContextMenuContent { class: "entry-context-menu",
                if quick_paste {
                    ContextMenuItem {
                        class: "entry-context-menu-item",
                        value: "quick-paste".to_string(),
                        index: 0usize,
                        on_select: move |_| {
                            if copy_entry(id, history, promote_on_copy, status) {
                                paste_window.set_minimized(true);
                                status.set("正在切换窗口并粘贴...".to_string());
                                spawn(async move {
                                    Delay::new(QUICK_PASTE_DELAY).await;
                                    match platform::clipboard::paste_shortcut() {
                                        Ok(()) => status.set("已快捷粘贴".to_string()),
                                        Err(error) => status.set(format!("快捷粘贴失败：{error}")),
                                    }
                                });
                            }
                        },
                        span { "快捷粘贴" }
                        kbd { "Ctrl+V" }
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
                            );
                        },
                        span { "保存为图片文件" }
                    }
                }
            }
        }
    }
}
