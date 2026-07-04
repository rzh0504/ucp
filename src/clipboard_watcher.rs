use crate::model::{AppLanguage, AppSettings, ClipboardContent, ClipboardEntry, ClipboardHistory};
use crate::platform;
use crate::platform::clipboard::ClipboardError;
use crate::storage;
use chrono::{DateTime, Duration as ChronoDuration, Local};
use dioxus::prelude::*;
use futures_channel::oneshot;
use futures_timer::Delay;
use std::sync::{OnceLock, mpsc as std_mpsc};
use std::thread;
use std::time::Duration;

const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(650);
type ClipboardReadReply = oneshot::Sender<Result<Option<ClipboardContent>, ClipboardError>>;

struct PersistCaptureJob {
    entry: Option<ClipboardEntry>,
    removed_ids: Vec<u64>,
    auto_cleanup_cutoff: Option<DateTime<Local>>,
    language: AppLanguage,
    reply: oneshot::Sender<Result<(), String>>,
}

pub(crate) async fn watch_clipboard(
    history: Signal<ClipboardHistory>,
    settings: Signal<AppSettings>,
    mut status: Signal<String>,
    ignored_clipboard_write: Signal<Option<ClipboardContent>>,
    clipboard_monitor_paused: Signal<bool>,
) {
    use futures_util::StreamExt;

    let (updates_tx, mut updates_rx) = futures_channel::mpsc::unbounded();
    let listener = platform::clipboard::listen_for_updates(move || {
        let _ = updates_tx.unbounded_send(());
    });

    let _listener = match listener {
        Ok(listener) => listener,
        Err(error) => {
            status.set(match settings.peek().language {
                AppLanguage::Chinese => format!("剪贴板事件监听失败，已切换轮询：{error}"),
                AppLanguage::English => {
                    format!("Clipboard event listener failed; switched to polling: {error}")
                }
            });
            poll_clipboard(
                history,
                settings,
                status,
                ignored_clipboard_write,
                clipboard_monitor_paused,
            )
            .await;
            return;
        }
    };

    capture_clipboard(
        history,
        settings,
        status,
        ignored_clipboard_write,
        clipboard_monitor_paused,
    )
    .await;
    while updates_rx.next().await.is_some() {
        capture_clipboard(
            history,
            settings,
            status,
            ignored_clipboard_write,
            clipboard_monitor_paused,
        )
        .await;
    }
}

async fn poll_clipboard(
    history: Signal<ClipboardHistory>,
    settings: Signal<AppSettings>,
    status: Signal<String>,
    ignored_clipboard_write: Signal<Option<ClipboardContent>>,
    clipboard_monitor_paused: Signal<bool>,
) {
    let mut last_sequence = None;

    loop {
        let sequence = platform::clipboard::sequence_number();
        if sequence.is_some() && sequence == last_sequence {
            Delay::new(CLIPBOARD_POLL_INTERVAL).await;
            continue;
        }

        capture_clipboard(
            history,
            settings,
            status,
            ignored_clipboard_write,
            clipboard_monitor_paused,
        )
        .await;

        if sequence.is_some() {
            last_sequence = sequence;
        }

        Delay::new(CLIPBOARD_POLL_INTERVAL).await;
    }
}

async fn capture_clipboard(
    mut history: Signal<ClipboardHistory>,
    settings: Signal<AppSettings>,
    mut status: Signal<String>,
    mut ignored_clipboard_write: Signal<Option<ClipboardContent>>,
    clipboard_monitor_paused: Signal<bool>,
) {
    if *clipboard_monitor_paused.peek() {
        return;
    }

    let language = settings.peek().language;
    match read_clipboard_content(language).await {
        Ok(Some(content)) => {
            let ignored_content = { ignored_clipboard_write.peek().clone() };
            if let Some(ignored_content) = ignored_content {
                ignored_clipboard_write.set(None);
                if ignored_content == content {
                    return;
                }
            }

            if !history.peek().would_push_change(&content) {
                return;
            }

            let result = history.write().push(content);
            let mut auto_cleanup_cutoff = None;

            if result.changed
                && let Some(days) = settings.peek().auto_cleanup_days
            {
                auto_cleanup_cutoff = Some(Local::now() - ChronoDuration::days(i64::from(days)));
                history.write().remove_older_than_days(days);
            }

            if result.entry.is_none()
                && result.removed_ids.is_empty()
                && auto_cleanup_cutoff.is_none()
            {
                return;
            }

            if let Err(message) = persist_capture_result(
                result.entry,
                result.removed_ids,
                auto_cleanup_cutoff,
                language,
            )
            .await
            {
                status.set(message);
            }
        }
        Ok(None) => {}
        Err(error) => status.set(match language {
            AppLanguage::Chinese => format!("剪贴板暂不可用：{error}"),
            AppLanguage::English => format!("Clipboard is temporarily unavailable: {error}"),
        }),
    }
}

