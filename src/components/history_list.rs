use super::filter_tabs::FilterTabs;
use super::icons::{AppIcon, Icon};
use crate::model::{
    ClipboardContent, ClipboardEntry, ClipboardFilter, ClipboardHistory, HistoryCounts,
};
use crate::platform;
use crate::storage;
use dioxus::events::MountedData;
use dioxus::html::Key;
use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton, ToolbarSeparator};
use std::rc::Rc;

#[component]
pub fn HistoryList(
    entries: Vec<ClipboardEntry>,
    history: Signal<ClipboardHistory>,
    entry_count: usize,
    active_filter: Signal<ClipboardFilter>,
    counts: HistoryCounts,
    keyboard_shortcuts: bool,
    auto_focus: bool,
    promote_on_copy: bool,
) -> Element {
    let mut selected_ids = use_signal(Vec::<u64>::new);
    let mut selection_anchor_id = use_signal(|| None::<u64>);
    let mut focused_id = use_signal(|| None::<u64>);
    let entry_ids = entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
    let keyboard_entry_ids = entry_ids.clone();
    let visible_selected_ids = selected_ids
        .read()
        .iter()
        .copied()
        .filter(|id| entry_ids.contains(id))
        .collect::<Vec<_>>();
    let visible_selected_count = visible_selected_ids.len();

    rsx! {
        div {
            class: "list-header",
            onclick: move |_| {
                selected_ids.set(Vec::new());
                selection_anchor_id.set(None);
            },
            FilterTabs { active_filter, counts }
            div { class: "list-header-actions",
                span { class: "list-count",
                    if visible_selected_count == 0 {
                        "{entry_count} 项"
                    } else {
                        "已选 {visible_selected_count} / {entry_count} 项"
                    }
                }
                if visible_selected_count > 0 {
                    Toolbar { class: "selection-actions", aria_label: "批量操作",
                        ToolbarButton {
                            class: "ghost-action selection-delete-action is-danger",
                            index: 0usize,
                            title: "删除已选",
                            on_click: move |_| {
                                for id in visible_selected_ids.clone() {
                                    if history.write().remove(id) {
                                        let _ = storage::delete_entry(id);
                                    }
                                }

                                selected_ids.set(Vec::new());
                                selection_anchor_id.set(None);
                            },
                            Icon { icon: AppIcon::Delete }
                        }
                    }
                }
            }
        }
        Separator { class: "list-separator", decorative: true }
        if entries.is_empty() {
            EmptyState {}
        } else {
            div {
                class: "history-list-click-target",
                tabindex: "-1",
                onmounted: move |event| {
                    if !auto_focus {
                        return;
                    }

                    let element = event.data();
                    spawn(async move {
                        let _ = element.set_focus(true).await;
                    });
                },
                onclick: move |_| {
                    selected_ids.set(Vec::new());
                    selection_anchor_id.set(None);
                },
                onkeydown: move |event| {
                    if !keyboard_shortcuts {
                        return;
                    }

                    let data = event.data();
                    let key = data.key();
                    let modifiers = data.modifiers();
                    let primary = modifiers.ctrl() || modifiers.meta();

                    match key {
                        Key::ArrowDown => {
                            event.prevent_default();
                            move_focus(
                                &keyboard_entry_ids,
                                &mut focused_id,
                                &mut selected_ids,
                                &mut selection_anchor_id,
                                1,
                                modifiers.shift(),
                                primary,
                            );
                        }
                        Key::ArrowUp => {
                            event.prevent_default();
                            move_focus(
                                &keyboard_entry_ids,
                                &mut focused_id,
                                &mut selected_ids,
                                &mut selection_anchor_id,
                                -1,
                                modifiers.shift(),
                                primary,
                            );
                        }
                        Key::Home => {
                            event.prevent_default();
                            focus_index(
                                &keyboard_entry_ids,
                                &mut focused_id,
                                &mut selected_ids,
                                &mut selection_anchor_id,
                                0,
                                modifiers.shift(),
                                primary,
                            );
                        }
                        Key::End => {
                            event.prevent_default();
                            focus_index(
                                &keyboard_entry_ids,
                                &mut focused_id,
                                &mut selected_ids,
                                &mut selection_anchor_id,
                                keyboard_entry_ids.len().saturating_sub(1),
                                modifiers.shift(),
                                primary,
                            );
                        }
                        Key::Enter => {
                            event.prevent_default();
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id()) {
                                copy_entry(id, history, promote_on_copy);
                            }
                        }
                        Key::Delete | Key::Backspace => {
                            event.prevent_default();
                            delete_focused_or_selected(
                                &keyboard_entry_ids,
                                focused_id(),
                                &mut selected_ids,
                                &mut selection_anchor_id,
                                history,
                            );
                        }
                        Key::Escape => {
                            if !selected_ids.read().is_empty() {
                                event.prevent_default();
                                event.stop_propagation();
                                selected_ids.set(Vec::new());
                                selection_anchor_id.set(None);
                            }
                        }
                        Key::Character(key) if primary && key.eq_ignore_ascii_case("a") => {
                            event.prevent_default();
                            selected_ids.set(keyboard_entry_ids.clone());
                            selection_anchor_id.set(keyboard_entry_ids.first().copied());
                            focused_id.set(keyboard_entry_ids.last().copied());
                        }
                        Key::Character(key) if !primary && key.eq_ignore_ascii_case("f") => {
                            event.prevent_default();
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id()) {
                                if let Some(entry) = history.write().toggle_favorite(id) {
                                    let _ = storage::save_entry(&entry);
                                }
                            }
                        }
                        Key::Character(key) if !primary && key.eq_ignore_ascii_case("p") => {
                            event.prevent_default();
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id()) {
                                if let Some(entry) = history.write().toggle_pin(id) {
                                    let _ = storage::save_entry(&entry);
                                }
                            }
                        }
                        _ => {}
                    }
                },
                ScrollArea {
                    class: "history-list",
                    direction: ScrollDirection::Vertical,
                    tabindex: "0",
                    aria_label: "剪贴板历史列表",
                    for (index, entry) in entries.iter().enumerate() {
                        HistoryRow {
                            key: "{entry.id}",
                            entry: entry.clone(),
                            index: index + 1,
                            entry_ids: entry_ids.clone(),
                            history,
                            selected_ids,
                            selection_anchor_id,
                            focused_id,
                            promote_on_copy,
                        }
                    }
                    div { class: "history-list-clear-space" }
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
    mut focused_id: Signal<Option<u64>>,
    promote_on_copy: bool,
) -> Element {
    let id = entry.id;
    let mut button_ref = use_signal(|| None::<Rc<MountedData>>);
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
    let row_main_class = if is_image {
        "history-row-main has-preview"
    } else {
        "history-row-main"
    };

    use_effect(move || {
        if focused_id() == Some(id) {
            if let Some(element) = button_ref() {
                spawn(async move {
                    let _ = element.set_focus(true).await;
                });
            }
        }
    });

    rsx! {
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
                    copy_entry(id, history, promote_on_copy);
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
                        if is_image {
                            span { "{entry_size}" }
                        }
                        span { "{entry_age}" }
                    }
                    if !is_image {
                        p { class: if entry.is_text() { "entry-title" } else { "entry-title is-rich" }, "{entry_title}" }
                        p { class: "entry-size", "{entry_size}" }
                    }
                }
            }
            Toolbar { class: "entry-actions", aria_label: "条目操作",
                ToolbarButton {
                    class: if entry.favorite { "ghost-action is-favorite is-on is-favorite-visible" } else { "ghost-action is-favorite" },
                    index: 0usize,
                    on_click: move |_| {
                        if let Some(entry) = history.write().toggle_favorite(id) {
                            let _ = storage::save_entry(&entry);
                        }
                    },
                    Icon { icon: if entry.favorite { AppIcon::FavoriteFilled } else { AppIcon::Favorite } }
                }
                ToolbarButton {
                    class: if entry.pinned { "ghost-action is-on is-pin-visible" } else { "ghost-action" },
                    index: 1usize,
                    on_click: move |_| {
                        if let Some(entry) = history.write().toggle_pin(id) {
                            let _ = storage::save_entry(&entry);
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
                            let _ = storage::delete_entry(id);
                        }
                    },
                    Icon { icon: AppIcon::Delete }
                }
            }
        }
    }
}

