use crate::model::{AppSettings, ClipboardContent, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::storage;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Root, Sizable as _, StyledExt as _, Theme, ThemeMode,
    TitleBar, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    input::{InputEvent, InputState},
    status_bar::StatusBar,
    v_flex,
};
use gpui_component_assets::Assets;

mod history;
mod settings;

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
    selected_entry_id: Option<u64>,
    expanded_image_id: Option<u64>,
    visible_entries: Vec<std::rc::Rc<crate::model::ClipboardEntry>>,
    search: Entity<InputState>,
    initial_focus: FocusHandle,
    history_scroll: gpui_component::VirtualListScrollHandle,
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
        let initial_focus = cx.focus_handle();
        initial_focus.focus(window, cx);
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
            selected_entry_id: None,
            expanded_image_id: None,
            visible_entries: Vec::new(),
            search,
            initial_focus,
            history_scroll: gpui_component::VirtualListScrollHandle::new(),
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
        let result_id = entry.as_ref().map(|entry| entry.id).unwrap_or_default();
        let removed_ids = result.removed_ids;
        cx.spawn(async move |entity, cx| {
            let saved_preview = cx
                .background_spawn(async move {
                    let saved_preview = entry
                        .as_ref()
                        .map(storage::save_entry)
                        .transpose()
                        .ok()
                        .flatten()
                        .flatten();
                    if !removed_ids.is_empty() {
                        let _ = storage::delete_entries(&removed_ids);
                    }
                    saved_preview
                })
                .await;
            if let Some(preview_url) = saved_preview {
                entity
                    .update(cx, |this, cx| {
                        if this.history.set_image_preview_url(result_id, preview_url) {
                            cx.notify();
                        }
                    })
                    .ok();
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
}

impl Render for ClipboardApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        let dialog_layer = Root::render_dialog_layer(window, cx);
        v_flex()
            .track_focus(&self.initial_focus)
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
                    .left(format!("{} 条记录", self.history.counts().total))
                    .child(self.status.clone())
                    .right(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Button::new("status-settings")
                                    .ghost()
                                    .large()
                                    .child(
                                        Icon::new(if page == AppPage::History {
                                            IconName::Settings2
                                        } else {
                                            IconName::ArrowLeft
                                        })
                                        .small(),
                                    )
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
                                let app = cx.entity().downgrade();
                                let filter = self.filter;
                                let (title, description, confirm_text) = match filter {
                                    ClipboardFilter::All => (
                                        "清空全部历史记录？",
                                        "此操作将永久删除全部剪贴板历史，且无法撤销。",
                                        "清空全部",
                                    ),
                                    ClipboardFilter::Text => (
                                        "清空全部文本记录？",
                                        "此操作将永久删除全部文本记录，且无法撤销。",
                                        "清空文本",
                                    ),
                                    ClipboardFilter::Image => (
                                        "清空全部图片记录？",
                                        "此操作将永久删除全部图片记录，且无法撤销。",
                                        "清空图片",
                                    ),
                                    ClipboardFilter::File => (
                                        "清空全部文件记录？",
                                        "此操作将永久删除全部文件记录，且无法撤销。",
                                        "清空文件",
                                    ),
                                    ClipboardFilter::Favorite => (
                                        "清空全部收藏记录？",
                                        "此操作将永久删除全部收藏记录，且无法撤销。",
                                        "清空收藏",
                                    ),
                                };
                                this.child(
                                    Button::new("status-clear")
                                        .ghost()
                                        .large()
                                        .child(
                                            Icon::new(IconName::Delete)
                                                .small()
                                                .text_color(cx.theme().danger),
                                        )
                                        .tooltip("清空历史")
                                        .on_click(move |_, window, cx| {
                                            let app = app.clone();
                                            window.open_alert_dialog(cx, move |alert, _, _| {
                                                let app = app.clone();
                                                alert
                                                    .title(title)
                                                    .description(description)
                                                    .button_props(
                                                        DialogButtonProps::default()
                                                            .ok_variant(ButtonVariant::Danger)
                                                            .ok_text(confirm_text)
                                                            .cancel_text("取消")
                                                            .show_cancel(true),
                                                    )
                                                    .on_ok(move |_, _, cx| {
                                                        if let Some(app) = app.upgrade() {
                                                            app.update(cx, |this, cx| {
                                                                this.clear_current_filter(cx);
                                                            });
                                                        }
                                                        true
                                                    })
                                            });
                                        }),
                                )
                            }),
                    ),
            )
            .children(dialog_layer)
    }
}
