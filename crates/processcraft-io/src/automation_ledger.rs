//! Session-bound request ledger shared by automation transports.
//! A full ledger refuses new IDs; it never forgets an ID and then replays it.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum Admission {
    New,
    Pending,
    Completed(Value),
    Conflict,
    UnknownReconcile,
    SessionMismatch,
}

#[derive(Debug)]
struct Entry {
    hash: [u8; 32],
    result: Result<Option<Value>, ()>,
}

#[derive(Debug)]
pub struct RequestLedger {
    session_id: String,
    capacity: usize,
    result_bytes: usize,
    entries: HashMap<String, Entry>,
}

impl RequestLedger {
    pub fn has_capacity(&self) -> bool {
        self.entries.len() < self.capacity
    }
    pub fn contains_request_id(&self, request_id: &str) -> bool {
        self.entries.contains_key(request_id)
    }
    pub fn new(session_id: String, capacity: usize) -> Self {
        Self {
            session_id,
            capacity: capacity.min(128),
            result_bytes: 0,
            entries: HashMap::new(),
        }
    }

    /// The caller must include all relevant authority and generation in payload.
    pub fn admit(&mut self, session_id: &str, request_id: &str, payload: &[u8]) -> Admission {
        if session_id != self.session_id {
            return Admission::SessionMismatch;
        }
        if request_id.is_empty() || request_id.len() > 256 {
            return Admission::Conflict;
        }
        let hash: [u8; 32] = Sha256::digest(payload).into();
        if let Some(entry) = self.entries.get(request_id) {
            if entry.hash != hash {
                return Admission::Conflict;
            }
            return match &entry.result {
                Ok(Some(value)) => Admission::Completed(value.clone()),
                Ok(None) => Admission::Pending,
                Err(()) => Admission::UnknownReconcile,
            };
        }
        if self.entries.len() >= self.capacity {
            return Admission::UnknownReconcile;
        }
        self.entries.insert(
            request_id.to_owned(),
            Entry {
                hash,
                result: Ok(None),
            },
        );
        Admission::New
    }

    pub fn complete(&mut self, request_id: &str, result: Value) -> bool {
        match self.entries.get_mut(request_id) {
            Some(entry) if matches!(entry.result, Ok(None)) => {
                let size = serde_json::to_vec(&result)
                    .map(|v| v.len())
                    .unwrap_or(usize::MAX);
                if size > 4 * 1024 * 1024 || size > 16 * 1024 * 1024 - self.result_bytes {
                    entry.result = Err(());
                } else {
                    self.result_bytes += size;
                    entry.result = Ok(Some(result));
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_never_replays_or_forgets() {
        let mut ledger = RequestLedger::new("session".into(), 1);
        assert_eq!(ledger.admit("session", "id", b"payload"), Admission::New);
        assert_eq!(
            ledger.admit("session", "id", b"payload"),
            Admission::Pending
        );
        assert_eq!(ledger.admit("session", "id", b"other"), Admission::Conflict);
        assert!(ledger.complete("id", serde_json::json!({"ok": true})));
        assert_eq!(
            ledger.admit("session", "id", b"payload"),
            Admission::Completed(serde_json::json!({"ok": true}))
        );
        assert_eq!(
            ledger.admit("session", "new", b"payload"),
            Admission::UnknownReconcile
        );
        assert_eq!(
            ledger.admit("other", "id", b"payload"),
            Admission::SessionMismatch
        );
    }

    #[test]
    fn oversized_result_leaves_a_reconciliation_tombstone() {
        let mut ledger = RequestLedger::new("session".into(), 1);
        assert_eq!(ledger.admit("session", "id", b"payload"), Admission::New);
        assert!(ledger.complete("id", Value::String("x".repeat(4 * 1024 * 1024))));
        assert_eq!(
            ledger.admit("session", "id", b"payload"),
            Admission::UnknownReconcile
        );
        assert_eq!(
            ledger.admit("session", "id", b"changed"),
            Admission::Conflict
        );
    }
}
