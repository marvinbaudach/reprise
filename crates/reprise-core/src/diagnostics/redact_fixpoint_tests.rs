//! The redactor's output is a fixed point of the redactor, structurally: the
//! rounds run until a round changes nothing, and a message that does not
//! settle is replaced as a whole. Round 3 of the review of #1374 found
//! 25-byte inputs that needed twelve rounds.

use super::{redact_log_message, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/marvin/Music".into()),
        home_dir: Some("/home/marvin".into()),
        username: Some("marvin".into()),
        hostname: Some("my-nas".into()),
    }
}

fn assert_fixpoint(input: &str, context: &RedactionContext) {
    let once = redact_log_message(input, context);
    assert_eq!(
        redact_log_message(&once, context),
        once,
        "not a fixed point: {input:?} -> {once:?}"
    );
}

#[test]
fn the_reviewers_minimal_inputs_are_fixed_points() {
    for input in [
        "E,\\\tU,\\\tP,\\ R1\\marviN.,$e/",
        "A,$\tMA\\\".,$osty\\marvin://",
        "M,$ Hi\\$.,IPvie/marviN/://",
        "Pink,\\ Floyd,\\ Dark,\\ Side,\\ Moon,\\ Wall,\\ Animals,\\ Meddle,\\ x.,$e/",
        "M,$ Pink,$ Floyd,$ Dark,$ Side,$ Moon,$ x/marvin/://",
    ] {
        assert_fixpoint(input, &context());
    }
}

/// Words that start, join and end the strings the fuzz builds.
const ATOMS: &[&str] = &[
    "/",
    "\\",
    "'",
    "\"",
    "`",
    " ",
    " ",
    "\t",
    "\n",
    ":",
    "=",
    "//",
    "://",
    "http",
    "file",
    "~",
    "$HOME",
    "…",
    "$REDACTED",
    "$USER",
    "$HOST",
    "$IP",
    "$MAC",
    "$XDG_MUSIC_DIR/…",
    "$HOME/…",
    "key",
    "password",
    "token",
    "title",
    "count=3",
    "error=",
    "a",
    "Music",
    "/home/marvin",
    "/mnt/",
    "marvin",
    "my-nas",
    ",",
    ";",
    ")",
    "(",
    "[",
    "]",
    "{",
    "}",
    "%",
    "::",
    "1",
    "192.168.1.1",
    "fe80::1",
    "é",
    ".flac",
    ".",
    "-",
    "@",
    "Pink",
    "Floyd",
    "the",
    "x=$REDACTED",
    "e",
    "$",
    "_",
    "ip",
    "mac",
    "AA_BB_CC_DD_EE_FF",
    "%2F",
    "／",
    "＝",
    "。",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn random_text_redacts_to_a_fixed_point_under_hostile_contexts() {
    let contexts = [
        context(),
        RedactionContext {
            username: Some("user".into()),
            hostname: Some("host".into()),
            ..RedactionContext::default()
        },
        RedactionContext {
            username: Some("e".into()),
            hostname: Some("a".into()),
            ..RedactionContext::default()
        },
        RedactionContext {
            username: Some("redacted".into()),
            hostname: Some("ip".into()),
            ..RedactionContext::default()
        },
    ];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..20_000 {
        let mut text = String::new();
        for _ in 0..1 + rng.next() % 24 {
            text.push_str(ATOMS[(rng.next() % ATOMS.len() as u64) as usize]);
        }
        for context in &contexts {
            assert_fixpoint(&text, context);
        }
    }
}

#[test]
fn the_output_stays_within_the_cap_and_is_a_fixed_point() {
    for filler in ["/a ", "é", "$HOST ", "my-nas ", "1.2.3.4 ", "marvin "] {
        let text = filler.repeat(9_000);
        let output = redact_log_message(&text, &context());

        assert!(output.len() <= 8 * 1024, "{filler:?}: {}", output.len());
        assert_fixpoint(&text, &context());
    }
}

#[test]
fn a_hostile_name_never_grows_a_marker() {
    for (user, host) in [
        ("user", "host"),
        ("redacted", "ip"),
        ("e", "a"),
        ("home", "mac"),
    ] {
        let context = RedactionContext {
            username: Some(user.into()),
            hostname: Some(host.into()),
            ..RedactionContext::default()
        };
        let text = format!("{user} {host} $USER $HOST $IP $MAC $REDACTED $HOME/…");
        assert_fixpoint(&text, &context);
    }
}
