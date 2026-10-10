---
slug: source-banner-copy
worktree: /home/marvin/Projects/reprise-source-banner-copy
branch: feature/source-banner-copy
phase: planned
codex_session:
created: 2026-10-10
---
# Source banners read as sentences (#1208)

Closes #1208. Grill decisions (2026-10-10): spelled-out age with minute/hour/day buckets; a new core headline variant; German translations filled, the other locales left empty.

## Problem

The cached-content banners on the Podcasts and YouTube views compose broken
sentences:

- "Showing downloaded content. Last checked Updated 2 min ago."
- "Showing the 1 episodes from Updated just now. Downloads play as usual."
- The Podcasts view's banner headline says "Couldn't check this channel for new
  uploads".

Causes on `origin/dev`:

- `updated_ago` (`crates/reprise-gnome/src/ui/podcasts/podcasts_presentation.rs:509`)
  returns a full label that starts with "Updated". `podcasts_failure_ui.rs:157-197`
  feeds that label into `SOURCE_OFFLINE_DESCRIPTION` and
  `SOURCE_CACHED_EPISODES_STILL_WORK` (`strings_sources.rs:42-51`) as if it were a
  bare time.
- `SOURCE_CACHED_EPISODES_STILL_WORK` has no plural form.
- `updated_ago` knows only minutes, so a cache that is days old reads
  "4320 min ago".
- Core maps both `SourceSurface::Podcast` and `SourceSurface::Youtube` banners to
  `FailureHeadline::CouldNotCheckChannel` (`reprise-core/src/source_error.rs:157-160`).
- The only copy test (`source_error_banner.rs:225`) uses the hand-written time
  "4 hours ago", so it never sees the real time phrase.

`docs/ux-rules.md` (NET-3, NET-3d) requires the cache's age to be visible, but it
pins no wording. This change needs no rule change.

## Tasks (test-first)

1. **The bare age phrase.** Add one GNOME helper that turns
   `(Option<i64> timestamp, now)` into a bare, plural-aware age phrase: "just now",
   "1 minute ago" / "N minutes ago", "1 hour ago" / "N hours ago",
   "1 day ago" / "N days ago". Use `strings::plural`, i.e. ngettext.
   - Rebuild `updated_ago` as "Updated {time}" on top of the helper, so the list
     label and the banners share one set of buckets.
   - Write the failing unit tests for the bucket edges first: 0, 59 s, 60 s,
     59 min, 60 min, 23 h, 24 h, a future timestamp, and `None`.
2. **Cached banner copy.**
   - Split `SOURCE_CACHED_EPISODES_STILL_WORK` into ngettext pairs, chosen by kind:
     - Podcasts: "Showing the episode from {time}." / "Showing the {count} episodes from {time}."
     - YouTube: "videos" instead of "episodes".
     - Both end with " Downloads play as usual."
   - `podcasts_failure_ui.rs` passes `self.kind` and the bare phrase.
   - `SOURCE_OFFLINE_DESCRIPTION` stays "Showing downloaded content. Last checked {time}."
     and now receives the bare phrase.
3. **Headline per kind.** Add `FailureHeadline::CouldNotCheckPodcast` in core and
   map `SourceSurface::Podcast` banners to it. `CouldNotCheckChannel` stays for
   YouTube. In GNOME:
   - add `SOURCE_COULD_NOT_CHECK_PODCAST` = "Couldn't check this podcast for new episodes";
   - update `headline_text`;
   - fix any exhaustive matches the compiler flags (the MCP DTO, the Android FFI).
   Update the core tests at `source_error.rs:536,576` to the new variant.
4. **Real-output test.** Write a GNOME unit test that feeds the actual helper output
   for 0 minutes, 2 minutes and 3 days through both banner strings. It asserts:
   - the full sentences, for both counts 1 and 2 and for both kinds;
   - that no string contains "Updated" twice or "Last checked Updated".
5. **Catalogs.** Run `scripts/update-catalogs.sh`. The changed msgids lose their old
   translations. Fill German (`po/de.po`) for every new or changed msgid, and
   leave the other locales untranslated, as earlier string changes did.

## Gates

`cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, the core purity grep, and
`scripts/tests/gettext-catalogs.sh`.

## Parallelität

The plan cannot be cut. Tasks 1, 2 and 4 all edit
`strings_sources.rs`/`strings_podcasts.rs` and `podcasts_failure_ui.rs`, and
task 5 regenerates catalogs from every string change. Splitting it would make
both strands write `po/*`. It runs as one strand.
