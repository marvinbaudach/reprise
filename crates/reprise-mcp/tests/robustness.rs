//! Operational robustness: stdout stays protocol-pure, logs go to stderr, the
//! read capability gates reads, a held foreign write transaction does not hang
//! the server, and a too-new schema is refused at startup (fail-closed).

mod common;

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::{set_bool_setting, structured_ok, tool_error_text, McpClient, SeedTrack};
use serde_json::{json, Value};
use tempfile::TempDir;

const CAP_LIBRARY_READ: &str = "agent.capability.library:read";
const CAP_PLAYLIST_CREATE: &str = "agent.capability.playlist:create";

fn seeded_db(dir: &TempDir) -> (std::path::PathBuf, Vec<i64>) {
    let path = dir.path().join("reprise.db");
    let ids = common::seed_tracks(
        &path,
        &[
            SeedTrack::simple("One", "Artist"),
            SeedTrack::simple("Two", "Artist"),
        ],
    );
    (path, ids)
}

#[test]
fn stdout_is_pure_protocol_and_logs_go_to_stderr() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = seeded_db(&dir);
    let mut client = McpClient::start(&path);

    // Exercise several message types.
    client.request("tools/list", json!({}));
    client.request("resources/list", json!({}));
    client.call_tool("music_search_tracks", json!({ "query": "" }));
    client.read_resource("reprise://library/summary");

    let finished = client.shutdown();

    assert!(
        !finished.stdout_lines.is_empty(),
        "server produced no output"
    );
    for line in &finished.stdout_lines {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .unwrap_or_else(|error| panic!("stdout line is not JSON ({error}): {line:?}"));
        assert_eq!(
            value.get("jsonrpc").and_then(Value::as_str),
            Some("2.0"),
            "every stdout frame must be JSON-RPC 2.0: {line}"
        );
    }

    assert!(
        finished.stderr.contains("reprise-mcp"),
        "startup log should appear on stderr: {:?}",
        finished.stderr
    );
    for line in &finished.stdout_lines {
        assert!(
            !line.contains("stdio server ready"),
            "log text must never appear on stdout: {line}"
        );
    }
}

#[test]
fn read_capability_revocation_refuses_reads() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = seeded_db(&dir);
    set_bool_setting(&path, CAP_LIBRARY_READ, false);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_search_tracks", json!({ "query": "" }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("library:read"),
        "read should be refused: {text}"
    );

    // The resource read is refused too (as a protocol error).
    let resource = client.read_resource("reprise://library/summary");
    assert!(
        resource.get("error").is_some(),
        "resource read should error: {resource}"
    );
}

#[test]
fn held_foreign_write_transaction_does_not_hang() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = seeded_db(&dir);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    // Hold an exclusive write transaction on a separate connection so the
    // server's write contends for the lock.
    let hold = rusqlite::Connection::open(&path).unwrap();
    hold.execute_batch("BEGIN IMMEDIATE").unwrap();

    // The call must return (rather than hang) even though the write cannot
    // proceed; the harness read timeout (20 s) would fail the test on a hang.
    let started = std::time::Instant::now();
    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": "Contended", "track_ids": ids.clone() }),
    );
    assert!(
        started.elapsed() < common::RESPONSE_TIMEOUT,
        "the contended write must not hang"
    );
    assert!(
        response.get("result").is_none(),
        "the contended write must not report success: {response}"
    );

    // Release the lock and prove the server recovered: the same write now
    // succeeds, so the earlier failure was contention, not a dead server.
    hold.execute_batch("ROLLBACK").unwrap();
    drop(hold);
    let recovered = client.call_tool(
        "music_create_playlist",
        json!({ "name": "After Release", "track_ids": ids }),
    );
    assert!(
        recovered.get("result").is_some(),
        "server should recover once the lock is released: {recovered}"
    );
}

#[test]
fn schema_newer_than_supported_is_refused_at_startup() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("reprise.db");
    // Create + migrate, then forge a future schema version.
    let conn = reprise_core::db::open_migrated(Some(&path)).unwrap();
    conn.pragma_update(None, "user_version", 9_999_i64).unwrap();
    drop(conn);

    let output = Command::new(env!("CARGO_BIN_EXE_reprise-mcp"))
        .arg("--db")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run reprise-mcp");

    assert_eq!(
        output.status.code(),
        Some(3),
        "schema-too-new should exit 3"
    );
    assert!(
        output.stdout.is_empty(),
        "no protocol output before a refused start"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("newer"),
        "stderr should explain the refusal: {stderr}"
    );
}

#[test]
fn a_large_request_under_the_line_cap_is_served() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = seeded_db(&dir);
    set_bool_setting(&path, CAP_PLAYLIST_CREATE, true);
    let mut client = McpClient::start(&path);

    // A ~3 MiB request line — far larger than any real message, but under the
    // 4 MiB per-line cap — must still be read and answered, proving the guard
    // does not clip a legitimately large (if unusual) frame. A playlist name is
    // used rather than a search query so no SQLite pattern-length limit is hit.
    let big_name = "n".repeat(3 * 1024 * 1024);
    let response = client.call_tool(
        "music_create_playlist",
        json!({ "name": big_name, "track_ids": [] }),
    );
    let structured = structured_ok(&response);
    assert_eq!(
        structured.get("track_count").and_then(Value::as_u64),
        Some(0),
        "the large-but-valid request is served, not clipped or rejected"
    );
}

#[test]
fn oversized_single_line_terminates_the_server_without_unbounded_growth() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = seeded_db(&dir);

    let mut child = Command::new(env!("CARGO_BIN_EXE_reprise-mcp"))
        .arg("--db")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn reprise-mcp");

    // Feed a single line far larger than the 4 MiB cap with no newline until the
    // very end. The capped reader must yield a read error so the server shuts
    // down instead of buffering the whole thing. The write happens on its own
    // thread and tolerates a broken pipe: the server closes its read end the
    // moment the cap trips, well before all 6 MiB are drained.
    let mut stdin = child.stdin.take().expect("child stdin");
    let writer = std::thread::spawn(move || {
        let giant = vec![b'x'; 6 * 1024 * 1024];
        let _ = stdin.write_all(&giant);
        let _ = stdin.write_all(b"\n");
        // Drop stdin -> EOF, regardless of how far the write got.
    });

    // The process must exit promptly (bounded memory), not hang or grow forever.
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = writer.join();
            panic!("server did not exit after an oversized line — it likely buffered unboundedly");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = writer.join();
    // A real exit code (not a signal) shows a graceful shutdown on the read
    // error, not an OOM kill (which would surface as a signal, code() == None).
    assert!(
        status.code().is_some(),
        "server should exit with a code (graceful), not be signal-killed: {status:?}"
    );
}

#[test]
fn bad_argument_exits_with_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_reprise-mcp"))
        .arg("--nonsense")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run reprise-mcp");
    assert_eq!(output.status.code(), Some(2), "bad args should exit 2");
}
