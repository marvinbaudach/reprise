---
slug: aur-reprise-git-package
worktree: /home/marvin/Projects/reprise-aur-reprise-git-package
branch: feature/aur-reprise-git-package
phase: coding
codex_session:
created: 2026-08-13
---

# An AUR package so Arch users can install Reprise

Reprise has no distribution channel outside "clone it and run Meson yourself".
The AUR RPC confirms it on 2026-08-13: `search/reprise?by=name` and
`info?arg[]=reprise&arg[]=reprise-git` both return `resultcount: 0`. Nothing
has ever been submitted.

This plan produces the **`reprise-git` VCS package** — the flavour that works
today. A versioned `reprise` package needs a release tag and an immutable
source tarball, neither of which exists yet; that is deliberately out of scope
here.

The file lists below are a **starting point, not a fence**. Adjacent files may
be changed minimally and named in the commit message. Stop only if the
*contract* below turns out to be wrong.

## Goal

`packaging/aur/` in this repository holds the maintained PKGBUILD source of
truth, plus the generated `.SRCINFO` and a short README explaining how the
directory is mirrored into the AUR git repository. A user on Arch can then run
`makepkg -si` (or let an AUR helper do it) and get a working Reprise.

## What the package must get right

These are established facts about this codebase — do not re-derive them, and do
not add dependencies "just in case".

**SQLite is not a runtime dependency.** All three consumers pin
`rusqlite = { version = "0.40", features = ["bundled"] }`
(`crates/reprise-core/Cargo.toml:15`, `crates/reprise-gnome/Cargo.toml:35`,
`crates/reprise-cli/Cargo.toml:43`). SQLite is compiled in. It needs a C
compiler at build time, which `base-devel` already provides, and it must
**not** appear in `depends`.

**The keyring needs no library.** `crates/reprise-gnome/Cargo.toml:51` uses
`oo7` with `default-features = false, features = ["async-std", "native_crypto"]`
— it speaks to the Secret Service over D-Bus. No `libsecret` link. A running
secret service belongs in `optdepends`, not `depends`.

**onnxruntime is `dlopen`ed, never linked.** `crates/reprise-stems/src/lib.rs`
documents the lookup order and `crates/reprise-stems/src/provision.rs:43`
defines `ORT_DYLIB_PATH`. Nothing is downloaded or linked at build time.
`onnxruntime` therefore belongs in `optdepends`, and the package must still
build and run when it is absent.

**The GStreamer elements the app actually names** — `lamemp3enc`, `id3v2mux`,
`souphttpsrc` — all live in `gst-plugins-good`; `playbin3` comes from
`gst-plugins-base-libs`. Verified with `gst-inspect-1.0` against the installed
packages on 2026-08-13. `scripts/check-device-sync-gstreamer.sh` is the
authority on the complete factory set; read it rather than guessing.

**The stem backend is on by default.** `meson_options.txt` defines
`stem_backend` as a boolean defaulting to `true`, which also builds the
out-of-process worker installed into `libexecdir`. Keep the default so the
package matches the documented upstream build.

## Task 1 — the PKGBUILD

Files (starting point): `packaging/aur/PKGBUILD`

Contract:

- `pkgname=reprise-git`, `provides=('reprise')`, `conflicts=('reprise')`.
- `pkgver()` derives the version from git in the standard VCS form, seeded from
  the Meson project version (`meson.build` currently declares `0.1.1`) so the
  first published version sorts sensibly.
- `source=('reprise::git+https://github.com/marvinbaudach/reprise.git')` with
  `sha256sums=('SKIP')`.
- `license=('GPL-3.0-or-later')` — and nothing else. The whole workspace was
  relicensed by `d4421a86d4` ("chore: relicense the whole workspace to
  GPL-3.0-or-later", 2026-08-11): `Cargo.toml:24` sets it in
  `[workspace.package]`, every crate inherits it via `license.workspace = true`,
  `LICENSING.md` documents copyleft throughout, and `LICENSE` and
  `crates/reprise-gnome/LICENSE` are byte-identical GPL texts. No MIT component
  remains. Install the root `LICENSE` under `/usr/share/licenses/$pkgname/`.
- `depends`: the GTK/GStreamer/GLib runtime the binary links plus the MTP
  volume monitor — derive the exact list from the `*-sys` crates in
  `Cargo.lock` and from `README.md`'s stated requirements (GTK 4.22+,
  libadwaita 1.9+, GStreamer 1.x with the Good plug-ins, GVfs with its MTP
  volume monitor). Do not include `sqlite`.
- `makedepends`: `git`, `meson`, `ninja`, `rust` (or `cargo`), `gettext`.
- `optdepends`: `onnxruntime` for stem separation, a secret service for the
  keyring, and the extra GStreamer codec plug-in packages for formats beyond
  the Good set — each with a short reason string.
- `prepare()` does the network work: fetch the crate dependencies with
  `cargo fetch --locked --manifest-path "$srcdir/reprise/Cargo.toml"`.
- `build()` runs `arch-meson` (or `meson setup` with the Arch flags) with
  `--prefix=/usr -Dprofile=release`, then `meson compile`. Cargo must be
  offline here so the build is reproducible: set `CARGO_NET_OFFLINE=true`.
- `package()` runs `DESTDIR="$pkgdir" meson install`. Nothing else — the Meson
  install already covers binary, runtime, `libexecdir` worker, desktop file,
  metainfo, icons, D-Bus service, systemd user unit and translations. Verify
  that claim against `meson.build` and `data/meson.build` rather than trusting
  it.

Note that `build-aux/meson-cargo-build.sh:30` invokes `cargo build` **without**
`--locked`. Add `--locked` there so a packaged build cannot silently update the
lock file; that is a one-line change and belongs in this commit.

Done when: `makepkg --printsrcinfo` runs clean and `namcap PKGBUILD` reports no
errors.

## Task 2 — the generated .SRCINFO and the mirror README

Files (starting point): `packaging/aur/.SRCINFO`, `packaging/aur/README.md`

- `.SRCINFO` is generated, never hand-edited:
  `makepkg --printsrcinfo > .SRCINFO`.
- The README documents, in the style of `flatpak/README.md`: what this
  directory is, that `.SRCINFO` is generated and must be regenerated after
  every PKGBUILD change, and the exact mirror procedure into
  `ssh://aur@aur.archlinux.org/reprise-git.git`.
- The README must **not** claim the package has been submitted, and must not
  invent an AUR URL. Submission is a separate manual step by the maintainer.

## Task 3 — wire it into the release checklist

Files (starting point): `RELEASING.md`

Add a short section stating that `packaging/aur/.SRCINFO` must be regenerated
whenever the PKGBUILD or the Meson install set changes, and that the AUR mirror
is pushed by hand. Keep it to the plain, checkable style of the surrounding
document.

## Out of scope

- Submitting anything to the AUR. No `git push` to `aur.archlinux.org`.
- A versioned `reprise` package, a release tag, or a source tarball.
- Any change to the Flatpak manifest.
- Any change that turns the stem backend off by default.

## Verification

The host is Manjaro, which has no `devtools` package, so a true Arch clean
chroot is not available here. Run what is available and report exactly what was
run — do not claim a clean-chroot build happened:

```sh
cd packaging/aur
makepkg --printsrcinfo > .SRCINFO   # must succeed
namcap PKGBUILD                     # must report no errors
```

A full `makepkg -s` compiles the whole workspace in release mode and takes a
long time; run it through `heavy-run medium --` and redirect the output to a
log file rather than printing it.

Do not launch the built application; this repository verifies headlessly.
