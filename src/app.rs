use crate::model::{AppSettings, ClipboardContent, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::storage;
use crate::updater::{self, UpdateCheck, UpdateInfo};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, FocusableExt as _, Icon, IconName, Root, Sizable as _, Theme, ThemeMode,
    TitleBar, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    input::{InputEvent, InputState},
    status_bar::StatusBar,
    v_flex,
};
use gpui_component_assets::Assets;
use std::borrow::Cow;

mod history;
mod settings;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppPage {
    History,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UpdateCheckState {
    Idle,
    Checking,
    UpToDate(String),
    Available(UpdateInfo),
    Failed(String),
}

struct AppAssets(Assets);

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "icons/pin.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/pin.svg"
            ))));
        }
        self.0.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = self.0.list(path)?;
        if "icons/pin.svg".starts_with(path) {
            assets.push("icons/pin.svg".into());
        }
        Ok(assets)
    }
}

pub fn run(visible: bool) {
    let app = gpui_platform::application().with_assets(AppAssets(Assets));
    app.run(move |cx| {
        gpui_component::init(cx);
        #[cfg(windows)]
        let tray = platform::tray::create().ok();
        #[cfg(windows)]
        let has_tray = tray.is_some();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(900.), px(660.)), cx)),
            show: visible,
            ..TitleBar::window_options()
        };
        cx.spawn(async move |cx| {
            let window = cx
                .open_window(options, |window, cx| {
                    window.set_window_title("UCP Clipboard");
                    let view = cx.new(|cx| ClipboardApp::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx).bordered(false))
                })
                .expect("Failed to open GPUI window");
            #[cfg(windows)]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, ShowWindow};

                let hwnd = window
                    .update(cx, |_, window, _| {
                        match HasWindowHandle::window_handle(window) {
                            Ok(handle) => match handle.as_raw() {
                                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                                _ => None,
                            },
                            Err(_) => None,
                        }
                    })
                    .ok()
                    .flatten();
                if has_tray && hwnd.is_some() {
                    window
                        .update(cx, |_, window, cx| {
                            window.on_window_should_close(cx, move |_, _| {
                                if let Some(hwnd) = hwnd {
                                    unsafe { ShowWindow(hwnd as _, SW_HIDE) };
                                }
                                false
                            });
                        })
                        .ok();
                }

                cx.spawn(async move |cx| {
                    let _tray = tray;
                    loop {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(100))
                            .await;
                        let should_show = platform::tray::take_show_request()
                            || platform::single_instance::take_activation_request();
                        let should_quit = platform::tray::take_quit_request()
                            || platform::single_instance::take_quit_request();
                        cx.update(|cx| {
                            if should_quit {
                                cx.quit();
                            } else if should_show {
                                if let Some(hwnd) = hwnd {
                                    unsafe { ShowWindow(hwnd as _, SW_SHOW) };
                                }
                                cx.activate(true);
                                window
                                    .update(cx, |_, window, _| window.activate_window())
                                    .ok();
                            }
                        });
                        if should_quit {
                            break;
                        }
                    }
                })
                .detach();
            }
        })
        .detach();
    });
}

struct ClipboardApp {
    storage: storage::StorageHandle,
    settings: AppSettings,
    history: ClipboardHistory,
    query: String,
    filter: ClipboardFilter,
    page: AppPage,
    status: String,
    monitor_paused: bool,
    always_on_top: bool,
    update_check: UpdateCheckState,
    selected_entry_id: Option<u64>,
    navigation_entry_id: Option<u64>,
    expanded_image_id: Option<u64>,
    expanded_text_id: Option<u64>,
    expanded_text_scroll_offset: Option<Point<Pixels>>,
    visible_entries: Vec<std::rc::Rc<crate::model::ClipboardEntry>>,
    search: Entity<InputState>,
    initial_focus: FocusHandle,
    history_scroll: gpui_component::VirtualListScrollHandle,
    _clipboard_listener: Option<platform::clipboard::ClipboardUpdateListener>,
    _subscriptions: Vec<Subscription>,
}

