use crate::components::{AppPage, FloatingSettingsButton, HistoryList, SettingsPage, TopBar};
use crate::model::{AppSettings, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::storage;
use dioxus::events::MountedData;
use dioxus::html::Key;
use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedReceiver;
use futures_timer::Delay;
use std::rc::Rc;
use std::time::Duration;

const STYLES: Asset = asset!("/assets/app.css");
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(650);

#[component]
pub fn App() -> Element {
    let settings =
        use_signal(|| storage::load_settings().unwrap_or_else(|_| AppSettings::default()));
    let history = use_signal(|| {
        storage::load_history(settings.peek().history_limit)
            .unwrap_or_else(|_| ClipboardHistory::new(settings.peek().history_limit))
    });
    let mut query = use_signal(String::new);
    let mut active_filter = use_signal(|| ClipboardFilter::All);
    let mut active_page = use_signal(|| AppPage::History);
    let search_input = use_signal(|| None::<Rc<MountedData>>);
    let mut shell = use_signal(|| None::<Rc<MountedData>>);
    let status = use_signal(|| "启动剪贴板监听...".to_string());

    let _watcher = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        watch_clipboard(history, status).await;
    });

    let snapshot = history.read().filtered(query().as_str(), active_filter());
    let counts = history.read().counts();
    let entry_count = snapshot.len();
    let settings_snapshot = settings();

    rsx! {
        document::Link { rel: "stylesheet", href: STYLES }
        main {
            class: "shell",
            tabindex: "-1",
            onmounted: move |event| {
                let element = event.data();
                shell.set(Some(element.clone()));
                spawn(async move {
                    let _ = element.set_focus(true).await;
                });
            },
            onkeydown: move |event| {
                let data = event.data();
                let modifiers = data.modifiers();
                let primary = modifiers.ctrl() || modifiers.meta();

                if !settings.read().keyboard_shortcuts {
                    return;
                }

                if primary && matches!(data.key(), Key::Character(key) if key.eq_ignore_ascii_case("f")) {
                    event.prevent_default();
                    active_page.set(AppPage::History);
                    if let Some(input) = search_input.read().clone() {
                        spawn(async move {
                            let _ = input.set_focus(true).await;
                        });
                    }
                    return;
                }

                if primary && matches!(data.key(), Key::Character(key) if key == ",") {
                    event.prevent_default();
                    active_page.set(if active_page() == AppPage::Settings {
                        AppPage::History
                    } else {
                        AppPage::Settings
                    });
                    return;
                }

                if primary {
                    if let Some(filter) = filter_shortcut(&data.key()) {
                        event.prevent_default();
                        active_page.set(AppPage::History);
                        active_filter.set(filter);
                        return;
                    }
                }

                if data.key() == Key::Escape {
                    if active_page() == AppPage::Settings {
                        event.prevent_default();
                        active_page.set(AppPage::History);
                    } else if !query.read().is_empty() {
                        event.prevent_default();
                        query.set(String::new());
                        if let Some(element) = shell.read().clone() {
                            spawn(async move {
                                let _ = element.set_focus(true).await;
                            });
                        }
                    }
                }
            },
            TopBar {
                query,
                active_page,
                search_input,
                keyboard_shortcuts: settings_snapshot.keyboard_shortcuts,
            }
            section { class: "content-panel",
                if active_page() == AppPage::Settings {
                    SettingsPage {
                        settings,
                        history,
                    }
                } else {
                    HistoryList {
                        entries: snapshot,
                        history,
                        entry_count,
                        active_filter,
                        counts,
                        keyboard_shortcuts: settings_snapshot.keyboard_shortcuts,
                        auto_focus: settings_snapshot.auto_focus_history,
                        promote_on_copy: settings_snapshot.promote_copied_entries,
                        quick_paste: settings_snapshot.quick_paste,
                        show_copy_time: settings_snapshot.show_copy_time,
                        show_text_length: settings_snapshot.show_text_length,
                        status,
                    }
                }
            }
            FloatingSettingsButton { active_page }
        }
    }
}

fn filter_shortcut(key: &Key) -> Option<ClipboardFilter> {
    match key {
        Key::Character(key) if key == "1" => Some(ClipboardFilter::All),
        Key::Character(key) if key == "2" => Some(ClipboardFilter::Text),
        Key::Character(key) if key == "3" => Some(ClipboardFilter::Image),
        Key::Character(key) if key == "4" => Some(ClipboardFilter::File),
        Key::Character(key) if key == "5" => Some(ClipboardFilter::Favorite),
        _ => None,
    }
}

#[cfg(windows)]
async fn watch_clipboard(history: Signal<ClipboardHistory>, mut status: Signal<String>) {
    use futures_util::StreamExt;

    let (updates_tx, mut updates_rx) = futures_channel::mpsc::unbounded();
    let listener = platform::clipboard::listen_for_updates(move || {
        let _ = updates_tx.unbounded_send(());
    });

    let _listener = match listener {
        Ok(listener) => listener,
        Err(error) => {
            status.set(format!("剪贴板事件监听失败，已切换轮询：{error}"));
            poll_clipboard(history, status).await;
            return;
        }
    };

    capture_clipboard(history, status);
    while updates_rx.next().await.is_some() {
        capture_clipboard(history, status);
    }
}

#[cfg(not(windows))]
async fn watch_clipboard(history: Signal<ClipboardHistory>, status: Signal<String>) {
    poll_clipboard(history, status).await;
}

async fn poll_clipboard(history: Signal<ClipboardHistory>, status: Signal<String>) {
    let mut last_sequence = None;

    loop {
        let sequence = platform::clipboard::sequence_number();
        if sequence.is_some() && sequence == last_sequence {
            Delay::new(CLIPBOARD_POLL_INTERVAL).await;
            continue;
        }

        capture_clipboard(history, status);

        if sequence.is_some() {
            last_sequence = sequence;
        }

        Delay::new(CLIPBOARD_POLL_INTERVAL).await;
    }
}

fn capture_clipboard(mut history: Signal<ClipboardHistory>, mut status: Signal<String>) {
    match platform::clipboard::read_content() {
        Ok(Some(content)) => {
            let label = content.kind().label();
            let result = history.write().push(content);
            let mut storage_error = None;

            if let Some(entry) = &result.entry {
                if let Err(error) = storage::save_entry(entry) {
                    storage_error = Some(format!("历史保存失败：{error}"));
                }
            }

            if let Err(error) = storage::delete_entries(&result.removed_ids) {
                storage_error = Some(format!("历史清理失败：{error}"));
            }

            if let Some(message) = storage_error {
                status.set(message);
            } else if result.changed {
                status.set(format!("已捕获新的{label}剪贴板内容"));
            } else {
                status.set("正在监听剪贴板".to_string());
            }
        }
        Ok(None) => status.set("正在监听剪贴板，当前无支持内容".to_string()),
        Err(error) => status.set(format!("剪贴板暂不可用：{error}")),
    }
}
