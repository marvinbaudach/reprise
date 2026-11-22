//! Secret words in free text: a word that announces a secret, followed by a
//! value in any form, takes the rest of the message.
//!
//! `password: x` and `token=x` are read by the key pass. This module reads the
//! rest: the word with another separator glued to its value (`password(x)`,
//! `password|x`, `password@x`), the word followed by its value in running text
//! (`token abc123 expired`, `--username bob`, `passwords hunter2`), a key
//! written in two words (`api key abc`, `session key abc`), a credential
//! after `Basic`, and the name of a network (`ssid Home Net`, `wifi network
//! Home Net`). A secret has no reliable end, so none of these has one.

const REDACTED: &str = "$REDACTED";

/// Words that mark a secret anywhere in a colon-form key: `password: hunter2`,
/// `x-api-key:`, `my_session_id:`. Compared with `-` and `_` removed and the
/// case folded, so `x-api-key`, `API_KEY` and `apikey` are one key.
const SECRET_KEY_NEEDLES: [&str; 19] = [
    "passw",
    "pwd",
    "passphrase",
    "passcode",
    "token",
    "secret",
    "credential",
    "username",
    "apikey",
    "accesskey",
    "privatekey",
    "secretkey",
    "sessionkey",
    "authkey",
    "authorization",
    "bearer",
    "cookie",
    "session",
    "signature",
];

/// Short words that mark a secret only as a whole segment of the key
/// (`pin`, `auth_code`): inside a longer word (`spinning`, `decode`) they are
/// prose.
const SECRET_KEY_SEGMENTS: [&str; 14] = [
    "pass", "pin", "auth", "sig", "otp", "code", "login", "user", "ssid", "pw", "psk", "jwt", "sk",
    "key",
];

/// Words that announce a secret in running text, found anywhere in a word:
/// `X-Api-Key abc123`, `tokens abc`, `userpassword abc`.
const BARE_NEEDLES: [&str; 17] = [
    "password",
    "passwd",
    "passwort",
    "passphrase",
    "passcode",
    "credential",
    "apikey",
    "accesskey",
    "privatekey",
    "secretkey",
    "sessionkey",
    "authkey",
    "authorization",
    "bearer",
    "token",
    "secret",
    "cookie",
];

/// Words that announce a secret in running text only as a whole word.
const BARE_WORDS: [&str; 7] = ["pw", "pwd", "psk", "jwt", "sk", "username", "ssid"];

/// The first word of a two-word secret: `api key`, `session key`.
const KEY_MODIFIERS: [&str; 12] = [
    "api",
    "access",
    "private",
    "secret",
    "session",
    "auth",
    "license",
    "encryption",
    "signing",
    "master",
    "ssh",
    "gpg",
];

/// The first word of the name of a network: `wifi network Home Net`.
const NETWORK_MODIFIERS: [&str; 4] = ["wifi", "wi-fi", "wlan", "wireless"];
const NETWORK_NOUNS: [&str; 2] = ["network", "name"];

/// The shortest run of key characters that reads as a credential after `Basic`.
const MIN_CREDENTIAL_CHARS: usize = 8;

/// Punctuation that may follow a secret word without being its value.
const TRAILING_PUNCTUATION: [char; 9] = [',', ';', '.', '!', '?', ')', ']', '}', '"'];

/// Whether a key names a secret: `password`, `x-api-key`, `Authorization`.
pub(super) fn is_secret_key(key: &str) -> bool {
    SECRET_KEY_NEEDLES
        .iter()
        .any(|needle| compact(key).contains(needle))
        || key
            .to_lowercase()
            .split(|ch: char| !ch.is_alphanumeric())
            .any(|segment| SECRET_KEY_SEGMENTS.contains(&segment))
}

fn compact(word: &str) -> String {
    word.chars()
        .filter(|ch| !matches!(ch, '-' | '_' | ' '))
        .flat_map(char::to_lowercase)
        .collect()
}

/// A word that announces a secret, or one whose `-` or `_` part does
/// (`x_sk`, `my-psk`).
fn is_secret_word(word: &str) -> bool {
    let compact = compact(word);
    BARE_NEEDLES.iter().any(|needle| compact.contains(needle))
        || parts(word).any(|part| BARE_WORDS.contains(&part.as_str()))
}

fn parts(word: &str) -> impl Iterator<Item = String> + '_ {
    word.split(['-', '_', '.', '/']).map(str::to_lowercase)
}

/// A token split into the key-like word it opens with and what follows it:
/// (start of the word, end of the word). Leading punctuation (`--`, `{"`) is
/// not part of the word. The word may be a dotted or slashed name
/// (`db.password`, `config/secret`): it ends behind the first part of it that
/// is a secret word, so what follows that part is its value (`password.x`),
/// and behind the whole run when none is.
fn key_run(token: &str) -> (usize, usize) {
    let start = token.len()
        - token
            .trim_start_matches(|ch: char| !ch.is_alphanumeric())
            .len();
    let run = token[start..]
        .find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/')))
        .unwrap_or(token.len() - start);
    let mut end = start;
    for part in token[start..start + run].split(['.', '/']) {
        end += part.len();
        if !part.is_empty() && is_secret_word(part) {
            return (start, end);
        }
        end += 1;
    }
    (start, (start + run).min(token.len()))
}

