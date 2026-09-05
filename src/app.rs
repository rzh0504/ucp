use crate::model::{AppSettings, ClipboardContent, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::services::{ClipboardService, ClipboardStorage};
use crate::updater::{self, UpdateCheck, UpdateInfo};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, FocusableExt as _, Icon, IconName, Root, Sizable as _, Theme, ThemeMode,
    ThemeRegistry, TitleBar, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    input::{InputEvent, InputState},
    notification::{Notification, NotificationDelivery},
    status_bar::StatusBar,
    v_flex,
};
use gpui_kit_assets::Assets;
use std::borrow::Cow;

mod history;
mod settings;
#[cfg(test)]
mod tests;

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
        if path == "icons/file-missing.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/file-missing.svg"
            ))));
        }
        self.0.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = self.0.list(path)?;
        if "icons/pin.svg".starts_with(path) {
            assets.push("icons/pin.svg".into());
        }
        if "icons/file-missing.svg".starts_with(path) {
            assets.push("icons/file-missing.svg".into());
        }
        Ok(assets)
    }
}

pub fn run(visible: bool) {
    let app = gpui_platform::application().with_assets(AppAssets(Assets));
    app.run(move |cx| {
        // Required on Windows before system notifications can be posted.
        cx.set_app_identity("dev.ucp.clipboard", "UCP");
        gpui_component::init(cx);
        ClipboardApp::install_themes(cx);
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
                    window.set_window_title("UCP");
                    let view = cx.new(|cx| ClipboardApp::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx).bordered(false))
                })
                .expect("Failed to open GPUI window");
            #[cfg(windows)]
            {
                let hwnd = window
                    .update(cx, |_, window, _| platform::windows::window_handle(window))
                    .ok()
                    .flatten();
                if has_tray && hwnd.is_some() {
                    window
                        .update(cx, |_, window, cx| {
                            window.on_window_should_close(cx, move |_, _| {
                                if let Some(hwnd) = hwnd {
                                    platform::windows::hide_window(hwnd);
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
                                    platform::windows::show_window(hwnd);
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
    storage: ClipboardStorage,
    settings: AppSettings,
    history: ClipboardHistory,
    history_loading: bool,
    query: String,
    filter: ClipboardFilter,
    page: AppPage,
    status: String,
    monitor_paused: bool,
    always_on_top: bool,
    editing_global_shortcut: bool,
    update_check: UpdateCheckState,
    selected_entry_ids: std::collections::HashSet<u64>,
    selection_anchor_id: Option<u64>,
    navigation_entry_id: Option<u64>,
    expanded_image_id: Option<u64>,
    expanded_image_scroll_offset: Option<Point<Pixels>>,
    expanded_text_id: Option<u64>,
    expanded_text_scroll_offset: Option<Point<Pixels>>,
    expanded_text_scroll: ScrollHandle,
    visible_entries: Vec<std::rc::Rc<crate::model::ClipboardEntry>>,
    file_icon_paths: std::collections::HashMap<u64, std::path::PathBuf>,
    file_icon_loading: std::collections::HashSet<u64>,
    file_icon_failed: std::collections::HashSet<u64>,
    missing_file_entries: std::collections::HashSet<u64>,
    search: Entity<InputState>,
    initial_focus: FocusHandle,
    window_handle: AnyWindowHandle,
    history_scroll: gpui_component::VirtualListScrollHandle,
    _clipboard_listener: Option<platform::clipboard::ClipboardUpdateListener>,
    _subscriptions: Vec<Subscription>,
}

impl ClipboardApp {
    fn refresh_visible_entries(&mut self) {
        self.visible_entries = self.history.filtered(&self.query, self.filter);
        self.retain_visible_selection();
    }

    fn update_visible_entry(&mut self, id: u64) {
        self.visible_entries.retain(|entry| entry.id != id);
        if let Some(entry) = self.history.entry(id)
            && crate::model::ClipboardHistory::entry_matches(&entry, &self.query, self.filter)
        {
            let position = self.history.position(id).unwrap_or(usize::MAX);
            let insert_at = self
                .visible_entries
                .iter()
                .position(|visible| {
                    self.history
                        .position(visible.id)
                        .is_some_and(|visible_position| visible_position > position)
                })
                .unwrap_or(self.visible_entries.len());
            self.visible_entries.insert(insert_at, entry);
        }
        self.retain_visible_selection();
    }

    fn remove_visible_entries(&mut self, ids: &[u64]) {
        self.visible_entries
            .retain(|entry| !ids.contains(&entry.id));
        self.retain_visible_selection();
    }

    fn retain_visible_selection(&mut self) {
        let visible_ids = self
            .visible_entries
            .iter()
            .map(|entry| entry.id)
            .collect::<std::collections::HashSet<_>>();
        self.selected_entry_ids
            .retain(|id| visible_ids.contains(id));
        if self
            .selection_anchor_id
            .is_some_and(|id| !visible_ids.contains(&id))
        {
            self.selection_anchor_id = None;
        }
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (storage, settings) =
            ClipboardService::initialize().expect("Failed to initialize clipboard service");
        let history = ClipboardHistory::from_entries(settings.history_limit, Vec::new());
        #[cfg(windows)]
        platform::single_instance::configure_global_hotkey(&settings.global_show_shortcut);
        let theme_mode = if matches!(settings.theme, crate::model::AppTheme::Dark) {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(theme_mode, Some(window), cx);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索剪贴板历史..."));
        let initial_focus = cx.focus_handle();
        let window_handle = window.window_handle();
        initial_focus.focus(window, cx);
        let subscriptions = vec![cx.subscribe_in(&search, window, {
            let search = search.clone();
            move |this, _, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.query = search.read(cx).value().to_string();
                        this.refresh_visible_entries();
                        this.preload_file_icons(cx);
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => {
                        let Some(entry) = this.visible_entries.first().cloned() else {
                            return;
                        };
                        let id = entry.id;
                        this.selected_entry_ids.clear();
                        this.selection_anchor_id = None;
                        this.navigation_entry_id = None;
                        let quick_paste = this.settings.quick_paste;
                        let window_handle = window.window_handle();
                        this.copy_entry(id, quick_paste, Some(window_handle), cx);
                    }
                    _ => {}
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
            history_loading: true,
            query: String::new(),
            filter: ClipboardFilter::All,
            page: AppPage::History,
            status: String::new(),
            monitor_paused: false,
            always_on_top: false,
            editing_global_shortcut: false,
            update_check: UpdateCheckState::Idle,
            selected_entry_ids: std::collections::HashSet::new(),
            selection_anchor_id: None,
            navigation_entry_id: None,
            expanded_image_id: None,
            expanded_image_scroll_offset: None,
            expanded_text_id: None,
            expanded_text_scroll_offset: None,
            expanded_text_scroll: ScrollHandle::new(),
            visible_entries: Vec::new(),
            file_icon_paths: std::collections::HashMap::new(),
            file_icon_loading: std::collections::HashSet::new(),
            file_icon_failed: std::collections::HashSet::new(),
            missing_file_entries: std::collections::HashSet::new(),
            search,
            initial_focus,
            window_handle,
            history_scroll: gpui_component::VirtualListScrollHandle::new(),
            _clipboard_listener: clipboard_listener,
            _subscriptions: subscriptions,
        };
        app.load_history(update_rx, cx);
        app
    }

    /// Loads the persisted history off the UI thread, then starts consuming
    /// clipboard updates. Events that arrive while loading wait in the
    /// channel, so no capture races the initial load.
    fn load_history(&mut self, updates: async_channel::Receiver<()>, cx: &mut Context<Self>) {
        let storage = self.storage.clone();
        cx.spawn(async move |entity, cx| {
            let entries = cx
                .background_spawn(async move { ClipboardService::load_history_entries(&storage) })
                .await;
            entity
                .update(cx, |this, cx| {
                    match entries {
                        Ok(entries) => {
                            this.history = ClipboardHistory::from_entries(
                                this.settings.history_limit,
                                entries,
                            );
                        }
                        Err(error) => {
                            let message = error.to_localized_string(this.settings.language);
                            this.status = message.clone();
                            this.show_error("历史加载失败", message, cx);
                        }
                    }
                    this.history_loading = false;
                    this.refresh_visible_entries();
                    this.preload_file_icons(cx);
                    this.start_clipboard_monitor(updates, cx);
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    /// Registers the bundled palette as proper theme configs, so
    /// `Theme::change` applies colors, legacy tokens, and the Base-layer
    /// projection (scrollbars, text selection) in one step.
    ///
    /// In the configs, `danger.active.background` mirrors the title bar and
    /// `danger.foreground` the foreground: the native close event hides the
    /// window before GPUI receives the mouse-up event, and this keeps the
    /// close button's pressed state visually neutral when the window is
    /// restored.
    fn install_themes(cx: &mut App) {
        let (light, dark) = {
            let registry = ThemeRegistry::global_mut(cx);
            registry
                .load_themes_from_str(include_str!("../assets/themes/ucp.json"))
                .expect("bundled theme must parse");
            (
                registry.themes().get("UCP Light").cloned(),
                registry.themes().get("UCP Dark").cloned(),
            )
        };
        let theme = Theme::global_mut(cx);
        if let Some(light) = light {
            theme.light_theme = light;
        }
        if let Some(dark) = dark {
            theme.dark_theme = dark;
        }
    }

    fn apply_theme(theme: crate::model::AppTheme, cx: &mut App) {
        let mode = if matches!(theme, crate::model::AppTheme::Dark) {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(mode, None, cx);
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
        let promote_copied_entries = self.settings.promote_copied_entries;
        if self.monitor_paused
            || !self
                .history
                .would_push_change_with_promotion(&content, promote_copied_entries)
        {
            return;
        }
        let result = self
            .history
            .push_with_promotion(content, promote_copied_entries);
        let entry = result.entry;
        let result_id = entry.as_ref().map(|entry| entry.id).unwrap_or_default();
        let removed_ids = result.removed_ids;
        self.update_visible_entry(result_id);
        self.remove_visible_entries(&removed_ids);
        let storage = self.storage.clone();
        cx.spawn(async move |entity, cx| {
            let saved_preview = cx
                .background_spawn(async move {
                    let saved_preview = entry
                        .as_ref()
                        .map(|e| ClipboardService::save_entry(&storage, e))
                        .transpose()
                        .ok()
                        .flatten()
                        .flatten();
                    if !removed_ids.is_empty() {
                        let _ = ClipboardService::delete_stored_entries(&storage, &removed_ids);
                    }
                    saved_preview
                })
                .await;
            if let Some(preview_url) = saved_preview {
                entity
                    .update(cx, |this, cx| {
                        if this.history.set_image_preview_url(result_id, preview_url) {
                            this.update_visible_entry(result_id);
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
                        Err(error) => {
                            let message = error.to_string();
                            this.show_error("检查更新失败", message.clone(), cx);
                            UpdateCheckState::Failed(message)
                        }
                    };
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn save_settings(&mut self, cx: &mut Context<Self>) {
        self.settings = self.settings.clone().normalized();
        if let Err(error) = ClipboardService::save_settings(&self.storage, &self.settings) {
            let message = error.to_string();
            self.status = message.clone();
            self.show_error("设置保存失败", message, cx);
        } else {
            self.status = "设置已保存".into();
        }
    }

    fn show_error(
        &self,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let notification = Notification::error(message).title(title);
        self.window_handle
            .update(cx, move |_, window, cx| {
                // Errors raised while the window is hidden in the tray or in
                // the background would go unseen as in-app toasts, so those
                // also reach the OS notification center.
                let notification = if window.is_window_active() {
                    notification
                } else {
                    notification.delivery(NotificationDelivery::InAppAndSystem)
                };
                window.push_notification(notification, cx);
            })
            .ok();
    }

    #[cfg(windows)]
    fn set_always_on_top(window: &Window, always_on_top: bool) -> bool {
        platform::windows::set_always_on_top(window, always_on_top)
    }

    #[cfg(not(windows))]
    fn set_always_on_top(_: &Window, _: bool) -> bool {
        false
    }
}

impl Render for ClipboardApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        let counts = self.history.counts();
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        v_flex()
            .track_focus(&self.initial_focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                if !this.handle_global_shortcut_key_down(event, cx) {
                    this.handle_history_key_down(event, window, cx);
                }
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
                        self.render_history(counts, window, cx).into_any_element()
                    } else {
                        self.render_settings(cx).into_any_element()
                    }),
            )
            .child(
                StatusBar::new()
                    .left(format!("{} 条记录", counts.total))
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
                                    .accessibility_label(if self.always_on_top {
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
                                    .accessibility_label(if page == AppPage::History {
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
                                        .accessibility_label(confirm_text)
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
            .children(notification_layer)
    }
}
