---
slug: toast-titles-are-parsed-as-markup
worktree: /home/marvin/Projects/reprise-toast-titles-are-parsed-as-markup
branch: feature/toast-titles-are-parsed-as-markup
phase: review
codex_session:
created: 2026-08-23
---
# No toast without its message — implementation plan

> **For agentic workers:** Implement this plan task by task with test-driven
> development. The checkboxes record the execution state.

**Goal:** A toast never appears without text. Closes #639.

**Architecture:** `AdwToast:use-markup` defaults to `TRUE`, while every caller
in this app passes plain text into that markup slot. A bare `&`, `<` or `>`
makes Pango reject the markup, GTK logs a warning and the label remains empty:
the pill and its button appear without the message. The repair belongs at the
construction point rather than at each caller: one shared constructor in
`ui::toasts` sets `use_markup(false)`, and a gate prevents future direct
construction elsewhere.

**Tech stack:** Rust, gtk4-rs/libadwaita (`libadwaita = "0.9.2"`, feature
`v1_9`; `set_use_markup` has been available since 1.4).

Read against `origin/dev` at `f4158cc10f`. Every original line reference came
from that revision; a missing reference means the base differs.

## Measured finding

Three deterministic headless arms used a real `AdwToast` in a real
`AdwToastOverlay`, then walked the rendered widget tree for label text:

| Arm | Title | Rendered label |
| --- | --- | --- |
| A | raw `&`, libadwaita default | **`[]` — no label** |
| B | markup-escaped | `Removed "… Gaming, Study  & Stress Relief Mix"` |
| C | raw `&`, `set_use_markup(false)` | `Removed "… Gaming, Study  & Stress Relief Mix"` |

Arm A logs `Gtk-WARNING: Failed to set text '…' from markup due to error
parsing markup: … escape ampersand as &amp;`.

**Why arm C, not arm B.** Escaping repairs one string at a time and moves the
obligation to every future caller. `use_markup(false)` removes the unwanted
markup semantics from the slot. No Reprise toast wants markup. The only `<b>`
strings (`strings_concerts.rs:87`, `strings_filter.rs:23`,
`strings_releases.rs:54`, `strings_podcasts.rs:422` and `:431`, and
`strings_radio.rs:108`) are filter-bar count lines and do not reach a toast.
This must be verified before implementation rather than assumed; stop and
report if one of those functions reaches a toast.

## Global constraints

- There is one construction path. After this plan, `adw::Toast::new` and
  `libadwaita::Toast::new` appear only in
  `crates/reprise-gnome/src/ui/toasts.rs`.
- No new user-visible string. No `msgid` is added, `po/` is not touched and the
  Gettext gate remains unchanged. Stop if a new string becomes necessary.
- All other behavior remains unchanged: timeouts, button labels, priority,
  `connect_dismissed`, `connect_button_clicked`, and toast displacement order.
- Code files remain below 800 lines.
- Repository content, including code, comments, test names and commit messages,
  is English.

## Task 1 — shared constructor

- [x] Add `pub(super) fn plain(text: &str) -> adw::Toast` to
      `crates/reprise-gnome/src/ui/toasts.rs`. It returns an unattached toast so
      callers can keep configuring buttons, timeouts, priority and signals.
- [x] Build it with `adw::Toast::new(text)` and immediately call
      `set_use_markup(false)`. Its one-sentence documentation names the
      plain-text disappearance and FB-11, not this plan.
- [x] Route `show` and `show_with_action` through `plain`.

## Task 2 — route every caller

- [x] Re-scan all Rust sources for `Toast::new` and `Toast::builder`, then route
      every result through `toasts::plain`, including any result not listed in
      the original finding.
- [x] Keep `plain` at `pub(super)` visibility and use
      `crate::ui::toasts::plain` from deeper modules. Do not make it public.
- [x] Carry no incidental behavior fix while changing construction calls.

The original finding covered direct calls in playback, scan, compact mode,
podcasts, radio, responsive window and track-list code, Library Doctor,
Missing files and tag editing. The list was a finding, not an allowlist; the
fresh repository scan remains authoritative.

## Task 3 — regression guard

- [x] Add a hard check to `scripts/check-gnome-idioms.sh`: any `Toast::new` or
      `Toast::builder` under `crates/reprise-gnome/src` outside
      `ui/toasts.rs` fails the gate and names the file and reason.
- [x] Name FB-11 and `toasts::plain` in the diagnostic so a future violation is
      directly actionable.

## Task 4 — behavior test

- [x] Add a rule-named `fb_11_` test with
      `#[ignore = "requires a display; run via xvfb-run"]` so the rule-named
      display runner discovers it.
- [x] Build a toast through `toasts::plain` using a title containing a bare `&`
      and `<`, add it to a real `adw::ToastOverlay`, drive the main loop until
      drawing and walk the widget tree for `GtkLabel` text. Require the title
      literally, with both characters unchanged.
- [x] In the same test, render a second toast with the same title after forcing
      `set_use_markup(true)` and require that it has no non-empty label. This
      control proves the assertion can fail and that the tree walk is live.

## Task 5 — UX rule

- [x] Add FB-11 after FB-9 in section G, leaving FB-10 reserved for PR #636.
      Stop rather than silently renumber if FB-11 already exists.
- [x] Mark it `[active] [gtk]` because behavior and coverage land together.
- [x] State that every toast carries its message, toast text is plain text and
      library data, errors, URLs and translations may contain `&`, `<` or `>`.
      Treat discarded text as a defect because the action and Undo lose their
      subject.
- [x] Name the measured 2026-08-23 YouTube-title trigger in one clause.

## Acceptance

- [x] A fresh `grep` for `Toast::new` and `Toast::builder` under `crates/`
      reports only `ui/toasts.rs`.
- [x] `scripts/check-gnome-idioms.sh` exits successfully, and an intentional
      direct constructor restored at another call site makes it fail with the
      FB-11 diagnostic before the mutation is removed.
- [x] The focused `fb_11_` display test passes.
- [x] Removing `set_use_markup(false)` from `toasts::plain` makes the `fb_11_`
      test fail; restoring it makes the test pass again. Record both outputs.
- [x] `scripts/check-display-tests.sh --rule-named` passes with FB-11 included.
- [ ] `scripts/check-ux-traceability.sh` passes with FB-11 active and covered.
- [ ] In the running app, remove a YouTube episode whose title contains `&` and
      verify that the toast shows both its text and Undo; attach a screenshot
      to the pull request.
- [ ] Run `MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh`.
      Record every skipped or unavailable stage instead of calling it green.

## Out of scope

- No timeout, displacement or Undo-duration change; those belong to FB-1.
- No change to FB-10 or PR #636.
- No truncation of long toast titles. Record a visually awkward long YouTube
  title as a follow-up rather than changing it here.

## Parallelism

There is no useful split. The five tasks share one file chain: `ui/toasts.rs`
provides the constructor, Task 2 consumes it, Task 3 makes it exclusive and
Task 4 measures it. Task 5 is small and its active status must land with the
test. A second worktree would either edit `toasts.rs` concurrently or wait for
it.