/// What replaces the first token (and, with the words after it, the rest of
/// the message) when the first token is a secret word with a value in some
/// other separator form than `=` or `:`.
pub(super) fn secret_cut(tokens: &[&str]) -> Option<String> {
    let body = tokens.first()?.trim_end_matches(char::is_whitespace);
    let (word_start, word_end) = key_run(body);
    let word = &body[word_start..word_end];
    let rest = &body[word_end..];
    if word.is_empty() || rest.starts_with(['=', ':']) {
        return None;
    }
    let more = tokens.len() > 1;
    if is_secret_word(word) {
        return glued_or_bare(body, word_end, rest, more);
    }
    // The next word, whatever whitespace stands between it and this one.
    let next = tokens
        .iter()
        .skip(1)
        .map(|token| token.trim_end_matches(char::is_whitespace))
        .find(|token| !token.is_empty())?;
    if is_value_probe(word, next) {
        return Some(format!("{body} {REDACTED}"));
    }
    let (next_start, next_end) = key_run(next);
    let next_word = compact(&next[next_start..next_end]);
    if parts(word).any(|part| KEY_MODIFIERS.contains(&part.as_str())) && next_word == "key" {
        let tail = glued_or_bare(next, next_end, &next[next_end..], tokens.len() > 2)?;
        return Some(format!("{body} {tail}"));
    }
    if NETWORK_MODIFIERS.contains(&compact(word).as_str())
        && NETWORK_NOUNS.contains(&next_word.as_str())
        && tokens.len() > 2
    {
        return Some(format!("{body} {next} {REDACTED}"));
    }
    if compact(word) == "basic" && is_basic_credential(next) {
        return Some(format!("{body} {REDACTED}"));
    }
    None
}

/// A short word that is a secret only when what follows it looks like one:
/// `pass hunter2`, `PIN 1234`, `session abc123def`, `code 4/0AX4XfWh`. Compared
/// with `-` and `_` removed and the case folded, after the last `.` or `/`.
const PROBE_WORDS: [&str; 11] = [
    "pass",
    "pin",
    "otp",
    "session",
    "sessionid",
    "sid",
    "auth",
    "code",
    "key",
    "sig",
    "login",
];

/// The longest number a status code is (`exit code 101`, `status code 404`):
/// after `code` it is no credential.
const MAX_STATUS_DIGITS: usize = 3;

/// An uppercase letter after a lowercase one, or this many characters of
/// letters and digits, make a word read as a token and not as a word.
const TOKEN_LIKE_MIN_LEN: usize = 20;

fn is_value_probe(word: &str, next: &str) -> bool {
    let last = word.rsplit(['.', '/']).next().unwrap_or(word);
    let probe = [last, last.rsplit(['-', '_']).next().unwrap_or(last)]
        .into_iter()
        .map(compact)
        .find(|name| PROBE_WORDS.contains(&name.as_str()));
    probe.is_some_and(|name| {
        looks_like_a_secret_value(next) && !(name == "code" && is_status_code(next))
    })
}

/// `1234`, `hunter2`, `abc123def`, `4/0AX4XfWh`, `AIzaSyD-abc`: a token has a
/// digit in it, a capital behind a small letter, or is long and unbroken.
/// `refused` and `frame` are words.
fn looks_like_a_secret_value(token: &str) -> bool {
    let value = token
        .trim_matches(|ch: char| TRAILING_PUNCTUATION.contains(&ch) || ch == '(' || ch == '\'');
    if value.is_empty() || value == REDACTED {
        return false;
    }
    let mut previous_lower = false;
    let mut mixed_case = false;
    for ch in value.chars() {
        mixed_case |= previous_lower && ch.is_uppercase();
        previous_lower = ch.is_lowercase();
    }
    value.chars().any(|ch| ch.is_ascii_digit())
        || mixed_case
        || value.chars().count() >= TOKEN_LIKE_MIN_LEN
            && value.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// One to three digits, as `status_code` reads them in the colon form.
fn is_status_code(token: &str) -> bool {
    let digits = token.trim_matches(|ch: char| TRAILING_PUNCTUATION.contains(&ch));
    (1..=MAX_STATUS_DIGITS).contains(&digits.len()) && digits.chars().all(|ch| ch.is_ascii_digit())
}

/// The secret word is followed by a glued value (`password(x)`), or by nothing
/// but punctuation and then more words (`password, x`).
fn glued_or_bare(body: &str, word_end: usize, rest: &str, more_words: bool) -> Option<String> {
    let bare = || more_words.then(|| format!("{body} {REDACTED}"));
    if rest.chars().all(|ch| TRAILING_PUNCTUATION.contains(&ch)) {
        return bare();
    }
    let separator = rest.chars().next()?;
    let value = &rest[separator.len_utf8()..];
    if value.is_empty() {
        return bare();
    }
    if value == REDACTED {
        return Some(body.to_owned());
    }
    let through = word_end + separator.len_utf8();
    Some(format!("{}{REDACTED}", &body[..through]))
}

/// `dXNlcjpwYXNz`: a base64 credential, not an English word. It has a digit,
/// a sign, or a capital after its first letter.
fn is_basic_credential(token: &str) -> bool {
    token.len() >= MIN_CREDENTIAL_CHARS
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '_' | '-'))
        && token
            .chars()
            .skip(1)
            .any(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase() || !ch.is_ascii_alphanumeric())
}
