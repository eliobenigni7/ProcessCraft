//! Bounded handoff from an authenticated local transport to the GUI owner.
//! This module does not create a listener or authenticate a peer.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};

pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentRequest {
    pub session_id: String,
    pub request_id: String,
    pub expected_generation: u64,
    pub method: String,
    pub params: Value,
}

pub struct PendingRequest {
    pub request: AttachmentRequest,
    reply: SyncSender<Value>,
}
impl PendingRequest {
    pub fn reply(self, result: Value) -> bool {
        self.reply.send(result).is_ok()
    }
}

#[derive(Clone)]
pub struct QueueSender {
    session_id: String,
    live: Arc<AtomicBool>,
    sender: SyncSender<PendingRequest>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}
pub struct OwnerQueue {
    live: Arc<AtomicBool>,
    receiver: Receiver<PendingRequest>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum QueueError {
    Revoked,
    SessionMismatch,
    Full,
}

pub fn owner_queue(session_id: String, capacity: usize) -> (QueueSender, OwnerQueue) {
    let live = Arc::new(AtomicBool::new(true));
    let (sender, receiver) = mpsc::sync_channel(capacity);
    (
        QueueSender {
            session_id,
            live: live.clone(),
            sender,
            wake: None,
        },
        OwnerQueue { live, receiver },
    )
}
impl QueueSender {
    pub fn with_wake(mut self, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.wake = Some(wake);
        self
    }
    pub fn revoke(&self) {
        self.live.store(false, Ordering::Release);
    }
    pub fn submit(&self, request: AttachmentRequest) -> Result<Receiver<Value>, QueueError> {
        if !self.live.load(Ordering::Acquire) {
            return Err(QueueError::Revoked);
        }
        if request.session_id != self.session_id {
            return Err(QueueError::SessionMismatch);
        }
        let (reply, receiver) = mpsc::sync_channel(1);
        match self.sender.try_send(PendingRequest { request, reply }) {
            Ok(()) => {
                if let Some(wake) = &self.wake {
                    wake();
                }
                Ok(receiver)
            }
            Err(TrySendError::Full(_)) => Err(QueueError::Full),
            Err(TrySendError::Disconnected(_)) => Err(QueueError::Revoked),
        }
    }
}
impl OwnerQueue {
    /// Called only by the GUI owner. Revocation also discards queued work.
    pub fn try_recv(&self) -> Option<PendingRequest> {
        if !self.live.load(Ordering::Acquire) {
            return None;
        }
        self.receiver.try_recv().ok()
    }
    pub fn revoke(&self) {
        self.live.store(false, Ordering::Release);
    }
}
impl Drop for OwnerQueue {
    fn drop(&mut self) {
        self.revoke();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(session: &str) -> AttachmentRequest {
        AttachmentRequest {
            session_id: session.into(),
            request_id: "1".into(),
            expected_generation: 0,
            method: "inspect".into(),
            params: Value::Null,
        }
    }
    #[test]
    fn queue_is_bounded_session_scoped_and_revocable() {
        let (sender, owner) = owner_queue("s".into(), 1);
        assert!(matches!(
            sender.submit(request("wrong")),
            Err(QueueError::SessionMismatch)
        ));
        let response = sender.submit(request("s")).unwrap();
        assert!(matches!(sender.submit(request("s")), Err(QueueError::Full)));
        assert!(
            owner
                .try_recv()
                .unwrap()
                .reply(serde_json::json!({"ok":true}))
        );
        assert_eq!(response.recv().unwrap()["ok"], true);
        owner.revoke();
        assert!(matches!(
            sender.submit(request("s")),
            Err(QueueError::Revoked)
        ));
    }
}
