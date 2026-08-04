# Sound profile and agent-capable playlist planning — Design

Status: ready for joint review

Branch: `feat/audio-character-mcp`

Base: `35045a33` (`main`, 2026-07-19)

## 1. Goal

Reprise analyzes music files exclusively locally and forms an explainable
**sound profile** from measurable audio signals. This profile provides the
user with interesting, cautiously worded information and feeds a single,
deterministic mix planner. The same planner is used by the native GTK surface
and later by a local MCP server.

The central product flow reads:

```text
Audiodatei (read-only)
    -> versionierte Audio evidence
    -> versioniertes Sound profile
    -> strukturierte Mix intent
    -> unveränderlicher Mix draft + Selection reasons
    -> explizite Draft approval
    -> manuelle Playlist
```

The feature does not claim to know an objective emotion of the track or of the
user. In the first stage the UI therefore uses "Audio Character" /
"Klangprofil" and measurement-near dimensions, not "happy", "sad", or a single
mood category.

## 2. Current state of Reprise

The plan builds on existing, verified behavior:

- `reprise-core::waveform::WaveformBackend` is already the platform-neutral
  seam for waveform extraction.
- `reprise-platform-linux::waveform::GstreamerWaveformBackend` decodes audio
  today via `gst-launch-1.0`, collects the complete 8 kHz mono signal in
  memory, and computes normalized RMS peaks.
- A startup/post-scan backfill starts up to four waveform workers. Errors are
  only counted; a durable state, pause, cancel, or backoff does not exist.
- `tracks.waveform_peaks` stores 1,000 `u8` peaks directly on the track.
- The smart playlist rules are a validated field whitelist, joined exclusively
  by `AND`. They can express ranges, but neither alternatives nor similarity
  distance, diversity, or a dramatic arc.
- `playlists::create_with_tracks` already creates a manual playlist together
  with its tracks atomically. This operation is the later persistence sink of
  an approved mix draft.
- The README already plans for a narrow MCP adapter over core contracts:
  explicit capabilities, read-only by default, no path or credential leaks.
- `My Stats` computes exclusively from local `listen_events`. Future sound
  profile statements must use the same event set and state their analysis
  coverage.

## 3. Binding product decisions

### D1 — "Sound profile" is the product term

The German UI uses "Klangprofil", the English UI "Audio Character".
"Atmosphere" may appear as a cautious interpretation, but not as stored truth.
"Mood" is neither a table term nor a main UI term.

### D2 — Continuous dimensions instead of exclusive labels

Stage 1A provides exactly these normalized dimensions (`0.0..=1.0`):

- **Intensity** — calm to intense;
- **Brightness** — dark to bright;
- **Dynamicity** — even/compressed to strongly dynamic;
- **Rhythmicity** — flowing to strongly pulse-/onset-driven.

In addition, measurement-near values are available:

- tempo in BPM plus tempo confidence;
- loudness or rather energy distribution;
- dynamic range;
- spectral centroid and roll-off;
- spectral flux and onset rate.

Valence, happiness, sadness, aggressiveness, acousticness, instrumentalness,
and free atmosphere words are not part of Stage 1A. They require a separately
licensed and evaluated semantic model.

### D3 — Measurement and projection are versioned separately

Audio evidence preserves the stable measured values. The sound profile is a
deterministic projection of them. If only the normalization or weighting
changes, Reprise can compute the profile without decoding again. If the
extractor changes, decoding is redone.

Every result states at least:

- `extractor_version`;
- `profile_version`;
- source identity from track ID, `file_mtime`, and `file_size`;
- `analyzed_at`;
- confidence or rather availability for each uncertain measured value.

A path change alone does not invalidate the analysis. A changed file size,
mtime, or extractor version does. Missing tracks keep their cache but never
appear as mix candidates.

### D4 — Local, read-only, and without model download

Stage 1A reads audio files, never writes them, and needs no network. It
downloads no model and transmits neither audio nor features. The feature is a
local library setting, not an entry on the plugins page reserved for external
integrations.

### D5 — Analysis is explicitly enabled and controllable

A new installation does not analyze the entire library unasked. The setting
"Analyze audio locally" enables the feature. After that:

- exactly one analysis worker by default;
- progress with done/total/failed;
- pause, resume, and cancel;
- resumption after a restart;
- existing playback, scan, and Android transcoding stay responsive;
- no analysis inside the atomic scan transaction;
- errors are persisted in typed form and not retried endlessly on every
  start;
- a deliberate "Retry failed" action resets the backoff;
- disabling stops new work but does not delete existing profiles.

