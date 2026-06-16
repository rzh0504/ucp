use crate::components::{AppPage, FloatingSettingsButton, HistoryList, SettingsPage, TopBar};
use crate::model::{AppSettings, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::storage;
use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedReceiver;
use futures_timer::Delay;
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
    let query = use_signal(String::new);
    let active_filter = use_signal(|| ClipboardFilter::All);
    let active_page = use_signal(|| AppPage::History);
    let status = use_signal(|| "启动剪贴板监听...".to_string());

    let _watcher = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        watch_clipboard(history, status).await;
    });

    let snapshot = history.read().filtered(query().as_str(), active_filter());
    let counts = history.read().counts();
    let entry_count = snapshot.len();

    rsx! {
        document::Link { rel: "stylesheet", href: STYLES }
        main { class: "shell",
            TopBar { query, active_page }
            section { class: "content-panel",
                if active_page() == AppPage::Settings {
                    SettingsPage {
                        settings,
                        history,
                    }
                } else {
                    HistoryList { entries: snapshot, history, entry_count, active_filter, counts }
                }
            }
            FloatingSettingsButton { active_page }
        }
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
