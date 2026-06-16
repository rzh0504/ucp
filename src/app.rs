use crate::components::{HistoryList, TopBar};
use crate::model::{ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::storage;
use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedReceiver;
use futures_timer::Delay;
use std::time::Duration;

const STYLES: Asset = asset!("/assets/app.css");
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(650);
const HISTORY_LIMIT: usize = 200;

#[component]
pub fn App() -> Element {
    let mut history = use_signal(|| {
        storage::load_history(HISTORY_LIMIT)
            .unwrap_or_else(|_| ClipboardHistory::new(HISTORY_LIMIT))
    });
    let query = use_signal(String::new);
    let active_filter = use_signal(|| ClipboardFilter::All);
    let mut status = use_signal(|| "启动剪贴板监听...".to_string());

    let _watcher = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        let mut last_sequence = None;

        loop {
            let sequence = platform::clipboard::sequence_number();
            if sequence.is_some() && sequence == last_sequence {
                Delay::new(CLIPBOARD_POLL_INTERVAL).await;
                continue;
            }

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

            if sequence.is_some() {
                last_sequence = sequence;
            }

            Delay::new(CLIPBOARD_POLL_INTERVAL).await;
        }
    });

    let snapshot = history.read().filtered(query().as_str(), active_filter());
    let counts = history.read().counts();
    let selected_count = snapshot.len();

    rsx! {
        document::Link { rel: "stylesheet", href: STYLES }
        main { class: "shell",
            TopBar { query }
            section { class: "content-panel",
                HistoryList { entries: snapshot, history, selected_count, active_filter, counts }
            }
        }
    }
}