The switch concerns audio evidence and the sound profile only. The already
shipped waveform remains an unconditional player feature: missing peaks
continue to be generated even with the sound profile switched off. If the
sound profile is active and both results are missing, the worker coordinates a
shared decode pass instead of starting two competing backfills.

### D6 — Streaming instead of complete PCM in memory

The Linux adapter decodes PCM in bounded blocks via a native GStreamer
pipeline. A complete track must not sit in memory as a `Vec<i16>`. The
existing waveform peaks and the new audio evidence are generated in the
background in one decode pass.

The existing on-demand waveform feature is retained as an independent
capability. The Linux adapter may internally use the same bounded decoder; a
failure of the sound profile must not prevent the player bar's waveform. A
disabled sound profile must likewise not prevent the waveform.

### D7 — One deep core planner belongs to the UI and agents jointly

The external seam of the new core module consists of few operations:

```rust
plan_mix(conn, intent) -> MixDraft
approve_mix_draft(conn, draft_id, playlist_name) -> PlaylistCommit
```

SQL, scale normalization, candidate selection, distance, diversity, duration
filling, draft persistence, and conflict checking remain implementation of the
module. GTK and MCP must not rebuild these rules.

### D8 — The agent translates language, Reprise decides music

In stages 1A, 1B, and 2, Reprise contains no LLM and parses no natural
language.
An agent translates, for example, "calm, dark music for a night train ride"
into a structured mix intent. Reprise validates this intent and delivers a
deterministic selection.

That preserves testability and makes the same result possible from GTK
sliders, an MCP client, or a future native platform.

### D9 — Hard conditions and soft wishes stay distinct

A mix intent contains:

**Hard conditions**

- optional source set (entire library, playlist, artist, album, or explicit
  track set);
- desired maximum count or rather target duration;
- required up-to-date analysis;
- minimum analysis confidence;
- excluded track, artist, or album identities;
- only present, not removed tracks.

**Soft wishes**

- target and weight per sound profile dimension;
- tempo target and weight;
- Familiarity (`familiar`, `balanced`, `discover`);
- Variety (`cohesive`, `balanced`, `wide`);
- optional energy curve (`flat`, `rise`, `fall`, `arc`).

Unknown fields, values outside their ranges, and contradictory hard conditions
are errors. They are never silently normalized.

### D10 — The mix draft is explainable, but contains no chain of thought

A mix draft contains:

- a stable `draft_id`;
- the normalized mix intent;
- source snapshot, extractor and profile version;
- ordered track IDs with displayable metadata, never file paths;
- total count and total duration;
- analysis coverage of the population considered;
- structured selection reasons per track;
- warnings such as "only 43 of 80 eligible tracks are analyzed";
- expiry time and status `current` or `stale`.

Selection reasons are short data such as `brightness_match`, `tempo_match`,
`artist_gap`, or `duration_fit`, not free internal trains of thought.

### D11 — Selection is deterministic and diverse

With an identical candidate/source snapshot, an identical mix intent, and an
identical seed, the same order results. The planner:

1. applies hard conditions in SQL;
2. computes the weighted distance to the target profile;
3. sorts stably by distance, then track ID;
4. selects greedily with a diversity penalty;
5. prevents duplicate tracks;
6. keeps by default at least four positions of distance between the same
   artist, provided the candidate set allows it;
7. fills the target duration up to the smaller deviation; an overshoot is at
   most the duration of one final track;
8. applies the desired energy curve only to the selected set, without
   subsequently changing membership.

If a hard condition cannot be satisfied, no draft is created. If only soft
wishes or the duration cannot be fully satisfied, a partial draft with a
machine-readable warning is created.

### D12 — Unanalyzed tracks are not invented

A sound profile mix contains only tracks with an up-to-date analysis. Reprise
does not silently fill a gap with genre, rating, or random tracks. The user or
agent can start the analysis or request an explicitly different,
metadata-based selection; that is a different mix intent.

### D13 — Draft and persistence are separate operations

`plan_mix` changes no playlist, queue, playback, or file. A
draft approval creates exactly one new manual playlist atomically. It changes
no existing playlist.

Approval is idempotent: the same draft plus the same idempotency key delivers
the same result. A draft that has expired, has already been approved, or has
become outdated in its selected tracks or hard conditions is not silently
replanned. A new analysis or a metadata change on an uninvolved track
explicitly does not make it stale. The caller only has to replan on a change
relevant to exactly this draft.

### D14 — GTK always shows the draft before saving

