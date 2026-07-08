mod actions;
mod file_display;
mod row;
mod selection;

use self::actions::{
    copy_entry, delete_entries_with_animation, delete_focused_or_selected, hide_window_after_copy,
    run_quick_paste_shortcut, save_entry_with_status,
};
use self::row::HistoryRow;
use self::selection::{focus_index, focused_entry_id, move_focus};
use super::filter_tabs::FilterTabs;
use super::icons::{AppIcon, Icon};
use crate::i18n;
use crate::model::{
    AppLanguage, ClipboardContent, ClipboardEntry, ClipboardFilter, ClipboardHistory, HistoryCounts,
};
use dioxus::desktop::use_window;
use dioxus::html::Key;
use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton};
use std::collections::HashSet;
use std::rc::Rc;

#[component]
pub fn HistoryList(
    entries: Vec<ClipboardEntry>,
    history: Signal<ClipboardHistory>,
    ignored_clipboard_write: Signal<Option<ClipboardContent>>,
    query: String,
    active_filter: Signal<ClipboardFilter>,
    counts: HistoryCounts,
    keyboard_shortcuts: bool,
    auto_focus: bool,
    promote_on_copy: bool,
    quick_paste: bool,
    hide_after_copy: bool,
    show_copy_time: bool,
    show_text_length: bool,
    language: AppLanguage,
    mut status: Signal<String>,
) -> Element {
    let mut selected_ids = use_signal(Vec::<u64>::new);
    let mut selection_anchor_id = use_signal(|| None::<u64>);
    let mut focused_id = use_signal(|| None::<u64>);
    let mut show_focus_highlight = use_signal(|| false);
    let deleting_ids = use_signal(Vec::<u64>::new);
    let entry_ids = Rc::new(entries.iter().map(|entry| entry.id).collect::<Vec<_>>());
    let entry_id_values = entry_ids.iter().copied().collect::<HashSet<_>>();
    let keyboard_entry_ids = entry_ids.clone();
    let paste_window = use_window();
    let deleting_id_values = deleting_ids.read().clone();
    let deleting_id_set = deleting_id_values.iter().copied().collect::<HashSet<_>>();
    let selected_id_values = selected_ids.read().clone();
    let selected_id_set = selected_id_values.iter().copied().collect::<HashSet<_>>();
    let visible_selected_ids = selected_id_values
        .iter()
        .copied()
        .filter(|id| entry_id_values.contains(id) && !deleting_id_set.contains(id))
        .collect::<Vec<_>>();
    let visible_selected_count = visible_selected_ids.len();

    rsx! {
        div {
            class: "list-header",
            onclick: move |_| {
                selected_ids.set(Vec::new());
                selection_anchor_id.set(None);
                show_focus_highlight.set(false);
            },
            FilterTabs { active_filter, counts, language }
            if visible_selected_count > 0 {
                div { class: "list-header-actions",
                    Toolbar { class: "selection-actions", aria_label: i18n::tr(language).batch_actions,
                        ToolbarButton {
                            class: "ghost-action selection-delete-action is-danger",
                            index: 0usize,
                            title: i18n::tr(language).delete_selected,
                            on_click: move |_| {
                                delete_entries_with_animation(
                                    visible_selected_ids.clone(),
                                    deleting_ids,
                                    history,
                                    status,
                                    language,
                                    i18n::tr(language).selected_history_deleted,
                                );
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
            EmptyState {
                filter: active_filter(),
                total_count: counts.total,
                query,
                language,
            }
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
                    show_focus_highlight.set(false);
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
                            show_focus_highlight.set(true);
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
                            show_focus_highlight.set(true);
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
                            show_focus_highlight.set(true);
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
                            show_focus_highlight.set(true);
                        }
                        Key::Enter => {
                            event.prevent_default();
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id()) {
                                let should_quick_paste = quick_paste
                                    && history.read().entry(id).is_some_and(|entry| entry.is_text());

                                if should_quick_paste {
                                    if copy_entry(id, history, ignored_clipboard_write, promote_on_copy, status, language) {
                                        paste_window.set_minimized(true);
                                        run_quick_paste_shortcut(status, language);
                                    }
                                } else if copy_entry(id, history, ignored_clipboard_write, promote_on_copy, status, language)
                                    && hide_after_copy
                                {
                                    hide_window_after_copy(&paste_window);
                                }
                            }
                        }
                        Key::Delete | Key::Backspace => {
                            event.prevent_default();
                            delete_focused_or_selected(
                                &keyboard_entry_ids,
                                focused_id(),
                                &mut selected_ids,
                                &mut selection_anchor_id,
                                deleting_ids,
                                history,
                                status,
                                language,
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
                            selected_ids.set(keyboard_entry_ids.to_vec());
                            selection_anchor_id.set(keyboard_entry_ids.first().copied());
                            focused_id.set(keyboard_entry_ids.last().copied());
                            show_focus_highlight.set(true);
                        }
                        Key::Character(key) if !primary && key.eq_ignore_ascii_case("f") => {
                            event.prevent_default();
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id())
                                && let Some(entry) = history.write().toggle_favorite(id)
                            {
                                save_entry_with_status(&entry, status, i18n::tr(language).favorite_status_updated, language);
                            }
                        }
                        Key::Character(key) if !primary && key.eq_ignore_ascii_case("p") => {
                            event.prevent_default();
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id())
                                && let Some(entry) = history.write().toggle_pin(id)
                            {
                                save_entry_with_status(&entry, status, i18n::tr(language).pin_status_updated, language);
                            }
                        }
                        _ => {}
                    }
                },
                ScrollArea {
                    class: "history-list",
                    direction: ScrollDirection::Vertical,
                    tabindex: "0",
                    aria_label: i18n::tr(language).clipboard_history_list,
                    for entry in entries {
                        HistoryRow {
                            key: "{entry.id}",
                            entry: entry.clone(),
                            entry_ids: entry_ids.clone(),
                            is_selected: selected_id_set.contains(&entry.id),
                            is_deleting: deleting_id_set.contains(&entry.id),
                            history,
                            ignored_clipboard_write,
                            selected_ids,
                            selection_anchor_id,
                            focused_id,
                            show_focus_highlight,
                            deleting_ids,
                            promote_on_copy,
                            quick_paste,
                            hide_after_copy,
                            show_copy_time,
                            show_text_length,
                            language,
                            status,
                        }
                    }
                    div { class: "history-list-clear-space" }
                }
            }
        }
    }
}

#[component]
fn EmptyState(
    filter: ClipboardFilter,
    total_count: usize,
    query: String,
    language: AppLanguage,
) -> Element {
    let state = i18n::empty_state_copy(language, filter, total_count, query.trim());

    rsx! {
        div { class: "empty-state",
            div { class: "empty-glyph", "{state.glyph}" }
            h2 { "{state.title}" }
            p { "{state.description}" }
        }
    }
}
