#!/usr/bin/env python3
"""Write compile-time build credentials into a Flatpak manifest, in place.

`option_env!` reads these at compile time, so they have to be present in the
environment of the cargo invocation itself. For the Flatpak build that
invocation happens inside flatpak-builder's sandbox, and **flatpak-builder has
no `--env` option** — a variable exported in the CI job cannot reach it. The
manifest's `build-options.env` is the only channel, so CI patches the
checked-out copy here; the committed manifest stays clean.

Absent variables are not an error: forks and pull requests have no secrets, and
the build must still work — `option_env!` then yields `None` and the app falls
back to per-user credentials. A missing *anchor* is an error, because that
means the manifest moved and the credential would be dropped silently, which is
the failure this whole path is written to avoid.

Values are never printed.
"""

import json
import os
import re
import sys

VARIABLES = ("REPRISE_LASTFM_API_KEY", "REPRISE_LASTFM_SHARED_SECRET")
ANCHOR = re.compile(r"^(\s+)CARGO_NET_OFFLINE:")


def main(argv):
    if len(argv) != 2:
        print(f"usage: {argv[0]} <manifest.yml>", file=sys.stderr)
        return 2
    manifest = argv[1]

    present = [name for name in VARIABLES if os.environ.get(name, "").strip()]
    if not present:
        print("no build credentials in the environment; "
              "building without bundled keys")
        return 0

    lines = open(manifest).read().splitlines(keepends=True)
    for index, line in enumerate(lines):
        match = ANCHOR.match(line)
        if match:
            break
    else:
        print(f"{manifest}: no 'CARGO_NET_OFFLINE:' line to anchor build "
              f"credentials to; refusing to build a release that would "
              f"silently omit {', '.join(present)}", file=sys.stderr)
        return 1

    indent = match.group(1)
    # json.dumps produces a double-quoted scalar with the escaping YAML wants.
    injected = [f"{indent}{name}: {json.dumps(os.environ[name])}\n"
                for name in present]
    lines[index + 1:index + 1] = injected
    open(manifest, "w").write("".join(lines))

    print(f"injected {len(present)} build credential(s) into {manifest}: "
          f"{', '.join(present)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
