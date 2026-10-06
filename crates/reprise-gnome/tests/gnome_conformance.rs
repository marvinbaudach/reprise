//! Gate wrappers: each test runs the shell gate that enforces its rule.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::OnceLock;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn run_gate(script: &str) -> Output {
    let root = repo_root();
    Command::new("bash")
        .arg(root.join("scripts").join(script))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("could not run {script}: {e}"))
}

/// Several rules share one gate script. Each script runs once per test binary;
/// every test then asserts on that one result, so a failing gate still fails
/// each rule that names it. `OnceLock` makes a test that arrives while the
/// script is running wait for it instead of starting a second copy.
fn appstream_gate() -> &'static Output {
    static RESULT: OnceLock<Output> = OnceLock::new();
    RESULT.get_or_init(|| run_gate("check-appstream.sh"))
}

fn gnome_idioms_gate() -> &'static Output {
    static RESULT: OnceLock<Output> = OnceLock::new();
    RESULT.get_or_init(|| run_gate("check-gnome-idioms.sh"))
}

fn ai_hygiene_gate() -> &'static Output {
    static RESULT: OnceLock<Output> = OnceLock::new();
    RESULT.get_or_init(|| run_gate("check-ai-hygiene.sh"))
}

fn assert_gate_passed(script: &str, out: &Output) {
    assert!(
        out.status.success(),
        "{script} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
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
    assert_gate_passed("check-appstream.sh", appstream_gate());
}

#[test]
fn gp_13_desktop_file_is_valid() {
    assert_gate_passed("check-appstream.sh", appstream_gate());
}

#[test]
fn gp_16_name_and_summary_stay_within_length_limits() {
    assert_gate_passed("check-appstream.sh", appstream_gate());
}

#[test]
fn gp_14_flatpak_manifest_passes_lint() {
    assert_gate_passed(
        "check-flatpak-manifest.sh",
        &run_gate("check-flatpak-manifest.sh"),
    );
}

#[test]
fn gp_2_no_blocking_calls_on_the_main_thread() {
    assert_gate_passed("check-gnome-idioms.sh", gnome_idioms_gate());
}

#[test]
fn gp_3_widget_closures_capture_weakly() {
    assert_gate_passed("check-gnome-idioms.sh", gnome_idioms_gate());
}

#[test]
fn gp_4_no_unwrap_in_ui_paths() {
    assert_gate_passed("check-gnome-idioms.sh", gnome_idioms_gate());
}

#[test]
fn gp_19_comments_carry_no_model_instructions() {
    assert_gate_passed("check-ai-hygiene.sh", ai_hygiene_gate());
}

#[test]
fn gp_20_no_dead_code_without_a_reason() {
    assert_gate_passed("check-ai-hygiene.sh", ai_hygiene_gate());
}
