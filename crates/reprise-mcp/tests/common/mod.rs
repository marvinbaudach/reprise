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
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

/// Generous per-response timeout — long enough to absorb a full 5 s SQLite
/// `busy_timeout`, short enough that a genuine hang fails the test.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);

/// The MCP protocol revision the tests negotiate (the current stable one, the
/// SDK default). Kept as a fixture so an SDK bump that changes the default is
/// caught here.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// Opens an independent raw connection to an already-migrated fixture.
///
/// Product code must use Core facades; integration tests use this only for
/// schema/setup assertions that deliberately sit outside the product API.
pub fn fixture_connection(path: &Path) -> Connection {
    let connection = Connection::open(path).expect("open independent fixture connection");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("enable fixture foreign keys");
    connection
        .pragma_update(
            None,
            "busy_timeout",
            reprise_core::db::DEFAULT_BUSY_TIMEOUT_MS,
        )
        .expect("configure fixture busy timeout");
    connection
}

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
    let db = reprise_core::db::Db::open_migrated(Some(path)).expect("open+migrate fixture db");
    drop(db);
    let conn = fixture_connection(path);
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
    let db = reprise_core::db::Db::open_migrated(Some(path)).expect("open fixture db");
    reprise_core::library::settings::set_bool(&db, key, value).expect("set setting");
}

// --- AI job (instrumental) fixtures & in-process worker ---------------------
//
// Package H2 tests drive a real worker in-process against the same temp DB the
// MCP server writes to, exactly as the plan intends (the core facades allow
// claiming/completing a job without the CLI). These helpers seed a promotable
// source track (a real FLAC on disk so the fake backend and the promotion
// tagger/scanner have a valid file), render every queued job into staging, and
// promote a staged render.

/// The settings key granting the `ai:create` capability.
pub const CAP_AI_CREATE: &str = "agent.capability.ai:create";

/// A fixed injected clock for the worker/promotion helpers — lease and
/// timestamp math is deterministic and needs no wall clock.
const WORKER_NOW: i64 = 1_700_000_000;

/// The in-process worker's claim token and lease — deterministic, no wall clock.
const WORKER: i64 = 42_042;
const LEASE_SECS: i64 = 300;

/// Copies the bundled `sine.flac` into `dir` as `<title>.flac` and seeds a
/// track row pointing at that real file, with the metadata the promotion path
/// reads. Returns `(track_id, flac_path)`.
pub fn seed_real_flac_track(
    db_path: &Path,
    dir: &Path,
    title: &str,
    artist: &str,
) -> (i64, PathBuf) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let flac = dir.join(format!("{title}.flac"));
    std::fs::copy(&source, &flac).expect("copy sine.flac fixture");
    let db = reprise_core::db::Db::open_migrated(Some(db_path)).expect("open fixture db");
    drop(db);
    let conn = fixture_connection(db_path);
    conn.execute(
        "INSERT INTO tracks \
           (path, title, artist, album, album_artist, year, track_no, genre, added_at) \
         VALUES (?1, ?2, ?3, ?4, ?3, 2020, 1, 'Test', 0)",
        params![
            flac.to_string_lossy(),
            title,
            artist,
            format!("{artist} Album")
        ],
    )
    .expect("insert real-flac track");
    (conn.last_insert_rowid(), flac)
}

/// Renders one claimed job into staging with the deterministic fake backend,
/// posting progress — the shared body of the in-process worker helpers.
fn render_claimed_job(
    db: &reprise_core::db::Db,
    staging: &reprise_core::ai_staging::StagingStore,
    backend: &reprise_core::stem_separation::FakeStemBackend,
    claimed: &reprise_core::ai_jobs::ClaimedJob,
) {
    use reprise_core::stem_separation::StemSeparationBackend;
    let source_id = claimed.source_track_id.expect("job has a source track");
    let source = reprise_core::queries::track_source_path(db, source_id)
        .expect("resolve source path")
        .expect("source track exists");
    let output = staging.path_for_job(claimed.id);
    backend
        .separate_instrumental(
            &source,
            &output,
            &mut |permille| {
                let _ = reprise_core::ai_jobs::set_progress(db, claimed.id, WORKER, permille);
            },
            &|| false,
        )
        .expect("render instrumental");
}