impl ClipboardApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let storage = storage::StorageHandle::new().expect("Failed to initialize storage");
        let settings = storage::load_settings(&storage).unwrap_or_default();
        let theme_mode = if matches!(settings.theme, crate::model::AppTheme::Dark) {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(theme_mode, Some(window), cx);
        Self::apply_palette(cx);
        let history = storage::load_history(&storage, settings.history_limit)
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
        let event_tx = update_tx.clone();
        let clipboard_listener = platform::clipboard::listen_for_updates(move || {
            let _ = event_tx.send_blocking(());
        })
        .ok();
        let mut app = Self {
            storage,
            settings,
            history,
            query: String::new(),
            filter: ClipboardFilter::All,
            page: AppPage::History,
            status: String::new(),
            monitor_paused: false,
            always_on_top: false,
            update_check: UpdateCheckState::Idle,
            selected_entry_id: None,
            navigation_entry_id: None,
            expanded_image_id: None,
            expanded_text_id: None,
            expanded_text_scroll_offset: None,
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

    fn apply_palette(cx: &mut App) {
        let theme = Theme::global_mut(cx);
        if theme.is_dark() {
            theme.background = rgb(0x181a1d).into();
            theme.foreground = rgb(0xe7e9ec).into();
            theme.muted = rgb(0x25282d).into();
            theme.muted_foreground = rgb(0x9ba1aa).into();
            theme.secondary = rgb(0x272a2f).into();
            theme.secondary_hover = rgb(0x30343a).into();
            theme.accent = rgb(0x2d3137).into();
            theme.border = rgb(0x34383f).into();
            theme.input = rgb(0x3b4048).into();
            theme.colors.list = rgb(0x1d1f23).into();
            theme.tab_bar_segmented = rgb(0x23262a).into();
            theme.tab_active = rgb(0x34383f).into();
            theme.title_bar = rgb(0x202226).into();
            theme.title_bar_border = rgb(0x34383f).into();
            theme.status_bar = rgb(0x202226).into();
            theme.status_bar_border = rgb(0x34383f).into();
        } else {
            theme.background = rgb(0xf7f8fa).into();
            theme.foreground = rgb(0x20242a).into();
            theme.muted = rgb(0xeff1f4).into();
            theme.muted_foreground = rgb(0x68707c).into();
            theme.secondary = rgb(0xf0f2f5).into();
            theme.secondary_hover = rgb(0xe7eaf0).into();
            theme.accent = rgb(0xe9edf2).into();
            theme.border = rgb(0xdde1e7).into();
            theme.input = rgb(0xcfd5dd).into();
            theme.colors.list = rgb(0xffffff).into();
            theme.tab_bar_segmented = rgb(0xeff1f4).into();
            theme.tab_active = rgb(0xffffff).into();
            theme.title_bar = rgb(0xf1f3f6).into();
            theme.title_bar_border = rgb(0xdde1e7).into();
            theme.status_bar = rgb(0xf1f3f6).into();
            theme.status_bar_border = rgb(0xdde1e7).into();
        }

        theme.tokens.background = theme.background.into();
        theme.tokens.muted = theme.muted.into();
        theme.tokens.secondary = theme.secondary.into();
        theme.tokens.accent = theme.accent.into();
        theme.tokens.status_bar = theme.status_bar.into();
    }

    fn apply_theme(theme: crate::model::AppTheme, cx: &mut App) {
        let mode = if matches!(theme, crate::model::AppTheme::Dark) {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(mode, None, cx);
        Self::apply_palette(cx);
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
        let storage = self.storage.clone();
        cx.spawn(async move |entity, cx| {
            let saved_preview = cx
                .background_spawn(async move {
                    let saved_preview = entry
                        .as_ref()
                        .map(|e| storage::save_entry(&storage, e))
                        .transpose()
                        .ok()
                        .flatten()
                        .flatten();
                    if !removed_ids.is_empty() {
                        let _ = storage::delete_entries(&storage, &removed_ids);
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

    fn start_update_check(&mut self, cx: &mut Context<Self>) {
        if matches!(self.update_check, UpdateCheckState::Checking) {
            return;
        }

        self.update_check = UpdateCheckState::Checking;
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_spawn(async { updater::check_for_updates() })
                .await;
            entity
                .update(cx, |this, cx| {
                    this.update_check = match result {
                        Ok(UpdateCheck::Available(info)) => UpdateCheckState::Available(info),
                        Ok(UpdateCheck::UpToDate { latest_version }) => {
                            UpdateCheckState::UpToDate(latest_version)
                        }
                        Err(error) => UpdateCheckState::Failed(error),
                    };
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn save_settings(&mut self) {
        self.settings = self.settings.clone().normalized();
        if let Err(error) = storage::save_settings(&self.storage, &self.settings) {
            self.status = format!("设置保存失败：{error}");
        } else {
            self.status = "设置已保存".into();
        }
    }

    #[cfg(windows)]
    fn set_always_on_top(window: &Window, always_on_top: bool) -> bool {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, HWND_NOTOPMOST, HWND_TOPMOST, SWP_FRAMECHANGED,
            SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, WS_EX_TOPMOST,
        };

        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return false;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return false;
        };
        let insert_after = if always_on_top {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };

        unsafe {
            let changed = SetWindowPos(
                handle.hwnd.get() as _,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
            ) != 0;
            let is_topmost = GetWindowLongPtrW(handle.hwnd.get() as _, GWL_EXSTYLE)
                & WS_EX_TOPMOST as isize
                != 0;
            changed && is_topmost == always_on_top
        }
    }

    #[cfg(not(windows))]
    fn set_always_on_top(_: &Window, _: bool) -> bool {
        false
    }
}

impl Render for ClipboardApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        let dialog_layer = Root::render_dialog_layer(window, cx);
        v_flex()
            .track_focus(&self.initial_focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                this.handle_history_key_down(event, window, cx);
            }))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new())
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
                    .right(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Button::new("always-on-top")
                                    .ghost()
                                    .focus_ring(false)
                                    .large()
                                    .child(
                                        Icon::default().path("icons/pin.svg").small().text_color(
                                            if self.always_on_top {
                                                rgb(0x3b82f6).into()
                                            } else {
                                                cx.theme().muted_foreground
                                            },
                                        ),
                                    )
                                    .tooltip(if self.always_on_top {
                                        "取消置顶"
                                    } else {
                                        "窗口置顶"
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let always_on_top = !this.always_on_top;
                                        if Self::set_always_on_top(window, always_on_top) {
                                            this.always_on_top = always_on_top;
                                            cx.notify();
                                        }
                                    })),
                            )
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
