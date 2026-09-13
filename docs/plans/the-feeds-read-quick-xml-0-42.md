---
slug: the-feeds-read-quick-xml-0-42
worktree: /home/marvin/Projects/reprise-the-feeds-read-quick-xml-0-42
branch: feature/the-feeds-read-quick-xml-0-42
phase: refactored
codex_session:
created: 2026-09-13
---
# The feeds read quick-xml 0.42

Closes #885.

## Why

Dependabot PR #768 bumped `quick-xml` from 0.41 to 0.42 and `reprise-core`
stopped compiling. 0.42 is a genuine API change, not a patch: the reader now
hands out string-based events, `QName` wraps `&str` instead of `&[u8]`, the
`Reader::decoder()` accessor and the `decode()` methods on `BytesText`,
`BytesCData` and `BytesRef` are gone, and
`Attribute::decoded_and_normalized_value` no longer takes a decoder. The PR
was closed unmerged; `dev` still pins 0.41.

This branch performs the migration from current `dev`, keeps the parsing
behaviour identical, and regenerates the Flatpak sources.

## Facts to build on

Two files use quick-xml, both in `crates/reprise-core`:

- `src/podcasts/feed.rs` — `Reader::from_str`, `config_mut().trim_text`,
  `config_mut().check_end_names`, the `Event::{Start, Empty, Text, CData,
  GeneralRef, End}` match, `local_name(element.name().as_ref())`,
  `attributes()`, `BytesRef::resolve_char_ref`,
  `attribute.decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())`,
  and a `local_name` helper that takes `&[u8]`.
- `src/library/rhythmbox_import.rs` — `Reader::from_reader`, `read_event_into`,
  `element.name().as_ref() == b"entry"` comparisons, `attributes().flatten()`,
  `decoded_and_normalized_value(…, reader.decoder())`, `text.decode()` on
  Text / CData, `reference.decode()` on GeneralRef followed by
  `quick_xml::escape::unescape`, and the error mapping over
  `quick_xml::Error`, `quick_xml::encoding::EncodingError`,
  `quick_xml::escape::EscapeError`.

The compile errors PR #768 produced, for orientation:

- `E0277: can't compare str with [u8; 5]` — QName comparisons against byte
  literals (`b"entry"` etc.) become `&str` comparisons.
- `E0599: no method decoder on Reader` — the decoder is gone; 0.42 decodes at
  the input boundary.
- `E0599: no method decode on BytesText / BytesCData / BytesRef` — the content
  is already `&str` (via `Deref`); `unescape` is still a separate step where it
  was one before.
- `decoded_and_normalized_value` takes only the `XmlVersion` now.

Read the crate's own changelog before touching the code: after
`cargo update -p quick-xml` it is under
`~/.cargo/registry/src/*/quick-xml-0.42.*/Changelog.md`. In particular settle
how 0.42 handles a feed or Rhythmbox database declared in a non-UTF-8
encoding (`<?xml … encoding="ISO-8859-1"?>`): whether that needs the
`encoding` feature and whether `from_reader` still transcodes. 0.41 handled it
through the decoder; the migration must not silently regress it. If 0.42 needs
the `encoding` feature for that, enable it in `crates/reprise-core/Cargo.toml`.

## Tasks

### T1 — bump

`quick-xml = "0.42"` (plus any feature the changelog reading above requires)
in `crates/reprise-core/Cargo.toml`, `cargo update -p quick-xml`.

### T2 — migrate `podcasts/feed.rs`

Mechanical translation to the 0.42 API. `local_name` becomes a `&str`
function (or uses `QName::local_name()` directly). Text and CData content is
read as `&str`; keep exactly the same unescape / char-ref resolution path the
0.41 code had (a `GeneralRef` is still resolved through `resolve_char_ref`
and, failing that, the named-entity path the code already has). No behaviour
change: the existing feed tests are the contract.

### T3 — migrate `library/rhythmbox_import.rs`

Same translation. The error enum keeps its variants; if
`quick_xml::encoding::EncodingError` no longer exists or moved, map whatever
0.42 raises for an encoding failure onto the same public error the caller sees
today. The existing Rhythmbox import tests are the contract.

### T4 — the two tests that the migration makes necessary

Add a test only where 0.42 could have changed observable behaviour, and only
if no existing test already covers it (check first):

1. a feed whose `<title>` mixes a named entity, a numeric character reference
   and a CDATA section (`AT&amp;T &#8211; <![CDATA[Live & Loud]]>`), asserting
   the decoded title;
2. a Rhythmbox `rhythmdb.xml` entry whose `<location>` and `<title>` contain
   `&amp;` and a numeric character reference, asserting the imported values.

If 0.42 changed non-UTF-8 handling, a third test with an ISO-8859-1-declared
document containing a `ä` byte (0xE4) asserting the decoded `ä` — otherwise
state in the summary why it is unnecessary.

### T5 — Flatpak sources and gates

Regenerate `flatpak/cargo-sources.json`
(`flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json`, the
generator is on PATH and needs network), run
`scripts/check-flatpak-cargo-sources.sh`, then the full Rust gate (see
*Verification scope*).

Commits: T1–T3 as `The feeds read quick-xml 0.42`, T4 as `The migrated feeds
prove their entities`, T5 as `quick-xml 0.42 arrives with its Flatpak sources`.

## Out of scope

- Any other dependency (#883, #884 are separate branches).
- Refactoring the feed parser beyond what the API change forces.

## Parallelität

Not cut. Both migration files share the dependency bump and the lock; T4
depends on T2/T3.
