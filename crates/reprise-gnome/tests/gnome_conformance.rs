//! Gate wrappers: each test runs the shell gate that enforces its rule.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn rulebook_lib_reports_planned_rules_without_failing() {
    let root = repo_root();
    let script = format!(
        r#"source "{}/scripts/lib/rulebook.sh"
           [ "$(rule_status GP-1)" = planned ] || {{ echo "GP-1 not planned"; exit 1; }}
           [ "$(rule_status GP-99)" = missing ] || {{ echo "GP-99 not missing"; exit 1; }}
           report_violation GP-1 "example"
           rulebook_exit"#,
        root.display()
    );
    let out = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("run rulebook lib");
    assert!(
        out.status.success(),
        "a planned rule must not fail the gate: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("warning:"),
        "a planned violation must still be reported as a warning"
    );
}
