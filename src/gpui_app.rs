use crate::model::{AppSettings, ClipboardContent, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::services::ClipboardService;
use crate::storage;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Root, Sizable as _, StyledExt as _, Theme, ThemeMode,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    switch::Switch,
    tab::{Tab, TabBar},
    v_flex, v_virtual_list,
};
use gpui_component_assets::Assets;
use std::time::Duration;

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
            ..Default::default()
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
            _subscriptions: subscriptions,
        };
        app.start_clipboard_monitor(cx);
        app
    }

    fn start_clipboard_monitor(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |entity, cx| {
            let mut last_sequence = None;
            loop {
                smol::Timer::after(Duration::from_millis(650)).await;
                let sequence = cx
                    .background_spawn(async { platform::clipboard::sequence_number() })
                    .await;
                if sequence.is_some() && sequence == last_sequence {
                    continue;
                }
                last_sequence = sequence;
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
            .pill()
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
            std::rc::Rc::new(vec![size(px(900.), px(72.)); self.visible_entries.len()]);
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

        v_flex().size_full().gap_3().child(filters).child(list)
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
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
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
            .p_4()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().text_xl().font_semibold().child("UCP Clipboard"))
                    .child(
                        Input::new(&self.search)
                            .prefix(IconName::Search)
                            .cleanable(true),
                    )
                    .child(
                        Button::new("settings")
                            .ghost()
                            .icon(IconName::Settings2)
                            .tooltip("设置")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page = if this.page == AppPage::History {
                                    AppPage::Settings
                                } else {
                                    AppPage::History
                                };
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("clear")
                            .danger()
                            .small()
                            .icon(IconName::Delete)
                            .label("清空")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.history.clear();
                                let _ = storage::clear_history();
                                this.status = "历史已清空".into();
                                cx.notify();
                            })),
                    ),
            )
            .child(if page == AppPage::History {
                self.render_history(cx).into_any_element()
            } else {
                self.render_settings(cx).into_any_element()
            })
            .child(
                h_flex()
                    .justify_between()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .pt_2()
                    .child(self.status.clone())
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
            )
    }
}

impl ClipboardApp {
    fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_4()
            .child(div().text_2xl().font_semibold().child("设置"))
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
            .child(
                Button::new("back")
                    .secondary()
                    .icon(IconName::ArrowLeft)
                    .label("返回历史")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.page = AppPage::History;
                        cx.notify();
                    })),
            )
    }
}
