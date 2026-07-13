# Minimal View + Native Preferences — Implementation Plan

## Global constraints

TDD RED→GREEN; English code/UI, German design docs; no real user data; no fake
settings; core purity; every touched file <800 lines; full gates before every commit.

## Task 1 — Typed persistent preferences

Add typed theme, density, sidebar/status visibility, minimal geometry, ReplayGain and
EQ setting accessors plus fallback/round-trip tests in core.
Commit: `feat: add typed appearance and playback settings`.

## Task 2 — Single-state minimal window mode

Add `minimal_view.rs`, pure transitions and display smoke. Reparent the existing
player bar, restore normal geometry/maximize state, expose a menu action and shortcut.
Commit: `feat: add compact minimal player view`.

## Task 3 — Appearance and layout preferences

Add `preferences.rs` with real theme, bar position, sidebar/status, density, minimal
mode and column-editor controls. Persist and apply immediately through callbacks.
Commit: `feat: add appearance and layout preferences`.

## Task 4 — Library and plugin preferences

Add library-root/actions and module rows generated from `ALL_MODULES`; reuse existing
scan, Rhythmbox import and cover-download actions. Persist MPRIS with explicit restart
copy. Commit: `feat: add library and plugin preferences`.

## Task 5 — Equalizer and ReplayGain playback backend

Extend the core contract with immutable audio-effects configuration. Build/test the
Linux GStreamer filter bin, preserve it through pipeline recovery, and degrade safely
when elements are unavailable. Commit: `feat: add equalizer and ReplayGain backend`.

## Task 6 — Playback preferences UI

Add enable, ten bands, presets and ReplayGain rows; live-apply through the controller
and persist. German translations complete. Commit: `feat: add playback effects preferences`.

## Task 7 — Close-out

Run release/debug tests, all display tests, isolated minimal/preferences/effects
smokes and release checker; adversarial review; update README, release/manual QA,
ledger and STATUS. Commit docs and release lock separately.

