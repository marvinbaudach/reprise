---
slug: device-overview-column-width
worktree: /home/marvin/Projects/reprise-device-overview-column-width
branch: feature/device-overview-column-width
phase: shipped
codex_session:
created: 2026-08-09
---
# `mtp_15` must measure the column, not the window settling

## The first attempt was aimed at the wrong thing

This plan originally said the sync overview card grows by 8 px when the phase
moves from `Transcoding` to `Copying`, and that the playlist card gives up those
pixels. **That diagnosis was wrong.** It was measured, in this worktree, at
`077809201f`:

| | Transcoding → Copying |
|---|---|
| overview card allocation | **414 → 414 px, unchanged** |
| page body | 817 → 809 |
| window / root | **881 → 873** |
| phase title | 166 → **137** (it shrinks) |
| only growing overview label | the dropdown, 220 → 229, with unchanged content |

The overview column takes nothing. The **toplevel** loses 8 px after
`present()`, and the playlist card absorbs the whole loss. Two further
observations pin it down as settlement rather than layout:

- with a 300 ms wait instead of 50 ms the same test passed once and failed once
- merely measuring the full widget hierarchy made it pass — the measurement
  itself stopped the toplevel from resizing

Independent corroboration from outside this worktree: the test is **green** when
run with an inherited real `XDG_CONFIG_HOME` and **red** with fresh XDG roots.
If the overview genuinely grew, it would be red in both. It is not — with a real
config the window already has a settled size, so nothing shrinks afterwards.

`mtp_15_sync_status_text_does_not_resize_the_playlist_workspace` therefore
measures "has the toplevel finished settling", not "does the status text move
the column". It is mis-specified, not a layout regression.

## What to change

Make the test measure what its name claims. Pin the outer window's width so the
toplevel cannot resize between the two measurements, and **keep both existing
assertions exactly as they are**. At a fixed page width, the question "does the
dynamic status copy shift the playlist column" is a real and worthwhile one —
that is the property to keep guarding.

Only the test file changes:
`crates/reprise-gnome/src/ui/device_sync/device_sync_page_display_tests.rs`.

**Do not:**

- change any product source to make this test pass. The measurements above say
  there is nothing there to fix; a source change that turns it green would be
  unrelated pinning or test gaming.
- weaken, relax or delete either assertion.
- add a longer sleep and call it fixed. A wait makes the race less likely, not
  absent — and the 300 ms experiment above already showed it failing anyway.

**Leave a comment in the test** saying why the window is pinned, in the style
this file already uses for `npp_1`. Include the measured numbers. Without that
reason written down, the next person removes the pin as noise and the flake
comes back.

## The sibling tests

`mtp_15_playlist_and_sync_overview_cards_share_the_same_edges` and
`mtp_14_full_page_uses_a_device_dashboard_instead_of_preferences_chrome` build
their windows the same way. They compare within one layout pass rather than
across two phases, so they are probably not exposed — but check rather than
assume. If either shows the same settlement dependence under the repeat run
below, give it the same pin and the same comment. If not, leave them untouched.

## Proving it — this is the part that matters

The defect is timing-dependent, so **one green run proves nothing**. Run the
gate's own recipe **ten times in a row** and report how many passed. Anything
below 10/10 is not fixed.

```bash
for i in $(seq 1 10); do
  data=$(mktemp -d); cache=$(mktemp -d); config=$(mktemp -d)
  runtime=$(mktemp -d); chmod 700 "$runtime"; tmp=$(mktemp -d)
  env XDG_DATA_HOME="$data" XDG_CACHE_HOME="$cache" XDG_CONFIG_HOME="$config" \
      XDG_RUNTIME_DIR="$runtime" TMPDIR="$tmp" \
      GIO_USE_VFS=local GTK_USE_PORTAL=0 \
      GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
      dbus-run-session -- xvfb-run --server-num=$((320 + i)) \
      cargo test -p reprise-gnome \
        ui::device_sync::device_sync_page::tests::display_tests::mtp_15_sync_status_text_does_not_resize_the_playlist_workspace \
        -- --ignored --exact
  echo "run $i exit=$?"
  rm -rf "$data" "$cache" "$config" "$runtime" "$tmp"
done
```

Run the same loop **before** the change too, so the before/after is a ratio and
not an anecdote. A baseline that comes out 10/10 red is as informative as one
that comes out 4/10 — report the number either way.

## Done when

1. Baseline ratio recorded (N of 10 passing **before** the change).
2. After the change: **10 of 10** passing, with the exit codes listed.
3. The two sibling tests above: 3 consecutive passes each under the same recipe,
   whether or not you touched them.
4. `cargo test -p reprise-gnome` (the non-ignored set) green — report the summed
   number before `passed`, not the word "ok".
5. `cargo fmt --all -- --check` and `scripts/check-architecture.sh` exit 0.
6. One focused commit, English message, test file only.
