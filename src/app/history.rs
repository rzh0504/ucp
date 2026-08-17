use super::ClipboardApp;
use crate::model::{AppLanguage, ClipboardEntry, ClipboardFilter};
use crate::services::ClipboardService;
use crate::storage;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    tab::{Tab, TabBar},
    v_flex, v_virtual_list,
};
use std::rc::Rc;

impl ClipboardApp {
    pub(super) fn render_history(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.settings.language;
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
                cx.notify();
            }))
            .child(Self::filter_tab(IconName::Inbox, "全部", counts.total))
            .child(Self::filter_tab(IconName::ALargeSmall, "文本", counts.text))
            .child(Self::filter_tab(IconName::Frame, "图片", counts.image))
            .child(Self::filter_tab(IconName::File, "文件", counts.file))
            .child(Self::filter_tab(IconName::Heart, "收藏", counts.favorite));
        self.visible_entries = self.history.filtered(&self.query, self.filter);
        let item_sizes = Rc::new(vec![size(px(0.), px(64.)); self.visible_entries.len()]);
        let list = v_virtual_list(
            cx.entity().clone(),
            "history-list",
            item_sizes,
            move |this, range, _window, cx| {
                range
                    .filter_map(|index| {
                        this.visible_entries.get(index).cloned().map(|entry| {
                            let selected = this.selected_entry_id == Some(entry.id);
                            Self::render_entry(entry, language, selected, cx)
                        })
                    })
                    .collect()
            },
        )
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
            list.into_any_element()
        };

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_4()
                    .items_center()
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
            .child(content)
    }

    fn filter_tab(icon: IconName, label: &'static str, count: usize) -> Tab {
        Tab::new().child(
            h_flex()
                .gap_1()
                .child(Icon::new(icon).small())
                .child(format!("{label} {count}")),
        )
    }

    fn render_entry(
        entry: Rc<ClipboardEntry>,
        language: AppLanguage,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.id;
        let title = entry.title_with_language(language);
        let meta = entry.size_label_with_language(language);
        let favorite = entry.favorite;
        let hover_group = SharedString::from(format!("entry-{id}"));
        let content = v_flex()
            .flex_1()
            .min_w_0()
            .child(div().truncate().child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(meta),
            );
        let copy = Button::new(("copy", id))
            .ghost()
            .large()
            .child(Icon::new(IconName::Copy).small())
            .tooltip("复制")
            .on_click(cx.listener(move |this, _, _, cx| this.copy_entry(id, cx)));
        let favorite_button = Button::new(("favorite", id))
            .ghost()
            .large()
            .child(
                Icon::new(if favorite {
                    IconName::Heart
                } else {
                    IconName::HeartOff
                })
                .small(),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_favorite(id, cx)));
        let delete = Button::new(("delete", id))
            .ghost()
            .large()
            .child(Icon::new(IconName::Delete).small())
            .on_click(cx.listener(move |this, _, _, cx| this.delete_entry(id, cx)));

        let actions = h_flex()
            .gap_1()
            .when(!selected, |this| {
                this.invisible()
                    .group_hover(hover_group.clone(), |style| style.visible())
            })
            .child(copy)
            .child(favorite_button)
            .child(delete);

        h_flex()
            .id(ElementId::NamedInteger("entry".into(), id))
            .group(hover_group)
            .w_full()
            .h(px(64.))
            .gap_2()
            .px_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .when(selected, |this| this.bg(cx.theme().secondary))
            .hover(|style| style.bg(cx.theme().secondary_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_entry_id = Some(id);
                cx.notify();
            }))
            .child(content)
            .child(actions)
            .into_any_element()
    }

    fn copy_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.history.entry(id) else {
            return;
        };
        let content = entry.content.clone();
        let should_promote =
            self.settings.promote_copied_entries && self.history.should_promote(id);
        let language = self.settings.language;
        self.status = "复制中...".into();
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_spawn(async move { ClipboardService::copy_content(id, content) })
                .await;
            entity
                .update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            if should_promote && let Some(updated) = this.history.promote(id) {
                                cx.background_spawn(async move {
                                    let _ = storage::save_entry(&updated);
                                })
                                .detach();
                            }
                            this.status = "已复制".into();
                        }
                        Err(error) => this.status = error.to_localized_string(language),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn toggle_favorite(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(updated) = self.history.toggle_favorite(id) {
            cx.background_spawn(async move {
                let _ = storage::save_entry(&updated);
            })
            .detach();
        }
        cx.notify();
    }

    fn delete_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.history.remove(id) {
            if self.selected_entry_id == Some(id) {
                self.selected_entry_id = None;
            }
            cx.background_spawn(async move {
                let _ = storage::delete_entries(&[id]);
            })
            .detach();
            self.status = "已删除".into();
            cx.notify();
        }
    }

    pub(super) fn clear_current_filter(&mut self, cx: &mut Context<Self>) {
        let filter = self.filter;
        let ids = self.history.deletable_ids_for_filter(filter, false);
        if filter == ClipboardFilter::All {
            self.history.clear();
        } else {
            for id in &ids {
                self.history.remove(*id);
            }
        }
        self.selected_entry_id = None;
        cx.background_spawn(async move {
            if filter == ClipboardFilter::All {
                let _ = storage::clear_history();
            } else {
                let _ = storage::delete_entries(&ids);
            }
        })
        .detach();
        self.status = match filter {
            ClipboardFilter::All => "全部历史已清空",
            ClipboardFilter::Text => "文本记录已清空",
            ClipboardFilter::Image => "图片记录已清空",
            ClipboardFilter::File => "文件记录已清空",
            ClipboardFilter::Favorite => "收藏记录已清空",
        }
        .into();
        cx.notify();
    }
}
