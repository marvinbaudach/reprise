---
slug: lastfm-preferences-one-primary-path
worktree: /home/marvin/Projects/reprise-lastfm-preferences-one-primary-path
branch: feature/lastfm-preferences-one-primary-path
phase: shipped
codex_session:
created: 2026-08-22
---
# Last.fm preferences: one primary path per build

## Why

PR #624 (merged 2026-08-22) gave the Flatpak build a bundled Last.fm
application credential, so `has_bundled_credentials()` is finally `true`
somewhere. That surfaced how the preferences row behaves in the two builds we
ship, and only one of them is right.

The row is built by `build_lastfm_expander`
(`crates/reprise-gnome/src/ui/preferences/preference_lastfm.rs:85-231`). It asks
`has_bundled_credentials()` once, at line 93, and branches on it at line 137.

### Defect 1 — the build without a bundled key hides its only path

When `bundled == false` no sign-in row is created at all (line 159,
`sign_in = None`). The only way to connect is the API-key form, and that form
lives inside `credentials_section` — an `adw::ExpanderRow` titled
**"Advanced setup"** (`LASTFM_ADVANCED_SETUP`, `strings_scrobbling.rs:67`) built
with `.expanded(false)` (lines 119-124) and appended at line 162.

So on Flathub, AUR, COPR and every self-build, the *only* way to enable
scrobbling is behind a collapsed row labelled "Advanced". A user who opens the
Last.fm expander sees no action they can take. This predates #624; #624 only
made it visible by giving the other branch something better.

### Defect 2 — the bundled arm has never been executed by a test

`build_lastfm_expander` reads `has_bundled_credentials()` internally instead of
taking it as an argument. The test binary compiles without `REPRISE_LASTFM_*`,
so both UI tests (`preference_lastfm_tests.rs:32` and `:65`) can only ever run
the `bundled == false` arm.

The consequence sits in `build_lastfm_row`, at lines 351-355 — note the
location: the `unwrap()`s are *not* inside `build_lastfm_expander`, they are in
the handler that `build_lastfm_row` attaches to the button that
`build_lastfm_expander` returned:

```rust
let key = reprise_core::scrobbling::BUNDLED_API_KEY.unwrap().to_string();
let sec = reprise_core::scrobbling::BUNDLED_SHARED_SECRET.unwrap().to_string();
```

Two `unwrap()`s on `Option`s, reachable only because the button that owns the
handler is created only when both are `Some` — and that invariant is enforced in
a *different function* from the one that relies on it. It is real today and
pinned by nothing.

### What is NOT a defect

The bundled arm showing both a "Sign in" button and a collapsed "Advanced setup"
is **specified**, not accidental. `docs/ux-rules.md:1126`, rule **SET-6b**
[active]:

> With bundled app credentials, Last.fm offers the normal browser login
> directly; custom API credentials sit collapsed under "Advanced setup".

`set_6b_lastfm_application_credentials_are_hidden_in_advanced_setup`
(`preference_lastfm_tests.rs:65`) pins exactly that. The plan keeps it.

The owner's constraint from 2026-08-22 points the same way: the BYO path must
stay reachable, because for builds without a bundled key it is the only way. No
feature gets disabled anywhere.

## The rule this plan applies

**The build's credential situation decides which path is primary. The other one
stays reachable.**

Today that rule is honoured in the bundled build and violated in the other.
SET-6b states the bundled half and is silent on the other half; the rule gets
its missing sentence.

## Tasks

### Task 1 — give `build_lastfm_expander` a seam, and make it carry values

Change the signature to take the bundled credential rather than a bool:

```rust
fn build_lastfm_expander(
    is_enabled: bool,
    connected: bool,
    status: &str,
    bundled: Option<(&str, &str)>,
) -> LastFmExpanderSurface
```

`build_lastfm_row` passes
`reprise_core::scrobbling::BUNDLED_API_KEY.zip(reprise_core::scrobbling::BUNDLED_SHARED_SECRET)`.

