# GNOME Conformance Findings

Collected on 2026-08-11 against section AI of `docs/ux-rules.md`. This document
changes no code. It is the input for Wave 2.

The Strand A gate scripts and GP-1 through GP-20 rule text were not present at
the audited base (`4f6dfc7cb2`), so the baseline uses the direct `ripgrep` and
`find` fallback prescribed by the audit plan. Counts include test modules under
`crates/reprise-gnome/src` because the prescribed scope does not exclude them.

## Baseline

| Rule | Measurement | Value |
|---|---|---|
| GP-2 | blocking-call pattern matches in `reprise-gnome/src` | 33 |
| GP-3 | `clone!` blocks with `#[strong]` | 0 |
| GP-4 | `unwrap()` calls in `reprise-gnome/src` | 2,116 |
| GP-5 | files with `ObjectSubclass` | 11 of 706 Rust files |
| GP-6 | GSettings schemas in the tree | 0 |
| GP-11 | custom-CSS pattern matches | 706 |

The raw command outputs contained 33 lines in `blocking.txt`, 0 in
`strong.txt`, 255 files in `unwrap.txt` totalling 2,116 matches, 11 files in
`subclassed.txt`, 0 in `gsettings.txt`, and 148 files in `css.txt` totalling
706 matches.

## Findings
