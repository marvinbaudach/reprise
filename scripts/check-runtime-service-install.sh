#!/usr/bin/env bash
# Proves that the runtime binary and its activation metadata ship together and
# agree with each other (plan section 9.4/2).
#
# The failure this exists to prevent is specific and quiet: bus activation
# works perfectly on a development machine, where the binary is on $PATH and
# nobody needs the .service file, and is stone dead on a user's, where the
# package installed the binary and forgot the metadata — or installed
# metadata pointing at a path the binary is not at. Neither shows up in a
# unit test or in a running app.
#
# The listing comes from Meson itself (`introspect --installed`), not from
# grepping meson.build, so a rule that exists but does not install is caught.
# The generated files are read from the configure output, so the @bindir@
# substitution is checked as it will actually be shipped.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

echo "== Runtime service install =="

prefix=/usr
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT

if ! meson setup "$build_dir" --prefix "$prefix" >"$build_dir/setup.log" 2>&1; then
  echo "meson setup failed; the install contract cannot be verified:" >&2
  tail -20 "$build_dir/setup.log" >&2
  exit 1
fi

installed=$(meson introspect "$build_dir" --installed)

# The three artifacts, by their installed destination.
binary="$prefix/bin/reprise-runtime"
activation="$prefix/share/dbus-1/services/org.reprise.Reprise1.service"
unit="$prefix/lib/systemd/user/reprise-runtime.service"

for destination in "$binary" "$activation" "$unit"; do
  if ! printf '%s' "$installed" | rg --fixed-strings --quiet "\"$destination\""; then
    echo "not installed: $destination" >&2
    echo "the runtime binary and its activation metadata must ship together" >&2
    exit 1
  fi
done

# The generated (not the .in) files, so substitution is what gets checked.
generated_activation="$build_dir/data/org.reprise.Reprise1.service"
generated_unit="$build_dir/data/reprise-runtime.service"

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

# The bus name the code claims, read from the source of truth rather than
# repeated here — if the constant changes, this check follows it and the
# metadata has to follow too.
bus_name=$(rg --no-line-number --replace '$1' \
  '^pub const BUS_NAME: &str = "(.*)";$' \
  crates/reprise-platform-linux/src/runtime_service/service.rs | head -1)
if [[ -z "$bus_name" ]]; then
  echo "cannot read BUS_NAME from the runtime service source" >&2
  exit 1
fi

expect "activation Name" "$(field "$generated_activation" Name)" "$bus_name"
expect "activation Exec" "$(field "$generated_activation" Exec)" "$binary"
expect "activation SystemdService" \
  "$(field "$generated_activation" SystemdService)" "$(basename "$unit")"
expect "unit BusName" "$(field "$generated_unit" BusName)" "$bus_name"
expect "unit ExecStart" "$(field "$generated_unit" ExecStart)" "$binary"

# The activation file's own name must be the bus name: dbus-daemon looks it up
# by filename, so a correct `Name=` inside a wrongly named file never runs.
expect "activation file name" "$(basename "$activation")" "$bus_name.service"

# Type=dbus is what makes systemd treat "owns the bus name" as "started". With
# Type=simple the unit counts as started before the handshake is possible, and
# a client's first command races the runtime's own startup.
expect "unit Type" "$(field "$generated_unit" Type)" "dbus"

echo "Runtime service install check passed"
