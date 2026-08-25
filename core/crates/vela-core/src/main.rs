use std::{
    collections::BTreeMap,
    env,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use tracing::{
    field::{Field, Visit},
    span::{Attributes, Id, Record},
    Event, Metadata, Subscriber,
};

fn socket_path() -> Result<PathBuf, String> {
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--socket" {
            return args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| "--socket requires a path".to_owned());
        }
    }
    Err("usage: vela-core --socket <path>".to_owned())
}

#[tokio::main]
async fn main() {
    let _ = tracing::subscriber::set_global_default(JsonSubscriber::default());
    let socket_path = match socket_path() {
        Ok(path) => path,
        Err(message) => {
            tracing::error!(component = "core", %message, "invalid arguments");
            std::process::exit(2);
        }
    };

    tracing::info!(
        component = "core",
        process_version = env!("CARGO_PKG_VERSION"),
        "vela-core starting"
    );
    if let Err(error) = assistant_ipc::serve(&socket_path).await {
        tracing::error!(component = "core", %error, "IPC server failed");
        std::process::exit(1);
    }
}

#[derive(Default)]
struct JsonSubscriber {
    next_span_id: AtomicU64,
}

impl Subscriber for JsonSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        *_metadata.level() <= tracing::Level::INFO
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(self.next_span_id.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        let metadata = event.metadata();
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let mut output = json!({
            "timestamp_ms": timestamp_ms,
            "level": metadata.level().to_string(),
            "target": metadata.target(),
            "process_version": env!("CARGO_PKG_VERSION"),
        });
        if let Some(object) = output.as_object_mut() {
            object.extend(visitor.fields);
        }
        eprintln!("{output}");
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

#[derive(Default)]
struct JsonVisitor {
    fields: BTreeMap<String, Value>,
}

impl Visit for JsonVisitor {
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name().to_owned(), json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_owned(), json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_owned(), json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields.insert(field.name().to_owned(), json!(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.fields
            .insert(field.name().to_owned(), json!(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_owned(), json!(format!("{value:?}")));
    }
}
