# Scrobbler Preferences Navigation — Implementation Plan

Design: `docs/superpowers/specs/2026-07-14-scrobbler-preferences-navigation-design.md`

## Global Constraints

- Follow RED → GREEN TDD and preserve provider isolation, explicit opt-in and
  secure keyring-only credential storage.
- Keep English code/comments/UI source strings/commits and German internal docs.
- Never access real accounts, credentials, music, user databases, desktop or session bus.
- Keep Core dependency-pure and every substantially edited file below 800 lines.
- Before committing run fmt, strict Clippy, workspace tests and audit; completion
  also requires Core purity, gettext, isolated GTK checks and adversarial review.

## Task 1 — Push stable ListenBrainz and Last.fm setup pages

**Files:** `crates/reprise-gnome/src/ui/preference_dependencies.rs`,
`crates/reprise-gnome/src/ui/preference_listenbrainz.rs`,
`crates/reprise-gnome/src/ui/preference_lastfm.rs`,
`crates/reprise-gnome/src/ui/preferences.rs`, `docs/agent-workflow/MANUAL-QA.md`,
design/plan/status.

**Interfaces:**

```rust
fn set_activation_pending(service: &adw::SwitchRow, pending: bool);
fn build_listenbrainz_page(connected: bool) -> ListenBrainzPageSurface;
fn build_lastfm_page(connected: bool) -> LastFmPageSurface;
fn push_listenbrainz_page(context: &Rc<PreferencesContext>, row: &adw::SwitchRow);
fn push_lastfm_page(context: &Rc<PreferencesContext>, row: &adw::SwitchRow);
```

1. RED: add a display regression requiring pending activation to retain active=true
   while insensitive, then restore sensitivity; replace both masked-row tests with
   detail-page regressions requiring native push/pop, masked inputs and disabled
   primary actions until all required fields are nonblank.
2. Run each display test in its own fully isolated Xvfb/DBus process and observe
   compile/assertion failure with the current dialog builders and switch rollback.
3. GREEN: build and push both `AdwNavigationPage`s through the existing Preferences
   navigation seam. Leave the requested switch on during keyring lookup/setup,
   prevent duplicate input while pending, and revert on page hiding only when the
   provider runtime is still inactive.
4. Reuse the unchanged validation/browser/keyring/enable/disconnect paths from page
   buttons. Parent Last.fm confirmation plus all provider errors to Preferences with
   safe main-window fallback.
5. Update Manual QA for stable toggles, same-window Back navigation and correct
   alert stacking. No new source strings are expected.
6. Run focused policy/display tests, gettext, full gates, Core purity and file-size
   proof. Adversarially review pending cleanup on every success/error/cancel path,
   RefCell discipline, page lifetimes, duplicate activation, provider isolation and
   absence of the former initial dialogs; fix findings and rerun affected gates.
7. Commit `fix: embed scrobbler setup in preferences` and release only the parallel
   coordination entry without modifying the active main-work lock.

Expected result: both scrobbler setup flows use stable switches and native second-
level Preferences pages, with all networking and secure storage behavior unchanged.
