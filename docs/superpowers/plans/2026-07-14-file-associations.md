# Desktop File Associations — Implementation Plan

## Global Constraints

- Follow RED → GREEN TDD and preserve the single-instance/single-window/single-player architecture.
- Keep English code/comments/UI source strings/commits and German internal design documentation.
- Never access the real desktop, music, database, cache, accounts, or session bus.
- Opening audio must never silently scan, import, or mutate a user's music files or library.
- Keep every substantially edited Rust file below 800 lines and Core dependency-pure.

## Task 1 — Advertise and dispatch local media files

Files: `data/org.reprise.Reprise.desktop`, `data/org.reprise.Reprise.metainfo.xml`,
`crates/reprise-gnome/src/main.rs`, `crates/reprise-gnome/src/ui/file_open.rs`,
`crates/reprise-gnome/src/ui/mod.rs`, `crates/reprise-gnome/src/ui/window.rs`,
`crates/reprise-gnome/src/ui/playlist_io.rs`, `crates/reprise-gnome/src/ui/strings.rs`, `po/de.po`,
matching design/Manual QA/status docs.

Interfaces:

- `FileOpenHandler::open(&self, files: &[gio::File])` consumes GApplication's local file batch.
- `classify_path(&Path) -> OpenFileKind` distinguishes supported audio, M3U/M3U8 and unsupported.
- `resolve_audio_ids(&Connection, &[PathBuf]) -> AudioResolution` preserves input order and reports
  unresolved paths without inserting rows.
- `window::build(...) -> FileOpenHandler` returns the handler wired to the one existing window,
  player, sidebar, toast overlay and database connection.

TDD steps:

1. Add failing pure tests for extension classification, canonical/exact path lookup, ordered partial
   matches, and the desktop/AppStream metadata contract. Observe RED before production metadata and
   `file_open` exist.
2. Add failing application-source tests requiring `HANDLES_OPEN`, one shared handler cell and an
   Open callback that reuses the existing primary window. Observe RED with activate-only startup.
3. Implement `ui/file_open.rs`, return its handler from the completed window composition, expose only
   the existing playlist-result UI seam, and connect GApplication activation/open without holding a
   `RefCell` borrow across callbacks.
4. Add translated singular/plural unsupported-library and playback-unavailable toasts, update German
   gettext, and declare matching desktop/AppStream media types with `Exec=reprise %F`.
5. Run focused tests, `desktop-file-validate`, AppStream validation, isolated real application smokes
   for an audio argument and an M3U argument, all workspace gates, Core-purity proof, release checker,
   file-size proof and adversarial review.
6. Commit `feat: add desktop file associations`.

Expected result: pure file-open and metadata regressions plus isolated end-to-end coverage; existing
workspace behavior remains unchanged when Reprise starts without file arguments.