The native mix builder offers profile targets, duration, Variety, Familiarity,
and energy curve. Before "Save as Playlist" it shows track order, duration,
analysis coverage, and warnings. Saving uses exclusively the `draft_id` shown;
it performs no second, invisible planning run.

### D15 — My Stats states population and coverage

Sound profile statistics use `listen_events` from the same period as the rest
of the screen. An insight appears only with at least 20 analyzed plays and at
least 70 percent analysis coverage of the plays whose track still exists.
Every insight states "based on N analyzed plays". Below the thresholds no
falsely precise statement appears.

### D16 — MCP is its own, narrow adapter

Stage 2 adds `crates/reprise-mcp` as a local binary. It depends on
`reprise-core`, not on GTK or `reprise-platform-linux`. The adapter opens the
local database via the normal core path and translates MCP schemas into core
types. Music logic, SQL fragments, and playlist transactions do not belong in
the adapter.

Initially only local `stdio` is supported. HTTP, remote access, OAuth,
Server-Sent Events, and a permanently listening socket are outside of
Stage 2. This way the library does not leave the host, and the server needs
no network credentials.

The implementation targets the stable MCP revision current at the start of
building. The state at the time of the design decision is `2025-11-25`; the
announced `2026-07-28` revision is still a release candidate on 2026-07-19 and
therefore only a compatibility watchpoint. The official Rust SDK is Tier 2;
its version is pinned and additionally secured with schema/protocol fixtures.

### D17 — MCP primitives stay small

Stage 2 exposes:

**Resources**

- `reprise://library/summary` — track/artist/album count, total duration, and
  analysis coverage;
- `reprise://audio-character/vocabulary` — dimensions, value ranges, and
  semantics of the mix intent;
- `reprise://playlists` — names, IDs, and track counts without paths.

**Read-only tools**

- `music_search_tracks` — bounded, paginated metadata search;
- `music_get_sound_profiles` — profiles for at most 100 explicit track IDs;
- `music_get_mix_draft` — re-reads an existing draft.

**Planning tool**

- `music_plan_playlist` — creates and persists a mix draft, but no playlist,
  queue, playback, or file change. The bounded draft persistence is
  explicitly named as a side effect in the tool contract.

**Write tool**

- `music_create_playlist_from_draft` — atomic approval of a draft.

In Stage 2 there is no arbitrary SQL, no file paths, lyrics, credential
values, tag write operation, library deletion, trash action, queue mutation,
or playback control. Prompts are initially not exposed; agents can use the
JSON schemas directly.

> **Addition (2026-07-21, multi-frontend-core plan):** Direct
> `music_create_playlist` (name plus explicit track IDs) is now allowed under
> the same capability `playlist:create`; the draft route described here
> (`music_create_playlist_from_draft`) remains a later coexisting path.
> Overwriting and deleting via agent remain excluded.

### D18 — Capabilities are fail-closed

The MCP server starts by default with `library:read` and `mix:plan`.
`playlist:create` is off. The user enables it explicitly in Reprise and then
restarts the MCP process; an environment variable must not override the
stored refusal.

Every tool checks its capability in the core-adjacent adapter before data
access. An unknown capability value means "denied". The write capability
allows exclusively the creation of a new manual playlist from a valid draft.
It allows neither overwriting nor deleting.

### D19 — No path or history leaks

MCP results contain opaque track IDs and the metadata title, artist, album,
duration, year, genre, rating, and sound profile. They never contain:

- audio or cover file paths;
- XDG, cache, or database paths;
- lyrics;
- device serial numbers or MTP paths;
- credentials, tokens, or settings values;
- raw listen events or exact listening timestamps.

Later access to aggregated listening preferences requires its own capability,
disabled by default. Stage 2 does not offer it.

### D20 — MCP stays stateless, drafts are long-lived core data

Mix drafts are stored in SQLite with a bounded lifetime, not in an MCP session
or in process memory. This way a draft survives a client restart and the
transport can stay stateless. Expired drafts are cleaned up on access or
rather in a bounded maintenance operation; no unbounded startup loop runs over
the entire table.

## 4. Architecture

```text
reprise-gnome                         reprise-mcp (stdio)
  Mix Builder                            MCP resources/tools
       |                                      |
       +--------------+-----------------------+
                      |
          reprise-core::sound_profile
          reprise-core::mix_planner
            - storage and staleness
            - projection and coverage
            - candidate queries
            - deterministic planning
            - durable drafts
            - atomic approval
                      |
                 SQLite DB

reprise-gnome background runtime
                      |
       core AudioAnalysisBackend seam
                      |
reprise-platform-linux GStreamer adapter
  bounded PCM decode + evidence extraction
```

