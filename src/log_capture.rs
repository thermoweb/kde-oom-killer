use crate::history::{push_log_line, SharedLogLines};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

struct FieldVisitor {
    message: String,
    fields: Vec<String>,
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            // fmt::Arguments Debug impl delegates to Display, so no extra quotes.
            self.message = format!("{value:?}");
        } else {
            self.fields.push(format!("{}={:?}", field.name(), value));
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.push(format!("{}={:.2}", field.name(), value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.push(format!("{}={}", field.name(), value));
    }
}

fn utc_hms() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

pub struct LogCaptureLayer {
    pub log_lines: SharedLogLines,
}

impl<S: Subscriber> Layer<S> for LogCaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Only capture logs from this crate, not third-party noise.
        if !event.metadata().target().starts_with("rambo") {
            return;
        }
        let mut visitor = FieldVisitor {
            message: String::new(),
            fields: Vec::new(),
        };
        event.record(&mut visitor);

        let level = event.metadata().level().as_str();
        let mut line = format!("[{} {:5}] {}", utc_hms(), level, visitor.message);
        if !visitor.fields.is_empty() {
            line.push(' ');
            line.push_str(&visitor.fields.join(" "));
        }
        push_log_line(&self.log_lines, line);
    }
}
