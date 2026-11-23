//! The template table: its freshness against the sources, the loader and the
//! lookup rule, and the form of a call site's file path.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::field_sweep::workspace_sites;
use super::field_sweep_literals::{table_text, TABLE_CRATES};
use super::log_template::HOLE;
use super::log_templates::{is_own_file, parse_line, parse_table, reduced_by, templates_for_file};

/// The table as checked in.
const CHECKED_IN: &str = include_str!("log_templates.txt");

/// The exact command that regenerates the table; the freshness test prints it.
const BLESS_COMMAND: &str = "cargo test -p reprise-core --lib \
     diagnostics::log_templates_tests::bless_log_templates -- --ignored --exact";

/// The scan must find at least this many distinct table lines and this many of
/// them from `invariant_broken!`, or it reads less than the code has and an
/// empty generator would bless an empty table.
const MIN_TABLE_LINES: usize = 1_200;
const MIN_INVARIANT_LINES: usize = 40;

fn table_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagnostics/log_templates.txt")
}

fn generated() -> (String, usize) {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let sweep = workspace_sites(&crates).expect("the workspace sources are readable");
    (table_text(&sweep.templates), sweep.templates.len())
}

fn line_set(text: &str) -> BTreeSet<&str> {
    text.lines().collect()
}

/// What is wrong with `checked_in` next to `fresh`, one line per difference.
fn freshness_problems(checked_in: &str, fresh: &str) -> Vec<String> {
    let (have, want) = (line_set(checked_in), line_set(fresh));
    let mut problems = Vec::new();
    for line in want.difference(&have) {
        problems.push(format!("missing from the table: {line:?}"));
    }
    for line in have.difference(&want) {
        problems.push(format!("stale in the table: {line:?}"));
    }
    if problems.is_empty() && checked_in != fresh {
        problems.push("the lines are right but not sorted, distinct or newline-ended".to_owned());
    }
    problems
}

#[test]
fn the_checked_in_table_is_what_the_sources_generate() {
    let (fresh, _) = generated();

    let problems = freshness_problems(CHECKED_IN, &fresh);

    assert!(
        problems.is_empty(),
        "crates/reprise-core/src/diagnostics/log_templates.txt is out of date; every line below \
         is a log text that was added, changed or removed. Regenerate the table and review its \
         diff, because each line of it is text that may leave the machine in a report:\n    \
         {BLESS_COMMAND}\n\n{}\n",
        problems.join("\n")
    );
}

#[test]
#[ignore = "rewrites log_templates.txt; run it on purpose with the command in the freshness test"]
fn bless_log_templates() {
    let (fresh, _) = generated();

    std::fs::write(table_path(), fresh).expect("the table file can be written");
}

#[test]
fn the_freshness_check_names_missing_and_stale_lines() {
    let problems = freshness_problems("a\tone\nb\told\n", "a\tone\nc\tnew\n");

    assert_eq!(
        problems,
        [
            "missing from the table: \"c\\tnew\"",
            "stale in the table: \"b\\told\""
        ]
    );
    assert!(freshness_problems("a\tone\n", "a\tone\n").is_empty());
    assert_eq!(freshness_problems("b\ty\na\tx\n", "a\tx\nb\ty\n").len(), 1);
}

#[test]
fn the_scan_finds_enough_to_make_the_table_meaningful() {
    let (fresh, found) = generated();

    let lines = fresh.lines().count();
    let invariant = fresh
        .lines()
        .filter(|line| line.contains("\tinvariant broken: "))
        .count();

    assert!(found >= lines, "{found} sites for {lines} lines");
    assert!(lines >= MIN_TABLE_LINES, "only {lines} table lines");
    assert!(
        invariant >= MIN_INVARIANT_LINES,
        "only {invariant} invariant lines"
    );
}

#[test]
fn the_table_has_no_malformed_line_and_every_file_is_a_workspace_source_of_a_table_crate() {
    for line in CHECKED_IN.lines() {
        let (file, _) = parse_line(line).unwrap_or_else(|| panic!("malformed line: {line:?}"));
        assert!(
            TABLE_CRATES
                .iter()
                .any(|krate| file.starts_with(&format!("crates/{krate}/src/"))),
            "{file}"
        );
        assert!(file.ends_with(".rs"), "{file}");
    }
}

#[test]
fn the_runtime_reads_back_exactly_what_the_generator_wrote() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let sweep = workspace_sites(&crates).expect("the workspace sources are readable");
    let text = table_text(&sweep.templates);
    let index = parse_table(&text);

    let loaded: usize = index.values().map(Vec::len).sum();
    assert_eq!(loaded, text.lines().count(), "a line did not parse");
    for site in &sweep.templates {
        let Some(templates) = index.get(&format!("crates/{}", site.file)) else {
            assert!(
                !TABLE_CRATES
                    .iter()
                    .any(|krate| site.file.starts_with(&format!("{krate}/"))),
                "{} is missing",
                site.file
            );
            continue;
        };
        assert!(
            templates.contains(&site.template),
            "{}:{} did not round-trip: {:?}",
            site.file,
            site.line,
            site.template
        );
    }
}

