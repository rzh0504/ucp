mod actions;
mod file_display;
mod row;
mod selection;

use self::actions::{
    copy_entry, delete_entry_with_status, delete_focused_or_selected, run_quick_paste_shortcut,
    save_entry_with_status,
};
use self::row::HistoryRow;
use self::selection::{focus_index, focused_entry_id, move_focus};
use super::filter_tabs::FilterTabs;
use super::icons::{AppIcon, Icon};
use crate::model::{ClipboardEntry, ClipboardFilter, ClipboardHistory, HistoryCounts};
use dioxus::desktop::use_window;
use dioxus::html::Key;
use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};
use dioxus_primitives::separator::Separator;
use dioxus_primitives::toolbar::{Toolbar, ToolbarButton};

#[component]
pub fn HistoryList(
    entries: Vec<ClipboardEntry>,
    history: Signal<ClipboardHistory>,
    entry_count: usize,
    query: String,
    active_filter: Signal<ClipboardFilter>,
    counts: HistoryCounts,
    keyboard_shortcuts: bool,
    auto_focus: bool,
    promote_on_copy: bool,
    quick_paste: bool,
    show_copy_time: bool,
    show_text_length: bool,
    mut status: Signal<String>,
) -> Element {
    let mut selected_ids = use_signal(Vec::<u64>::new);
    let mut selection_anchor_id = use_signal(|| None::<u64>);
    let mut focused_id = use_signal(|| None::<u64>);
    let entry_ids = entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
    let keyboard_entry_ids = entry_ids.clone();
    let keyboard_entries = entries.clone();
    let paste_window = use_window();
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
                                        delete_entry_with_status(id, status);
                                    }
                                }

                                selected_ids.set(Vec::new());
                                selection_anchor_id.set(None);
                                status.set("已删除所选历史".to_string());
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
                                let should_quick_paste = quick_paste
                                    && keyboard_entries
                                        .iter()
                                        .any(|entry| entry.id == id && entry.is_text());

                                if should_quick_paste {
                                    if copy_entry(id, history, promote_on_copy, status) {
                                        paste_window.set_minimized(true);
                                        run_quick_paste_shortcut(status);
                                    }
                                } else {
                                    copy_entry(id, history, promote_on_copy, status);
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
                                history,
                                status,
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
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id())
                                && let Some(entry) = history.write().toggle_favorite(id)
                            {
                                save_entry_with_status(&entry, status, "收藏状态已更新");
                            }
                        }
                        Key::Character(key) if !primary && key.eq_ignore_ascii_case("p") => {
                            event.prevent_default();
                            if let Some(id) = focused_entry_id(&keyboard_entry_ids, focused_id())
                                && let Some(entry) = history.write().toggle_pin(id)
                            {
                                save_entry_with_status(&entry, status, "置顶状态已更新");
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
                            quick_paste,
                            show_copy_time,
                            show_text_length,
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
fn EmptyState(filter: ClipboardFilter, total_count: usize, query: String) -> Element {
    let state = empty_state_copy(filter, total_count, query.trim());

    rsx! {
        div { class: "empty-state",
            div { class: "empty-glyph", "{state.glyph}" }
            h2 { "{state.title}" }
            p { "{state.description}" }
        }
    }
}

struct EmptyStateCopy {
    glyph: &'static str,
    title: &'static str,
    description: &'static str,
}

fn empty_state_copy(filter: ClipboardFilter, total_count: usize, query: &str) -> EmptyStateCopy {
    if !query.is_empty() {
        return EmptyStateCopy {
            glyph: "⌕",
            title: "没有匹配的历史",
            description: "当前筛选范围内没有找到相关内容。试试缩短关键词，或切换到全部标签。",
        };
    }

    if total_count == 0 {
        return EmptyStateCopy {
            glyph: "⌘C",
            title: "复制任意内容开始",
            description: "UCP 会在后台监听剪贴板，并把新的文本、图像和文件整理成可搜索历史。",
        };
    }

    match filter {
        ClipboardFilter::All => EmptyStateCopy {
            glyph: "⌘C",
            title: "暂无历史记录",
            description: "复制文本、图像或文件后，新的剪贴板内容会出现在这里。",
        },
        ClipboardFilter::Text => EmptyStateCopy {
            glyph: "TXT",
            title: "还没有文本记录",
            description: "复制一段文字后，文本历史会自动归入这个标签。",
        },
        ClipboardFilter::Image => EmptyStateCopy {
            glyph: "IMG",
            title: "还没有图像记录",
            description: "截图或复制图片后，图像预览会按原比例显示在这里。",
        },
        ClipboardFilter::File => EmptyStateCopy {
            glyph: "FILE",
            title: "还没有文件记录",
            description: "复制文件或文件夹后，文件路径和应用图标会整理到这个标签。",
        },
        ClipboardFilter::Favorite => EmptyStateCopy {
            glyph: "★",
            title: "还没有收藏记录",
            description: "点击历史项右侧的星标，常用内容会集中显示在这里。",
        },
    }
}
