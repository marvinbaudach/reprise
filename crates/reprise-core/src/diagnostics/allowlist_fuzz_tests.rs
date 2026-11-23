//! The invariant fuzz of the allowlist redactor.
//!
//! Over random template sets and random renders, the message part of an own
//! event is always the reduced form of one template of its file, or `…`, and
//! it is the specific one: the best match by literal characters, the first
//! on a tie. The renders hold the needle `zqx` in their hole text; the
//! templates never do, so a needle in the output would be copied text.
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

fn random_templates(rng: &mut Xorshift) -> Vec<LogTemplate> {
    let count = 1 + rng.below(MAX_TEMPLATES);
    (0..count)
        .filter_map(|_| LogTemplate::parse(&random_format(rng)).ok())
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
/// `…`. Written out here so the fuzz does not lean on the code under test.
fn expected_message(templates: &[LogTemplate], message: &str) -> String {
    let mut best: Option<&LogTemplate> = None;
    for template in templates
        .iter()
        .filter(|template| template.matches(message))
    {
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
        let templates = random_templates(&mut rng);
        if templates.is_empty() {
            continue;
        }
        let message = random_message(&mut rng, &templates);
        let recorded = RecordedEvent {
            message: Some(message.clone()),
            fields: Vec::new(),
        };

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
            one_line(&expected_message(&templates, &message)),
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
