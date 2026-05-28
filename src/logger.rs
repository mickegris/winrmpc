//! In-app log capture.  A tracing Layer that appends records to a static
//! ring-buffer so the Log view can display them without a terminal.

use std::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

const MAX_ENTRIES: usize = 500;

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
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

struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for InAppLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        let now = chrono::Local::now()
            .format("%H:%M:%S%.3f")
            .to_string();

        let entry = LogEntry {
            timestamp: now,
            level: event.metadata().level().to_string(),
            target: event.metadata().target().to_string(),
            message: visitor.0,
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
