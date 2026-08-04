# ADR 001: Music uses a scoped TrackList browser

## Status

Accepted on 2026-07-20.

## Context

Music currently shows Tracks, Albums and Artists as synchronized visual modes,
even though album and artist details already query the shared TrackList through
`ViewSource`. As a result, filters, the playing marker, focus, scroll memory,
history and the player's metadata links have to be coordinated separately for
three presentations of the same library. The resulting interactions are
ambiguous and have caused visible focus and scroll bugs.

Album and artist identity is derived from track metadata. Reprise has no
persistent album or artist entities whose lifecycle would justify these
projections as app places of their own.

## Decision

Music owns one virtualized TrackList browser. The library collection has three
scopes: all tracks, one album or one artist. Album and artist scopes are
navigable history entries with local refinements and stable view bookmarks, but
not persistent database entities. My Stats remains a dashboard of its own that
can navigate to scopes or create playback snapshots.

The compatible query variants `ViewSource::Album` and `ViewSource::Artist`
remain in place during the migration; the domain interface, however, is formed
by `BrowserPlace`, `TrackCollection` and `LibraryScope`.

## Consequences

- Tracks, albums and artists share one playing-marker, selection, focus and
  scroll implementation.
- Fresh scope navigation never inherits hidden filters; back and forward
  restore the complete previous place.
- A scope whose last member disappears is kept for the session as an honest
  empty state and falls back to Music after a restart if it is no longer
  resolvable.
- The Tracks/Albums/Artists switcher, the grid-specific focus system and the
  cross-mode synchronization go away as soon as all callers use the new
  interface.
- A future cover browser is a separate feature, not a fourth synchronized Music
  mode.

## Alternatives considered

- Keep the three modes and repair every synchronization edge. Rejected, because
  three implementations of the same library state machine remain.
- Treat album and artist as ordinary filter chips. Rejected, because history,
  scope headers, canonical container actions and clear back behavior are lost.
- Introduce persistent album and artist tables. Rejected, because tag changes
  would require merge/split identity rules and a lifecycle of empty entities
  with no product benefit.