`LastFmExpanderSurface.sign_in` changes from `Option<gtk4::Button>` to
`Option<LastFmSignIn>`:

```rust
struct LastFmSignIn {
    button: gtk4::Button,
    api_key: String,
    shared_secret: String,
}
```

The button and the credentials it signs in with are then **one value**. The
handler in `build_lastfm_row` closes over `sign_in.api_key.clone()` and
`sign_in.shared_secret.clone()` instead of reading the constants, and both
`unwrap()`s disappear — not because a check was added, but because the absent
case is no longer representable.

The rejected alternative: wiring the click handler inside
`build_lastfm_expander`. It needs `Rc<PreferencesContext>`, which
`build_lastfm_expander` deliberately does not have — that is exactly what makes
it callable from a test. Keep it a free function.

**`has_bundled_credentials()` is deleted, not kept.** The draft of this plan
claimed `client_for` and others call it; they do not. `client_for` (line 268)
goes through `LastFmClient::bundled()`. Line 93 is its only call site in the
tree, so Task 1 orphans it and it must go in the same commit.

### Task 2 — the non-bundled build leads with the form

When `bundled.is_none()`, `credentials_section` is the primary and only path:

- built with `.expanded(true)`,
- titled `LASTFM_OWN_APPLICATION` instead of `LASTFM_ADVANCED_SETUP` — it is not
  advanced when it is the only option.

It stays a **nested `adw::ExpanderRow`** in both arms. Flattening the entries
into the outer expander would read slightly better but makes the two arms
structurally different and breaks the `api_key.ancestor(ExpanderRow)` lookup that
both existing tests use to find the section. Same shape, different title and
different initial state, is the smaller and better-pinned change.

The hint row (`LASTFM_DIALOG_BODY`) and the browser row stay inside the section,
unchanged, in both arms.

When `bundled.is_some()`, nothing changes: hint row, sign-in row, then
`credentials_section` collapsed and still titled "Advanced setup", exactly as
SET-6b requires and as the existing test asserts.

### Task 3 — strings

`LASTFM_ADVANCED_SETUP` / `LASTFM_ADVANCED_SETUP_DESCRIPTION` stay for the
bundled arm. Add to `crates/reprise-gnome/src/ui/strings_scrobbling.rs`,
through `N_!` like their neighbours:

- `LASTFM_OWN_APPLICATION` — `"Last.fm application"`
- `LASTFM_OWN_APPLICATION_DESCRIPTION` — states that this build carries no
  application key and names <https://www.last.fm/api/account/create> as where to
  register one.

`LASTFM_DIALOG_BODY` carries no URL today, so the address is new information
rather than a duplicate. No string is deleted, so no translation regresses.

### Task 4 — amend SET-6b

`docs/ux-rules.md:1126` gets the missing sentence: without bundled app
credentials the application form is the primary path — expanded, and not
labelled "Advanced". Removing behaviour without updating its rule is how a rule
becomes a lie; adding behaviour is the same.

SET-6b stays `[active]` and keeps its ID — this is an amendment to a rule about
the same surface, not a new rule, so the append-only ID contract in
`AGENTS.md:67-81` is untouched. What that contract *does* require is that the
new half has a rule-named test: every test added in Task 5 that pins the rule
carries the `set_6b_` prefix, and `scripts/check-ux-traceability.sh` must pass.

### Task 5 — tests

Extend `preference_lastfm_tests.rs`. All UI tests here carry
`#[ignore = "requires a display; run via xvfb-run"]` and that stays.

1. `set_6b_lastfm_own_application_is_the_primary_path_without_bundled_credentials`
   — `bundled == None`: `credentials_section.is_expanded()`, its title is
   `LASTFM_OWN_APPLICATION` and *not* `LASTFM_ADVANCED_SETUP`, and
   `surface.sign_in.is_none()`.
