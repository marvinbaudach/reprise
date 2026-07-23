//! MCP handshake: the server negotiates the protocol, advertises exactly the
//! tools + resources capabilities, and identifies itself.

mod common;

use common::{McpClient, SeedTrack, PROTOCOL_VERSION};
use serde_json::Value;
use tempfile::TempDir;

fn fixture_db() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("reprise.db");
    common::seed_tracks(&path, &[SeedTrack::simple("Song", "Artist")]);
    (dir, path)
}

#[test]
fn initialize_negotiates_protocol_and_capabilities() {
    let (_dir, path) = fixture_db();
    let mut client = McpClient::spawn(&path);

    let result = client.handshake();

    assert_eq!(
        result.get("protocolVersion").and_then(Value::as_str),
        Some(PROTOCOL_VERSION),
        "server should negotiate the stable protocol revision"
    );
    let capabilities = result.get("capabilities").expect("capabilities");
    assert!(
        capabilities.get("tools").is_some(),
        "tools capability must be advertised"
    );
    assert!(
        capabilities.get("resources").is_some(),
        "resources capability must be advertised"
    );
}

#[test]
fn initialize_reports_server_identity_and_instructions() {
    let (_dir, path) = fixture_db();
    let mut client = McpClient::spawn(&path);

    let result = client.handshake();

    assert_eq!(
        result
            .get("serverInfo")
            .and_then(|info| info.get("name"))
            .and_then(Value::as_str),
        Some("reprise-mcp"),
    );
    assert!(
        result
            .get("instructions")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty()),
        "server should send usage instructions"
    );
}

#[test]
fn server_shuts_down_cleanly_on_stdin_eof() {
    let (_dir, path) = fixture_db();
    let client = McpClient::start(&path);
    let finished = client.shutdown();
    assert_eq!(finished.code, Some(0), "clean EOF should exit 0");
}
