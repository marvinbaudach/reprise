#!/usr/bin/env python3
"""Read one application's accessibility tree straight off the a11y bus.

cua-driver's `get_window_state` walk reports only interactive and structural
roles — measured on 0.21.0, the role `label` never appears at all, and the tool
has no switch to include it. Anything a scenario needs to claim about static
text therefore cannot be decided through the driver. This probe talks to
AT-SPI directly so those claims stay measurable.

It always writes the full tree it saw to `--dump`, so a failure is inspectable
rather than just asserted.
"""

import argparse
import json
import re
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


def collect(node, depth, out, max_depth):
    try:
        role = node.get_role_name()
        name = node.get_name() or ""
    except Exception as error:  # a node can vanish mid-walk
        out.append({"depth": depth, "role": "<gone>", "name": "", "error": str(error)})
        return
    out.append({"depth": depth, "role": role, "name": name})
    if depth >= max_depth:
        return
    try:
        count = node.get_child_count()
    except Exception:
        return
    for index in range(count):
        try:
            child = node.get_child_at_index(index)
        except Exception:
            continue
        if child is not None:
            collect(child, depth + 1, out, max_depth)


def application_for(pid, deadline):
    """The AT-SPI bridge registers seconds after the window; poll for it."""
    while True:
        desktop = Atspi.get_desktop(0)
        for index in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(index)
            if app is None:
                continue
            try:
                if app.get_process_id() == pid:
                    return app
            except Exception:
                continue
        if time.monotonic() >= deadline:
            return None
        time.sleep(0.25)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--dump", required=True, help="where to write the tree it saw")
    parser.add_argument("--role", action="append", default=[],
                        help="restrict matches to these roles; omit to accept any")
    parser.add_argument("--name-matches", required=True, help="regex the name must match")
    parser.add_argument("--min-count", type=int, default=1)
    parser.add_argument("--timeout-secs", type=float, default=10.0)
    parser.add_argument("--max-depth", type=int, default=40)
    args = parser.parse_args()

    Atspi.init()
    deadline = time.monotonic() + args.timeout_secs
    app = application_for(args.pid, deadline)
    if app is None:
        json.dump({"error": "no AT-SPI application for pid", "pid": args.pid},
                  open(args.dump, "w"), indent=2)
        print(f"AT-SPI exposes no application for pid {args.pid}", file=sys.stderr)
        return 1

    pattern = re.compile(args.name_matches)
    roles = set(args.role)

    # The tree fills in after the window appears, so retry the whole walk
    # rather than judging the first snapshot of it.
    while True:
        nodes = []
        collect(app, 0, nodes, args.max_depth)
        matches = [n for n in nodes
                   if pattern.search(n["name"]) and (not roles or n["role"] in roles)]
        if len(matches) >= args.min_count or time.monotonic() >= deadline:
            break
        time.sleep(0.25)

    json.dump({
        "pid": args.pid,
        "node_count": len(nodes),
        "roles_seen": sorted({n["role"] for n in nodes}),
        "required": {"role": sorted(roles), "name_matches": args.name_matches,
                     "min_count": args.min_count},
        "matches": matches,
        "nodes": nodes,
    }, open(args.dump, "w"), indent=2)

    if len(matches) < args.min_count:
        print(f"AT-SPI exposes {len(matches)} node(s) matching "
              f"/{args.name_matches}/ (need {args.min_count}); tree in {args.dump}",
              file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
