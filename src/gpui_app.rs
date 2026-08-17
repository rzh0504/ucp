use crate::model::{
    AppSettings, ClipboardContent, ClipboardFilter, ClipboardHistory, ClipboardKind,
};
use crate::platform;
use crate::services::ClipboardService;
use crate::storage;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Root, Sizable as _, StyledExt as _, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    status_bar::StatusBar,
    switch::Switch,
    tab::{Tab, TabBar},
    v_flex, v_virtual_list,
};
use gpui_component_assets::Assets;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppPage {
    History,
    Settings,
}

pub fn run(visible: bool) {
    let app = gpui_platform::application().with_assets(Assets);
    app.run(move |cx| {
        gpui_component::init(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(900.), px(660.)), cx)),
            show: visible,
            ..TitleBar::window_options()
        };
        cx.spawn(async move |cx| {
            cx.open_window(options, |window, cx| {
                window.set_window_title("UCP Clipboard");
                let view = cx.new(|cx| ClipboardApp::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx).bordered(false))
            })
            .expect("Failed to open GPUI window");
        })
        .detach();
    });
}

struct ClipboardApp {
    settings: AppSettings,
    history: ClipboardHistory,
    query: String,
    filter: ClipboardFilter,
    page: AppPage,
    status: String,
    monitor_paused: bool,
    visible_entries: Vec<std::rc::Rc<crate::model::ClipboardEntry>>,
    search: Entity<InputState>,
    _clipboard_listener: Option<platform::clipboard::ClipboardUpdateListener>,
    _subscriptions: Vec<Subscription>,
}

