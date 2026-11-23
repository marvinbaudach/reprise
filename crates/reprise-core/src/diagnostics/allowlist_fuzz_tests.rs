//! The invariant fuzz of the allowlist redactor.
//!
//! Over random template sets and random renders, the message part of an own
//! event is always the reduced form of one template of its file, or `…`, and
//! it is the specific one: the best match by literal characters, the first
//! on a tie. The renders hold the needle `zqx` in their hole text; the
//! templates never do, so a needle in the output would be copied text.
//!
//! `LogTemplate::matches` is checked against `reference_matches`, an
//! independent glob matcher that reads the format string itself and shares no
//! code with the one under test; the expected message is built from it. An
//! exhaustive pass over small alphabets covers the holes next to repeated
//! literals that random input reaches only by luck.
//!
//! A xorshift generator with a fixed seed makes a failure reproducible: the
//! assertion message names the iteration.

use std::collections::BTreeSet;

use super::log_template::{LogTemplate, HOLE};
use super::safe_event::{one_line, own_event, Callsite, LogLevel, RecordedEvent};

const FILE: &str = "crates/reprise-gnome/src/ui/fuzz.rs";
const EVENTS: usize = 240_000;
const SEED: u64 = 0x9e37_79b9_7f4a_7c15;
const MAX_TEMPLATES: usize = 5;
const MAX_HOLES: usize = 3;
const MAX_PIECE_TOKENS: usize = 4;
const MAX_FILL_TOKENS: usize = 4;
const MAX_RANDOM_TOKENS: usize = 12;
const NEEDLE: &str = "zqx";

/// What a template's literal pieces are made of: few characters, so that
/// pieces recur, plus a line feed and the escaped braces.
const PIECE_TOKENS: &[&str] = &["a", "b", "c", " ", ":", "\n", "{{", "}}", "ab", "ba"];
/// What a hole may print: the same characters, the needle, the next literal's
/// text, braces and the placeholder itself.
const FILL_TOKENS: &[&str] = &[
    "a", "b", "c", " ", ":", "\n", "zqx", "Zqx", "ab", "{", "}", "…", "", "\r",
];

/// One position of a glob: a literal character or a hole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glob {
    Char(char),
    Any,
}

/// The glob a format string stands for, read independently of `LogTemplate`:
/// `{{` and `}}` are braces, a `{…}` is a hole.
fn glob_of(format: &str) -> Vec<Glob> {
    let chars = format.chars().collect::<Vec<_>>();
    let mut glob = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match (chars[at], chars.get(at + 1)) {
            ('{', Some('{')) => {
                glob.push(Glob::Char('{'));
                at += 2;
            }
            ('}', Some('}')) => {
                glob.push(Glob::Char('}'));
                at += 2;
            }
            ('{', _) => {
                while at < chars.len() && chars[at] != '}' {
                    at += 1;
                }
                glob.push(Glob::Any);
                at += 1;
            }
            (other, _) => {
                glob.push(Glob::Char(other));
                at += 1;
            }
        }
    }
    glob
}

/// The oracle: whether `glob` matches the whole of `text`. A hole tries every
/// split of the rest of the text (backtracking), and a table remembers each
/// `(glob position, text position)` outcome so the work stays polynomial.
fn reference_matches(glob: &[Glob], text: &str) -> bool {
    let text = text.chars().collect::<Vec<_>>();
    let mut memo = vec![vec![None; text.len() + 1]; glob.len() + 1];
    reference_from(glob, &text, 0, 0, &mut memo)
}

fn reference_from(
    glob: &[Glob],
    text: &[char],
    glob_at: usize,
    text_at: usize,
    memo: &mut Vec<Vec<Option<bool>>>,
) -> bool {
    if let Some(known) = memo[glob_at][text_at] {
        return known;
    }
    let outcome = match glob.get(glob_at) {
        None => text_at == text.len(),
        Some(Glob::Char(wanted)) => {
            text.get(text_at) == Some(wanted)
                && reference_from(glob, text, glob_at + 1, text_at + 1, memo)
        }
        Some(Glob::Any) => (text_at..=text.len())
            .any(|resume| reference_from(glob, text, glob_at + 1, resume, memo)),
    };
    memo[glob_at][text_at] = Some(outcome);
    outcome
}

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn pick<'a>(&mut self, tokens: &[&'a str]) -> &'a str {
        tokens[self.below(tokens.len())]
    }

    fn text(&mut self, tokens: &[&str], max: usize) -> String {
        let count = self.below(max + 1);
        (0..count).map(|_| self.pick(tokens)).collect()
    }
}

/// A random format string with up to `MAX_HOLES` holes, adjacent and empty
/// pieces allowed.
fn random_format(rng: &mut Xorshift) -> String {
    let holes = rng.below(MAX_HOLES + 1);
    let mut format = rng.text(PIECE_TOKENS, MAX_PIECE_TOKENS);
    for _ in 0..holes {
        format.push_str("{}");
        format.push_str(&rng.text(PIECE_TOKENS, MAX_PIECE_TOKENS));
    }
    format
}

/// Random templates, each with the oracle's glob of the same format string.
fn random_templates(rng: &mut Xorshift) -> Vec<(LogTemplate, Vec<Glob>)> {
    let count = 1 + rng.below(MAX_TEMPLATES);
    (0..count)
        .filter_map(|_| {
            let format = random_format(rng);
            let template = LogTemplate::parse(&format).ok()?;
            Some((template, glob_of(&format)))
        })
        .collect()
}

