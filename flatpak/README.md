# Flatpak packaging

`io.github.marvinbaudach.Reprise.yml` is the local, reproducible development
manifest. It uses GNOME Platform/SDK 50 and the matching stable Rust SDK
extension. Cargo is forced offline and consumes the checksummed crate archives
generated in `cargo-sources.json`.

Regenerate the Cargo sources after every `Cargo.lock` change with the official
Flatpak Builder Tools generator:

```sh
flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json
```

Formatting the generated JSON compactly is optional and does not alter it.

## Local build

Install `flatpak-builder` plus the Flathub GNOME 50 SDK/runtime and Rust SDK
extension, then build from the repository root with a build directory outside
the source tree:

```sh
flatpak-builder --user --install-deps-from=flathub --force-clean \
  --install /tmp/reprise-flatpak-build io.github.marvinbaudach.Reprise.yml
flatpak run io.github.marvinbaudach.Reprise
```

Every static permission and why no portal replaces it:

- `--share=ipc`, `--socket=wayland`, `--socket=fallback-x11`, `--device=dri` —
  display and rendering; no portal covers these.
- `--socket=pulseaudio` — audio playback; PipeWire is reached through the same
  socket.
- `--share=network` — cover art, lyrics from LRCLIB, podcast and radio feeds,
  and optional scrobbling.
- `--own-name=org.mpris.MediaPlayer2.reprise` — media keys and the lock screen.
  MPRIS is a standardised namespace and has no portal equivalent.
- `--talk-name=org.gtk.vfs.*` and `--filesystem=xdg-run/gvfsd` — reaching the
  desktop's existing MTP mounts for Android synchronisation. There is no portal
  for MTP devices.

Library access is not static: it comes from the user's folder choice through
the FileChooser and Documents portals. Moving files to the trash uses the Trash
portal. There is no broad home directory, direct USB, session bus or system bus
access.

The runtime must provide the GStreamer Good Plug-ins used by Android MP3
synchronization, in particular `lamemp3enc` and `id3v2mux`. Run
`scripts/check-device-sync-gstreamer.sh` in the built runtime as part of every
packaging verification; synchronization blocks before any managed deletion if
the fixed MP3/ID3 pipeline is unavailable.

## Public-source handoff

The repository currently has no public remote or immutable release artifact,
so this local manifest intentionally uses:

```yaml
- type: dir
  path: .
```

Before a real Flathub submission, replace exactly that source entry with one
public immutable release archive carrying its SHA-256 checksum, for example:

```yaml
- type: archive
  url: https://example.invalid/reprise/releases/download/v0.1.1/reprise-0.1.1.tar.xz
  sha256: REPLACE_WITH_ARCHIVE_SHA256
```

Do not invent or publish the example URL. The real URL, checksum, AppStream
homepage, release tag, and Flathub submission must come from a public project
identity controlled by the maintainer. No architecture exception is currently
required, so there is deliberately no `flathub.json`.

## Verification status

`flatpak-builder`, `flatpak`, `appstreamcli` and `desktop-file-validate` are
available on the development machine, so the manifest can be linted and a
sandbox build can be run locally. The gates that cover this are
`scripts/check-flatpak-manifest.sh` and `scripts/check-appstream.sh`.
