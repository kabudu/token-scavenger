/// A `tracing_subscriber` Layer that forwards formatted log events
/// into a `tokio::sync::broadcast::Sender<String>` so the UI SSE
/// stream can surface them to the browser in real-time.
use tokio::sync::broadcast;
use tracing::Event;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

pub struct BroadcastLayer {
    tx: broadcast::Sender<String>,
}

impl BroadcastLayer {
    pub fn new(tx: broadcast::Sender<String>) -> Self {
        Self { tx }
    }
}

impl<S: tracing::Subscriber> Layer<S> for BroadcastLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Format a compact, human-readable line from the event.
        let meta = event.metadata();
        let level = meta.level().as_str();
        let target = meta.target();

        // Collect fields into a string via a simple visitor.
        let mut fields = FieldCollector::default();
        event.record(&mut fields);

        let line = format!("[{}] {}: {}", level, target, fields.message);
        // Drop silently if no receivers are connected yet.
        let _ = self.tx.send(line);
    }
}

#[derive(Default)]
struct FieldCollector {
    message: String,
    extras: Vec<String>,
}

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
            // Strip outer quotes added by Debug for plain strings.
            if self.message.starts_with('"') && self.message.ends_with('"') {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        } else {
            self.extras.push(format!("{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.extras.push(format!("{}={}", field.name(), value));
        }
    }
}