/// Claims and renders every queued job into `staging_dir` with the deterministic
/// fake backend, marking each `done` (staged, unsaved) via `mark_done` — the
/// in-process worker for tests that then drive the save decision themselves.
pub fn run_worker_until_idle(db_path: &Path, staging_dir: &Path) {
    let db = reprise_core::db::Db::open_migrated(Some(db_path)).expect("open fixture db");
    let staging = reprise_core::ai_staging::StagingStore::new(staging_dir);
    staging.ensure_dir().expect("ensure staging dir");
    let backend = reprise_core::stem_separation::FakeStemBackend::new();

    while let Some(claimed) = reprise_core::ai_jobs::claim_next(&db, WORKER, WORKER_NOW, LEASE_SECS)
        .expect("claim next job")
    {
        render_claimed_job(&db, &staging, &backend, &claimed);
        reprise_core::ai_jobs::mark_done(&db, claimed.id, WORKER, WORKER_NOW).expect("mark done");
    }
}

/// Like [`run_worker_until_idle`] but completes each job through
/// `ai_promotion::complete_render`, exactly as the real CLI worker does: a job
/// carrying the auto-promote intent (`save=true`) is promoted into `library_root`
/// on completion with no manual save step, while a no-intent job is left staged.
pub fn run_worker_completing(db_path: &Path, staging_dir: &Path, library_root: &Path) {
    let db = reprise_core::db::Db::open_migrated(Some(db_path)).expect("open fixture db");
    let staging = reprise_core::ai_staging::StagingStore::new(staging_dir);
    staging.ensure_dir().expect("ensure staging dir");
    let config = reprise_core::ai_promotion::PromotionConfig::new(library_root);
    let backend = reprise_core::stem_separation::FakeStemBackend::new();

    while let Some(claimed) = reprise_core::ai_jobs::claim_next(&db, WORKER, WORKER_NOW, LEASE_SECS)
        .expect("claim next job")
    {
        render_claimed_job(&db, &staging, &backend, &claimed);
        reprise_core::ai_promotion::complete_render(
            &db, &staging, &config, claimed.id, WORKER, WORKER_NOW,
        )
        .expect("complete render");
    }
}

/// Promotes a finished, staged render into `library_root`, returning the new
/// library track id.
pub fn promote_job(db_path: &Path, staging_dir: &Path, library_root: &Path, job_id: i64) -> i64 {
    let db = reprise_core::db::Db::open_migrated(Some(db_path)).expect("open fixture db");
    let staging = reprise_core::ai_staging::StagingStore::new(staging_dir);
    let config = reprise_core::ai_promotion::PromotionConfig::new(library_root);
    reprise_core::ai_promotion::promote(&db, &staging, &config, job_id, WORKER_NOW)
        .expect("promote staged render")
        .result_track_id
}

/// The number of rows in `ai_jobs`.
pub fn count_ai_jobs(db_path: &Path) -> i64 {
    let db = reprise_core::db::Db::open_migrated(Some(db_path)).expect("open fixture db");
    drop(db);
    fixture_connection(db_path)
        .query_row("SELECT COUNT(*) FROM ai_jobs", [], |row| row.get(0))
        .expect("count ai_jobs")
}

/// Reads a job's `(status, result_track_id)` directly via the core facade.
pub fn job_state(db_path: &Path, job_id: i64) -> (String, Option<i64>) {
    let db = reprise_core::db::Db::open_migrated(Some(db_path)).expect("open fixture db");
    let job = reprise_core::ai_jobs::get_job(&db, job_id)
        .expect("get job")
        .expect("job exists");
    (job.state.as_str().to_string(), job.result_track_id)
}

/// Whether `dbus-run-session` can be spawned to give a test a private,
/// player-less D-Bus session bus. Mirrors `reprise-cli`'s own check
/// (`tests/playback.rs`).
pub fn private_bus_available() -> bool {
    Command::new("dbus-run-session")
        .arg("--help")
        .output()
        .is_ok()
}

