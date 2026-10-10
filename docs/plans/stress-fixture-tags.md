---
slug: stress-fixture-tags
worktree: /home/marvin/Projects/reprise-stress-fixture-tags
branch: feature/stress-fixture-tags
phase: reviewed
codex_session:
created: 2026-10-10
---
# Stress fixture audio carries real tags

Today the `large-library-stress` deck mission writes its writable fixture tags
only into the disposable database. `scripts/cua-explore/fixtures.py`
`_write_disposable_tracks` (around line 161) copies the committed
`crates/reprise-core/tests/fixtures/sine.flac` verbatim. The FLAC carries no
title, artist, album, album artist, genre or year.

After `batch-edit` writes the 512 files, the app re-reads them. The rows then come
back with an empty Artist and Album. The list is sorted by Artist, so it
reorders, and `scroll_anchor_restored` cannot hold. This is the known gap in
`scripts/cua-explore/README.md` under "Known gaps on cua-driver 0.33 and 0.34",
and it is not an app defect.

## Tasks (test-first)

1. **Tag each copy.** In `_write_disposable_tracks`, write the same values the
   DB `UPDATE` uses into each copied FLAC:
   - TITLE, ARTIST, ALBUM, ALBUMARTIST, GENRE, DATE (the year) and TRACKNUMBER.
   - Use `metaflac --remove-all-tags --set-tag=…` (in place, no re-encode).
     `metaflac` is at `/usr/bin/metaflac`. If the launcher has a preflight list of
     required binaries, add `metaflac` to it. A missing `metaflac` raises
     `FixtureError` with a clear message and does not silently produce
     untagged files.
   - Build the values once and feed both the DB update and the tag write from
     them, so the two cannot drift.
   - Also set the DB duration column of these rows to the real duration of
     `sine.flac`, so a re-read does not change the Length column. Look up the
     actual column name in the schema.
2. **Manifest and audit.** `writable_audio_sha256` was one hash, of the untagged
   source. Every copy now differs from the source, so `audit_batch_edit` would
   count every file as changed before any edit.
   - Replace that key with a per-file map, `writable_audio_sha256_by_file`
     (file name → sha256 after tagging).
   - `audit_batch_edit` counts a file as changed when its hash differs from its
     own baseline.
   - Remove the old key outright; there is no compatibility reader (AGENTS.md:
     not released).
3. **Tests.** Extend the existing deck tests for the fixture and audit (find them;
   the README names `cua-explore-fixture-integrity.py`, and search
   `scripts/tests/` for `fixtures`). Each test must fail before the change:
   - Tag round trip: after `prepare_profile` (or `_write_disposable_tracks` on a
     scratch root), `metaflac --export-tags-to=-` on a copy returns the same
     artist, album and genre as its DB row.
   - The audit reports `audio_files_changed == 0` on a fresh profile and `== N`
     after rewriting N files.
4. **README.** Update the `batch-edit` bullet in "Known gaps" and the "Isolation
   and test data" paragraph: the copies are now tagged, so the empty-Artist reorder
   is gone. Do not claim that the mission completes; it has not been rerun.

## Scope

Only `scripts/cua-explore/**`, the matching `scripts/tests/*` deck tests, and a
launcher preflight if one exists. No Rust changes.

## Gates

The deck's Python test suite: find how `scripts/check-merge-readiness.sh` invokes
the `cua-explore` tests and run that command. Run `cargo` gates only if a Rust file
changed (none should).

## Parallelität

Single strand, one file group.
