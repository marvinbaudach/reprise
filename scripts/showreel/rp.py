#!/usr/bin/env python3
"""AT-SPI driving helpers for the Reprise window (background, no focus steal)."""
import os

import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

Atspi.init()

APP = 'reprise'


def app_root():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        a = desktop.get_child_at_index(i)
        if a is not None and a.get_name() == APP and a.get_child_count():
            return a
    raise SystemExit('Reprise not on the accessibility bus')


def walk(node, depth=0, limit=40):
    if depth > limit:
        return
    yield node
    for i in range(node.get_child_count()):
        c = node.get_child_at_index(i)
        if c is not None:
            yield from walk(c, depth + 1, limit)


def find(name, role=None, root=None, exact=True):
    root = root or app_root()
    for n in walk(root):
        try:
            nm = n.get_name()
        except Exception:
            continue
        hit = (nm == name) if exact else (name.lower() in (nm or '').lower())
        if hit and (role is None or n.get_role_name() == role):
            return n
    return None


def actions(node):
    try:
        ai = node.get_action_iface()
        return [ai.get_action_name(i) for i in range(ai.get_n_actions())]
    except Exception:
        return []


def do(node, action='click'):
    ai = node.get_action_iface()
    names = [ai.get_action_name(i) for i in range(ai.get_n_actions())]
    if action not in names:
        return False, names
    return ai.do_action(names.index(action)), names


def work_dir():
    """Where a run drops its intermediates. Mirrors common.sh so the shell and
    the Python halves of a take agree without either owning the other."""
    base = os.environ.get('SHOWREEL_WORK')
    if not base:
        cache = os.environ.get('XDG_CACHE_HOME') or os.path.expanduser('~/.cache')
        base = os.path.join(cache, 'reprise-showreel')
    os.makedirs(base, exist_ok=True)
    return base