/// A private D-Bus **session** bus that outlives multiple processes, so one
/// test can register a stub player on it AND point a spawned `reprise-mcp` at
/// the same bus. `start_under_private_bus` (via `dbus-run-session`) gives the
/// MCP process its own throwaway bus but no way to share it; here we run
/// `dbus-daemon` ourselves, capture the address it prints, and hand it to both
/// sides. The daemon is killed on drop.
pub struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    /// Spawns `dbus-daemon --session --nofork --print-address` and reads the one
    /// address line it prints. The read blocks until that line arrives, so no
    /// sleep/poll is needed. Returns `None` when `dbus-daemon` cannot be spawned
    /// or prints no address — the caller then skips, documenting itself as
    /// environment-limited rather than faking a bus (mirrors the sibling
    /// `dbus-run-session` tests).
    pub fn start() -> Option<Self> {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdout = child.stdout.take()?;
        let mut address = String::new();
        let read = BufReader::new(stdout).read_line(&mut address);
        if read.is_err() || address.trim().is_empty() {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        Some(Self {
            child,
            address: address.trim().to_owned(),
        })
    }

    /// The `unix:path=…,guid=…` address a connection or spawned process uses to
    /// join this bus.
    pub fn address(&self) -> &str {
        &self.address
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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

    /// Spawns the server with path-valued test boundary overrides.
    pub fn start_with_env(db_path: &Path, env: &[(&str, &Path)]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_reprise-mcp"));
        for (key, value) in env {
            command.env(key, value);
        }
        let mut client = Self::spawn_command(command, db_path);
        client.handshake();
        client
    }

    /// Spawns the server without performing the handshake (for handshake tests).
    pub fn spawn(db_path: &Path) -> Self {
        Self::spawn_command(Command::new(env!("CARGO_BIN_EXE_reprise-mcp")), db_path)
    }

    /// Spawns the server against `db_path` under `dbus-run-session` — a
    /// private, deterministic, player-less D-Bus session bus — and completes
    /// the handshake. Returns `None` when `dbus-run-session` is unavailable in
    /// this environment (the caller should then skip, documenting itself as
    /// environment-limited rather than faking a bus). Mirrors `reprise-cli`'s
    /// own `dbus-run-session` pattern (`tests/playback.rs`): playback tools
    /// that reach the real bus need a guaranteed-empty one so "no running
    /// Reprise app" is the deterministic outcome, independent of whatever the
    /// ambient session bus happens to hold.
    pub fn start_under_private_bus(db_path: &Path) -> Option<Self> {
        if !private_bus_available() {
            return None;
        }
        let mut command = Command::new("dbus-run-session");
        command.arg("--").arg(env!("CARGO_BIN_EXE_reprise-mcp"));
        let mut client = Self::spawn_command(command, db_path);
        client.handshake();
        Some(client)
    }

    /// Spawns the server against `db_path` on the given private session bus
    /// (via `DBUS_SESSION_BUS_ADDRESS`) and completes the handshake — so a stub
    /// player registered on the same [`PrivateBus`] receives the MCP's real
    /// D-Bus calls. Unlike [`Self::start_under_private_bus`] (a throwaway,
    /// player-less bus), this shares an already-running bus with the test.
    pub fn start_on_bus(db_path: &Path, bus: &PrivateBus) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_reprise-mcp"));
        command.env("DBUS_SESSION_BUS_ADDRESS", bus.address());
        let mut client = Self::spawn_command(command, db_path);
        client.handshake();
        client
    }

    fn spawn_command(mut command: Command, db_path: &Path) -> Self {
        let mut child = command
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

/// Asserts a `tools/call` response is a success (not `isError`) and returns its
/// concatenated text content — the plain-text counterpart to [`structured_ok`]
/// for tools whose success payload is a human-readable summary (the playback
/// tools).
pub fn tool_success_text(response: &Value) -> String {
    let result = response
        .get("result")
        .unwrap_or_else(|| panic!("expected result, got: {response}"));
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    assert!(!is_error, "expected success, got tool error: {response}");
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
