//! Helpers for spawning long-running work on the tokio runtime and reporting
//! progress back through SSE events.

use tokio::sync::broadcast;

use crate::events::{ResourceKind, ToastLevel, UiEvent};

pub fn toast_info(tx: &broadcast::Sender<UiEvent>, msg: impl Into<String>) {
    let _ = tx.send(UiEvent::Toast { level: ToastLevel::Info, msg: msg.into() });
}
pub fn toast_success(tx: &broadcast::Sender<UiEvent>, msg: impl Into<String>) {
    let _ = tx.send(UiEvent::Toast { level: ToastLevel::Success, msg: msg.into() });
}
pub fn toast_error(tx: &broadcast::Sender<UiEvent>, msg: impl Into<String>) {
    let _ = tx.send(UiEvent::Toast { level: ToastLevel::Error, msg: msg.into() });
}
pub fn invalidate(tx: &broadcast::Sender<UiEvent>, r: ResourceKind) {
    let _ = tx.send(UiEvent::Invalidate { resource: r });
}

pub fn task_started(tx: &broadcast::Sender<UiEvent>, id: u64, label: impl Into<String>) {
    let _ = tx.send(UiEvent::TaskStarted { id, label: label.into() });
}
pub fn task_progress(tx: &broadcast::Sender<UiEvent>, id: u64, note: impl Into<String>) {
    let _ = tx.send(UiEvent::TaskProgress { id, note: note.into() });
}
pub fn task_finished(tx: &broadcast::Sender<UiEvent>, id: u64, ok: bool, msg: impl Into<String>) {
    let _ = tx.send(UiEvent::TaskFinished { id, ok, msg: msg.into() });
}
