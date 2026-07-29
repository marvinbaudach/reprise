#!/usr/bin/env bash
# Proves that the runtime binary and its activation metadata ship together,
# agree with each other, and land somewhere systemd actually looks (plan
# section 9.4/2).
#
# The failure this exists to prevent is specific and quiet: bus activation
# works perfectly on a development machine, where the binary is on $PATH and
# nobody needs the .service file, and is stone dead on a user's, where the
# package installed the binary and forgot the metadata — or installed
# metadata pointing at a path the binary is not at, or installed the unit to
# a directory systemd's user manager never scans. Neither shows up in a unit
# test or in a running app.
#
# Two prefixes are checked, not one: distro packaging (`/usr`) and the
# developer/user install README.md documents
# (`meson setup ... --prefix="$HOME/.local"`). A directory that is correct
# for one and wrong for the other is exactly the class of bug this script
# exists to catch — see data/meson.build's comment on why the obvious
# `lib/systemd/user` is invisible to systemd under a per-user prefix.
#
# The listing comes from Meson itself (`introspect --installed`), not from
# grepping meson.build, so a rule that exists but does not install — or
# installs somewhere unexpected — is caught. The generated files are read
# from the configure output, so the @bindir@ substitution is checked as it
# will actually be shipped.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

echo "== Runtime service install =="

if [[ -z "${HOME:-}" ]]; then
  echo "\$HOME is not set; cannot verify the per-user install prefix documented in README.md" >&2
  exit 1
fi

prefixes=("/usr" "$HOME/.local")

build_dirs=()
cleanup() {
  local d
  for d in "${build_dirs[@]:-}"; do
    [[ -n "$d" ]] && rm -rf "$d"
  done
}
trap cleanup EXIT

field() {
  # Reads `Key=value` from a keyfile-shaped file, first match wins.
  rg --no-line-number --replace '$1' "^$2=(.*)$" "$1" | head -1
}

expect() {
  local label=$1 actual=$2 wanted=$3
  if [[ "$actual" != "$wanted" ]]; then
    echo "$label is '$actual'; expected '$wanted'" >&2
    exit 1
  fi
}

# Reads the actual install destination Meson chose for a generated file, by
# matching the *source* side of the introspect mapping. This is deliberately
# not a hardcoded expected path: the whole point of this script is to catch
# meson.build installing an artifact somewhere unexpected, so the "actual"
# side has to come from what Meson really did, not from an assumption baked
# into the test.
installed_dest() {
  local installed=$1 source_suffix_regex=$2
  # --only-matching: the introspect JSON is emitted as a single line, and
  # --replace alone substitutes only the matched span while still printing
  # the rest of that (very long) line untouched — silently returning the
  # whole document instead of the one field wanted.
  printf '%s' "$installed" \
    | rg --no-line-number --only-matching --replace '$1' "\"[^\"]*$source_suffix_regex\": \"([^\"]*)\"" \
    | head -1
}

# Directories systemd's --user manager actually scans for unit files.
# Preferred source: `systemd-analyze --user unit-paths` on this machine,
# because it reports the real manager's real search list for the real
# $HOME/$XDG_DATA_HOME of whoever runs this check — no assumption needed
# about how those variables resolve in CI. Falls back to the load path
# documented in systemd.unit(5) (the package-tree entries under $HOME and
# under /usr, /usr/local — excluding the purely runtime/transient/generator
# paths, which no installer writes to) for environments without systemd
# installed at all, e.g. a minimal container image. The fallback is not a
# guess: it is what the manual documents as fixed regardless of environment,
# so an absent systemd-analyze must not silently skip the check.
search_path_dirs() {
  if command -v systemd-analyze >/dev/null 2>&1; then
    systemd-analyze --user unit-paths 2>/dev/null
  else
    printf '%s\n' \
      "$HOME/.config/systemd/user" \
      "$HOME/.local/share/systemd/user" \
      "/etc/systemd/user" \
      "/usr/local/share/systemd/user" \
      "/usr/share/systemd/user" \
      "/usr/local/lib/systemd/user" \
      "/usr/lib/systemd/user"
  fi
}

on_search_path() {
  local dir=$1 candidate
  while IFS= read -r candidate; do
    [[ "$dir" == "$candidate" ]] && return 0
  done < <(search_path_dirs)
  return 1
}

# The bus name the code claims, read from the source of truth rather than
# repeated here — if the constant changes, this check follows it and the
# metadata has to follow too. That source is the *protocol* crate: the
# address is part of the contract, and a client must be able to learn it
# without depending on the service.
bus_name=$(rg --no-line-number --replace '$1' \
  '^pub const BUS_NAME: &str = "(.*)";$' \
  crates/reprise-runtime-protocol/src/endpoint.rs | head -1)
if [[ -z "$bus_name" ]]; then
  echo "cannot read BUS_NAME from the runtime service source" >&2
  exit 1
fi

for prefix in "${prefixes[@]}"; do
  echo "-- prefix: $prefix --"
  build_dir=$(mktemp -d)
  build_dirs+=("$build_dir")

  if ! meson setup "$build_dir" --prefix "$prefix" >"$build_dir/setup.log" 2>&1; then
    echo "meson setup failed for prefix $prefix; the install contract cannot be verified:" >&2
    tail -20 "$build_dir/setup.log" >&2
    exit 1
  fi

  installed=$(meson introspect "$build_dir" --installed)

  # Anchored on the closing quote so e.g. the binary pattern does not also
  # match the unit file, which shares its stem.
  binary=$(installed_dest "$installed" '/reprise-runtime')
  activation=$(installed_dest "$installed" '/data/org\.reprise\.Reprise1\.service')
  unit=$(installed_dest "$installed" '/data/reprise-runtime\.service')

  for name_dest in "binary:$binary" "activation:$activation" "unit:$unit"; do
    name=${name_dest%%:*}
    dest=${name_dest#*:}
    if [[ -z "$dest" ]]; then
      echo "not installed for prefix $prefix: $name" >&2
      echo "the runtime binary and its activation metadata must ship together" >&2
      exit 1
    fi
  done

  unit_dir=$(dirname "$unit")
  if ! on_search_path "$unit_dir"; then
    echo "unit installs to '$unit_dir', which is not on systemd's --user unit search path" >&2
    echo "(checked against: $(search_path_dirs | paste -sd, -))" >&2
    exit 1
  fi

  # The generated (not the .in) files, so substitution is what gets checked.
  generated_activation="$build_dir/data/org.reprise.Reprise1.service"
  generated_unit="$build_dir/data/reprise-runtime.service"

  expect "activation Name" "$(field "$generated_activation" Name)" "$bus_name"
  expect "activation Exec" "$(field "$generated_activation" Exec)" "$binary"
  expect "activation SystemdService" \
    "$(field "$generated_activation" SystemdService)" "$(basename "$unit")"
  expect "unit BusName" "$(field "$generated_unit" BusName)" "$bus_name"
  expect "unit ExecStart" "$(field "$generated_unit" ExecStart)" "$binary"

  # The activation file's own name must be the bus name: dbus-daemon looks it
  # up by filename, so a correct `Name=` inside a wrongly named file never
  # runs.
  expect "activation file name" "$(basename "$activation")" "$bus_name.service"

  # Type=dbus is what makes systemd treat "owns the bus name" as "started".
  # With Type=simple the unit counts as started before the handshake is
  # possible, and a client's first command races the runtime's own startup.
  expect "unit Type" "$(field "$generated_unit" Type)" "dbus"
done

echo "Runtime service install check passed"
