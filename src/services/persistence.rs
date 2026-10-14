//! Ordered, UI-independent persistence. Submission itself is synchronous: work
//! is enqueued before callers spawn a task to await the returned reply.

use crate::model::{AppSettings, ClipboardEntry};
use crate::storage::{self, StorageError, StorageHandle};
use async_channel::Receiver;
use chrono::{DateTime, Local};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;

type Job = Box<dyn FnOnce(&StorageHandle) + Send>;

/// Clones share one FIFO worker. The worker owns only the receiver and storage,
/// so dropping the last writer closes the queue, drains it, and ends the thread.
#[derive(Clone)]
pub struct StorageWriter {
    sender: mpsc::Sender<Job>,
}

impl StorageWriter {
    pub fn new(storage: StorageHandle) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        // A failed spawn drops the receiver. Subsequent submissions return a
        // normal storage error rather than panicking or hanging their callers.
        let _ = std::thread::Builder::new()
            .name("ucp-persistence".into())
            .spawn(move || {
                for job in receiver {
                    job(&storage);
                }
            });
        Self { sender }
    }

    fn submit<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&StorageHandle) -> Result<T, StorageError> + Send + 'static,
    ) -> Receiver<Result<T, StorageError>> {
        let (reply, receiver) = async_channel::bounded(1);
        let failure_reply = reply.clone();
        let job = Box::new(move |storage: &StorageHandle| {
            let result = catch_unwind(AssertUnwindSafe(|| operation(storage)))
                .unwrap_or(Err(StorageError::WorkerUnavailable));
            // A caller may abandon its reply; that must not stop the queue.
            let _ = reply.try_send(result);
        });
        if self.sender.send(job).is_err() {
            let _ = failure_reply.try_send(Err(StorageError::WorkerUnavailable));
        }
        receiver
    }

    pub fn save(
        &self,
        entry: ClipboardEntry,
        removed_ids: Vec<u64>,
    ) -> Receiver<Result<Option<String>, StorageError>> {
        self.submit(move |storage| {
            let preview = storage::save_entry(storage, &entry)?;
            storage::delete_entries(storage, &removed_ids)?;
            Ok(preview)
        })
    }

    pub fn set_favorites(&self, changes: Vec<(u64, bool)>) -> Receiver<Result<(), StorageError>> {
        self.submit(move |storage| storage::set_favorites(storage, &changes))
    }

    pub fn promote(
        &self,
        id: u64,
        captured_at: DateTime<Local>,
    ) -> Receiver<Result<(), StorageError>> {
        self.submit(move |storage| storage::promote_entry(storage, id, captured_at))
    }

    pub fn delete(&self, ids: Vec<u64>) -> Receiver<Result<(), StorageError>> {
        self.submit(move |storage| storage::delete_entries(storage, &ids))
    }

    pub fn save_settings(&self, settings: AppSettings) -> Receiver<Result<(), StorageError>> {
        self.submit(move |storage| storage::save_settings(storage, &settings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnected_worker_returns_storage_error() {
        let (sender, receiver) = mpsc::channel::<Job>();
        drop(receiver);
        let writer = StorageWriter { sender };
        assert!(matches!(
            writer.delete(vec![1]).recv_blocking().unwrap(),
            Err(StorageError::WorkerUnavailable)
        ));
    }
}
