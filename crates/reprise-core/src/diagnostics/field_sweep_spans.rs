//! Test support: the fields of `#[instrument]`, for `field_sweep`.
//!
//! The attribute names its fields in `fields(…)`, like a span macro does, but
//! it also records every function argument by name unless `skip_all` says
//! otherwise, and `ret` and `err` record the return value and the error. The
//! sweep cannot see an argument's name as a field and cannot classify a return
//! value, so it accepts only the shape it can read in full and refuses the rest
//! with an error, rather than letting a key slip past the classification.
//!
//! Accepted: `#[instrument(skip_all, fields(…))]` with any of `level`, `name`,
//! `target`, `parent` and `follows_from` beside it, bare or as
//! `#[tracing::instrument(…)]`. Refused: no `skip_all`, `skip(…)`, `ret`,
//! `err`, any other option, and `instrument` anywhere else in an attribute
//! (`cfg_attr`, another crate's `instrument`).

use super::field_sweep::{field_keys, top_level_parts, unreadable_field, FieldSite};
use super::field_sweep_lex::{matching_close, Spanned, Token};

/// Options that take a value and record no field of their own.
const VALUE_OPTIONS: [&str; 5] = ["level", "name", "target", "parent", "follows_from"];

/// The sites of every `#[instrument]` among `tokens`.
pub(super) fn instrument_sites(name: &str, tokens: &[Spanned]) -> Result<Vec<FieldSite>, String> {
    let mut sites = Vec::new();
    for (at, spanned) in tokens.iter().enumerate() {
        if !matches!(spanned.token, Token::Punct('#'))
            || !matches!(
                tokens.get(at + 1).map(|s| &s.token),
                Some(Token::Punct('['))
            )
        {
            continue;
        }
        let Some(close) = matching_close(tokens, at + 1) else {
            continue;
        };
        let body = &tokens[at + 2..close];
        if !body.iter().any(|s| is_word(s, "instrument")) {
            continue;
        }
        let refuse = |reason: &str| {
            format!(
                "{name}:{}: this `#[instrument]` is not scanned for fields ({reason}); write \
                 `#[instrument(skip_all, fields(…))]` and name every field",
                spanned.line
            )
        };
        let Some(arguments) = instrument_arguments(body) else {
            return Err(refuse(
                "`instrument` must be the attribute itself, as `instrument` or \
                 `tracing::instrument`, not inside `cfg_attr` or another crate's path",
            ));
        };
        for key in instrument_fields(arguments).map_err(|part| match part {
            Refusal::Option(option) => refuse(&format!("`{option}` is not allowed")),
            Refusal::Field(part) => unreadable_field(name, spanned.line, &part),
        })? {
            sites.push(FieldSite {
                key,
                file: name.to_owned(),
                line: spanned.line,
            });
        }
    }
    Ok(sites)
}

enum Refusal {
    /// An option the sweep does not accept, or the missing `skip_all`.
    Option(String),
    /// A field it cannot read.
    Field(String),
}

fn is_word(spanned: &Spanned, word: &str) -> bool {
    matches!(&spanned.token, Token::Ident(text) if text == word)
}

/// The tokens inside `instrument(…)` for an attribute body that is exactly
/// `instrument`, `tracing::instrument` or `::tracing::instrument`, with or
/// without arguments; `None` for anything else that mentions `instrument`.
fn instrument_arguments(body: &[Spanned]) -> Option<&[Spanned]> {
    let mut at = 0;
    while matches!(body.get(at).map(|s| &s.token), Some(Token::Punct(':'))) {
        at += 1;
    }
    if body.get(at).is_some_and(|s| is_word(s, "tracing")) {
        let colons = body[at + 1..]
            .iter()
            .take_while(|s| matches!(s.token, Token::Punct(':')))
            .count();
        if colons != 2 {
            return None;
        }
        at += 3;
    } else if at != 0 {
        return None;
    }
    if !body.get(at).is_some_and(|s| is_word(s, "instrument")) {
        return None;
    }
    match body.get(at + 1).map(|s| &s.token) {
        None => Some(&[]),
        Some(Token::Punct('(')) => {
            let close = matching_close(body, at + 1)?;
            (close + 1 == body.len()).then(|| &body[at + 2..close])
        }
        Some(_) => None,
    }
}

fn instrument_fields(arguments: &[Spanned]) -> Result<Vec<String>, Refusal> {
    let mut keys = Vec::new();
    let mut skip_all = false;
    for part in top_level_parts(arguments) {
        let Some(Token::Ident(option)) = part.first().map(|s| &s.token) else {
            return Err(Refusal::Option("an argument that is not a name".to_owned()));
        };
        let value = part.get(1).map(|s| &s.token);
        match (option.as_str(), value) {
            ("skip_all", None) => skip_all = true,
            ("fields", Some(Token::Punct('('))) => {
                let close = matching_close(part, 1).unwrap_or(part.len());
                keys.extend(field_keys(&part[2..close], 0).map_err(Refusal::Field)?);
            }
            (name, Some(Token::Punct('='))) if VALUE_OPTIONS.contains(&name) => {}
            (other, _) => return Err(Refusal::Option(other.to_owned())),
        }
    }
    if skip_all {
        Ok(keys)
    } else {
        Err(Refusal::Option(
            "no `skip_all`, so every argument is recorded by name".to_owned(),
        ))
    }
}
