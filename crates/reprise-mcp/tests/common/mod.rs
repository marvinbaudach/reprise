//! Shared harness for the `reprise-mcp` integration tests.
//!
//! Each test spawns the **real** `reprise-mcp` binary (via
//! `CARGO_BIN_EXE_reprise-mcp`) against a throwaway temp database and speaks
//! newline-delimited JSON-RPC over its stdio — the same wire an agent client
//! uses. A background thread drains stdout into a channel so every read can
//! time out (no test can hang), and stderr is collected so tests can prove
//! logging never touches stdout.

#![allow(dead_code)]
// Test-harness ergonomics: request builders take owned `serde_json::Value`
// payloads (callers pass `json!({...})` literals); the values are serialized
// into the outgoing frame, so by-value is the natural shape here.
#![allow(clippy::needless_pass_by_value)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rusqlite::params;
use serde_json::{json, Value};

/// Generous per-response timeout — long enough to absorb a full 5 s SQLite
/// `busy_timeout`, short enough that a genuine hang fails the test.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);

/// The MCP protocol revision the tests negotiate (the current stable one, the
/// SDK default). Kept as a fixture so an SDK bump that changes the default is
/// caught here.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// A track to seed into a fixture database.
pub struct SeedTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: Option<i32>,
    pub duration_ms: i64,
    pub rating: i32,
}

impl SeedTrack {
    /// A minimal track with sensible defaults; `path` is derived from `title`.
    pub fn simple(title: &str, artist: &str) -> Self {
        Self {
            path: format!("/music/{artist}/{title}.flac"),
            title: title.to_string(),
            artist: artist.to_string(),
            album: format!("{artist} Album"),
            genre: "Test".to_string(),
            year: Some(2020),
            duration_ms: 180_000,
            rating: 0,
        }
    }
}

/// Creates and migrates a fresh database at `path`, inserts `tracks`, and
/// returns the assigned row ids in insertion order. Test-fixture SQL is
/// explicitly permitted by `scripts/check-architecture.sh`.
pub fn seed_tracks(path: &Path, tracks: &[SeedTrack]) -> Vec<i64> {
    let conn = reprise_core::db::open_migrated(Some(path)).expect("open+migrate fixture db");
    let mut ids = Vec::with_capacity(tracks.len());
    for track in tracks {
        conn.execute(
            "INSERT INTO tracks \
             (path, title, artist, album, genre, year, duration_ms, rating, added_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                track.path,
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.year,
                track.duration_ms,
                track.rating,
            ],
        )
        .expect("insert fixture track");
        ids.push(conn.last_insert_rowid());
    }
    ids
}

/// Sets a boolean setting (e.g. a capability key) on the fixture database via
/// the core facade.
pub fn set_bool_setting(path: &Path, key: &str, value: bool) {
    let conn = reprise_core::db::open_migrated(Some(path)).expect("open fixture db");
    reprise_core::library::settings::set_bool(&conn, key, value).expect("set setting");
}

/// A live client speaking JSON-RPC to a spawned `reprise-mcp` process.
pub struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_rx: Receiver<String>,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_buf: Arc<Mutex<String>>,
    stderr_reader: Option<JoinHandle<()>>,
    received: Vec<String>,
    next_id: i64,
}

impl McpClient {
    /// Spawns the server against `db_path` and completes the MCP handshake.
    pub fn start(db_path: &Path) -> Self {
        let mut client = Self::spawn(db_path);
        client.handshake();
        client
    }

