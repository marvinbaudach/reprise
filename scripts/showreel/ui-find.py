#!/usr/bin/env python3
"""Find a tappable element on the phone by its label, and print its centre.

Every phone take before this one pinned pixel coordinates to a Pixel 10 Pro XL
at one app version, and every one of them went stale. The 2026-08-29 measurement
found three coordinates wrong at once and, worse, a whole class of failure that
looks like nothing: the collapsible search bar shifts everything below it by
177 px, so a tap lands on the first album instead of the Play button and the
take walks on as if it had succeeded.

So resolve at run time instead. `uiautomator dump` is the same source those
coordinates were read off by hand; doing it in the take costs about a second per
lookup and removes the entire class.

    adb shell uiautomator dump /sdcard/ui.xml
    adb shell cat /sdcard/ui.xml > dump.xml
    ui-find.py dump.xml "Play Lorna Shore"        -> "127 634"
    ui-find.py dump.xml --contains "Play "        -> first match
    ui-find.py dump.xml --nth 1 --class ...Image  -> the second image node

Exit 2 and a message on stderr when nothing matches, so a caller that forgets to
check still fails loudly rather than tapping 0 0 — a tap at the origin lands on
the status bar and does nothing visible, which is the worst way to lose a take.
"""
import argparse
import sys
# The only input is a uiautomator dump pulled from a device this session already
# holds the lock on — a local view hierarchy, not network data — so the stdlib
# parser is the right size for it rather than a new dependency.
import xml.etree.ElementTree as ET

BOUNDS_CHARS = str.maketrans('[],', '   ')


def centre(node):
    """Pixel centre of a node's bounds attribute, as (x, y)."""
    x1, y1, x2, y2 = (int(v) for v in
                      node.get('bounds', '').translate(BOUNDS_CHARS).split())
    return (x1 + x2) // 2, (y1 + y2) // 2


def labels(node):
    """Both label sources, because Compose fills one or the other."""
    return node.get('text', ''), node.get('content-desc', '')


def matches(node, wanted, contains, cls):
    if cls and cls not in node.get('class', ''):
        return False
    if wanted is None:
        return True
    return any(wanted in text if contains else wanted == text
               for text in labels(node))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('dump')
    ap.add_argument('label', nargs='?')
    ap.add_argument('--contains', action='store_true',
                    help='substring match instead of an exact label')
    ap.add_argument('--class', dest='cls',
                    help='also require this substring in the class attribute')
    ap.add_argument('--nth', type=int, default=0,
                    help='take the nth match in document order (default 0)')
    ap.add_argument('--list', action='store_true',
                    help='print every labelled node instead of one centre')
    args = ap.parse_args()

    root = ET.parse(args.dump).getroot()

    if args.list:
        for node in root.iter('node'):
            text, desc = labels(node)
            if text or desc:
                x, y = centre(node)
                print(f'{x:5d} {y:5d}  {text or desc!r}  {node.get("class", "")}')
        return

    hits = [n for n in root.iter('node')
            if matches(n, args.label, args.contains, args.cls)]
    if len(hits) <= args.nth:
        print(f'ui-find: no match {args.nth} for {args.label!r}'
              f'{" (contains)" if args.contains else ""}'
              f'{f" class~{args.cls}" if args.cls else ""}'
              f' among {len(hits)} hits', file=sys.stderr)
        sys.exit(2)
    x, y = centre(hits[args.nth])
    print(f'{x} {y}')


if __name__ == '__main__':
    main()
