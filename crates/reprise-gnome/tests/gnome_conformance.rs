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

fn run_gate(script: &str) -> std::process::Output {
    let root = repo_root();
    Command::new("bash")
        .arg(root.join("scripts").join(script))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("could not run {script}: {e}"))
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

#[test]
fn gp_12_metainfo_passes_appstream_validation() {
    let out = run_gate("check-appstream.sh");
    assert!(
        out.status.success(),
        "check-appstream.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gp_13_desktop_file_is_valid() {
    let out = run_gate("check-appstream.sh");
    assert!(
        out.status.success(),
        "check-appstream.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gp_16_name_and_summary_stay_within_length_limits() {
    let out = run_gate("check-appstream.sh");
    assert!(
        out.status.success(),
        "check-appstream.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gp_14_flatpak_manifest_passes_lint() {
    let out = run_gate("check-flatpak-manifest.sh");
    assert!(
        out.status.success(),
        "check-flatpak-manifest.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gp_2_no_blocking_calls_on_the_main_thread() {
    let out = run_gate("check-gnome-idioms.sh");
    assert!(
        out.status.success(),
        "check-gnome-idioms.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gp_3_widget_closures_capture_weakly() {
    let out = run_gate("check-gnome-idioms.sh");
    assert!(
        out.status.success(),
        "check-gnome-idioms.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gp_4_no_unwrap_in_ui_paths() {
    let out = run_gate("check-gnome-idioms.sh");
    assert!(
        out.status.success(),
        "check-gnome-idioms.sh failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
