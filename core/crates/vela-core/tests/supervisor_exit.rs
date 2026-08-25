use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread::sleep,
    time::{Duration, Instant},
};

/// Starts Core from a shell that exits immediately, so Core is orphaned the way a
/// crashed Vela app orphans it, and returns the orphan's process ID.
fn spawn_orphaned_core(socket_path: &Path, extra_arguments: &[&str]) -> u32 {
    // The shell waits for the socket before exiting so Core observes it as parent,
    // then leaves, reproducing the orphan a crashed app leaves behind.
    let script = r#"binary="$1"; socket="$2"; shift 2
"$binary" --socket "$socket" "$@" >/dev/null 2>&1 &
core_pid=$!
attempt=0
while [ ! -S "$socket" ] && [ "$attempt" -lt 100 ]; do
    sleep 0.05
    attempt=$((attempt + 1))
done
printf '%s' "$core_pid""#;
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(env!("CARGO_BIN_EXE_vela-core"))
        .arg(socket_path);
    for argument in extra_arguments {
        command.arg(argument);
    }
    let output = command.output().expect("shell launch failed");
    assert!(
        output.status.success(),
        "shell exited with {}",
        output.status
    );
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("shell did not report a process ID")
}

fn is_running(process_id: u32) -> bool {
    Command::new("kill")
        .args(["-0", &process_id.to_string()])
        .output()
        .expect("kill probe failed")
        .status
        .success()
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    condition()
}

fn socket_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("vela-core-{name}-{}.sock", std::process::id()))
}

fn stop(process_id: u32) {
    let _ = Command::new("kill").arg(process_id.to_string()).output();
}

#[test]
fn orphaned_core_exits_with_exit_with_parent() {
    let socket_path = socket_path("orphan-exits");
    let _ = fs::remove_file(&socket_path);
    let process_id = spawn_orphaned_core(&socket_path, &["--exit-with-parent"]);

    assert!(
        wait_until(Duration::from_secs(5), || socket_path.exists()),
        "Core never created its socket"
    );
    let exited = wait_until(Duration::from_secs(5), || !is_running(process_id));
    if !exited {
        stop(process_id);
    }
    let _ = fs::remove_file(&socket_path);
    assert!(exited, "orphaned Core kept running after its parent exited");
}

#[test]
fn orphaned_core_survives_without_the_flag() {
    let socket_path = socket_path("orphan-survives");
    let _ = fs::remove_file(&socket_path);
    let process_id = spawn_orphaned_core(&socket_path, &[]);

    assert!(
        wait_until(Duration::from_secs(5), || socket_path.exists()),
        "Core never created its socket"
    );
    let exited = wait_until(Duration::from_secs(2), || !is_running(process_id));
    stop(process_id);
    let _ = fs::remove_file(&socket_path);
    assert!(!exited, "Core must only self-terminate when asked to");
}

#[test]
fn unknown_arguments_fail_visibly() {
    let output = Command::new(env!("CARGO_BIN_EXE_vela-core"))
        .args(["--socket", "/tmp/vela-core-unused.sock", "--nope"])
        .output()
        .expect("Core launch failed");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown argument --nope"),
        "missing diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
