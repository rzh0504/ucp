use crate::model::{AppLanguage, AppSettings, ClipboardHistory};
use dioxus::prelude::*;

/// Hook to perform startup cleanup of old history entries
pub fn use_startup_cleanup_effect(
    history: Signal<ClipboardHistory>,
    settings: Signal<AppSettings>,
    status: Signal<String>,
    mut startup_cleanup_done: Signal<bool>,
) {
    use_effect(move || {
        if startup_cleanup_done() {
            return;
        }

        startup_cleanup_done.set(true);
        if let Some(days) = settings.peek().auto_cleanup_days {
            let language = settings.peek().language;
            match crate::clipboard_watcher::prune_history_by_age(
                history,
                days,
                settings.peek().preserve_favorites_on_delete,
            ) {
                Ok(removed) if removed > 0 => {
                    let mut status = status;
                    status.set(match language {
                        AppLanguage::Chinese => format!("已自动清理 {removed} 项过期历史"),
                        AppLanguage::English => {
                            format!("Automatically cleaned up {removed} expired history items")
                        }
                    });
                }
                Err(error) => {
                    let mut status = status;
                    status.set(match language {
                        AppLanguage::Chinese => format!("自动清理历史失败：{error}"),
                        AppLanguage::English => format!("Failed to auto-clean history: {error}"),
                    });
                }
                _ => {}
            }
        }
    });
}