### Module boundaries

`sound_profile` is a deep core module. Its public surface comprises profiles,
coverage, pending work, and persisted results; table shape, SQL, and
projection weights remain internal.

`audio_analysis` defines the platform-neutral analysis contract and the
versioned result values. The Linux adapter is the production implementation;
tests use a deterministic fake adapter.

`mix_planner` owns mix intent, mix draft, selection reasons, and approval. It
uses `sound_profile` and existing playlist functions internally. Neither GTK
nor MCP sees its SQL implementation.

`reprise-mcp` is deliberately flat: transport, schema, and capability
adapter. If one were to delete the crate, every selection and persistence
function in core remains fully usable.

## 5. Persistence concept

The concrete schema version is determined at the start of the implementation
from the `main` current at that point; today's state is v17, but branches
running in parallel may claim the next number.

Logically the following are required:

### Track analysis

One row per track with:

- source fingerprint (`file_mtime`, `file_size`);
- extractor/profile version;
- raw audio evidence;
- normalized sound profile;
- confidences;
- status `ready | failed` plus typed error;
- analysis time and retry state.

`pending` is not materialized millions of times: a present track without an
up-to-date row is pending. Track deletion cascades. Missing/removed stay
excluded from pending and mix queries.

### Mix drafts

A draft header row contains intent JSON in canonical form, source snapshot,
profile version, seed, expiry time, status, and diagnostics.
Position rows store track ID, position, score, and selection reasons.

The source snapshot stores the identity/fingerprints of the selected tracks
and the hard source conditions. Approval re-checks exactly this selection:
tracks must still be PRESENT, their analysis fingerprints must match, and they
must still belong to the requested source set. New or changed uninvolved
tracks do not make the draft stale. Approval and playlist creation run in one
transaction.

## 6. Analysis quality and calibration

Stage 1A requires a reproducible fixture corpus in the repository:

- silence and a very quiet signal;
- sine waves at low and high frequencies;
- impulse/click track with known BPM;
- a signal swelling and ebbing dynamically;
- an evenly compressed signal;
- a noisy or rather broadband signal;
- short real, redistributable music fixtures for container/codec integration.

Synthetic fixtures check exact mathematical properties. Real fixtures check
only robust ranges and orderings, never subjective atmosphere. The fixture's
origin and its licence are documented next to the file.

Release benchmarks measure:

- peak RSS or rather the demonstrable PCM buffer cap;
- decode/analysis time per audio minute;
- database size per 10,000 tracks;
- pending query at 100,000 tracks;
- mix planning at 1,000, 10,000, and 100,000 profile rows;
- result determinism independent of the SQLite plan and thread ordering.

Hard contracts:

- PCM memory is bounded by fixed chunk/FFT windows, not by track duration;
- default parallelism is 1;
- mix planning loads no complete PCM, waveform, or embedding BLOBs;
- at most 500 candidates pass from the SQL preselection into the greedy
  diversity phase;
- MCP responses are paginated or rather hard-limited.

## 7. Error and edge cases

- **Track changes during the analysis:** store the result only if the source
  fingerprint still matches; otherwise discard it and make it pending again.
- **Track becomes missing:** a running analysis may fail; no profile is
  emitted as a candidate.
- **App is terminated:** the worker holds no GTK borrow; cancellation ends the
  current chunk/track path cleanly and leaves the remaining tracks pending.
- **Tempo not determinable:** BPM stays `None`, the remaining dimensions are
  valid; no invented zero value.
- **Silence:** intensity and rhythmicity are set to a defined low value,
  confidence states the limited significance; division by zero is excluded.
- **Too few candidates:** partial draft with a diagnostic, provided all hard
  conditions are satisfied.
- **Draft stale:** approval refuses if selected tracks or hard source
  conditions no longer hold; no silent replan. Changes to uninvolved tracks
  remain without consequence.
- **Playlist name exists:** Reprise preserves today's semantics of manual
  playlists and may create a second playlist of the same name. Idempotency
  only prevents the duplicate commit of the same draft; an existing playlist
  is never overwritten.
- **MCP client requests 100,000 IDs:** the schema/runtime limit refuses
  before SQL.
- **Manipulated intent/draft JSON:** typed validation, no dynamic SQL
  identifier and no free WHERE clause.