fn focused_entry_id(entry_ids: &[u64], focused_id: Option<u64>) -> Option<u64> {
    focused_id
        .filter(|id| entry_ids.contains(id))
        .or_else(|| entry_ids.first().copied())
}

fn focused_index(entry_ids: &[u64], focused_id: Option<u64>) -> Option<usize> {
    focused_id
        .and_then(|id| entry_ids.iter().position(|entry_id| *entry_id == id))
        .or(if entry_ids.is_empty() { None } else { Some(0) })
}

fn move_focus(
    entry_ids: &[u64],
    focused_id: &mut Signal<Option<u64>>,
    selected_ids: &mut Signal<Vec<u64>>,
    selection_anchor_id: &mut Signal<Option<u64>>,
    offset: isize,
    shift: bool,
    preserve_selection: bool,
) {
    let Some(index) = focused_index(entry_ids, *focused_id.read()) else {
        return;
    };
    let next_index = index
        .saturating_add_signed(offset)
        .min(entry_ids.len().saturating_sub(1));

    focus_index(
        entry_ids,
        focused_id,
        selected_ids,
        selection_anchor_id,
        next_index,
        shift,
        preserve_selection,
    );
}

fn focus_index(
    entry_ids: &[u64],
    focused_id: &mut Signal<Option<u64>>,
    selected_ids: &mut Signal<Vec<u64>>,
    selection_anchor_id: &mut Signal<Option<u64>>,
    index: usize,
    shift: bool,
    preserve_selection: bool,
) {
    let Some(id) = entry_ids.get(index).copied() else {
        return;
    };

    let mut selection = selected_ids.read().clone();
    let mut anchor = (*selection_anchor_id.read()).or(*focused_id.read());

    if shift {
        update_selection(
            entry_ids,
            &mut selection,
            &mut anchor,
            id,
            preserve_selection,
            true,
        );
        selected_ids.set(selection);
        selection_anchor_id.set(anchor);
    } else if !preserve_selection {
        selected_ids.set(vec![id]);
        selection_anchor_id.set(Some(id));
    }

    focused_id.set(Some(id));
}

