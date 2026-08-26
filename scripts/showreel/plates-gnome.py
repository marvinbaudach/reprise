#!/usr/bin/env python3
"""Re-shoot the eight GNOME plates the showroom ships, from the running app.

Native PNG per shot (the compositor's own pixels), cropped to the window and
resized to the ladder's top step — sharper than a frame pulled out of the take.
"""
import os
import subprocess
import sys
import time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rp import app_root, walk, actions, do, work_dir  # noqa: E402

OUT = os.path.join(work_dir(), 'plates')
subprocess.run(['mkdir', '-p', OUT], check=True)


def frame():
    return app_root().get_child_at_index(0)


def click(name, role=None, nth=0, dwell=1.0):
    hits = [n for n in walk(frame())
            if n.get_name() == name and (role is None or n.get_role_name() == role) and 'click' in actions(n)]
    if len(hits) <= nth:
        print(f'  !! {name} not clickable')
        return False
    do(hits[nth])
    time.sleep(dwell)
    return True


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


def shoot(name, settle=2.0):
    time.sleep(settle)
    raw = f'{OUT}/{name}-raw.png'
    subprocess.run(['cua-driver', 'get_desktop_state', '{}', '--screenshot-out-file', raw],
                   capture_output=True)
    subprocess.run(['magick', raw, '-crop', '2880x1747+0+53', '+repage',
                    '-resize', '2400x1456!', f'{OUT}/{name}.png'], check=True)
    print(f'  shot {name}')


print('gnome-library')
click('Music', role='button', dwell=3.0)
shoot('gnome-library')

print('gnome-podcasts')
click('Podcasts', role='button', dwell=3.5)
shoot('gnome-podcasts')

print('gnome-youtube')
click('YouTube', role='button', dwell=3.5)
shoot('gnome-youtube')

print('gnome-radio')
click('Radio', role='button', dwell=3.0)
for cand in ('Add station', 'Discover stations', 'Add radio station', 'Add station…'):
    if click(cand, role='button', dwell=3.0):
        break
shoot('gnome-radio')
for cand in ('Cancel', 'Close'):
    if click(cand, role='button', dwell=1.0):
        break

print('gnome-library-doctor')
click('Library Doctor', role='button', dwell=4.0)
shoot('gnome-library-doctor')

print('gnome-device-sync')
click('DEVICES', role='button', dwell=1.5)
click('Open Pixel 10 Pro XL', role='button', dwell=4.0)
shoot('gnome-device-sync')

print('gnome-listening-stats')
click('My Stats', role='button', dwell=4.0)
shoot('gnome-listening-stats')

print('gnome-layout-controls')
do(frame(), 'win.preferences')
time.sleep(2.5)
prefs_page('Layout')
shoot('gnome-layout-controls', settle=3.0)
for n in walk(frame()):
    if n.get_name() == 'Close' and n.get_role_name() == 'button':
        do(n)
        break
time.sleep(1.5)
click('Music', role='button', dwell=2.0)
print('done')