- **Capability changes while the process is running:** the stored setting is
  re-read per write call; revocation takes effect without a server restart.
  New grants only become visible after a restart, so that no client
  unexpectedly receives additional tools.

## 8. UX direction for stages 1A and 1B

### Settings

Under Library, "Audio Analysis" appears:

- toggle "Analyze audio locally";
- explanation "Reads your music locally. Nothing is uploaded.";
- coverage "1,204 of 1,686 tracks analyzed";
- progress/error count;
- pause/resume;
- "Retry failed";
- "Reanalyze library" only with confirmation, because it is compute-intensive
  but deletes no user data.

### Now-playing panel

The existing right-hand now-playing panel gains, alongside "Up Next" and
"Lyrics", a third tab "Audio Character" for the currently loaded track. It
shows four labeled scales, BPM, and a short line "Analyzed locally". Where the
analysis is missing it shows "Not analyzed" plus the activation/analysis
action. Color is never the sole carrier of information. A general detail
surface for arbitrary selected library tracks is deliberately not part of
Stage 1A.

### Mix Builder

The builder offers presets as starting values, but stores the structured
intent:

- "Calm & dark";
- "Bright & energetic";
- "Dynamic focus";
- "Steady pulse".

Sliders and selection fields remain editable after a preset is chosen.
"Preview" creates a mix draft. "Save as Playlist" stays disabled until there
is a valid, current draft.

### My Stats

The first stats extension is small: a sound profile summary of the listened,
analyzed plays plus a deep link to the Mix Builder. No new large chart family
and no semantic "your mood".

## 9. MCP security and protocol boundary

MCP separates resources, prompts, and tools; tools are model-driven. The
local server therefore exposes only clearly annotated, tightly validated
tools. The client should involve the user on writes, but Reprise does not rely
on that alone: `playlist:create` stays fail-closed on the server side.

Tool results use structured output plus a short text summary for less complete
clients. Read-only, destructive, and idempotent hints are set correctly, but
never misunderstood as a security control.

The stdio process writes log output exclusively to `stderr`; `stdout` stays
MCP. Logs contain tool name, result status, duration, and anonymized counts,
never track metadata, paths, intents, or credentials.

## 10. Licence and model boundary

The MIT licence of `reprise-core` and `reprise-platform-linux` must be
preserved. Stage 1A therefore uses no Essentia library and distributes no
Essentia models. Essentia is suitable as a non-integrated research
comparison, and is itself AGPLv3; the models it provides carry, depending on
generation, additional CC-BY-NC-SA/ND terms or rather a proprietary licence
option.

A later semantic stage begins only with a licence/quality gate of its own:

- commercial use and redistribution compatible with an MIT core/proprietary
  frontends;
- training data and model provenance documented;
- local CPU path without a mandatory cloud;
- genre/culture bias measured on a fixed corpus;
- model version and confidence in the result;
- user corrections stored separately from the model result.

Without a passed gate, Reprise stays with the explainable sound profile.

## 11. Stage boundaries

### Stage 1A — Sound profile foundation

Contains the analysis contract, Linux adapter, persistence, worker, settings,
and the Audio Character tab of the now-playing panel. It is the next
executable plan section and on its own already a complete user benefit.

### Stage 1B — Native mix planner

Contains the shared mix planner, draft/approval, mix builder, and a small
My Stats projection. It begins only after explicit approval following the
Stage 1A review.

### Stage 2 — Local MCP adapter

Contains the separate stdio binary, resources, read-only tools, mix planning,
and capability-protected playlist creation. It begins only after explicit
approval following the Stage 1B review.

### Stage 3 — Semantic atmosphere and similarity

Optional: a licensed model for valence/arousal or audio-text embeddings, user
corrections, "similar to this", and free atmospheres. Not part of the current
implementation scope.

## 12. Primary sources

- MCP stable specification `2025-11-25`:
  <https://modelcontextprotocol.io/specification/2025-11-25>
- MCP server primitives:
  <https://modelcontextprotocol.io/specification/2025-06-18/server/index>
- Official Rust SDK (Tier 2):
  <https://github.com/modelcontextprotocol/rust-sdk>
- MCP `2026-07-28` release candidate:
  <https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/>
- Essentia licensing:
  <https://essentia.upf.edu/licensing_information.html>
- Essentia model inventory (research comparison only):
  <https://essentia.upf.edu/models.html>
- Audio/lyrics valence-arousal comparison:
  <https://arxiv.org/abs/1809.07276>
- CLAP audio-text representation (deferred research direction):
  <https://arxiv.org/abs/2206.04769>