    /// Spawns the server without performing the handshake (for handshake tests).
    pub fn spawn(db_path: &Path) -> Self {
        let exe = env!("CARGO_BIN_EXE_reprise-mcp");
        let mut child = Command::new(exe)
            .arg("--db")
            .arg(db_path)
            .env("REPRISE_LOG", "info")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn reprise-mcp");

        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        let stderr = child.stderr.take().expect("child stderr");

        let (tx, stdout_rx) = mpsc::channel();
        let stdout_reader = std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        let stderr_buf = Arc::new(Mutex::new(String::new()));
        let stderr_clone = Arc::clone(&stderr_buf);
        let stderr_reader = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let mut buf = stderr_clone.lock().expect("stderr lock");
                buf.push_str(&line);
                buf.push('\n');
            }
        });

        Self {
            child,
            stdin: Some(stdin),
            stdout_rx,
            stdout_reader: Some(stdout_reader),
            stderr_buf,
            stderr_reader: Some(stderr_reader),
            received: Vec::new(),
            next_id: 1,
        }
    }

    /// Runs `initialize` + `notifications/initialized`, returning the
    /// `initialize` result object.
    pub fn handshake(&mut self) -> Value {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "reprise-mcp-tests", "version": "0.0.0" }
            }),
        );
        let result = response
            .get("result")
            .cloned()
            .unwrap_or_else(|| panic!("initialize failed: {response}"));
        self.notify("notifications/initialized", json!({}));
        result
    }

    /// Sends a request and returns the full response object (with `result` or
    /// `error`).
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.send(&request);
        self.await_response(id)
    }

    /// Sends a notification (no id, no response expected).
    pub fn notify(&mut self, method: &str, params: Value) {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.send(&notification);
    }

    /// Convenience: `tools/call` for `name` with `arguments`.
    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
    }

    /// Convenience: `resources/read` for `uri`.
    pub fn read_resource(&mut self, uri: &str) -> Value {
        self.request("resources/read", json!({ "uri": uri }))
    }

    fn send(&mut self, message: &Value) {
        let line = serde_json::to_string(message).expect("serialize request");
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(line.as_bytes()).expect("write stdin");
        stdin.write_all(b"\n").expect("write newline");
        stdin.flush().expect("flush stdin");
    }

    fn await_response(&mut self, id: i64) -> Value {
        loop {
            let line = self.recv_line();
            let value: Value = serde_json::from_str(&line)
                .unwrap_or_else(|error| panic!("non-JSON line on stdout ({error}): {line:?}"));
            if value.get("id").and_then(Value::as_i64) == Some(id) {
                return value;
            }
            // A response for another id or a server notification: keep reading.
        }
    }

    fn recv_line(&mut self) -> String {
        match self.stdout_rx.recv_timeout(RESPONSE_TIMEOUT) {
            Ok(line) => {
                self.received.push(line.clone());
                line
            }
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for a response line"),
            Err(RecvTimeoutError::Disconnected) => {
                panic!("server closed stdout before responding")
            }
        }
    }

    /// Every stdout line the client has read. Combined with the JSON assertion
    /// in [`Self::await_response`], this backs the stdout-purity tests.
    pub fn stdout_lines(&self) -> &[String] {
        &self.received
    }

    /// Closes stdin, drains remaining stdout, waits for exit, and returns the
    /// final state (all stdout lines, full stderr, exit code).
    pub fn shutdown(mut self) -> Finished {
        // Dropping stdin signals EOF so the server's `waiting()` returns.
        self.stdin.take();
        while let Ok(line) = self.stdout_rx.recv_timeout(RESPONSE_TIMEOUT) {
            self.received.push(line);
        }
        if let Some(handle) = self.stdout_reader.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_reader.take() {
            let _ = handle.join();
        }
        let code = self.child.wait().ok().and_then(|status| status.code());
        Finished {
            stdout_lines: std::mem::take(&mut self.received),
            stderr: self.stderr_buf.lock().expect("stderr lock").clone(),
            code,
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Best-effort cleanup so a panicking test never leaves a child behind.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Final state of a shut-down server.
pub struct Finished {
    pub stdout_lines: Vec<String>,
    pub stderr: String,
    pub code: Option<i32>,
}

/// Asserts that a JSON string contains none of the D19-forbidden leak markers.
/// Call on the raw serialized bytes of any response.
pub fn assert_no_leaks(haystack: &str) {
    for needle in [
        "/music/",
        "/home/",
        ".flac",
        ".db",
        ".local/share",
        "XDG_",
        "lyrics",
        "password",
        "token",
        "credential",
        "serial",
    ] {
        assert!(
            !haystack.contains(needle),
            "response leaked forbidden marker {needle:?}: {haystack}"
        );
    }
}

/// Extracts the `structuredContent` object from a successful `tools/call`
/// response, asserting the call did not error.
pub fn structured_ok(response: &Value) -> Value {
    let result = response
        .get("result")
        .unwrap_or_else(|| panic!("expected result, got: {response}"));
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    assert!(!is_error, "expected success, got tool error: {response}");
    result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| panic!("expected structuredContent: {response}"))
}

/// Asserts a `tools/call` response is a caller-visible tool error and returns
/// its concatenated text content.
pub fn tool_error_text(response: &Value) -> String {
    let result = response
        .get("result")
        .unwrap_or_else(|| panic!("expected result, got: {response}"));
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    assert!(is_error, "expected a tool error, got: {response}");
    result
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}
