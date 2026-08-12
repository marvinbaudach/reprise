#!/usr/bin/env python3
"""Dumpt den AT-SPI-Baum der Probe-App: Rolle, Name, Interfaces, Aktionen."""
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

WANTED_APP = sys.argv[1] if len(sys.argv) > 1 else "A11yProbe"


def describe(node):
    try:
        role = Atspi.Accessible.get_role_name(node)
    except Exception as exc:  # noqa: BLE001
        role = f"<err {exc}>"
    try:
        name = Atspi.Accessible.get_name(node)
    except Exception:  # noqa: BLE001
        name = ""
    try:
        ifaces = sorted(Atspi.Accessible.get_interfaces(node))
    except Exception:  # noqa: BLE001
        ifaces = []
    actions = []
    if "Action" in ifaces:
        try:
            n = Atspi.Action.get_n_actions(node)
            for i in range(n):
                try:
                    an = Atspi.Action.get_action_name(node, i)
                except Exception:  # noqa: BLE001
                    an = "?"
                try:
                    kb = Atspi.Action.get_key_binding(node, i)
                except Exception:  # noqa: BLE001
                    kb = ""
                actions.append(f"{an}{'/' + kb if kb else ''}")
        except Exception as exc:  # noqa: BLE001
            actions.append(f"<err {exc}>")
    return role, name, ifaces, actions


def walk(node, depth, out, limit_depth=12):
    role, name, ifaces, actions = describe(node)
    out.append(
        {
            "depth": depth,
            "role": role,
            "name": name,
            "actions": actions,
            "has_action_iface": "Action" in ifaces,
        }
    )
    if depth >= limit_depth:
        return
    try:
        count = Atspi.Accessible.get_child_count(node)
    except Exception:  # noqa: BLE001
        return
    for i in range(count):
        try:
            child = Atspi.Accessible.get_child_at_index(node, i)
        except Exception:  # noqa: BLE001
            continue
        if child is not None:
            walk(child, depth + 1, out, limit_depth)


def find_app(deadline):
    while time.time() < deadline:
        desktop = Atspi.get_desktop(0)
        for i in range(Atspi.Accessible.get_child_count(desktop)):
            app = Atspi.Accessible.get_child_at_index(desktop, i)
            if app is None:
                continue
            try:
                name = Atspi.Accessible.get_name(app)
            except Exception:  # noqa: BLE001
                continue
            if WANTED_APP.lower() in (name or "").lower():
                return app
        time.sleep(0.5)
    return None


def main():
    Atspi.init()
    names = []
    desktop = Atspi.get_desktop(0)
    for i in range(Atspi.Accessible.get_child_count(desktop)):
        child = Atspi.Accessible.get_child_at_index(desktop, i)
        if child is not None:
            names.append(Atspi.Accessible.get_name(child) or "<unnamed>")
    print(f"# apps on bus: {names}")

    app = find_app(time.time() + 20)
    if app is None:
        print("FEHLER: Probe-App nicht im AT-SPI-Baum gefunden", file=sys.stderr)
        return 2

    out = []
    walk(app, 0, out)
    for entry in out:
        indent = "  " * entry["depth"]
        acts = ", ".join(entry["actions"]) if entry["actions"] else "-"
        flag = "ACTION" if entry["actions"] else ("iface-only" if entry["has_action_iface"] else "")
        print(f"{indent}{entry['role']:<20} name={entry['name']!r:<32} actions=[{acts}] {flag}")
    return 0


sys.exit(main())
