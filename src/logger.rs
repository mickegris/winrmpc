//! In-app log capture.  A tracing Layer that appends records to a static
//! ring-buffer so the Log view can display them without a terminal.

use std::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

const MAX_ENTRIES: usize = 500;

/// Commands slower than this are worth flagging — see `MpdClient::cmd`,
/// which both logs quiet/high-frequency commands when they cross this
/// threshold and tags the entry so the Log view can highlight it.
pub const SLOW_COMMAND_MS: u64 = 2000;

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    /// Set from a `duration_ms` tracing field, when the event carries one
    /// (currently only `MpdClient::cmd`). `None` for ordinary log lines.
    pub duration_ms: Option<u64>,
}

impl LogEntry {
    pub fn is_slow(&self) -> bool {
        self.duration_ms.is_some_and(|d| d >= SLOW_COMMAND_MS)
    }
}

static LOG_ENTRIES: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());

pub fn get_entries() -> Vec<LogEntry> {
    LOG_ENTRIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn clear_entries() {
    if let Ok(mut v) = LOG_ENTRIES.lock() {
        v.clear();
    }
}

// ── tracing Layer ────────────────────────────────────────────────────────────

pub struct InAppLayer;

#[derive(Default)]
struct MessageVisitor {
    message: String,
    duration_ms: Option<u64>,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "duration_ms" {
            self.duration_ms = Some(value);
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for InAppLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let now = chrono::Local::now()
            .format("%H:%M:%S%.3f")
            .to_string();

        let entry = LogEntry {
            timestamp: now,
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            message: visitor.message,
            duration_ms: visitor.duration_ms,
        };

        if let Ok(mut entries) = LOG_ENTRIES.lock() {
            entries.push(entry);
            if entries.len() > MAX_ENTRIES {
                let overflow = entries.len() - MAX_ENTRIES;
                entries.drain(0..overflow);
            }
        }
    }
}
