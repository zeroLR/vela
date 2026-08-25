use std::{
    collections::BTreeMap,
    env,
    future::pending,
    os::unix::process::parent_id,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use tracing::{
    field::{Field, Visit},
    span::{Attributes, Id, Record},
    Event, Metadata, Subscriber,
};

const USAGE: &str = "usage: vela-core --socket <path> [--exit-with-parent]";

/// How often an orphaned process notices that its supervisor is gone.
const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Orphans are reparented to `launchd`/`init`.
const INIT_PROCESS_ID: u32 = 1;

struct Arguments {
    socket_path: PathBuf,
    exit_with_parent: bool,
}

fn arguments() -> Result<Arguments, String> {
    let mut socket_path = None;
    let mut exit_with_parent = false;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--socket" => {
                socket_path = Some(
                    args.next()
                        .map(PathBuf::from)
                        .ok_or_else(|| "--socket requires a path".to_owned())?,
                );
            }
            "--exit-with-parent" => exit_with_parent = true,
            unknown => return Err(format!("unknown argument {unknown}. {USAGE}")),
        }
    }
    Ok(Arguments {
        socket_path: socket_path.ok_or_else(|| USAGE.to_owned())?,
        exit_with_parent,
    })
}

/// A supervisor that dies without terminating Core leaves an orphan holding the
/// socket. The orphan is reparented, so a changed parent process ID is the exit
/// signal. A parent of `init` also counts: a supervisor that died before Core was
/// scheduled leaves this process already reparented, and `--exit-with-parent`
/// states that Core was not launched by `launchd` directly.
///
/// The socket file is deliberately left in place. The next Core removes a stale
/// socket when it binds, so deleting it here could remove a successor's live socket.
async fn wait_for_supervisor_exit(supervisor_process_id: u32) {
    loop {
        tokio::time::sleep(SUPERVISOR_POLL_INTERVAL).await;
        let current_parent = parent_id();
        if current_parent != supervisor_process_id || current_parent == INIT_PROCESS_ID {
            return;
        }
    }
}

#[tokio::main]
async fn main() {
    let _ = tracing::subscriber::set_global_default(JsonSubscriber::default());
    let supervisor_process_id = parent_id();
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(message) => {
            tracing::error!(component = "core", %message, "invalid arguments");
            std::process::exit(2);
        }
    };

    tracing::info!(
        component = "core",
        process_version = env!("CARGO_PKG_VERSION"),
        exit_with_parent = arguments.exit_with_parent,
        "vela-core starting"
    );

    let supervisor_exit = async {
        if arguments.exit_with_parent {
            wait_for_supervisor_exit(supervisor_process_id).await;
        } else {
            pending::<()>().await;
        }
    };

    tokio::select! {
        result = assistant_ipc::serve(&arguments.socket_path) => {
            if let Err(error) = result {
                tracing::error!(component = "core", %error, "IPC server failed");
                std::process::exit(1);
            }
        }
        () = supervisor_exit => {
            tracing::info!(
                component = "core",
                supervisor_process_id,
                "supervising process exited, shutting down"
            );
        }
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