fn copy_entry(id: u64, mut history: Signal<ClipboardHistory>, promote_on_copy: bool) {
    let Some(content) = history.read().entry(id).map(|entry| entry.content.clone()) else {
        return;
    };

    if platform::clipboard::write_content(&content).is_ok() && promote_on_copy {
        if let Some(entry) = history.write().promote(id) {
            let _ = storage::save_entry(&entry);
        }
    }
}

fn delete_focused_or_selected(
    entry_ids: &[u64],
    focused_id: Option<u64>,
    selected_ids: &mut Signal<Vec<u64>>,
    selection_anchor_id: &mut Signal<Option<u64>>,
    mut history: Signal<ClipboardHistory>,
) {
    let mut ids = selected_ids
        .read()
        .iter()
        .copied()
        .filter(|id| entry_ids.contains(id))
        .collect::<Vec<_>>();

    if ids.is_empty() {
        if let Some(id) = focused_entry_id(entry_ids, focused_id) {
            ids.push(id);
        }
    }

    for id in ids {
        if history.write().remove(id) {
            let _ = storage::delete_entry(id);
        }
    }

    selected_ids.set(Vec::new());
    selection_anchor_id.set(None);
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

    if let Some(index) = selected_ids
        .iter()
        .position(|selected_id| *selected_id == id)
    {
        selected_ids.remove(index);
        *anchor_id = selected_ids.last().copied();
        return;
    }

    if ctrl {
        selected_ids.push(id);
    } else {
        selected_ids.clear();
        selected_ids.push(id);
    }

    *anchor_id = Some(id);
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
