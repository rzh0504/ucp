use crate::model::{AppSettings, ClipboardContent, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::services::persistence::StorageWriter;
use crate::services::{ClipboardService, ClipboardStorage};
use crate::updater::{self, UpdateCheck, UpdateInfo};
use gpui_kit::assets::Assets;
use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme as _, FocusableExt as _, Icon, IconName, Sizable as _, Theme, ThemeMode,
    ThemeRegistry, TitleBar, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    input::{InputEvent, InputState},
    notification::{Notification, NotificationDelivery},
    status_bar::StatusBar,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
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

fn init_ui(cx: &mut App) {
    // Required on Windows before system notifications can be posted.
    cx.set_app_identity("dev.ucp.clipboard", "UCP");
    // Register the per-window presentation plugins before opening any Root.
    gpui_kit::init(cx);
    ClipboardApp::install_themes(cx);
}

pub fn run(visible: bool) {
    let app = gpui_kit::application().with_assets(AppAssets(Assets));
    app.run(move |cx| {
        init_ui(cx);
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
                .update(|cx| {
                    // Kit supplies the Base Root, window chrome, and all overlays.
                    gpui_kit::open_window(options, cx, |window, cx| {
                        window.set_window_title("UCP");
                        cx.new(|cx| ClipboardApp::new(window, cx))
                    })
                })
                .expect("Failed to open GPUI window")
                .0;
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

/// Installed atomically only after initialization, history loading and pruning
/// succeed. Every later write uses this session's single FIFO worker.
struct StorageSession {
    storage: ClipboardStorage,
    writer: StorageWriter,
}

fn clipboard_update_channel() -> (impl FnMut() + Send + 'static, async_channel::Receiver<()>) {
    let (sender, receiver) = async_channel::bounded(1);
    (
        move || {
            let _ = sender.try_send(());
        },
        receiver,
    )
}

struct ClipboardApp {
    storage_session: Option<StorageSession>,
    persistence_error: Option<String>,
    settings: AppSettings,
    history: ClipboardHistory,
    history_loading: bool,
    startup_error: Option<String>,
    monitor_error: Option<String>,
    monitor_starting: bool,
    query: String,
    search_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    search_pending: bool,
    filter: ClipboardFilter,
    page: AppPage,
    status: String,
    monitor_paused: bool,
    always_on_top: bool,
    editing_global_shortcut: bool,
    update_check: UpdateCheckState,
    selected_entry_ids: std::collections::HashSet<u64>,
    pending_delete_ids: std::collections::HashSet<u64>,
    selection_anchor_id: Option<u64>,
    navigation_entry_id: Option<u64>,
    expanded_image_id: Option<u64>,
    expanded_image_scroll_offset: Option<Point<Pixels>>,
    expanded_text_id: Option<u64>,
    expanded_text_scroll_offset: Option<Point<Pixels>>,
    expanded_text_scroll: ScrollHandle,
    visible_entries: Vec<std::sync::Arc<crate::model::ClipboardEntry>>,
    history_render_cache: history::HistoryRenderCache,
    file_icon_paths: std::collections::HashMap<u64, std::path::PathBuf>,
    file_icon_loading: std::collections::HashSet<u64>,
    file_icon_failed: std::collections::HashSet<u64>,
    missing_file_entries: std::collections::HashSet<u64>,
    search: Entity<InputState>,
    initial_focus: FocusHandle,
    window_handle: AnyWindowHandle,
    history_scroll: gpui_kit::component::VirtualListScrollHandle,
    _clipboard_listener: Option<platform::clipboard::ClipboardUpdateListener>,
    _subscriptions: Vec<Subscription>,
}

impl ClipboardApp {
    fn refresh_visible_entries(&mut self, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        let generation = self.search_generation.fetch_add(1, Ordering::Relaxed) + 1;
        if !self.query.trim().is_empty() {
            let snapshot = self.history.snapshot();
            let query = self.query.clone();
            let filter = self.filter;
            let current_generation = self.search_generation.clone();
            self.search_pending = true;
            // Never let Enter copy a result belonging to the previous query.
            self.visible_entries.clear();
            self.history_render_cache
                .invalidate_rows(&self.history.snapshot());
            cx.spawn(async move |entity, cx| {
                let entries = cx
                    .background_spawn(async move {
                        ClipboardHistory::filter_snapshot_while(snapshot, &query, filter, || {
                            current_generation.load(Ordering::Relaxed) == generation
                        })
                    })
                    .await;
                entity
                    .update(cx, |this, cx| {
                        if this.search_generation.load(Ordering::Relaxed) != generation {
                            return;
                        }
                        if let Some(entries) = entries {
                            this.visible_entries = entries;
                            this.search_pending = false;
                            this.history_render_cache
                                .invalidate_rows(&this.history.snapshot());
                            this.retain_visible_selection();
                            this.preload_file_icons(cx);
                            cx.notify();
                        }
                    })
                    .ok();
            })
            .detach();
            return;
        }
        self.search_pending = false;
        self.visible_entries = self.history.filtered("", self.filter);
        self.history_render_cache
            .invalidate_rows(&self.history.snapshot());
        self.retain_visible_selection();
    }

    fn update_visible_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.query.trim().is_empty() {
            self.refresh_visible_entries(cx);
            return;
        }
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
        self.history_render_cache
            .invalidate_rows(&self.history.snapshot());
        self.retain_visible_selection();
    }

    fn remove_visible_entries(&mut self, ids: &[u64], cx: &mut Context<Self>) {
        if !self.query.trim().is_empty() {
            self.refresh_visible_entries(cx);
            return;
        }
        self.visible_entries
            .retain(|entry| !ids.contains(&entry.id));
        self.history_render_cache
            .invalidate_rows(&self.history.snapshot());
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
        // No storage, migration, listener setup or blocking work before returning
        // the initial loading view (including when starting hidden in the tray).
        let settings = AppSettings::default();
        let history = ClipboardHistory::from_entries(settings.history_limit, Vec::new());
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
            move |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.query = search.read(cx).value().to_string();
                    this.refresh_visible_entries(cx);
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
        })];
        let mut app = Self {
            storage_session: None,
            persistence_error: None,
            settings,
            history,
            history_loading: false,
            startup_error: None,
            monitor_error: None,
            monitor_starting: false,
            query: String::new(),
            search_generation: Default::default(),
            search_pending: false,
            filter: ClipboardFilter::All,
            page: AppPage::History,
            status: String::new(),
            monitor_paused: true,
            always_on_top: false,
            editing_global_shortcut: false,
            update_check: UpdateCheckState::Idle,
            selected_entry_ids: std::collections::HashSet::new(),
            pending_delete_ids: std::collections::HashSet::new(),
            selection_anchor_id: None,
            navigation_entry_id: None,
            expanded_image_id: None,
            expanded_image_scroll_offset: None,
            expanded_text_id: None,
            expanded_text_scroll_offset: None,
            expanded_text_scroll: ScrollHandle::new(),
            visible_entries: Vec::new(),
            history_render_cache: Default::default(),
            file_icon_paths: std::collections::HashMap::new(),
            file_icon_loading: std::collections::HashSet::new(),
            file_icon_failed: std::collections::HashSet::new(),
            missing_file_entries: std::collections::HashSet::new(),
            search,
            initial_focus,
            window_handle,
            history_scroll: gpui_kit::component::VirtualListScrollHandle::new(),
            _clipboard_listener: None,
            _subscriptions: subscriptions,
        };
        app.retry_startup(cx);
        app
    }

    /// Opens/migrates SQLite, loads settings and history, and prunes entirely
    /// off the GPUI thread. A failed startup never installs an empty allocator.
    fn retry_startup(&mut self, cx: &mut Context<Self>) {
        if self.history_loading || self.storage_session.is_some() {
            return;
        }
        self.history_loading = true;
        self.startup_error = None;
        self.monitor_paused = true;
        self.status = "正在加载剪贴板历史...".into();
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let loaded = cx
                .background_spawn(async {
                    ClipboardService::initialize().and_then(|(storage, settings)| {
                        let entries = ClipboardService::load_history_entries(
                            &storage,
                            settings.history_limit,
                        )?;
                        let writer = StorageWriter::new(storage.clone());
                        let history =
                            ClipboardHistory::from_entries(settings.history_limit, entries);
                        Ok((StorageSession { storage, writer }, settings, history))
                    })
                })
                .await;
            entity
                .update(cx, |this, cx| {
                    this.history_loading = false;
                    let (session, settings, history) = match loaded {
                        Ok(loaded) => loaded,
                        Err(error) => {
                            let message = error.to_localized_string(this.settings.language);
                            this.status = message.clone();
                            this.startup_error = Some(message.clone());
                            this.show_error("历史加载失败", message, cx);
                            cx.notify();
                            return;
                        }
                    };
                    // Settings navigation stays disabled until this atomic install,
                    // so persisted settings cannot overwrite user changes.
                    this.settings = settings;
                    this.history = history;
                    this.storage_session = Some(session);
                    Self::apply_theme(this.settings.theme, cx);
                    #[cfg(windows)]
                    platform::single_instance::configure_global_hotkey(
                        &this.settings.global_show_shortcut,
                    );
                    this.status.clear();
                    this.refresh_visible_entries(cx);
                    this.preload_file_icons(cx);
                    this.retry_clipboard_monitor(cx);
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    /// Listener construction may wait for a platform thread or process; it must
    /// not run on GPUI. A single pending event coalesces clipboard bursts while
    /// a read is in flight without ever blocking the platform listener.
    fn retry_clipboard_monitor(&mut self, cx: &mut Context<Self>) {
        if self.storage_session.is_none()
            || self.monitor_starting
            || self._clipboard_listener.is_some()
        {
            return;
        }
        self.monitor_starting = true;
        self.monitor_error = None;
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let (mut on_update, update_rx) = clipboard_update_channel();
            // Capture the current state after loading/retry, even if the last
            // clipboard change happened before the native listener was ready.
            on_update();
            let listener = cx
                .background_spawn(async move { platform::clipboard::listen_for_updates(on_update) })
                .await;
            entity
                .update(cx, |this, cx| {
                    this.monitor_starting = false;
                    match listener {
                        Ok(listener) => {
                            this._clipboard_listener = Some(listener);
                            this.monitor_paused = false;
                            this.status.clear();
                            this.start_clipboard_monitor(update_rx, cx);
                        }
                        Err(error) => {
                            let message = error.to_string();
                            this.monitor_paused = true;
                            this.monitor_error = Some(message.clone());
                            this.status = message.clone();
                            this.show_error("剪贴板监听启动失败", message, cx);
                        }
                    }
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
        let Some(session) = self.storage_session.as_ref() else {
            return;
        };
        let writer = session.writer.clone();
        let promote_copied_entries = self.settings.promote_copied_entries;
        if self.monitor_paused
            || !self.history.would_push_change_excluding(
                &content,
                promote_copied_entries,
                &self.pending_delete_ids,
            )
        {
            return;
        }
        let result =
            self.history
                .push_excluding(content, promote_copied_entries, &self.pending_delete_ids);
        let entry = result.entry;
        let result_id = entry.as_ref().map(|entry| entry.id).unwrap_or_default();
        let removed_ids = result.removed_ids;
        self.update_visible_entry(result_id, cx);
        self.remove_visible_entries(&removed_ids, cx);
        if let Some(entry) = entry {
            // Submission happens on the UI thread, before awaiting the reply.
            // The worker therefore observes the same order as user actions.
            let reply = writer.save(entry, removed_ids);
            self.persist(
                reply,
                move |this, preview, cx| {
                    if let Some(preview_url) = preview
                        && this.history.set_image_preview_url(result_id, preview_url)
                    {
                        this.update_visible_entry(result_id, cx);
                        cx.notify();
                    }
                },
                cx,
            );
        }
        cx.notify();
    }

    fn persist<T: Send + 'static>(
        &self,
        reply: async_channel::Receiver<Result<T, crate::storage::StorageError>>,
        on_success: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |entity, cx| {
            let result = reply
                .recv()
                .await
                .unwrap_or(Err(crate::storage::StorageError::WorkerUnavailable));
            entity
                .update(cx, |this, cx| match result {
                    Ok(value) => on_success(this, value, cx),
                    Err(error) => this.report_persistence_error(error.to_string(), cx),
                })
                .ok();
        })
        .detach();
    }

    fn report_persistence_error(&mut self, message: String, cx: &mut Context<Self>) {
        // An unrelated later success cannot restore a previously failed write.
        // Keep the unsaved indicator and avoid a toast storm during disk failure.
        if self.persistence_error.is_none() {
            self.show_error("存储写入失败", message.clone(), cx);
        }
        self.persistence_error = Some(message.clone());
        self.status = message;
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
        let Some(session) = self.storage_session.as_ref() else {
            return;
        };
        self.settings = self.settings.clone().normalized();
        let reply = session.writer.save_settings(self.settings.clone());
        self.persist(
            reply,
            |this, (), cx| {
                this.status = "设置已保存".into();
                cx.notify();
            },
            cx,
        );
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
        v_flex()
            .track_focus(&self.initial_focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                if this.storage_session.is_none() {
                    return;
                }
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
                    .left(if self.persistence_error.is_some() {
                        format!("{} 条记录 · 存储写入失败，部分改动未保存", counts.total)
                    } else if self.monitor_error.is_some() {
                        format!("{} 条记录 · 剪贴板监听失败", counts.total)
                    } else if self.monitor_starting {
                        format!("{} 条记录 · 正在启动监听...", counts.total)
                    } else if self.monitor_paused {
                        format!("{} 条记录 · 监听已暂停", counts.total)
                    } else {
                        format!("{} 条记录", counts.total)
                    })
                    .right(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .when(self.monitor_error.is_some(), |this| {
                                this.child(Button::new("retry-monitor").label("重试监听").on_click(
                                    cx.listener(|this, _, _, cx| this.retry_clipboard_monitor(cx)),
                                ))
                            })
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
                                    .disabled(self.storage_session.is_none())
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
                                        if this.storage_session.is_none() {
                                            return;
                                        }
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
                                        "此操作将永久删除当前历史，且无法撤销。执行期间的新记录会保留。",
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
                                        .disabled(self.storage_session.is_none())
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
    }
}

#[cfg(test)]
mod startup_tests {
    use super::clipboard_update_channel;
    use crate::platform;

    #[test]
    fn clipboard_events_coalesce_without_blocking_and_rearm_after_read() {
        let (mut on_update, updates) = clipboard_update_channel();
        for _ in 0..10_000 {
            on_update();
        }
        assert_eq!(updates.len(), 1);
        assert_eq!(updates.try_recv(), Ok(()));
        assert!(updates.try_recv().is_err());

        on_update();
        assert_eq!(updates.try_recv(), Ok(()));
        drop(updates);
        // Listener callbacks may race shutdown; a closed receiver is harmless.
        on_update();
    }

    #[test]
    fn clipboard_listener_can_be_initialized_on_background_executor() {
        fn assert_send<T: Send>() {}
        assert_send::<platform::clipboard::ClipboardUpdateListener>();
    }
}
