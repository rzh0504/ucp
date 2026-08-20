use super::ClipboardApp;
use crate::model::{AppLanguage, ClipboardContent, ClipboardEntry, ClipboardFilter};
use crate::services::ClipboardService;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex,
    input::Input,
    menu::{ContextMenuExt as _, PopupMenuItem},
    scroll::{Scrollbar, ScrollbarMode},
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex, v_virtual_list,
};
use std::rc::Rc;

const COLLAPSED_TEXT_LINES: usize = 6;
const TEXT_LINE_HEIGHT: f32 = 24.;
const TEXT_VERTICAL_PADDING: f32 = 16.;
const TEXT_FOOTER_HEIGHT: f32 = 24.;
const TEXT_ROW_CHROME_HEIGHT: f32 = 8.;

impl ClipboardApp {
    pub(super) fn render_history(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.settings.language;
        let show_copy_time = self.settings.show_copy_time;
        let show_text_length = self.settings.show_text_length;
        let double_click_copy = self.settings.double_click_copy;
        let quick_paste = self.settings.quick_paste;
        let counts = self.history.counts();
        let filters = TabBar::new("filters")
            .segmented()
            .large()
            .selected_index(match self.filter {
                ClipboardFilter::All => 0,
                ClipboardFilter::Text => 1,
                ClipboardFilter::Image => 2,
                ClipboardFilter::File => 3,
                ClipboardFilter::Favorite => 4,
            })
            .on_click(cx.listener(|this, index, _, cx| {
                this.filter = match *index {
                    1 => ClipboardFilter::Text,
                    2 => ClipboardFilter::Image,
                    3 => ClipboardFilter::File,
                    4 => ClipboardFilter::Favorite,
                    _ => ClipboardFilter::All,
                };
                this.refresh_visible_entries();
                cx.notify();
            }))
            .child(Self::filter_tab(IconName::Inbox, "全部", counts.total))
            .child(Self::filter_tab(IconName::ALargeSmall, "文本", counts.text))
            .child(Self::filter_tab(IconName::Frame, "图片", counts.image))
            .child(Self::filter_tab(IconName::File, "文件", counts.file))
            .child(Self::filter_tab(IconName::Heart, "收藏", counts.favorite));
        let item_sizes = Rc::new(
            self.visible_entries
                .iter()
                .map(|entry| {
                    size(
                        px(0.),
                        px(Self::entry_height(
                            entry,
                            self.expanded_image_id == Some(entry.id),
                            self.expanded_text_id == Some(entry.id),
                        )),
                    )
                })
                .collect(),
        );
        let list = v_virtual_list(
            cx.entity().clone(),
            "history-list",
            item_sizes,
            move |this, range, _window, cx| {
                range
                    .filter_map(|index| {
                        this.visible_entries.get(index).cloned().map(|entry| {
                            let selected = this.selected_entry_ids.contains(&entry.id);
                            let multiple_selected = this.selected_entry_ids.len() > 1;
                            let selected_ids = this.selected_entry_ids.iter().copied().collect();
                            let all_selected_favorite = multiple_selected
                                && this.selected_entry_ids.iter().all(|selected_id| {
                                    this.history
                                        .entry(*selected_id)
                                        .is_some_and(|selected_entry| selected_entry.favorite)
                                });
                            let image_expanded = this.expanded_image_id == Some(entry.id);
                            let text_expanded = this.expanded_text_id == Some(entry.id);
                            let navigated = this.navigation_entry_id == Some(entry.id)
                                || image_expanded
                                || text_expanded;
                            Self::render_entry(
                                entry,
                                index + 1,
                                language,
                                selected,
                                multiple_selected,
                                selected_ids,
                                all_selected_favorite,
                                navigated,
                                image_expanded,
                                text_expanded,
                                (
                                    show_copy_time,
                                    show_text_length,
                                    double_click_copy,
                                    quick_paste,
                                ),
                                cx,
                            )
                        })
                    })
                    .collect()
            },
        )
        .track_scroll(&self.history_scroll)
        .size_full();

        let content = if self.visible_entries.is_empty() {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(IconName::Inbox)
                .child(div().font_medium().child(if self.query.is_empty() {
                    "暂无剪贴板记录"
                } else {
                    "没有匹配的记录"
                }))
                .child(div().text_sm().child("复制文本、图片或文件后会显示在这里"))
                .into_any_element()
        } else {
            div()
                .relative()
                .size_full()
                .overflow_hidden()
                .child(list)
                .child(Scrollbar::vertical(&self.history_scroll).mode(ScrollbarMode::Scrolling))
                .into_any_element()
        };

        v_flex()
            .size_full()
            .bg(cx.theme().colors.list)
            .child(
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_4()
                    .items_center()
                    .bg(cx.theme().background)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(filters)
                    .child(div().flex_1())
                    .child(
                        Input::new(&self.search)
                            .large()
                            .w(px(300.))
                            .prefix(IconName::Search)
                            .cleanable(true),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
    }

    fn filter_tab(icon: IconName, label: &'static str, count: usize) -> Tab {
        Tab::new().child(
            h_flex()
                .gap_1()
                .child(Icon::new(icon).small())
                .child(format!("{label} {count}")),
        )
    }

    pub(super) fn handle_history_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page != super::AppPage::History
            || !self.initial_focus.is_focused(window)
            || self.visible_entries.is_empty()
        {
            return;
        }

        let key = event.keystroke.key.as_str();
        if key == "delete" && event.keystroke.modifiers == Modifiers::none() {
            let mut ids = self.selected_entry_ids.iter().copied().collect::<Vec<_>>();
            if let Some(id) = self.navigation_entry_id
                && !ids.contains(&id)
            {
                ids.push(id);
            }
            if !ids.is_empty() {
                self.delete_entries(ids, cx);
                cx.stop_propagation();
            }
            return;
        }
        if event.keystroke.modifiers != Modifiers::none() {
            return;
        }

        let Some(next_index) = (match key {
            "up" => Some(
                self.navigation_index()
                    .map_or(self.visible_entries.len() - 1, |index| {
                        index.saturating_sub(1)
                    }),
            ),
            "down" => self.navigation_index().map_or(Some(0), |index| {
                Some((index + 1).min(self.visible_entries.len() - 1))
            }),
            "home" => Some(0),
            "end" => Some(self.visible_entries.len() - 1),
            "escape" => {
                self.navigation_entry_id = None;
                cx.stop_propagation();
                cx.notify();
                None
            }
            _ => return,
        }) else {
            return;
        };

        self.navigation_entry_id = Some(self.visible_entries[next_index].id);
        self.history_scroll
            .scroll_to_item(next_index, ScrollStrategy::Center);
        cx.stop_propagation();
        cx.notify();
    }

    fn navigation_index(&self) -> Option<usize> {
        self.navigation_entry_id
            .or(self.selection_anchor_id)
            .and_then(|id| self.visible_entries.iter().position(|entry| entry.id == id))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_entry(
        entry: Rc<ClipboardEntry>,
        position: usize,
        language: AppLanguage,
        selected: bool,
        multiple_selected: bool,
        selected_ids: Vec<u64>,
        all_selected_favorite: bool,
        navigated: bool,
        image_expanded: bool,
        text_expanded: bool,
        options: (bool, bool, bool, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (show_copy_time, show_text_length, double_click_copy, quick_paste) = options;
        let id = entry.id;
        let is_image = matches!(entry.content, ClipboardContent::Image(_));
        let title = match &entry.content {
            ClipboardContent::Text(text) if text_expanded => text.clone(),
            ClipboardContent::Text(text) => Self::text_preview(text, COLLAPSED_TEXT_LINES),
            _ => entry.title_with_language(language),
        };
        let meta = match &entry.content {
            ClipboardContent::Text(_) if !show_text_length => None,
            ClipboardContent::Image(image) => Some(format!("{} x {}", image.width, image.height)),
            _ => Some(entry.size_label_with_language(language)),
        };
        let copy_time = crate::i18n::relative_time(language, entry.captured_at);
        let favorite = entry.favorite;
        let app = cx.entity().downgrade();
        let row_height = Self::entry_height(&entry, image_expanded, text_expanded);
        let muted_foreground = cx.theme().muted_foreground;
        let image_content = match &entry.content {
            ClipboardContent::Image(image) => {
                let placeholder = move || {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(muted_foreground)
                        .child(Icon::new(IconName::Frame))
                        .into_any_element()
                };
                let preview = ClipboardService::image_preview_path(image.preview_url.as_deref())
                    .map(|path| {
                        img(path)
                            .size_full()
                            .object_fit(if image_expanded {
                                ObjectFit::Contain
                            } else {
                                ObjectFit::ScaleDown
                            })
                            .with_loading(placeholder)
                            .with_fallback(placeholder)
                            .into_any_element()
                    })
                    .unwrap_or_else(placeholder);
                Some(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .overflow_hidden()
                                .p_2()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .when(!image_expanded, |this| this.w(px(220.)).h(px(140.)))
                                        .when(image_expanded, |this| this.size_full())
                                        .overflow_hidden()
                                        .bg(cx.theme().background)
                                        .child(preview),
                                ),
                        )
                        .child(
                            h_flex()
                                .h(px(24.))
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(muted_foreground)
                                .when(show_copy_time, |this| this.child(copy_time.clone()))
                                .when(favorite, |this| {
                                    this.child(
                                        Tag::warning()
                                            .small()
                                            .rounded_full()
                                            .ml_2()
                                            .h(px(14.))
                                            .py_0()
                                            .text_size(px(10.))
                                            .child("收藏"),
                                    )
                                })
                                .child(div().flex_1())
                                .child(
                                    h_flex()
                                        .id(ElementId::NamedInteger("image-expand".into(), id))
                                        .gap_1()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(cx.theme().secondary_hover))
                                        .child(
                                            Icon::new(if image_expanded {
                                                IconName::ChevronUp
                                            } else {
                                                IconName::ChevronDown
                                            })
                                            .xsmall(),
                                        )
                                        .child(if image_expanded { "收起" } else { "展开" })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.toggle_image_expansion(id, cx);
                                        })),
                                )
                                .child(div().flex_1())
                                .when_some(meta.clone(), |this, meta| this.child(meta))
                                .child(div().w(px(20.)).text_right().child(position.to_string())),
                        ),
                )
            }
            _ => None,
        };
        let is_multiline = entry.is_multiline();
        let can_expand_text = entry.text_line_count() > COLLAPSED_TEXT_LINES;
        let content = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .py_2()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .text_size(px(14.))
                    .line_height(px(TEXT_LINE_HEIGHT))
                    .when(!is_multiline, |this| this.line_clamp(2))
                    .child(title),
            )
            .child(
                h_flex()
                    .h(px(24.))
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(cx.theme().muted_foreground)
                    .when(show_copy_time, |this| this.child(copy_time.clone()))
                    .when(favorite, |this| {
                        this.child(
                            Tag::warning()
                                .small()
                                .rounded_full()
                                .ml_2()
                                .h(px(14.))
                                .py_0()
                                .text_size(px(10.))
                                .child("收藏"),
                        )
                    })
                    .child(div().flex_1())
                    .when(can_expand_text, |this| {
                        this.child(
                            h_flex()
                                .id(ElementId::NamedInteger("text-expand".into(), id))
                                .gap_1()
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(|style| style.bg(cx.theme().secondary_hover))
                                .child(
                                    Icon::new(if text_expanded {
                                        IconName::ChevronUp
                                    } else {
                                        IconName::ChevronDown
                                    })
                                    .xsmall(),
                                )
                                .child(if text_expanded { "收起" } else { "展开" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_text_expansion(id, cx);
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .when_some(meta, |this, meta| this.child(meta)),
            );

        h_flex()
            .id(ElementId::NamedInteger("entry".into(), id))
            .w_full()
            .h(px(row_height))
            .relative()
            .overflow_hidden()
            .gap_2()
            .pl_4()
            .pr(px(52.))
            .border_2()
            .border_color(cx.theme().border.opacity(0.))
            .rounded_sm()
            .when(selected, |this| {
                this.bg(cx.theme().blue.opacity(0.12))
                    .when(!multiple_selected, |this| {
                        this.border_color(cx.theme().blue.opacity(0.78))
                    })
            })
            .when(navigated && !selected, |this| {
                this.border_color(cx.theme().blue.opacity(0.78))
            })
            .when(!selected && !navigated, |this| {
                this.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(1.))
                        .bg(cx.theme().border),
                )
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if (double_click_copy || quick_paste) && event.click_count() == 2 {
                    this.selected_entry_ids.clear();
                    this.selection_anchor_id = None;
                    this.navigation_entry_id = None;
                    this.copy_entry(id, quick_paste, Some(window.window_handle()), cx);
                    return;
                }
                this.navigation_entry_id = None;
                let modifiers = event.modifiers();
                if modifiers.shift {
                    let anchor_index = this
                        .selection_anchor_id
                        .and_then(|anchor_id| {
                            this.visible_entries
                                .iter()
                                .position(|entry| entry.id == anchor_id)
                        })
                        .unwrap_or(position - 1);
                    let clicked_index = position - 1;
                    let (start, end) = if anchor_index <= clicked_index {
                        (anchor_index, clicked_index)
                    } else {
                        (clicked_index, anchor_index)
                    };
                    this.selected_entry_ids = this.visible_entries[start..=end]
                        .iter()
                        .map(|entry| entry.id)
                        .collect();
                    this.selection_anchor_id.get_or_insert(id);
                } else if modifiers.secondary() {
                    if !this.selected_entry_ids.remove(&id) {
                        this.selected_entry_ids.insert(id);
                    }
                    this.selection_anchor_id = Some(id);
                } else {
                    let was_only_selected =
                        this.selected_entry_ids.len() == 1 && this.selected_entry_ids.contains(&id);
                    this.selected_entry_ids.clear();
                    this.selection_anchor_id = if was_only_selected { None } else { Some(id) };
                    if !was_only_selected {
                        this.selected_entry_ids.insert(id);
                    }
                }
                cx.notify();
            }))
            .when_some(image_content, |this, image| this.child(image))
            .when(!is_image, |this| {
                this.child(
                    h_flex()
                        .w(px(24.))
                        .flex_none()
                        .justify_end()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(position.to_string()),
                )
                .child(div().w(px(4.)).flex_none())
                .child(content)
            })
            .context_menu(move |menu, _, cx| {
                if let Some(app) = app.upgrade() {
                    app.update(cx, |this, cx| {
                        this.navigation_entry_id = Some(id);
                        cx.notify();
                    });
                }
                let copy_app = app.clone();
                let favorite_app = app.clone();
                let delete_app = app.clone();
                if multiple_selected {
                    let selected_ids_for_favorite = selected_ids.clone();
                    let selected_ids_for_delete = selected_ids.clone();
                    let favorite_label = if all_selected_favorite {
                        "取消收藏"
                    } else {
                        "收藏"
                    };
                    menu.item(
                        PopupMenuItem::new(favorite_label)
                            .icon(if all_selected_favorite {
                                IconName::HeartOff
                            } else {
                                IconName::Heart
                            })
                            .on_click(move |_, _, cx| {
                                if let Some(app) = favorite_app.upgrade() {
                                    app.update(cx, |this, cx| {
                                        this.set_favorite_for_entries(
                                            selected_ids_for_favorite.clone(),
                                            !all_selected_favorite,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("删除").icon(IconName::Delete).on_click(
                            move |_, _, cx| {
                                if let Some(app) = delete_app.upgrade() {
                                    app.update(cx, |this, cx| {
                                        this.delete_entries(selected_ids_for_delete.clone(), cx)
                                    });
                                }
                            },
                        ),
                    )
                } else {
                    let favorite_label = if favorite { "取消收藏" } else { "收藏" };
                    menu.item(PopupMenuItem::new("复制").icon(IconName::Copy).on_click(
                        move |_, _, cx| {
                            if let Some(app) = copy_app.upgrade() {
                                app.update(cx, |this, cx| this.copy_entry(id, false, None, cx));
                            }
                        },
                    ))
                    .item(
                        PopupMenuItem::new(favorite_label)
                            .icon(if favorite {
                                IconName::HeartOff
                            } else {
                                IconName::Heart
                            })
                            .on_click(move |_, _, cx| {
                                if let Some(app) = favorite_app.upgrade() {
                                    app.update(cx, |this, cx| this.toggle_favorite(id, cx));
                                }
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("删除").icon(IconName::Delete).on_click(
                            move |_, _, cx| {
                                if let Some(app) = delete_app.upgrade() {
                                    app.update(cx, |this, cx| this.delete_entry(id, cx));
                                }
                            },
                        ),
                    )
                }
            })
            .into_any_element()
    }

    fn entry_height(entry: &ClipboardEntry, image_expanded: bool, text_expanded: bool) -> f32 {
        match &entry.content {
            ClipboardContent::Image(image) => {
                if !image_expanded {
                    return 180.;
                }
                const EXPANDED_IMAGE_WIDTH: f32 = 800.;
                const MIN_EXPANDED_IMAGE_HEIGHT: f32 = 180.;
                const MAX_EXPANDED_IMAGE_HEIGHT: f32 = 600.;
                const IMAGE_ROW_CHROME_HEIGHT: f32 = 40.;
                let aspect_height =
                    EXPANDED_IMAGE_WIDTH * image.height as f32 / image.width.max(1) as f32;
                aspect_height.clamp(MIN_EXPANDED_IMAGE_HEIGHT, MAX_EXPANDED_IMAGE_HEIGHT)
                    + IMAGE_ROW_CHROME_HEIGHT
            }
            ClipboardContent::Text(_) => {
                if entry.text_line_count() <= 1 {
                    return 64.;
                }
                let text = match &entry.content {
                    ClipboardContent::Text(text) => text,
                    _ => unreachable!(),
                };
                let displayed_lines = if text_expanded {
                    entry.text_line_count()
                } else {
                    Self::text_preview(text, COLLAPSED_TEXT_LINES)
                        .lines()
                        .count()
                };
                displayed_lines as f32 * TEXT_LINE_HEIGHT
                    + TEXT_VERTICAL_PADDING
                    + TEXT_FOOTER_HEIGHT
                    + TEXT_ROW_CHROME_HEIGHT
            }
            _ => 64.,
        }
    }

    fn toggle_image_expansion(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.expanded_image_id == Some(id) {
            self.expanded_image_id = None;
            if let Some(offset) = self.expanded_image_scroll_offset.take() {
                self.history_scroll.set_offset(offset);
            }
            cx.notify();
            return;
        }

        self.expanded_image_scroll_offset = Some(self.history_scroll.offset());
        self.expanded_image_id = Some(id);
        cx.notify();
    }

    fn toggle_text_expansion(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.expanded_text_id == Some(id) {
            self.expanded_text_id = None;
            if let Some(offset) = self.expanded_text_scroll_offset.take() {
                self.history_scroll.set_offset(offset);
            }
            cx.notify();
            return;
        }

        self.expanded_text_scroll_offset = Some(self.history_scroll.offset());
        self.expanded_text_id = Some(id);
        cx.notify();
    }

    fn text_preview(text: &str, max_lines: usize) -> String {
        if max_lines == 0 {
            return String::new();
        }

        let mut lines = text.lines();
        let preview_lines = lines.by_ref().take(max_lines).collect::<Vec<_>>();

        preview_lines.join("\n")
    }

    fn copy_entry(
        &mut self,
        id: u64,
        allow_quick_paste: bool,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.history.entry(id) else {
            return;
        };
        let content = entry.content.clone();
        let should_quick_paste = allow_quick_paste
            && self.settings.quick_paste
            && matches!(&content, ClipboardContent::Text(_));
        let should_promote =
            self.settings.promote_copied_entries && self.history.should_promote(id);
        let language = self.settings.language;
        self.status = "复制中...".into();
        cx.notify();
        let storage = self.storage.clone();
        let storage_for_promote = storage.clone();
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_spawn(
                    async move { ClipboardService::copy_content(&storage, id, content) },
                )
                .await;
            let copied = result.is_ok();
            entity
                .update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            if should_promote && let Some(updated) = this.history.promote(id) {
                                this.update_visible_entry(id);
                                let storage = storage_for_promote.clone();
                                cx.background_spawn(async move {
                                    let _ = ClipboardService::save_entry(&storage, &updated);
                                })
                                .detach();
                            }
                            if should_quick_paste {
                                if let Some(window) = window {
                                    window
                                        .update(cx, |_, window, _| {
                                            crate::platform::hide_window(window);
                                        })
                                        .ok();
                                }
                                this.status = "正在切换窗口并粘贴...".into();
                            } else {
                                this.status = "已复制".into();
                            }
                        }
                        Err(error) => {
                            let message = error.to_localized_string(language);
                            this.status = message.clone();
                            this.show_error("复制失败", message, cx);
                        }
                    }
                    cx.notify();
                })
                .ok();

            if copied && should_quick_paste {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(260))
                    .await;
                let result = cx
                    .background_spawn(async { crate::platform::clipboard::paste_shortcut() })
                    .await;
                entity
                    .update(cx, |this, cx| {
                        this.status = match result {
                            Ok(()) => "已快捷粘贴".into(),
                            Err(error) => {
                                let message = error.to_string();
                                this.show_error("快捷粘贴失败", message.clone(), cx);
                                message
                            }
                        };
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    }

    fn toggle_favorite(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(updated) = self.history.toggle_favorite(id) {
            self.update_visible_entry(id);
            let storage = self.storage.clone();
            cx.background_spawn(async move {
                let _ = ClipboardService::save_entry(&storage, &updated);
            })
            .detach();
        }
        cx.notify();
    }

    fn set_favorite_for_entries(&mut self, ids: Vec<u64>, favorite: bool, cx: &mut Context<Self>) {
        let mut updated_entries = Vec::new();
        for id in ids {
            let already_favorite = self.history.entry(id).is_some_and(|entry| entry.favorite);
            if already_favorite != favorite
                && let Some(updated) = self.history.toggle_favorite(id)
            {
                self.update_visible_entry(id);
                updated_entries.push(updated);
            }
        }

        if !updated_entries.is_empty() {
            let storage = self.storage.clone();
            cx.background_spawn(async move {
                for entry in updated_entries {
                    let _ = ClipboardService::save_entry(&storage, &entry);
                }
            })
            .detach();
        }
        cx.notify();
    }

    fn delete_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        self.delete_entries(vec![id], cx);
    }

    fn delete_entries(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        let ids = ids
            .into_iter()
            .filter(|id| self.history.entry(*id).is_some())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }

        ClipboardService::suppress_entry_saves(&self.storage, &ids);
        self.status = "删除中...".into();
        cx.notify();
        let storage = self.storage.clone();
        cx.spawn(async move |entity, cx| {
            let delete_ids = ids.clone();
            let result = cx
                .background_spawn(async move {
                    ClipboardService::delete_stored_entries(&storage, &delete_ids)
                })
                .await;
            entity
                .update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            for id in &ids {
                                this.history.remove(*id);
                                this.selected_entry_ids.remove(id);
                            }
                            this.remove_visible_entries(&ids);
                            if this.navigation_entry_id.is_some_and(|id| ids.contains(&id)) {
                                this.navigation_entry_id = None;
                            }
                            if this.selection_anchor_id.is_some_and(|id| ids.contains(&id)) {
                                this.selection_anchor_id = None;
                            }
                            if this.expanded_image_id.is_some_and(|id| ids.contains(&id)) {
                                this.expanded_image_id = None;
                                this.expanded_image_scroll_offset = None;
                            }
                            if this.expanded_text_id.is_some_and(|id| ids.contains(&id)) {
                                this.expanded_text_id = None;
                                this.expanded_text_scroll_offset = None;
                            }
                            this.status = format!("已删除 {} 条记录", ids.len());
                        }
                        Err(error) => {
                            ClipboardService::allow_entry_saves(&this.storage, &ids);
                            let message = error.to_string();
                            this.status = message.clone();
                            this.show_error("删除失败", message, cx);
                        }
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    pub(super) fn clear_current_filter(&mut self, cx: &mut Context<Self>) {
        let filter = self.filter;
        let ids = self.history.deletable_ids_for_filter(filter, false);
        ClipboardService::suppress_entry_saves(&self.storage, &ids);
        self.status = "清空中...".into();
        let storage = self.storage.clone();
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_spawn({
                    let ids = ids.clone();
                    async move {
                        if filter == ClipboardFilter::All {
                            ClipboardService::clear_stored_history(&storage)
                        } else {
                            ClipboardService::delete_stored_entries(&storage, &ids)
                        }
                    }
                })
                .await;
            entity
                .update(cx, |this, cx| {
                    if let Err(error) = result {
                        ClipboardService::allow_entry_saves(&this.storage, &ids);
                        let message = error.to_string();
                        this.status = message.clone();
                        this.show_error("清空失败", message, cx);
                        cx.notify();
                        return;
                    }

                    if filter == ClipboardFilter::All {
                        this.history.clear();
                    } else {
                        for id in &ids {
                            this.history.remove(*id);
                        }
                    }
                    this.remove_visible_entries(&ids);
                    this.selected_entry_ids.clear();
                    this.selection_anchor_id = None;
                    this.navigation_entry_id = None;
                    this.expanded_image_id = None;
                    this.expanded_image_scroll_offset = None;
                    this.status = match filter {
                        ClipboardFilter::All => "全部历史已清空",
                        ClipboardFilter::Text => "文本记录已清空",
                        ClipboardFilter::Image => "图片记录已清空",
                        ClipboardFilter::File => "文件记录已清空",
                        ClipboardFilter::Favorite => "收藏记录已清空",
                    }
                    .into();
                    cx.notify();
                })
                .ok();
        })
        .detach();
        cx.notify();
    }
}
