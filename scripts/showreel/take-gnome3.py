#!/usr/bin/env python3
"""Second pickup take: the two flows the 60 s film never filmed.

Both are adds, not views. The podcast one opens on Apple's country chart —
`strings_podcasts.rs` heads it `PODCASTS · TOP IN {country}` — so the take
must let the chart render and subscribe *from it*, never type past it the way
`take-gnome2.py` did. YouTube has no chart (its dialog carries a genre chip),
so there the flow is a search by name, which is what the shot wants anyway.

Neither dialog's entries are known ahead of time: chart rows change daily and a
YouTube result is whatever the API returns. Both are therefore found by the
accessible name every subscribe button shares — `Subscribe to {source}` from
`strings_sources.rs` — and the chosen row is logged, so the cut can be checked
against what was actually on screen.
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rp import app_root, walk, actions, do, work_dir  # noqa: E402

import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

T0, TL = time.time(), []
OUT = os.path.join(work_dir(), 'timeline3.json')

# Which chart row to take. Not row 0: the top of a live chart is whatever the
# day handed you, and this ends up on a landing page for months. Override with
# SHOWREEL_CHART_ROW once the take operator has looked at the rendered chart.
CHART_ROW = int(os.environ.get('SHOWREEL_CHART_ROW', '1'))
CHANNEL = os.environ.get('SHOWREEL_CHANNEL', 'NPR Music')
# Which flow to shoot. The first run lost the podcast half — the window fell
# out of focus mid-take and the sidebar click never landed — while the YouTube
# half came out fine, so re-shooting has to be able to take one without the
# other rather than spending 80 s to replace 1.8.
ONLY = os.environ.get('SHOWREEL_ONLY', 'both')
SUBSCRIBE_PREFIX = 'Subscribe to '


def frame():
    return app_root().get_child_at_index(0)


def mark(name):
    now = time.time() - T0
    if TL:
        TL[-1]['end'] = round(now, 2)
    TL.append({'scene': name, 'start': round(now, 2)})
    print(f'[{now:6.1f}s] {name}', flush=True)


def note(key, value):
    """Record what was actually on screen, so the cut is checkable later."""
    if TL:
        TL[-1][key] = value
    print(f'         {key}: {value}', flush=True)


def click(name, role=None, dwell=1.0, nth=0):
    hits = [n for n in walk(frame())
            if n.get_name() == name
            and (role is None or n.get_role_name() == role)
            and 'click' in actions(n)]
    if len(hits) <= nth:
        print(f'  !! {name} not clickable', flush=True)
        return False
    do(hits[nth])
    time.sleep(dwell)
    return True


def type_into(node, text, per_char=0.14):
    et = node.get_editable_text_iface()
    for i, ch in enumerate(text):
        et.insert_text(i, ch, 1)
        time.sleep(per_char)


def dialog_entry(timeout=6.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        hits = [n for n in walk(frame())
                if n.get_role_name() in ('entry', 'text') and 'activate' in actions(n)]
        if hits:
            return hits[0]
        time.sleep(0.3)
    return None


def showing(node):
    try:
        return node.get_state_set().contains(Atspi.StateType.SHOWING)
    except Exception:
        return False


def wait_for_chart(timeout=45.0):
    """The chart comes over the network, and existing is not the same as being
    on screen. Two runs died on a fixed 4 s and 7 s sleep and reported an empty
    chart, which reads exactly like being offline; a third polled for the rows,
    found eleven, and filmed a dialog still saying "Searching…" — AT-SPI hands
    the rows over before they are painted. SHOWING is the state that matches
    what a camera would see."""
    started = time.time()
    deadline = started + timeout
    while time.time() < deadline:
        rows = subscribe_buttons()
        if len(rows) >= 3:
            # The rows exist; the dialog still says "Searching…". SHOWING does
            # not flip for these, so there is nothing left to poll on — the
            # paint and the covers cost a fixed extra beat and that is that.
            time.sleep(12.0)
            return subscribe_buttons(), round(time.time() - started, 1)
        time.sleep(0.5)
    return [], timeout


def wait_for_show(name, timeout=25.0):
    """The subscribed show reaches the list a good while after the dialog
    closes. Polling for it by name beats guessing a dwell."""
    started = time.time()
    while time.time() - started < timeout:
        hits = [n for n in walk(frame())
                if n.get_name() == name and n.get_role_name() == 'button'
                and 'click' in actions(n) and showing(n)]
        if hits:
            return hits[0], round(time.time() - started, 1)
        time.sleep(0.5)
    return None, timeout


def subscribe_buttons():
    return [n for n in walk(frame())
            if (n.get_name() or '').startswith(SUBSCRIBE_PREFIX)
            and n.get_role_name() == 'button'
            and 'click' in actions(n)]


def chart_heading():
    """The chart's own heading, so the log proves a chart was on screen rather
    than a search result — the two look alike in a shot list and nothing else
    distinguishes them after the fact."""
    for n in walk(frame()):
        name = n.get_name() or ''
        if 'TOP IN' in name.upper():
            return name
    return None


def close_dialog(dwell=1.0):
    for label in ('Cancel', 'Close'):
        if click(label, role='button', dwell=dwell):
            return True
    return False


time.sleep(2.5)

# ---------------------------------------------------------------- podcast chart
if ONLY in ('both', 'podcast'):
    mark('podcast-chart')
    click('Podcasts', role='button', dwell=2.5)
    click('Add podcast', role='button', dwell=1.0)
    rows, waited = wait_for_chart()
    note('chart_waited_s', waited)
    time.sleep(3.0)                                      # the covers, after the rows
    note('heading', chart_heading())
    note('chart_rows', len(rows))
    if rows:
        row = rows[min(CHART_ROW, len(rows) - 1)]
        show = (row.get_name() or '')[len(SUBSCRIBE_PREFIX):]
        note('picked', show)
        mark('podcast-subscribe')
        do(row)
        time.sleep(3.5)
        close_dialog(dwell=1.0)
        time.sleep(5.0)                                  # the new show lands in the list

        # Subscribing is only half the claim. A show that was just added and
        # then sits there proves the button worked; an episode that starts
        # proves the feature does. The play button is found by name rather
        # than by position because the row order is whatever the feed returns.
        mark('podcast-play')
        node, show_waited = wait_for_show(show)
        note('show_waited_s', show_waited)
        if node is not None:
            do(node)
            time.sleep(9.0)                              # episodes and their artwork
            plays = [n for n in walk(frame())
                     if n.get_role_name() == 'button'
                     and 'play' in (n.get_name() or '').lower()
                     and 'click' in actions(n) and showing(n)]
            note('play_buttons', len(plays))
            if plays:
                note('playing', plays[0].get_name())
                do(plays[0])
                time.sleep(9.0)                          # the episode starts
            else:
                print('  !! no play button on the show', flush=True)
        else:
            print('  !! could not open the new show', flush=True)
    else:
        print('  !! no chart rows — offline, or the chart failed to load', flush=True)
        close_dialog()

# ------------------------------------------------------------------- youtube add
if ONLY in ('both', 'youtube'):
    mark('youtube-search')
    click('YouTube', role='button', dwell=2.5)
    click('Add channel', role='button', dwell=2.0)
    entry = dialog_entry()
    if entry is None:
        print('  !! channel dialog entry missing', flush=True)
    else:
        type_into(entry, CHANNEL)
        time.sleep(0.8)
        click('Search', role='button', dwell=4.0)
        hits = subscribe_buttons()
        note('results', len(hits))
        if hits:
            name = (hits[0].get_name() or '')[len(SUBSCRIBE_PREFIX):]
            note('picked', name)
            mark('youtube-subscribe')
            do(hits[0])
            time.sleep(3.5)
            close_dialog(dwell=1.0)
            time.sleep(3.0)                              # the channel and its uploads
        else:
            print('  !! no channel results', flush=True)
            close_dialog()

mark('tail')
click('Music', role='button', dwell=2.0)

TL[-1]['end'] = round(time.time() - T0, 2)
json.dump({'t0': T0, 'scenes': TL}, open(OUT, 'w'), indent=2)
print(f'total {time.time() - T0:.1f}s -> {OUT}', flush=True)
