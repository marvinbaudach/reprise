# What you listened to on the train

The desktop → phone direction is finished and proven on hardware: selection,
audio, analyses, ratings and play counts all travel outward, and the phone shows
them. Nothing comes back. Every play on the phone, every heart pressed on the
train, ends there — while "Recently played", "Top rated", My Stats and every
smart playlist on the desktop quietly pretend that time did not happen.

This is the return direction. It was deliberately deferred in July ("one
direction is enough to start with"); that start is now done.

## What was decided

- **Both play counts and ratings** come back — mirroring what the outbound list
  already carries.
- **Plays arrive as individual listen events with their original timestamp**,
  not as a bumped counter. The desktop has `listen_events(track_id, played_at,
  ms_played)` and My Stats is built on it; a play that arrives without its
  moment lands in the wrong week forever.
- **The newer change wins** when both sides changed the same rating.
- **Read on every sync**, including the automatic one on connect — it is a small
  file, and the automatic run is the common one.
- **The desktop acknowledges, the phone prunes.** Nothing is discarded until the
  desktop has said what it applied, so an interrupted run costs nothing and
  counts nothing twice.

## What is already there

- `reprise-track-metadata.rpl` (`RPT-LIST`) at the sync root carries rating and
  play count outward, keyed by **device-relative path**. That identity is the
  right one here too, and its module says why: "database row ids are not".
- The phone already counts plays: `play_recorder` writes at the half-played
  mark on its own writer thread, and `play_journal` is a file-backed queue with
  sequence numbers and an applied high-water mark.
- `device_settings.ratings_back` exists as a column but is forced to `false`
  when settings are loaded — a placeholder from before this was designed.

## Slice A — Core: the format, the schema, and applying a report

**Schema.** `tracks.rating` carries no timestamp, so "newer wins" has nothing to
compare. Add `rated_at` (nullable, unix seconds) in a new migration; bump
`SUPPORTED_SCHEMA_VERSION` from 62. Both sides use this same core schema, so one
migration serves desktop and phone. Every rating write in core sets it.

Existing ratings keep `rated_at = NULL`, and that is the honest value: nobody
knows when they were set. NULL therefore loses to any timestamp — which is
correct, because the phone only ever reports a rating it changed itself, so a
reported rating always carries one.

**The report format.** A second versioned file at the sync root, written by the
phone and read by the desktop. Magic `RPT-BACK`, a `u16` version, then two
counted sections — listens and rating changes — each entry keyed by
device-relative path and carrying a monotone `u64` sequence number:

- listen: sequence, path, `played_at` (unix seconds), `ms_played`
- rating: sequence, path, `rating`, `rated_at` (unix seconds)

Decode must reject a wrong magic, an unknown version, and a truncated body
without panicking or allocating on an attacker-chosen length — the same
discipline `analysis_sidecar` and `track_metadata_list` already follow. Reuse
their shape rather than inventing a third dialect.

**The acknowledgement.** A third small file the desktop writes: the highest
sequence number it has fully applied. One number, because the sequence covers
both sections.

**Applying, and the two rules that must not drift.** Given a decoded report and
a database, apply it once:

- A listen becomes a `listen_events` row at its original `played_at`, and bumps
  `tracks.play_count` and `last_played_at` the way a local play does. Deciding
  "what a play does to a track" in two places is how this project has shipped
  audible bugs twice; find where a local play already does it and go through
  that, or extract it so both go through one.
- A rating is written only when the report's `rated_at` is newer than the row's.
  Equal timestamps keep the desktop's value: a tie is not a change.
- Applying the same report twice must be a no-op. The sequence number is the
  only thing that decides this — not "does a similar listen_event already
  exist".
- A path the desktop cannot resolve to a track is skipped and counted, never
  fatal. A phone can hold a file the desktop deleted last week.

Slice A is pure core: format, migration, apply. No GTK, no FFI.

## Slice B — The phone: recording what to send, and writing it

The phone needs an **export journal** with its own sequence and its own applied
high-water mark. Do not reuse `play_journal`'s mark: that one answers "did my
own database get this play", this one answers "did the desktop get it". One
number answering two questions is exactly the bug this project keeps finding.

Rating changes on the phone are journalled the same way, in the same sequence.

**Writing the file is the part that does not exist yet.** `SafSource` is
read-only — `open_read_fd` and nothing else. Follow the app's existing division:
Rust produces the bytes, Kotlin writes them into the sync folder through the
tree it already holds. Rust must not learn SAF.

On the phone's side the acknowledgement is only read: everything at or below the
acknowledged sequence is pruned, everything above stays for the next run. A
missing or unreadable acknowledgement means "nothing acknowledged" — never
"everything acknowledged".

## Slice C — The desktop run

Read the report during every synchronization, before the outbound metadata list
is written, so a play that came back is already counted in what goes out.

Apply it, then write the acknowledgement. Count what happened in the run log
that now exists: how many listens and how many ratings were applied, and how
many paths could not be resolved. `MTP-20`'s deviations are the right home for
the unresolved ones.

`ratings_back` stops being a lie: either it becomes the switch that governs the
rating half of this, or it goes. Decide which, and say which in the summary — a
column that is read into `_ratings_back` and forced to `false` must not survive
this package in that state.

## Proof

Each slice mutation-proven. Beyond the per-rule tests, three that describe the
package as a whole:

1. A report applied twice changes the database exactly once.
2. A rating changed on both sides since the last sync ends up with the newer
   one — proven in both directions, not just the phone-wins one.
3. A listen for a path the desktop does not know is skipped, counted, and does
   not stop the rest of the report from applying.

## Not in this package

Anything the phone did to playlists or the queue. This is listening history and
ratings only.
