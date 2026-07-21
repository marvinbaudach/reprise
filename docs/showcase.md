# GitHub showcase decisions

This note keeps the repository presentation consistent across future sessions.
It records communication and visual decisions, not product behavior.

## Positioning

- Present Reprise as a polished native product and an evidence-led engineering
  portfolio project.
- Lead with the product and the strongest technical proof. Avoid a wall of
  implementation detail before the reader understands why the project matters.
- Use calm, precise language. Prefer a small number of defensible claims over
  broad marketing superlatives.
- Publish the showroom in English and German only. Additional translations add
  maintenance and review cost without serving the intended audience.
- Keep future architecture goals to thin native frontends that reuse the
  portable core and MCP or CLI adapters over the same tested application
  layer. Describe them in natural prose. Product features, experiments,
  packaging, and release work belong in their own context rather than under an
  architecture heading.

## Visual system

- Use repository-native SVG for architecture and other relationship-oriented
  visuals. Fixed layouts are preferred over Mermaid for primary showcase
  graphics because GitHub rendering must not clip text or reroute edges
  unpredictably.
- Default performance evidence to a compact comparison table that keeps the
  workload, before/after result, method, limitation, and trade-off together.
  A performance visual earns its space only when it explains causality, such
  as a query-plan transition; enlarged KPI cards are less informative than the
  table they repeat.
- Keep a consistent 1440×900 canvas, dark neutral background, Reprise blue for
  product/frontend context, mint green for verified outcomes, and restrained
  amber only for costs or trade-offs.
- Every visual needs an SVG title/description, useful Markdown alt text, and a
  legible 720 px render. Decorative effects must not carry meaning.
- Real application screenshots remain the preferred product proof after a
  native GNOME visual pass. Never fabricate a running-app screenshot.

## Evidence policy

- Verify architecture against the live crates and dependency graph. Mark
  future frontends and integrations as planned rather than implying they ship.
- Performance claims must name the profile size, before/after values, execution
  context, and material trade-offs. Same-host timing comparisons are evidence,
  not portable thresholds.
- Keep deterministic budgets separate from host-sensitive observations. The
  accepted 100,000-track evidence currently includes eight cached SQL windows,
  1,600 retained rows, and the measured queue-memory result.
- Do not claim installed GTK startup, live row counts, or CUA scroll timings
  until the isolated display suite has run successfully on a capable host.
- Benchmarks use generated metadata and isolated profiles, never a real music
  library or user database.

## Current showcase assets

- `docs/assets/reprise-architecture.svg` — current three-crate architecture,
  dependency direction, and enforced core purity.
- `docs/assets/reprise-performance.svg` — causal before/change/after view of
  the accepted 100,000-track comparison: query-plan problem, partial-index
  intervention, both measured read-path effects, and the storage trade-off.

The performance figures originate from the accepted same-host release pair on
`feat/performance-optimizations`: baseline `ddaa3f3`, index implementation
`bf8394d`, and comparison contract `b3644cc`.

## Verified engineering detail map

Use this map when deeper portfolio copy is needed. Re-check live repository
state before publishing changing counts or branch status.

| Story | Verified implementation detail | Primary evidence |
|---|---|---|
| Large-library data path | `GtkColumnView` virtualizes row widgets while `TrackListModel`, a custom `GListModel`, lazily reads 200-row SQL windows and retains at most eight windows / 1,600 tracks. One-row invalidation avoids a full cache reset after local edits. | `crates/reprise-gnome/src/ui/track_list/track_list_model.rs` |
| Portable engine | Core owns `PlaybackBackend`, `MediaIntegrationHandles`, and `WaveformBackend`; the Linux crate supplies GStreamer, MPRIS/D-Bus, waveform, MTP, and Trash implementations. | `crates/reprise-core/src/{playback,media_integration,waveform}.rs`, `crates/reprise-platform-linux/src/lib.rs` |
| Identity-preserving library maintenance | Move detection first checks device/inode, then an exact tag/duration/file-size fingerprint for copy-and-delete moves. Only one valid candidate is accepted; ambiguity deliberately degrades to no match. Relinking leaves ratings, play counts, added dates, and history columns untouched. | `crates/reprise-core/src/library/scanner_move.rs` |
| Stable queue semantics | Queue operations preserve the current track by stable identity while repeat, shuffle, reorder, and removal change ordering around it. Playback transition selection remains behind the backend contract. | `crates/reprise-core/src/queue.rs`, `crates/reprise-core/src/playback.rs` |
| Race-resistant GTK virtualization | Per-cell and per-worker generation tokens reject late cover, portrait, reveal, lyrics-scroll, scrobble-status, and progress updates after a recycled widget or visible identity changes. | `crates/reprise-gnome/src/ui/track_list/track_list_columns.rs`, `crates/reprise-gnome/src/ui/library_views/album_card_state.rs`, `crates/reprise-gnome/src/ui/scrobbling/scrobble_runtime.rs` |
| Optional online integrations | Artist news, ListenBrainz, and Last.fm have stable module IDs and persisted opt-in flags; ListenBrainz and Last.fm default off. MPRIS is described by the registry but deliberately excluded from the user-toggle list. | `crates/reprise-core/src/modules.rs` |
| Executable architecture | The architecture gate proves core dependency purity, keeps Rust files below 800 lines, constrains UI composition roots, rejects direct frontend GStreamer/blocking HTTP/productive SQL, and restricts new unsafe code. | `scripts/check-architecture.sh` |
| Executable product rules | Active UX rules require a rule-named Rust or CUA test; manual rules must be mapped into the release checklist, and tests cannot target replaced or unknown rule IDs. | `docs/ux-rules.md`, `scripts/check-ux-traceability.sh` |

## Claims to keep deferred

- Do not claim installed GTK startup, realized row/provider counts, or CUA
  scroll latency until the private display benchmark completes on a capable
  host.
- Do not describe macOS, Windows, mobile, MCP, AI music generation, or visual
  effects as shipped. They are roadmap directions and must remain visibly
  labelled as planned.
- Do not publish a test count, source-line count, or active-rule count from
  memory. Regenerate it from committed source immediately before use.
