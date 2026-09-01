---
slug: the-repo-is-ready-to-show-a
worktree: /home/marvin/Projects/reprise-the-repo-is-ready-to-show-a
branch: feature/the-repo-is-ready-to-show-a
phase: shipped
codex_session:
created: 2026-08-31
---
# Strand a — What ships

Part of `docs/plans/the-repo-is-ready-to-show.md`. Read the mother plan first:
it carries the decisions, the full cut, the merge order and the post-merge
cross-checks. **Merge position: first.**

Packaging, the Flatpak, Android, and the *packaging half* of the runtime
shelving — the build and install targets live here, the crates belong to strand
b.

## File ownership

`meson.build`, `meson_options.txt`, `data/**` **except**
`data/io.github.marvinbaudach.Reprise.metainfo.xml` and
`data/io.github.marvinbaudach.Reprise.desktop` (those are strand c's),
`io.github.marvinbaudach.Reprise.yml`, `flatpak/**`, `RELEASING.md`,
`scripts/check-merge-readiness.sh`, `scripts/check-runtime-service-install.sh`,
`scripts/check-flatpak-*.sh`, `scripts/android-*.sh`,
`scripts/check-android-*.sh`, `.github/scripts/**`, `android/**`,
`crates/reprise-android-ffi/**`.

Touch nothing else. In particular: do not edit any file under `crates/` other
than `crates/reprise-android-ffi/`, and do not edit the metainfo or desktop
file.

---

## a1 — The Flatpak stops shipping onnxruntime

`meson_options.txt:12` sets `stem_backend` to `value: true`, and the Flatpak
manifest never overrides it. The ort/ort-sys-linked `reprise-worker` therefore
builds and installs — confirmed by its entries in `cargo-sources.json:3138` —
against decision E5 and against the manifest's own comment claiming otherwise.

E5 exists to remove the binary blob and make the x86_64 and aarch64 builds
identical, which GNOME Circle requires.

Add `-Dstem_backend=false` to the manifest's `config-opts`. Then verify:
`meson introspect` (or the configured build directory) shows the option off, and
no `reprise-worker` target is configured. Do not rely on reading the manifest
back.

Commit: `build(flatpak): stop shipping the stem worker`

## a2 — The Flatpak source moves to the pinned form

`type: dir, path: .` is self-documented as not submission-ready.

Replace it with `type: archive` plus a `url` pointing at the tagged release
tarball and a `sha256` placeholder. This plan deliberately does **not** cut
release 0.1.111, so the concrete hash is filled in at tag time. Add the step
that produces the tag and the hash to `RELEASING.md`, next to the existing
release steps, so the placeholder cannot be forgotten.

Leave a comment in the manifest naming `RELEASING.md` as the place the hash
comes from.

Commit: `build(flatpak): pin the source to a released archive`

## a3 — The runtime service stops being installed

Nothing shipped ever dials `io.github.marvinbaudach.Reprise.Runtime1`, yet the
binary, a D-Bus activation file and a systemd user unit install on every system.

- `meson.build:35-48` — remove the `reprise-runtime` binary target
  (`build_by_default: true, install: true`).
- `data/meson.build:39-70` — remove the D-Bus activation file and the systemd
  user unit, and delete the sources
  `data/io.github.marvinbaudach.Reprise.Runtime1.service.in` and
  `data/reprise-runtime.service.in`.
- Delete `scripts/check-runtime-service-install.sh` — the gate that currently
  enforces that binary, D-Bus file and unit stay in agreement, and so keeps the
  shelf-ware shipping.
- Remove its invocation from **`.github/scripts/check-gnome-ci.sh:17`** too.
  Found by grep during the grill: that file calls the script as well, and it is
  the only reference outside `check-merge-readiness.sh`. Missing it leaves CI
  calling a deleted script from a branch that is green locally.

`meson.build:37-38` names the binary and `data/meson.build:49-50,68-69` name both
`.service.in` inputs. A dangling `configure_file` input fails meson *configure*,
not compile, so `cargo build` alone will not catch a miss — run a meson configure
after this task.

**Do not touch `crates/`.** Strand b deletes the crates afterwards. After this
strand the crates still exist and are simply not installed, which is what keeps
this branch green on its own.

Commit: `build: stop installing the runtime service`

## a4 — The gate list gains and loses one entry

In `scripts/check-merge-readiness.sh`:

- remove the `check-runtime-service-install.sh` entry (deleted in a3);
- add `scripts/check-release-metadata.sh` in its **full** mode, which exists but
  is never run by the merge gate — this is why the metainfo drifted to 0.1.84
  while `Cargo.toml` reached 0.1.111. Strand c writes the missing release
  entries; this gate stops the drift from returning.

The showroom derives its displayed gate count from this script's own `gate()`
calls. One gate leaves and one arrives, so the total stays at 27 — verify the
displayed count still comes from the script rather than a constant, and that it
reflects the new list. This is post-merge cross-check 2 in the mother plan;
here, just do not break the derivation.

Commit: `build: gate the release metadata instead of the runtime service`

## a5 — Android serves every reader from one database handle

Up to five independent SQLite connections exist in one process:

- `android/.../MainActivity.kt:68` opens `MusicLibrary` (writer + reader
  mutexes),
- `ReprisePlaybackService.kt:150` independently opens a third via
  `crates/reprise-android-ffi/src/playback_session.rs:530`
  (`Db::open_migrated`),
- `PlayRecorder::spawn` (`crates/reprise-android-ffi/src/play_recorder.rs:140`)
  and `ListenExportRecorder::spawn` each open their own on dedicated threads.

None honours the "writer before tree" lock order documented at
`crates/reprise-android-ffi/src/library_types.rs:22`. This undoes the
coordination that the (now completed) scan-lock split built.

Route every consumer through the single `MusicLibrary` handle.

**Failing test first**, mirroring the existing
`crates/reprise-android-ffi/src/read_during_scan_tests.rs`: open the playback
session while a scan holds the writer and assert the read succeeds.

Commit: `fix(android): serve every reader from one database handle`

## a6 — Android moves off the spike package name

`android/app/build.gradle.kts:49` still has `namespace = "de.reprise.spike"`
while `applicationId` (`:55`) was already renamed. There are 265
`de.reprise.spike` package declarations across 187 files, and the release APK's
`classes.dex` carries `Lde/reprise/spike` class paths.

Rename the package tree to `io.github.marvinbaudach.reprise`.

Checked during planning: this needs **no** JNI symbol changes — there are zero
`Java_de_reprise_spike` occurrences in `crates/reprise-android-ffi/src`.

Mechanical but wide. Give it its own commit with nothing else in it, so the diff
stays reviewable.

Commit: `refactor(android): move off the spike package name`

## a7 — Android restores the library off the main thread

`MainActivity.kt:207 → 372 → 531` calls `LibrarySession.kt:103 restore()`
synchronously in `onCreate`; that file contains zero `withContext` /
`viewModelScope` hops. The branch that was to fix this
(`feature/android-list-scroll-performance`) is gone and never landed.

Move the restore onto a coroutine dispatcher and hold the existing loading state
until it resolves.

**Honesty requirement:** frame-time proof needs a device or emulator, and debug
builds do not produce usable frame times. If no device is available, ship this
with the JVM-level test only and say in the commit message that the improvement
is unmeasured — do not claim a measured win.

Commit: `perf(android): restore the library off the main thread`

---

## Done when

`meson introspect` shows `stem_backend` off for the Flatpak configuration and no
`reprise-worker` target; the manifest source is `type: archive` and
`RELEASING.md` describes how its hash is produced; no runtime binary, D-Bus file
or systemd unit is installed; `check-merge-readiness.sh` no longer names
`check-runtime-service-install.sh` and does name `check-release-metadata.sh` in
full mode; the Android JVM suite is green including the new
read-during-playback-session test; no `de.reprise.spike` remains in `android/`
or in a release APK's `classes.dex`; `restore()` runs off the main thread.

`crates/` outside `reprise-android-ffi` is untouched by this branch.
