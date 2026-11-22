use super::field_policy::KEEP_FIELDS;
use super::model::RedactionContext;
use super::redact_assignments::redact_sensitive_assignments;
use super::redact_filenames::redact_filenames;
use super::redact_markers::ELLIPSIS;
use super::redact_names::{host_names, replace_name};
use super::redact_network::redact_network_identifiers;
use super::redact_normalise::normalised;
use super::redact_paths::redact_paths;

/// The longest message that is redacted. No line of ours comes near it, and
/// the passes read each message more than once: what lies beyond is cut off
/// (and so never leaves the machine) instead of being scanned. The cap holds
/// for what a round writes as well as for what it reads.
const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// The message up to [`MAX_MESSAGE_BYTES`], cut at a character boundary and
/// marked with an ellipsis.
fn capped(message: String) -> String {
    if message.len() <= MAX_MESSAGE_BYTES {
        return message;
    }
    let mut end = MAX_MESSAGE_BYTES - ELLIPSIS.len();
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ELLIPSIS}", &message[..end])
}

/// How often the passes are applied to their own output at most. A message
/// that still changes after this many rounds is not a message to explain: see
/// [`redact_log_message`].
const MAX_ROUNDS: usize = 8;

/// What a message that does not settle is replaced with. It is a fixed point
/// of every round, whatever the names of the user and the machine are.
const UNSETTLED: &str = "$REDACTED";

/// Removes values that must not leave the user's machine in a copied report.
///
/// The passes run on their own output until a round changes nothing: the
/// report redacts the lines the file mirror has already redacted, and a pass
/// that cuts a word short can leave a prefix an earlier pass reads differently
/// the second time. What comes back is therefore a fixed point by
/// construction, never by hope: either a text one more round leaves exactly
/// as it is (so a second redaction is a no-op), or, when eight rounds have not
/// settled the text, `$REDACTED` in its place. Nothing that did not settle
/// leaves the function.
pub fn redact_log_message(message: &str, context: &RedactionContext) -> String {
    let mut current = capped(message.to_owned());
    for _ in 0..MAX_ROUNDS {
        let next = redact_round(&current, context);
        if next == current {
            return current;
        }
        current = next;
    }
    UNSETTLED.to_owned()
}

pub(super) fn redact_round(message: &str, context: &RedactionContext) -> String {
    let normalised = normalised(message);
    let mut redacted = redact_paths(&normalised, context);
    redacted = redact_network_identifiers(&redacted);
    // The key pass reads the line before the name is replaced, so a user whose
    // name is a key (`password`, `id`) cannot change what that key means. The
    // report redacts the file's lines again; `id=$USER` is a value that pass
    // accepts as it is (`is_plain_value`), so the second run changes nothing.
    redacted = redact_sensitive_assignments(&redacted);
    capped(redact_filenames(&redact_names(&redacted, context)))
}

/// The user's name and the machine's name, replaced by their markers.
pub(super) fn redact_names(text: &str, context: &RedactionContext) -> String {
    let mut redacted = text.to_owned();
    if let Some(username) = context
        .username
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        redacted = replace_name(&redacted, username, "$USER");
    }
    if let Some(hostname) = context
        .hostname
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        for name in host_names(hostname) {
            redacted = replace_name(&redacted, name, "$HOST");
        }
    }
    redacted
}

/// Whether a structured field's value may be kept in a diagnostics line.
///
/// A lookup in `KEEP_FIELDS`: both redaction passes ask this one question,
/// and every key not listed there is redacted. The lookup is an exact match
/// against lowercase names. `EventFields` passes a field's name as written,
/// so `Count` is unknown there; the free-text pass lowercases the key first
/// (`split_assignment`), so `Count=3` reads as `count=3`. That is harmless:
/// it can only keep a value whose lowercase key is kept anyway. An unknown key
/// is never safe. `field_policy` explains how a key earns its place.
pub fn is_safe_structured_field(key: &str) -> bool {
    KEEP_FIELDS.binary_search(&key).is_ok()
}