2. `set_6b_lastfm_sign_in_is_offered_with_bundled_credentials` —
   `bundled == Some(("k", "s"))`: `surface.sign_in.is_some()`, and SET-6b still
   holds — `credentials_section` present, collapsed, titled
   `LASTFM_ADVANCED_SETUP`. **This is the first test that ever enters that
   branch.**
3. The existing
   `set_6b_lastfm_application_credentials_are_hidden_in_advanced_setup` keeps
   asserting what it asserts, but is called with `Some(("k", "s"))` so it tests
   the arm the rule is about. Today it silently tests the other one.
4. `lastfm_sign_in_carries_the_credentials_it_was_built_from` — asserts
   `surface.sign_in` reports exactly `("k", "s")`. This is the test that pins
   what Task 1 bought: the handler can no longer reach for the constants.

`expander_row_has_enable_switch_credentials_and_action_buttons` needs its two
call sites updated for the new parameter (`None`); its assertions are
unaffected.

## Verification

- `cargo test -p reprise-gnome preference_lastfm` for the non-display tests.
- The display tests through the project's xvfb path, both arms. **A run that
  reports 0 tests is a failed run, not a pass** — assert the count, and remember
  that `--exact` / `--lib` filter combinations here silently run nothing.
- Read the verdict from the command itself, never through a pipe — `… | tail`
  reports `tail`'s exit status, which is always `0`.
- `scripts/check-ux-traceability.sh` — Task 4 changes an `[active]` rule.
- Control arm, and it is the only evidence that the *shipped* behaviour differs.
  `option_env!` resolves at compile time, so this means two builds: one plain,
  and one with `REPRISE_LASTFM_API_KEY` and `REPRISE_LASTFM_SHARED_SECRET` set to
  dummy values. Open the Last.fm preferences in each and keep the screenshot
  pair. Run this once at the end, not per iteration — the unit tests cover both
  arms cheaply now, which is the whole point of Task 1.
- `has_bundled_credentials()` is gone after Task 1; if anything still references
  it, the deletion was incomplete.

## Out of scope

- **The third derivation of "is this bundled", at line 589.** Inside the token
  exchange, `is_bundled` compares the *values* it was handed against the
  constants, to decide whether to blank them before storing. That is a storage
  question, not a primary-path question, and the value comparison is the right
  mechanism for it — it also catches a user who typed the bundled key into the
  BYO form. Named here so it is not mistaken for an oversight.
- **Splitting the file.** The draft proposed this against a "400-line house
  rule". `AGENTS.md:180` sets the limit at **< 800 lines**; at 739 (plus ~35 from
  this plan) the file is compliant, and eleven files in `preferences/` are over
  400 — `preferences_window.rs` at 790 and `preferences.rs` at 781 are both
  closer to the real limit. A split here would be a discretionary refactor
  justified by a rule that does not exist, and it would make Tasks 1-3 read as a
  rewrite. If the split is wanted, it is its own plan and it looks at those two
  files too.
- `preference_listenbrainz.rs` (709 lines), for the same reason.
- The Ticketmaster and AcoustID credential gaps (#625 and its follow-up).
- Whether Flathub should carry credentials at all. It cannot, by construction,
  and this plan makes that build behave well rather than arguing with it.
- The toast markup bug found the same day; separate plan.

## Parallelität

**No cut. One strand.**

Tasks 1, 2 and 5 all edit
`crates/reprise-gnome/src/ui/preferences/preference_lastfm.rs` and its test file.
There is no disjoint file group, and the file ownership of any two strands would
intersect on the first task.

Tasks 3 (`strings_scrobbling.rs`) and 4 (`docs/ux-rules.md`) do own distinct
files and could in principle run apart, but each is a handful of lines and
neither is on the critical path — a strand per file here buys nothing in
wall-clock and costs a worktree and a merge.

Sequence within the single strand: 1 → 2 → 3 → 5 → 4.

Post-merge cross-checks: none. Nothing in this plan is verified against a file
another strand would write.
