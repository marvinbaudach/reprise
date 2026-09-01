#!/usr/bin/env python3
"""Drive the running Reprise window through the showreel scenes.

Every action goes through AT-SPI so the window keeps focus and the
compositor never sees a synthetic pointer — the recording shows the app
alone. The script writes a timeline so the cut can be derived from the
run instead of eyeballed.
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rp import app_root, find, walk, actions, do, work_dir  # noqa: E402

T0 = time.time()
TIMELINE = []
OUT = os.path.join(work_dir(), 'timeline.json')


def frame():
    return app_root().get_child_at_index(0)


def mark(name):
    """Open a scene; the previous one ends here."""
    now = time.time() - T0
    if TIMELINE:
        TIMELINE[-1]['end'] = round(now, 2)
    TIMELINE.append({'scene': name, 'start': round(now, 2)})
    print(f'[{now:6.1f}s] {name}', flush=True)


def sidebar(label, dwell=6.0):
    b = find(label, role='button', root=frame())
    if b is None:
        print(f'  !! sidebar {label} not found', flush=True)
        return False
    do(b)
    time.sleep(dwell)
    return True


def tab(label, dwell=7.0):
    for n in walk(frame()):
        if n.get_role_name() == 'page tab' and n.get_name() == label:
            do(n)
            time.sleep(dwell)
            return True
    print(f'  !! tab {label} not found', flush=True)
    return False


def prefs_page(page):
    for n in walk(frame()):
        if n.get_name() == page:
            p = n
            for _ in range(6):
                p = p.get_parent()
                if p is None:
                    return False
                if p.get_role_name() == 'list item':
                    return p.get_parent().get_selection_iface().select_child(p.get_index_in_parent())
    return False


def click_named(name, role=None, dwell=1.2, nth=0):
    hits = [n for n in walk(frame())
            if n.get_name() == name and (role is None or n.get_role_name() == role) and 'click' in actions(n)]
    if len(hits) <= nth:
        print(f'  !! {name} not clickable', flush=True)
        return False
    do(hits[nth])
    time.sleep(dwell)
    return True


def search(term, per_char=0.28, dwell=6.0):
    do(frame(), 'win.focus-search')
    time.sleep(1.2)
    e = find('Search all fields', role='entry', root=frame())
    if e is None:
        print('  !! search entry not found', flush=True)
        return False
    et = e.get_editable_text_iface()
    for i, ch in enumerate(term):
        et.insert_text(i, ch, 1)
        time.sleep(per_char)
    time.sleep(dwell)
    n = e.get_text_iface().get_character_count()
    et.delete_text(0, n)
    time.sleep(0.8)
    do(frame(), 'win.clear-all-filters')
    time.sleep(0.8)
    return True


# ---------------------------------------------------------------- the take
time.sleep(2.5)                      # pre-roll: a still frame to cut into

mark('library')
sidebar('Music', dwell=6.0)

mark('search')
search('lorna', dwell=6.0)

mark('releases')
sidebar('Releases', dwell=6.5)

mark('concerts')
sidebar('Concerts', dwell=5.5)

mark('podcasts')
sidebar('Podcasts', dwell=6.5)

mark('youtube')
sidebar('YouTube', dwell=6.5)

mark('sync')
sidebar('DEVICES', dwell=1.5)
click_named('Open Pixel 10 Pro XL', role='button', dwell=6.5)

mark('library-doctor')
sidebar('Library Doctor', dwell=7.5)

mark('visuals')
sidebar('Music', dwell=1.5)
tab('Visuals', dwell=9.0)

mark('lyrics')
tab('Lyrics', dwell=7.5)

mark('stats')
tab('Up Next', dwell=0.8)
sidebar('My Stats', dwell=7.5)

mark('settings-layout')
do(frame(), 'win.preferences')
time.sleep(2.2)
prefs_page('Layout')
time.sleep(3.5)
click_named('Top', role='radio button', dwell=3.5)       # player bar to the top
click_named('Bottom', role='radio button', dwell=3.0)    # and back

mark('settings-plugins')
prefs_page('Plugins')
time.sleep(4.0)

mark('outro')
for n in walk(frame()):
    if n.get_name() == 'Close' and n.get_role_name() == 'button':
        do(n)
        break
time.sleep(1.5)
sidebar('Music', dwell=3.0)

TIMELINE[-1]['end'] = round(time.time() - T0, 2)
json.dump({'t0': T0, 'scenes': TIMELINE}, open(OUT, 'w'), indent=2)
print(f'\ntotal {time.time() - T0:.1f}s -> {OUT}', flush=True)
