#!/usr/bin/env python3
"""Pickup take: subscribing to a known podcast, the search with its suggestions,
and lyrics on a track that actually has them."""
import json
import os
import sys
import time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rp import app_root, find, walk, actions, do, work_dir  # noqa: E402
import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

T0, TL = time.time(), []
OUT = os.path.join(work_dir(), 'timeline2.json')
PODCAST = 'Darknet Diaries'


def frame():
    return app_root().get_child_at_index(0)


def mark(name):
    now = time.time() - T0
    if TL:
        TL[-1]['end'] = round(now, 2)
    TL.append({'scene': name, 'start': round(now, 2)})
    print(f'[{now:6.1f}s] {name}', flush=True)


def click(name, role=None, dwell=1.0, nth=0):
    hits = [n for n in walk(frame())
            if n.get_name() == name and (role is None or n.get_role_name() == role) and 'click' in actions(n)]
    if len(hits) <= nth:
        print(f'  !! {name} not clickable', flush=True)
        return False
    do(hits[nth])
    time.sleep(dwell)
    return True


def tab(label, dwell=0.0):
    for n in walk(frame()):
        if n.get_role_name() == 'page tab' and n.get_name() == label:
            do(n)
            time.sleep(dwell)
            return True
    return False


def type_into(node, text, per_char=0.14):
    et = node.get_editable_text_iface()
    for i, ch in enumerate(text):
        et.insert_text(i, ch, 1)
        time.sleep(per_char)


def open_search(timeout=6.0):
    t = find('Search all fields', role='toggle button', root=frame())
    if t is not None and not t.get_state_set().contains(Atspi.StateType.CHECKED):
        do(t)
    deadline = time.time() + timeout
    while time.time() < deadline:
        e = find('Search all fields', role='entry', root=frame())
        if e is not None:
            return e
        time.sleep(0.3)
    return None


time.sleep(2.5)

mark('podcast-add')
click('Podcasts', role='button', dwell=3.0)
click('Add podcast', role='button', dwell=2.5)
dlg = [n for n in walk(frame()) if n.get_role_name() in ('entry', 'text') and 'activate' in actions(n)]
if dlg:
    type_into(dlg[0], PODCAST)
    time.sleep(1.0)
    click('Search', role='button', dwell=5.0)          # results with their covers
    click(f'Subscribe to {PODCAST}', role='button', dwell=5.0)
    if not click('Cancel', role='button', dwell=1.0):
        click('Close', role='button', dwell=1.0)
    time.sleep(4.0)                                     # the new show in the list
else:
    print('  !! podcast dialog entry missing', flush=True)

mark('search')
click('Music', role='button', dwell=2.0)
e = open_search()
if e is None:
    print('  !! search entry never appeared', flush=True)
else:
    time.sleep(1.0)
    type_into(e, 'lorna', per_char=0.32)
    time.sleep(7.0)
    e.get_editable_text_iface().delete_text(0, e.get_text_iface().get_character_count())
    time.sleep(1.0)
    do(frame(), 'win.clear-all-filters')
    time.sleep(1.5)

mark('lyrics')
tab('Lyrics', dwell=10.0)

mark('tail')
tab('Up Next', dwell=2.0)

TL[-1]['end'] = round(time.time() - T0, 2)
json.dump({'t0': T0, 'scenes': TL}, open(OUT, 'w'), indent=2)
print(f'total {time.time() - T0:.1f}s', flush=True)
