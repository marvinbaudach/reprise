#!/usr/bin/env python3
"""How long after the last keystroke does the query finish?

The app logs `query matched N tracks` from `run_query` with a millisecond
timestamp, which makes an external probe unnecessary — and unavailable anyway,
since a fresh `dbus-run-session` has no AT-SPI registry.

Two scenarios, because they are two different complaints:

  type   five characters at a fixed interval, then wait. Measures the wait a
         user sits through after stopping typing.
  clear  Esc on a filtered list. Measures the wait for going back, which is
         not a typing sequence and should not be debounced at all.

The keystroke timestamp is taken *around* the xdotool call and the later edge
is used, so the number is never flattered: it can only overstate the app's
share, never understate it.
"""
import argparse
import datetime as dt
import re
import subprocess
import time

ANSI = re.compile(r"\x1b\[[0-9;]*m")
QUERY = re.compile(
    r"(\d{4}-\d{2}-\d{2}T[\d:.]+Z)\s+INFO.*query matched (\d+) tracks"
)


def log_events(path):
    """Every completed query in the log, as (unix seconds, match count)."""
    out = []
    with open(path, "rb") as handle:
        for raw in handle:
            line = ANSI.sub("", raw.decode("utf-8", "replace"))
            found = QUERY.search(line)
            if not found:
                continue
            stamp = dt.datetime.strptime(
                found.group(1), "%Y-%m-%dT%H:%M:%S.%fZ"
            ).replace(tzinfo=dt.timezone.utc)
            out.append((stamp.timestamp(), int(found.group(2))))
    return out


def x(display, *args):
    subprocess.run(
        ["xdotool", *args],
        env={"DISPLAY": display, "PATH": "/usr/bin:/bin"},
        check=False,
        capture_output=True,
    )


def clear_field(display):
    """Empty the search entry, verifiably.

    `ctrl+a` + `BackSpace` looked right and was not: on several runs the
    characters landed on top of the previous query instead of replacing it, so
    the run measured "loveloveloveove" — zero hits, and a cheaper reload than
    the one under test. Select-all can miss because focus is not always inside
    the entry at that moment, and one BackSpace on a non-selection deletes one
    character. Enough BackSpaces to outlast any query this harness types is
    crude but has no failure mode.
    """
    for _ in range(24):
        x(display, "key", "BackSpace")


def run(display, window, logfile, scenario, chars, interval_ms, settle_s):
    x(display, "windowactivate", "--sync", window)
    time.sleep(0.4)

    if scenario == "type":
        # Start from an empty, open search field.
        x(display, "key", "ctrl+f")
        time.sleep(0.6)
        clear_field(display)
        time.sleep(settle_s)
        before = len(log_events(logfile))
        for index, char in enumerate(chars):
            if index:
                time.sleep(interval_ms / 1000.0)
            x(display, "key", char)
        last_keystroke = time.time()
    else:
        # Arrive filtered, then clear with Esc.
        x(display, "key", "ctrl+f")
        time.sleep(0.6)
        clear_field(display)
        time.sleep(0.4)
        for index, char in enumerate(chars):
            if index:
                time.sleep(interval_ms / 1000.0)
            x(display, "key", char)
        time.sleep(settle_s)
        before = len(log_events(logfile))
        x(display, "key", "Escape")
        last_keystroke = time.time()

    deadline = last_keystroke + 5.0
    while time.time() < deadline:
        events = log_events(logfile)
        if len(events) > before:
            stamp, count = events[before]
            return (stamp - last_keystroke) * 1000.0, count, len(events) - before
        time.sleep(0.02)
    return None, None, 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--display", default=":77")
    parser.add_argument("--window", required=True)
    parser.add_argument("--log", required=True)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--interval-ms", type=int, default=120)
    parser.add_argument("--settle", type=float, default=1.5)
    parser.add_argument("--chars", default="l,o,v,e,r")
    args = parser.parse_args()

    chars = args.chars.split(",")
    for scenario in ("type", "clear"):
        values = []
        for index in range(args.runs):
            latency, count, extra = run(
                args.display,
                args.window,
                args.log,
                scenario,
                chars,
                args.interval_ms,
                args.settle,
            )
            if latency is None:
                print(f"  {scenario} run {index + 1}: NO QUERY within 5 s")
                continue
            values.append(latency)
            note = f", {extra} queries" if extra > 1 else ""
            print(f"  {scenario} run {index + 1}: {latency:7.1f} ms ({count} hits{note})")
            time.sleep(args.settle)
        if values:
            values.sort()
            median = values[len(values) // 2]
            print(f"{scenario.upper():>6} median {median:7.1f} ms   min {values[0]:.1f}  max {values[-1]:.1f}")
        print()


if __name__ == "__main__":
    main()