/// A message: a render of one of the templates with random hole text, a render
/// that was then cut or extended, or a random text.
fn random_message(rng: &mut Xorshift, templates: &[LogTemplate]) -> String {
    let render = |rng: &mut Xorshift| {
        let template = &templates[rng.below(templates.len())];
        let fill = rng.text(FILL_TOKENS, MAX_FILL_TOKENS);
        template.rendered_with(&fill)
    };
    match rng.below(4) {
        0 | 1 => render(rng),
        2 => {
            let mut message = render(rng);
            if rng.below(2) == 0 {
                message.pop();
            } else {
                message.push_str(rng.pick(FILL_TOKENS));
            }
            message
        }
        _ => rng.text(FILL_TOKENS, MAX_RANDOM_TOKENS),
    }
}

/// The specific match: the most literal characters, the first on a tie, or
/// `…`. Matching goes through the oracle, so the fuzz does not lean on the
/// matcher under test.
fn expected_message(cases: &[(LogTemplate, Vec<Glob>)], message: &str) -> String {
    let mut best: Option<&LogTemplate> = None;
    for (template, glob) in cases {
        if !reference_matches(glob, message) {
            continue;
        }
        if best.is_none_or(|kept| template.literal_chars() > kept.literal_chars()) {
            best = Some(template);
        }
    }
    best.map_or_else(|| HOLE.to_owned(), LogTemplate::reduced)
}

#[test]
fn over_random_template_sets_and_renders_the_message_is_a_reduced_template_or_an_ellipsis() {
    let callsite = Callsite {
        file: Some(FILE),
        line: Some(1),
        target: "reprise::fuzz",
        level: LogLevel::Warn,
    };
    let mut rng = Xorshift(SEED);
    let (mut matched, mut unmatched) = (0_usize, 0_usize);
    for iteration in 0..EVENTS {
        let cases = random_templates(&mut rng);
        if cases.is_empty() {
            continue;
        }
        let templates = cases
            .iter()
            .map(|(template, _)| template.clone())
            .collect::<Vec<_>>();
        let message = random_message(&mut rng, &templates);
        let recorded = RecordedEvent {
            message: Some(message.clone()),
            fields: Vec::new(),
        };

        for (template, glob) in &cases {
            assert_eq!(
                template.matches(&message),
                reference_matches(glob, &message),
                "iteration {iteration}: {:?} against {message:?}",
                template.canonical()
            );
        }

        let event = own_event(&callsite, FILE, &templates, &recorded);

        let allowed = templates
            .iter()
            .map(|template| one_line(&template.reduced()))
            .chain([HOLE.to_owned()])
            .collect::<BTreeSet<_>>();
        assert!(
            allowed.contains(event.body()),
            "iteration {iteration}: {message:?} showed {:?}",
            event.body()
        );
        assert_eq!(
            event.body(),
            one_line(&expected_message(&cases, &message)),
            "iteration {iteration}: {message:?}"
        );
        assert!(
            !event.line().to_lowercase().contains(NEEDLE),
            "iteration {iteration}: the needle reached {:?}",
            event.line()
        );
        if event.body() == HOLE {
            unmatched += 1;
        } else {
            matched += 1;
        }
    }
    // Both outcomes must be exercised in volume, or the fuzz proves little.
    assert!(matched > EVENTS / 10, "{matched} matched");
    assert!(unmatched > EVENTS / 10, "{unmatched} unmatched");
}

/// Every template over a small alphabet of literals and holes against every
/// text over `a` and `b`: holes next to repeated literals, empty pieces, and
/// prefixes and suffixes that overlap.
#[test]
fn the_matcher_agrees_with_the_oracle_on_every_small_template_and_text() {
    const FORMAT_TOKENS: &[&str] = &["a", "b", "ab", "{}"];
    const MAX_FORMAT_TOKENS: usize = 5;
    const MAX_TEXT_CHARS: usize = 8;

    let mut formats = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..MAX_FORMAT_TOKENS {
        layer = layer
            .iter()
            .flat_map(|head| {
                FORMAT_TOKENS
                    .iter()
                    .map(move |token| format!("{head}{token}"))
            })
            .collect();
        formats.extend(layer.iter().cloned());
    }
    let mut texts = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..MAX_TEXT_CHARS {
        layer = layer
            .iter()
            .flat_map(|head| ["a", "b"].map(|ch| format!("{head}{ch}")))
            .collect();
        texts.extend(layer.iter().cloned());
    }

    let (mut matching, mut missing) = (0_usize, 0_usize);
    for format in &formats {
        let template = LogTemplate::parse(format).unwrap_or_else(|error| panic!("{error}"));
        let glob = glob_of(format);
        for text in &texts {
            let expected = reference_matches(&glob, text);
            assert_eq!(
                template.matches(text),
                expected,
                "{format:?} against {text:?}"
            );
            if expected {
                matching += 1;
            } else {
                missing += 1;
            }
        }
    }
    assert!(matching > 10_000, "{matching} matched");
    assert!(missing > 10_000, "{missing} did not match");
}

#[test]
fn the_oracle_itself_is_right_on_known_cases() {
    let cases = [
        ("a{}a", "a", false),
        ("a{}a", "aa", true),
        ("a{}a", "abaa", true),
        ("{}ab{}ab", "xabyab", true),
        ("{}ab{}ab", "abab", true),
        ("{}ab{}ab", "xabyaba", false),
        ("{{}}", "{}", true),
        ("{}", "", true),
        ("", "", true),
        ("", "a", false),
    ];
    for (format, text, expected) in cases {
        assert_eq!(
            reference_matches(&glob_of(format), text),
            expected,
            "{format:?} against {text:?}"
        );
    }
}
