#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

echo "== Rust gates =="
scripts/tests/qa-linters.sh
scripts/tests/msrv.sh
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
env XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" cargo test --workspace
cargo audit
cargo check --manifest-path crates/reprise-core/Cargo.toml
if cargo tree -p reprise-core | grep -Eq 'gtk4|libadwaita|gstreamer|zbus'; then
  echo "reprise-core purity check failed" >&2
  exit 1
fi

echo "== gettext =="
xgettext --directory=. --files-from=po/POTFILES.in --output="$tmp_root/reprise.pot" \
  --from-code=UTF-8 --language=Rust '--keyword=N_!:1' --keyword=plural:1,2 \
  --package-name=Reprise --package-version=0.1.0 \
  --msgid-bugs-address='Marvin Baudach' --copyright-holder='Marvin Baudach'
msgfmt --check --check-format -o "$tmp_root/reprise.mo" po/de.po
msgcmp --use-fuzzy po/de.po "$tmp_root/reprise.pot"
test -z "$(msgattrib --untranslated po/de.po)"
test -z "$(msgattrib --only-fuzzy po/de.po)"

echo "== Desktop metadata =="
desktop-file-validate data/org.reprise.Reprise.desktop
appstreamcli validate --pedantic --no-net \
  --override=cid-contains-uppercase-letter=info,url-homepage-missing=info \
  data/org.reprise.Reprise.metainfo.xml
xmllint --noout data/org.reprise.Reprise.metainfo.xml \
  data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg \
  data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg

echo "== Flatpak manifest and Cargo sources =="
python3 -c 'import sys, yaml; data=yaml.safe_load(open(sys.argv[1], encoding="utf-8")); assert data["app-id"] == "org.reprise.Reprise"; assert data["runtime"] == "org.gnome.Platform"; assert data["runtime-version"] == "50"; assert data["sdk"] == "org.gnome.Sdk"' org.reprise.Reprise.yml
jq empty flatpak/cargo-sources.json
awk -F'"' '/^checksum = / { print $2 }' Cargo.lock | sort > "$tmp_root/lock-checksums"
jq -r '.[] | select(.type == "archive") | .sha256' flatpak/cargo-sources.json | sort > "$tmp_root/source-checksums"
cmp "$tmp_root/lock-checksums" "$tmp_root/source-checksums"
test "$(rg -c '^checksum = ' Cargo.lock)" -eq "$(jq '[.[] | select(.type == "archive")] | length' flatpak/cargo-sources.json)"
bash scripts/check-flatpak-device-permissions.sh org.reprise.Reprise.yml
if command -v flatpak-builder-lint >/dev/null; then
  flatpak-builder-lint manifest org.reprise.Reprise.yml
elif flatpak info org.flatpak.Builder >/dev/null 2>&1; then
  flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest org.reprise.Reprise.yml
else
  echo "SKIP: flatpak-builder-lint is not installed"
fi

echo "== Source file sizes =="
while IFS= read -r file; do
  lines=$(wc -l < "$file")
  case "$file" in
    crates/reprise-core/src/library/playlists.rs) limit=1242 ;;
    crates/reprise-core/src/queue.rs) limit=1223 ;;
    crates/reprise-core/src/library/scanner_tests.rs) limit=805 ;;
    *) limit=799 ;;
  esac
  if (( lines > limit )); then
    echo "$file has $lines lines (limit $limit)" >&2
    exit 1
  fi
done < <(find crates -name '*.rs' -type f | sort)

echo "== Optimized Meson install =="
meson setup "$tmp_root/build" . --prefix=/usr -Dprofile=release
meson compile -C "$tmp_root/build"
DESTDIR="$tmp_root/root" meson install -C "$tmp_root/build"
test -x "$tmp_root/root/usr/bin/reprise"
test -f "$tmp_root/root/usr/share/applications/org.reprise.Reprise.desktop"
test -f "$tmp_root/root/usr/share/metainfo/org.reprise.Reprise.metainfo.xml"
test -f "$tmp_root/root/usr/share/icons/hicolor/scalable/apps/org.reprise.Reprise.svg"
test -f "$tmp_root/root/usr/share/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg"
test -f "$tmp_root/root/usr/share/locale/de/LC_MESSAGES/reprise.mo"

echo "Release checks passed"
