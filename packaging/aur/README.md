# Local Arch build recipe

`PKGBUILD` builds the default branch of the upstream Reprise git repository
through the same Meson entry point upstream uses. Build it locally with
`makepkg -si` from this directory.

The AUR does not accept AI-built projects, so Reprise has no AUR package and
nothing from this directory is pushed there. The directory name is historical.

`.SRCINFO` is generated metadata and is optional, because nothing in the
release tooling reads it. Regenerate it after a `PKGBUILD` change if you want it
to stay in step:

```fish
cd packaging/aur
makepkg --printsrcinfo > .SRCINFO
```
