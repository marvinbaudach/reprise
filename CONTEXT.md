# Reprise Domain Language

This glossary names the terms of the music library that must mean the same
thing in Core, in the native frontends and in future agent adapters.

## Audio understanding

**Audio evidence**:
Versioned descriptors measured locally from the decoded audio signal, such as
tempo, loudness distribution, spectral brightness and onset density.
_Avoid_: mood, emotion, atmosphere

**Sound profile**:
A versioned, normalized projection of the audio evidence onto a few stable
dimensions along which humans and selection logic can compare tracks.
_Avoid_: mood tag, genre, audio features

**Atmosphere**:
A human-readable, uncertain interpretation of a sound profile, never an
objective fact about the track or its listener.
_Avoid_: emotion, ground truth, mood label

**Analysis coverage**:
The share of eligible library tracks or plays with a current sound profile,
always named together with the population it describes.
_Avoid_: completion that includes stale or ineligible tracks

## Playlist planning

**Mix intent**:
A declarative set of hard conditions and soft wishes for an ordered music
selection; it contains no natural-language prompt and changes no user data.
_Avoid_: prompt, query, playlist

**Mix draft**:
An immutable, ordered selection derived from a mix intent and bound to its
source snapshot, together with coverage diagnostics and structured selection
reasons.
_Avoid_: playlist, preview query

**Selection reason**:
A structured statement about which profile dimension, condition or diversity
rule brought a track into a mix draft.
_Avoid_: chain of thought, free-form justification

**Draft approval**:
Explicit authority to persist exactly one unchanged mix draft as a manual
playlist.
_Avoid_: tool call, implicit consent

## Agent access

**Agent capability**:
A separately granted class of operations through an agent adapter; reading, mix
planning and playlist creation are different grants.
_Avoid_: server access, all-or-nothing permission

## Library navigation

**Browser place**:
A navigable destination with its own refinement, sort, anchor, selection and
content focus state. Back and forward restore the same place; a fresh
navigation creates a fresh place.
_Avoid_: view, tab, global filter state

**Track source**:
The domain origin of a set of tracks, for example library, playlist, smart
playlist or queue.
_Avoid_: view, scope

**Library scope**:
A navigable section of the library derived from track metadata: all tracks, one
album, one artist or one genre. A scope uses the same track list and is neither
a presentation style of its own nor a persistent entity.
_Avoid_: mode, tab, filter chip, album object

**Refinement**:
A local restriction of the visible result set of a browser place, for example
text search, genre, year or rating.
_Avoid_: scope, queue, global filter

**Playback snapshot**:
The ordered set of stable track IDs frozen at start, together with the cursor.
Later navigation, refinement or source mutation does not recompute it.
_Avoid_: visible list, live query

**Playback origin**:
The structured browser place and frozen display name a playback snapshot was
started from. It serves later reveal, but does not own the playback itself.
_Avoid_: current view, queue

## Release reconciliation

**Release ownership**:
A specific album or EP release counts as present only if the library contains
it completely as that release. Individual recordings that were released
separately or previously as singles do not establish ownership of the later
album or EP.
_Avoid_: songs present, track overlap

**Discography gap**:
A regular album or EP by a library artist for which there is no release
ownership. Individual recordings or singles that are present do not close the
gap.
_Avoid_: missing song, new release

## AI versions and provenance

**Instrumental version**:
An explicitly commissioned, permanent variant of a library track from which the
vocals have been removed by ML stem separation; a regular track, clearly marked
as AI-manipulated, with the title suffix "(Instrumental)", not a transient
effect during playback and not a rule playlist.
_Avoid_: karaoke track, remix, transient render, vocal toggle

**AI provenance**:
The disclosed origin of an AI-generated or AI-manipulated track, stored twice:
primarily as a row in the provenance registry of the database (flag and
optional source title) and secondarily in human-readable file tags, so that the
marking survives rescans and export out of Reprise. The hide filter keys on the
DB flag, never on the storage folder.
_Avoid_: watermark, hidden marking, app-internal ID in the tag

## Change propagation

**change_log (outbox)**:
The transactional outbox: one row appended per mutation in the same
transaction, which records the *what* of a change in a total order (entity,
entity ID, operation, writer token). It is the truth about changes between
processes, not the wake-up call itself; consumers do not replay it, they read
the current state from it.
_Avoid_: log file, audit trail, message queue, event sourcing

**Notifier**:
The cross-process wake-up call: a background thread with its own connection
that observes the database and the WAL and, after a short quiet period, checks
`PRAGMA data_version` — which changes only on commits of *other* connections.
It reports only *that* something happened, upon which consumers read the
change_log; if no filesystem watch can be armed, it degrades to 2-second
polling instead of giving up.
_Avoid_: daemon, push service, socket signal, IPC channel
