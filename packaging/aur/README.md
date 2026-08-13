# AUR packaging

This directory is the maintained source of truth for the `reprise-git` AUR
package. The package follows the default branch of the upstream Reprise git
repository and builds it through the same Meson entry point used by upstream.

`.SRCINFO` is generated metadata. Regenerate it after every `PKGBUILD` change:

```fish
cd packaging/aur
makepkg --printsrcinfo > .SRCINFO
git diff -- PKGBUILD .SRCINFO
```

Also regenerate `.SRCINFO` when the Meson install set changes, then check that
the package metadata and installed file set still agree.

## Manual AUR mirror

The AUR git repository is a separate publication target. From the Reprise
repository root, mirror the maintained files into a clean checkout, review the
diff, commit it, and perform the final push manually:

```fish
set aur_checkout ../reprise-git-aur
git clone ssh://aur@aur.archlinux.org/reprise-git.git $aur_checkout
cp packaging/aur/PKGBUILD packaging/aur/.SRCINFO packaging/aur/README.md $aur_checkout/
git -C $aur_checkout diff --check
git -C $aur_checkout status --short
git -C $aur_checkout add PKGBUILD .SRCINFO README.md
git -C $aur_checkout commit -m "chore: update reprise-git package"
git -C $aur_checkout push
```

These instructions do not claim that the package has been submitted. Creating
or updating the AUR package is a separate maintainer action.
