mod common;

use common::{code, stderr, Harness};

/// The exact wording the CLI prints for a too-new schema (English per AGENTS.md).
const SCHEMA_TOO_NEW_MESSAGE: &str =
    "Database schema is newer than this reprise-cli — please update.";

/// Bumps the migrated temp database's `user_version` past what this binary
/// supports, so `open_migrated` fails the fail-closed guard.
fn make_schema_too_new(h: &Harness) {
    h.conn()
        .pragma_update(None, "user_version", 999_i64)
        .expect("bump user_version");
}

#[test]
fn schema_too_new_exits_5_with_the_exact_message() {
    let h = Harness::new();
    make_schema_too_new(&h);
    let out = h.run(&["library", "summary"]);
    assert_eq!(code(&out), 5);
    assert!(
        stderr(&out).contains(SCHEMA_TOO_NEW_MESSAGE),
        "stderr was: {:?}",
        stderr(&out)
    );
}

#[test]
fn schema_guard_blocks_before_any_command_runs() {
    // The guard sits at the open boundary, so it fires for every subcommand,
    // read or write, before the command can touch anything.
    let h = Harness::new();
    make_schema_too_new(&h);
    for args in [
        vec!["playlist", "list"],
        vec!["search", "x"],
        vec!["playlist", "create", "Nope"],
    ] {
        let out = h.run(&args);
        assert_eq!(code(&out), 5, "args {args:?} should hit the schema guard");
    }
}