impl ClipboardApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = storage::load_settings().unwrap_or_default();
        let theme_mode = if matches!(settings.theme, crate::model::AppTheme::Dark) {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(theme_mode, Some(window), cx);
        let history = storage::load_history(settings.history_limit)
            .unwrap_or_else(|_| ClipboardHistory::new(settings.history_limit));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索剪贴板历史..."));
        let subscriptions = vec![cx.subscribe_in(&search, window, {
            let search = search.clone();
            move |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.query = search.read(cx).value().to_string();
                    cx.notify();
                }
            }
        })];
        let (update_tx, update_rx) = async_channel::unbounded();
        let _ = update_tx.try_send(());
        let event_tx = update_tx.clone();
        let clipboard_listener = platform::clipboard::listen_for_updates(move || {
            let _ = event_tx.send_blocking(());
        })
        .ok();
        let mut app = Self {
            settings,
            history,
            query: String::new(),
            filter: ClipboardFilter::All,
            page: AppPage::History,
            status: String::new(),
            monitor_paused: false,
            visible_entries: Vec::new(),
            search,
            _clipboard_listener: clipboard_listener,
            _subscriptions: subscriptions,
        };
        app.start_clipboard_monitor(update_rx, cx);
        app
    }

    fn start_clipboard_monitor(
        &mut self,
        updates: async_channel::Receiver<()>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |entity, cx| {
            while updates.recv().await.is_ok() {
                let paused = entity
                    .update(cx, |this, _| this.monitor_paused)
                    .unwrap_or(true);
                if paused {
                    continue;
                }
                let content = cx
                    .background_spawn(async { platform::clipboard::read_content().ok().flatten() })
                    .await;
                let Some(content) = content else { continue };
                if entity
                    .update(cx, |this, cx| this.capture(content, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn capture(&mut self, content: ClipboardContent, cx: &mut Context<Self>) {
        if self.monitor_paused || !self.history.would_push_change(&content) {
            return;
        }
        let result = self.history.push(content);
        let entry = result.entry;
        let removed_ids = result.removed_ids;
        cx.background_spawn(async move {
            if let Some(entry) = entry.as_ref() {
                let _ = storage::save_entry(entry);
            }
            if !removed_ids.is_empty() {
                let _ = storage::delete_entries(&removed_ids);
            }
        })
        .detach();
        cx.notify();
    }

    fn save_settings(&mut self) {
        self.settings = self.settings.clone().normalized();
        if let Err(error) = storage::save_settings(&self.settings) {
            self.status = format!("设置保存失败：{error}");
        } else {
            self.status = "设置已保存".into();
        }
    }

    fn render_history(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.settings.language;
        let counts = self.history.counts();
        let filters = TabBar::new("filters")
            .underline()
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
            .child(Tab::new().label(format!("全部 {}", counts.total)))
            .child(Tab::new().label(format!("文本 {}", counts.text)))
            .child(Tab::new().label(format!("图片 {}", counts.image)))
            .child(Tab::new().label(format!("文件 {}", counts.file)))
            .child(Tab::new().label(format!("收藏 {}", counts.favorite)));
        self.visible_entries = self.history.filtered(&self.query, self.filter);
        let item_sizes =
            std::rc::Rc::new(vec![size(px(900.), px(64.)); self.visible_entries.len()]);
        let list = v_virtual_list(
            cx.entity().clone(),
            "history-list",
            item_sizes,
            move |this, range, _window, cx| {
                range
                    .filter_map(|index| {
                        this.visible_entries
                            .get(index)
                            .cloned()
                            .map(|entry| Self::render_entry(entry, language, cx))
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
                    .gap_3()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(filters)
                    .child(div().flex_1())
                    .child(
                        Input::new(&self.search)
                            .small()
                            .w(px(280.))
                            .prefix(IconName::Search)
                            .cleanable(true),
                    ),
            )
            .child(content)
    }

    fn render_entry(
        entry: std::rc::Rc<crate::model::ClipboardEntry>,
        language: crate::model::AppLanguage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.id;
        let title = entry.title_with_language(language);
        let meta = entry.size_label_with_language(language);
        let favorite = entry.favorite;
        let kind_icon = match entry.kind() {
            ClipboardKind::Text => IconName::ALargeSmall,
            ClipboardKind::Image => IconName::Frame,
            ClipboardKind::File => IconName::File,
        };
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
            .xsmall()
            .icon(IconName::Copy)
            .tooltip("复制")
            .on_click(cx.listener(move |this, _, _, cx| {
                match ClipboardService::copy_entry(
                    &this.history,
                    id,
                    this.settings.promote_copied_entries,
                ) {
                    Ok(result) => {
                        if result.should_promote
                            && let Some(updated) = this.history.promote(id)
                        {
                            let _ = storage::save_entry(&updated);
                        }
                        this.status = "已复制".into();
                    }
                    Err(error) => this.status = error.to_localized_string(this.settings.language),
                }
                cx.notify();
            }));
        let favorite_button = Button::new(("favorite", id))
            .ghost()
            .xsmall()
            .icon(if favorite {
                IconName::Heart
            } else {
                IconName::HeartOff
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(updated) = this.history.toggle_favorite(id) {
                    let _ = storage::save_entry(&updated);
                }
                cx.notify();
            }));
        let delete = Button::new(("delete", id))
            .ghost()
            .xsmall()
            .icon(IconName::Delete)
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.history.remove(id) {
                    let _ = storage::delete_entries(&[id]);
                    this.status = "已删除".into();
                    cx.notify();
                }
            }));

        h_flex()
            .id(ElementId::NamedInteger("entry".into(), id))
            .w_full()
            .h(px(64.))
            .gap_2()
            .px_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .hover(|style| style.bg(cx.theme().secondary_hover))
            .child(
                div()
                    .size_8()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(cx.theme().secondary)
                    .text_color(cx.theme().muted_foreground)
                    .child(kind_icon),
            )
            .child(content)
            .child(copy)
            .child(favorite_button)
            .child(delete)
            .into_any_element()
    }
}

impl Render for ClipboardApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .child(div().font_semibold().child("UCP")),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(if page == AppPage::History {
                        self.render_history(cx).into_any_element()
                    } else {
                        self.render_settings(cx).into_any_element()
                    }),
            )
            .child(
                StatusBar::new()
                    .left(if self.status.is_empty() {
                        if self.monitor_paused {
                            "监听已暂停".to_string()
                        } else {
                            "就绪".to_string()
                        }
                    } else {
                        self.status.clone()
                    })
                    .child(format!("{} 条记录", self.history.counts().total))
                    .right(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Button::new("status-settings")
                                    .ghost()
                                    .xsmall()
                                    .icon(if page == AppPage::History {
                                        IconName::Settings2
                                    } else {
                                        IconName::ArrowLeft
                                    })
                                    .tooltip(if page == AppPage::History {
                                        "设置"
                                    } else {
                                        "返回历史"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.page = if this.page == AppPage::History {
                                            AppPage::Settings
                                        } else {
                                            AppPage::History
                                        };
                                        cx.notify();
                                    })),
                            )
                            .when(page == AppPage::History, |this| {
                                this.child(
                                    Button::new("status-clear")
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Delete)
                                        .tooltip("清空历史")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.history.clear();
                                            let _ = storage::clear_history();
                                            this.status = "历史已清空".into();
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(
                                Switch::new("monitor")
                                    .small()
                                    .label("监听剪贴板")
                                    .checked(!self.monitor_paused)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.monitor_paused = !*checked;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
    }
}

impl ClipboardApp {
    fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_4()
            .gap_4()
            .child(
                Switch::new("startup")
                    .label("开机启动")
                    .checked(self.settings.launch_at_startup)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.settings.launch_at_startup = *checked;
                        if let Err(error) = platform::startup::set_enabled(*checked) {
                            this.status = error;
                        } else {
                            this.save_settings();
                        }
                        cx.notify();
                    })),
            )
            .child(
                Switch::new("promote")
                    .label("复制后提升记录")
                    .checked(self.settings.promote_copied_entries)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.settings.promote_copied_entries = *checked;
                        this.save_settings();
                        cx.notify();
                    })),
            )
            .child(
                Switch::new("quick-paste")
                    .label("启用快捷粘贴")
                    .checked(self.settings.quick_paste)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.settings.quick_paste = *checked;
                        this.save_settings();
                        cx.notify();
                    })),
            )
    }
}
