# Flatpak packaging

`org.reprise.Reprise.yml` is the local, reproducible development manifest. It
uses GNOME Platform/SDK 50 and the matching stable Rust SDK extension. Cargo is
forced offline and consumes the checksummed crate archives generated in
`cargo-sources.json`.

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
  --install /tmp/reprise-flatpak-build org.reprise.Reprise.yml
flatpak run org.reprise.Reprise
```

The application receives only display, graphics, audio, network, its own MPRIS
name, and narrow access to the desktop's existing GVfs mounts for Android MTP
synchronization. Library access is granted by the user's folder choice through
the desktop FileChooser/Documents portals; moving files to Trash uses the Trash
portal inside Flatpak. There is no broad home-directory, direct-USB,
session-bus, or system-bus access.

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
  url: https://example.invalid/reprise/releases/download/v0.1.0/reprise-0.1.0.tar.xz
  sha256: REPLACE_WITH_ARCHIVE_SHA256
```

Do not invent or publish the example URL. The real URL, checksum, AppStream
homepage, release tag, and Flathub submission must come from a public project
identity controlled by the maintainer. No architecture exception is currently
required, so there is deliberately no `flathub.json`.

## Verification status

The manifest and every generated crate URL/checksum can be validated locally.
This workstation has Flatpak but no `flatpak-builder` executable or GNOME 50
SDK/runtime installed, so a full sandbox build cannot be run here without an
external tool/runtime installation. The equivalent Meson release
build-and-DESTDIR install is part of the release verification and remains the
minimum offline packaging proof in that environment.
