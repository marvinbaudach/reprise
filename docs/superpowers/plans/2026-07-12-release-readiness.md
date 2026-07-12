# Reprise Release Readiness — Implementation Plan

**Goal:** Installable GNOME integration, complete German gettext localization,
portal-safe Flatpak behavior, and a locally reproducible GNOME-50 Flatpak package.

**Baseline:** `fade83a`; 458 passed, 1 ignored; only accepted audit warning is
RUSTSEC-2024-0436 (`paste`). Nothing is pushed or published.

All normal gates from `AGENTS.md` run before every commit. Every app smoke uses the
full isolated `dbus-run-session` + Xvfb + scratch XDG + X11 + fakesink command.

## Task 1 — Meson install layer and GNOME identity assets

**Files:** create root/data/po Meson files; create Desktop, AppStream, full-color SVG
and symbolic SVG; update README build/install status.

Build Meson around the existing Cargo workspace without changing direct `cargo run`.
Install binary/data/icons to a scratch prefix. Desktop ID, filenames, icon names,
MPRIS desktop entry and `APP_ID` must all remain `org.reprise.Reprise`.

RED release checks first: missing installed binary/desktop/metainfo/icons. Then make
`meson setup`, `meson compile`, `meson install --destdir`, `desktop-file-validate`,
`appstreamcli validate --pedantic`, and XML parsing pass. SVGs follow GNOME 128×128
and 16×16 symbolic geometry.

Expected 458 passed; 1 ignored.

Commit: `build: add installable GNOME application metadata`

## Task 2 — Runtime gettext and complete German UI

**Files:** add frontend `i18n.rs`; modify `main.rs`, `strings.rs` and all UI string
call sites; create `po/POTFILES.in`, `po/LINGUAS`, `po/reprise.pot`, `po/de.po`;
complete Meson gettext wiring.

Initialize gettext before Adwaita. Mark every English source msgid. Translate all
static and dynamic UI strings; use `ngettext` for count-sensitive text and a small
pure named-placeholder formatter.

RED test: placeholder formatting preserves unknown markers and replaces repeated
known markers deterministically. Run `msgfmt --check --check-format po/de.po`, compare
POT/PO coverage, then run isolated `LANGUAGE=de` with a permanent
`REPRISE_SMOKE_I18N=1` log proving representative wizard/sidebar/player strings are
German. English fallback smoke remains English.

Expected 459 passed; 1 ignored.

Commit: `feat: localize the complete interface with gettext and German`

## Task 3 — Portal-safe trash backend for Flatpak

**Files:** create `reprise-platform-linux/src/trash.rs`; modify platform/core Cargo
files and module exports, core `trash_tracks.rs`, frontend `delete_tracks.rs`.

Core retains only `trash_tracks_with`. Linux `trash::delete` selects host trash when
not sandboxed and XDG Trash portal when `/.flatpak-info` is present. Portal path opens
the file read/write and sends its FD to `TrashFile`; only result 1 succeeds. Never
fall back to unlink/permanent delete or private sandbox trash after a portal failure.

RED pure tests: environment selects Host vs Portal; result 0 is failure and 1 success.
Existing scratch-only delete smokes must remain green. Add an isolated portal probe
against a scratch file when the portal is available; absence is an explicit skipped
environmental check, not a product fallback.

Expected 461 passed; 1 ignored.

Commit: `fix: use the desktop trash portal inside Flatpak`

## Task 4 — GNOME 50 Flatpak manifest and offline Cargo sources

**Files:** create top-level `org.reprise.Reprise.yml`, generated
`flatpak/cargo-sources.json`, `flathub.json` only if architecture policy requires it,
and `flatpak/README.md` documenting the public-source handoff.

Use `org.gnome.Platform`/`org.gnome.Sdk` branch 50 and the matching stable Rust SDK
extension. Build with Meson/Cargo offline. Finish args are limited to Wayland,
fallback-X11, IPC, DRI, PulseAudio, network, and own-name MPRIS. No broad filesystem
or bus permissions. Local manifest uses the checked-out source directory; the README
names the one required substitution for a real immutable public source release.

Validate YAML, generated crate checksums, `flatpak-builder-lint` when available, and
perform full builder/install/start smoke when the local Builder + runtime can be
installed. If the external runtime download is unavailable, manifest lint plus Meson
DESTDIR build is the honest hard minimum and the missing runtime is recorded.

Expected 461 passed; 1 ignored.

Commit: `build: add reproducible GNOME 50 Flatpak packaging`

## Task 5 — Release documentation and distribution checks

**Files:** update README from stale Stage-3 copy; add `RELEASING.md`; add a compact
`scripts/check-release.sh` (read-only checks only).

Document complete features, privacy/network behavior, supported formats via runtime
codecs, build/install/Flatpak commands, manual QA, and the no-push/public-source
handoff. The check script runs formatting, strict clippy, workspace tests, audit,
core purity, gettext, desktop/AppStream, Meson DESTDIR, file sizes and manifest lint
without touching real user data.

Expected 461 passed; 1 ignored.

Commit: `docs: document release build and verification workflow`

## Task 6 — Release close-out

Run the entire release check, audit, standalone core/purity, German/English isolated
smokes, first-run and two-start session regressions, installed-prefix startup, and
Flatpak build/start if the runtime is available. Review permissions, translation
coverage, app-ID consistency, no-autoplay, portal trash, data-file validity and every
touched file size.

Update STATUS and ledger. Keep OWNER `codex` until the release-ready repository is
committed; then release OWNER to FREE. Record external publication blockers exactly:
no public immutable source remote and no verified ownership for `org.reprise`.

Commit: `docs: close release readiness stage`

