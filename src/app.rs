use crate::components::{FilterTabs, HistoryList, TopBar};
use crate::model::{ClipboardFilter, ClipboardHistory};
use crate::platform;
use dioxus::prelude::*;
use futures_channel::mpsc::UnboundedReceiver;
use futures_timer::Delay;
use std::time::Duration;

const STYLES: Asset = asset!("/assets/app.css");
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(650);
const HISTORY_LIMIT: usize = 200;

#[component]
pub fn App() -> Element {
    let mut history = use_signal(|| ClipboardHistory::new(HISTORY_LIMIT));
    let query = use_signal(String::new);
    let active_filter = use_signal(|| ClipboardFilter::All);
    let mut status = use_signal(|| "启动剪贴板监听...".to_string());

    let _watcher = use_coroutine(move |_rx: UnboundedReceiver<()>| async move {
        loop {
            match platform::clipboard::read_text() {
                Ok(Some(text)) => {
                    if history.write().push_text(text) {
                        status.set("已捕获新的文本剪贴板内容".to_string());
                    } else {
                        status.set("正在监听文本剪贴板".to_string());
                    }
                }
                Ok(None) => status.set("正在监听剪贴板，当前无文本内容".to_string()),
                Err(error) => status.set(format!("剪贴板暂不可用：{error}")),
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
            TopBar { query, status: status() }
            section { class: "content-area",
                FilterTabs { active_filter, counts }
                HistoryList { entries: snapshot, history, selected_count }
            }
        }
    }
}