async fn read_clipboard_content(
    language: AppLanguage,
) -> Result<Option<ClipboardContent>, ClipboardError> {
    let (reply, receiver) = oneshot::channel();
    if clipboard_read_worker().send(reply).is_err() {
        return Err(ClipboardError::Unavailable(read_task_interrupted(language)));
    }

    receiver
        .await
        .unwrap_or_else(|_| Err(ClipboardError::Unavailable(read_task_interrupted(language))))
}

async fn persist_capture_result(
    entry: Option<ClipboardEntry>,
    removed_ids: Vec<u64>,
    auto_cleanup_cutoff: Option<DateTime<Local>>,
    language: AppLanguage,
) -> Result<(), String> {
    let (reply, receiver) = oneshot::channel();
    let job = PersistCaptureJob {
        entry,
        removed_ids,
        auto_cleanup_cutoff,
        language,
        reply,
    };

    if storage_persist_worker().send(job).is_err() {
        return Err(save_task_interrupted(language));
    }

    receiver
        .await
        .unwrap_or_else(|_| Err(save_task_interrupted(language)))
}

fn clipboard_read_worker() -> &'static std_mpsc::Sender<ClipboardReadReply> {
    static CLIPBOARD_READ_WORKER: OnceLock<std_mpsc::Sender<ClipboardReadReply>> = OnceLock::new();
    CLIPBOARD_READ_WORKER.get_or_init(|| {
        let (sender, receiver) = std_mpsc::channel::<ClipboardReadReply>();
        thread::spawn(move || {
            while let Ok(reply) = receiver.recv() {
                let _ = reply.send(platform::clipboard::read_content());
            }
        });
        sender
    })
}

fn storage_persist_worker() -> &'static std_mpsc::Sender<PersistCaptureJob> {
    static STORAGE_PERSIST_WORKER: OnceLock<std_mpsc::Sender<PersistCaptureJob>> = OnceLock::new();
    STORAGE_PERSIST_WORKER.get_or_init(|| {
        let (sender, receiver) = std_mpsc::channel::<PersistCaptureJob>();
        thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                let result = persist_capture_result_blocking(
                    job.entry,
                    job.removed_ids,
                    job.auto_cleanup_cutoff,
                    job.language,
                );
                let _ = job.reply.send(result);
            }
        });
        sender
    })
}

fn persist_capture_result_blocking(
    entry: Option<ClipboardEntry>,
    removed_ids: Vec<u64>,
    auto_cleanup_cutoff: Option<DateTime<Local>>,
    language: AppLanguage,
) -> Result<(), String> {
    if let Some(entry) = &entry
        && let Err(error) = storage::save_entry(entry)
    {
        return Err(match language {
            AppLanguage::Chinese => format!("历史保存失败：{error}"),
            AppLanguage::English => format!("Failed to save history: {error}"),
        });
    }

    if let Err(error) = storage::delete_entries(&removed_ids) {
        return Err(match language {
            AppLanguage::Chinese => format!("历史清理失败：{error}"),
            AppLanguage::English => format!("Failed to clean history: {error}"),
        });
    }

    if let Some(cutoff) = auto_cleanup_cutoff
        && let Err(error) = storage::delete_entries_older_than(cutoff)
    {
        return Err(match language {
            AppLanguage::Chinese => format!("自动清理历史失败：{error}"),
            AppLanguage::English => format!("Failed to auto-clean history: {error}"),
        });
    }

    Ok(())
}

fn read_task_interrupted(language: AppLanguage) -> String {
    match language {
        AppLanguage::Chinese => "剪贴板读取任务已中断".to_string(),
        AppLanguage::English => "Clipboard read task was interrupted".to_string(),
    }
}

fn save_task_interrupted(language: AppLanguage) -> String {
    match language {
        AppLanguage::Chinese => "历史保存任务已中断".to_string(),
        AppLanguage::English => "History save task was interrupted".to_string(),
    }
}

pub(crate) fn prune_history_by_age(
    mut history: Signal<ClipboardHistory>,
    days: u16,
) -> Result<usize, storage::StorageError> {
    let cutoff = Local::now() - ChronoDuration::days(i64::from(days));
    storage::delete_entries_older_than(cutoff)?;
    Ok(history.write().remove_older_than_days(days))
}
