//! Shared output helpers.
//!
//! `--json` selects machine-readable output; without it commands print a
//! compact human-readable rendering. JSON always goes to stdout as a single
//! pretty-printed document so a caller can pipe it straight into a parser.

use serde_json::Value;

/// Prints `value` as pretty JSON on stdout. Serialization of a plain
/// `serde_json::Value` cannot fail, so this is infallible in practice.
pub fn print_json(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("failed to serialize output: {error}"),
    }
}

/// Makes an untrusted string safe to print to a terminal in human-readable
/// output. Track and playlist metadata come from files this CLI never wrote, so
/// a hostile tag can carry raw ANSI/OSC escape sequences (`ESC [ … m`, title
/// setters, hyperlinks) that would otherwise reach and reprogram the terminal.
/// Every control character is replaced with U+FFFD except the `\n`/`\t` this CLI
/// emits itself as layout — covering C0 (including `ESC`, 0x1b), `DEL`, and the
/// C1 range. Apply it to every untrusted field in text output; `--json` is left
/// untouched because serde already escapes control characters there.
pub fn sanitize_for_terminal(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\n' | '\t' => character,
            other if other.is_control() => '\u{FFFD}',
            other => other,
        })
        .collect()
}

/// Formats a duration in milliseconds as `H:MM:SS` (hours dropped when zero),
/// for human-readable listings.
pub fn format_duration_ms(duration_ms: i64) -> String {
    let total_seconds = duration_ms.max(0) / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_for_terminal;

    #[test]
    fn sanitize_replaces_control_and_escape_bytes_but_keeps_layout_and_text() {
        // A hostile "color + hide" ANSI sequence and a DEL are neutralized...
        let hostile = "Song\u{1b}[31m\u{7f}Title";
        let cleaned = sanitize_for_terminal(hostile);
        assert!(
            !cleaned.contains('\u{1b}'),
            "ESC must not survive: {cleaned:?}"
        );
        assert!(!cleaned.contains('\u{7f}'), "DEL must not survive");
        assert!(cleaned.contains('\u{FFFD}'), "controls become U+FFFD");
        // ...while the CLI's own layout whitespace and ordinary (incl.
        // non-ASCII) text pass through untouched.
        assert_eq!(sanitize_for_terminal("a\tb\nc"), "a\tb\nc");
        assert_eq!(sanitize_for_terminal("Björk – Jóga"), "Björk – Jóga");
    }
}