#[test]
fn the_checked_in_table_loads_and_knows_its_own_files() {
    let (file, _) = CHECKED_IN
        .lines()
        .next()
        .and_then(parse_line)
        .expect("the table has a line");

    assert!(is_own_file(file));
    assert!(!templates_for_file(file).is_empty());
    assert!(!is_own_file("crates/not-a-crate/src/lib.rs"));
    assert!(templates_for_file("/rustc/abc/library/core/src/fmt.rs").is_empty());
}

#[test]
fn a_real_template_without_holes_reduces_to_itself() {
    let (file, template) = CHECKED_IN
        .lines()
        .filter_map(parse_line)
        .find(|(_, template)| template.hole_count() == 0)
        .expect("some message has no hole");
    let text = template.reduced();

    let templates = templates_for_file(file);
    assert_eq!(reduced_by(templates, &text), text);
    assert_eq!(reduced_by(templates, &format!("{text} and more")), HOLE);
}

#[test]
fn a_malformed_line_is_skipped_and_the_rest_of_the_table_loads() {
    let table = "a.rs\tgood {}\n\
                 no tab here\n\
                 \tno file\n\
                 b.rs\tbad {\n\
                 b.rs\tstray }\n\
                 b.rs\tbad \\q escape\n\
                 b.rs\ttrailing backslash\\\n\
                 \n\
                 b.rs\tfine\n";

    let index = parse_table(table);

    assert_eq!(index.len(), 2);
    assert_eq!(index["a.rs"].len(), 1);
    assert_eq!(index["b.rs"].len(), 1);
    assert_eq!(index["b.rs"][0].reduced(), "fine");
}

#[test]
fn the_table_escapes_read_back_as_the_characters_they_stand_for() {
    let table = "a.rs\tone\\ttwo\\nthree\\\\four\\rfive {{x}} {}\n";

    let index = parse_table(table);

    assert_eq!(
        index["a.rs"][0].reduced(),
        "one\ttwo\nthree\\four\rfive {x} …"
    );
}

#[test]
fn a_table_with_crlf_line_ends_loads_without_a_carriage_return_in_any_template() {
    let table = "a.rs\tone {}\r\nb.rs\ttwo\r\n";

    let index = parse_table(table);

    assert_eq!(index["a.rs"][0].reduced(), "one …");
    assert_eq!(index["b.rs"][0].reduced(), "two");
}

fn lookup(table: &str, file: &str, rendered: &str) -> String {
    let index = parse_table(table);
    reduced_by(index.get(file).map_or(&[], Vec::as_slice), rendered)
}

#[test]
fn the_template_with_the_most_literal_characters_wins() {
    let table = "f.rs\t{}\nf.rs\topened {}\nf.rs\topened {} in {}\n";

    assert_eq!(lookup(table, "f.rs", "opened x in y"), "opened … in …");
    assert_eq!(lookup(table, "f.rs", "opened x"), "opened …");
    assert_eq!(lookup(table, "f.rs", "something else"), "…");
}

#[test]
fn a_tie_goes_to_the_first_line_of_the_table() {
    let table = "f.rs\tab {}\nf.rs\t{} ab\nf.rs\tab {} ab\n";

    assert_eq!(lookup(table, "f.rs", "ab ab"), "ab …");
    assert_eq!(lookup(table, "f.rs", "ab ab ab"), "ab … ab");
    let tied = "f.rs\tx{}y\nf.rs\t{}xy\n";
    assert_eq!(lookup(tied, "f.rs", "xy"), "x…y");
    let reversed = "f.rs\t{}xy\nf.rs\tx{}y\n";
    assert_eq!(lookup(reversed, "f.rs", "xy"), "…xy");
}

#[test]
fn no_matching_template_and_an_unknown_file_both_give_the_placeholder() {
    let table = "f.rs\tonly this\n";

    assert_eq!(lookup(table, "f.rs", "only that"), HOLE);
    assert_eq!(lookup(table, "other.rs", "only this"), HOLE);
    assert_eq!(lookup("", "f.rs", ""), HOLE);
}

#[test]
fn the_result_never_contains_the_rendered_text() {
    let table = "f.rs\topened {}\n";
    let secret = "zqx-secret";

    let shown = lookup(table, "f.rs", &format!("opened {secret}"));

    assert_eq!(shown, "opened …");
    assert!(!shown.contains(secret));
}

#[test]
fn a_call_sites_file_has_the_tables_path_form() {
    // `Metadata::file()` is `file!()` at the call site. A build that changes
    // its form (a different working directory, `--remap-path-prefix`, a
    // vendored build) would make every own message look foreign, so the form
    // is pinned here, once per crate that logs to the desktop.
    assert_eq!(
        file!(),
        "crates/reprise-core/src/diagnostics/log_templates_tests.rs"
    );
}
