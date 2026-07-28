#!/usr/bin/env python3
"""gvfs MTP: an overwriting rename is acknowledged but never applied.

`g_file_move (…, G_FILE_COPY_OVERWRITE)` onto an *existing* target on an MTP
mount returns success, removes the previous file, and leaves the source under
its original name. The same rename onto a *free* name works every time, so the
overwrite path is the one that breaks.

The script demonstrates it by running the identical workload twice against the
same directory: pass 1 renames onto free names, pass 2 renames onto the names
pass 1 just created. It remounts before counting, because the gvfs directory
cache reports both the old and the new name for a while after a rename and
would otherwise hide — or invent — failures.

Usage:
    ./gvfs-mtp-overwriting-rename.py mtp://<host>/<storage volume>

Leaves nothing behind: the scratch directory is removed at the end.
"""
import subprocess
import sys

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib  # noqa: E402  (must follow require_version)

SCRATCH = "gvfs-mtp-rename-repro"
COUNT = 40
PAYLOAD = 1024 * 1024


def remount(uri):
    """Drops the gvfs directory cache, which lies right after a rename."""
    host = uri.split("/")[2]
    subprocess.run(["gio", "mount", "-u", f"mtp://{host}/"], capture_output=True)
    for _ in range(20):
        result = subprocess.run(
            ["gio", "mount", f"mtp://{host}/"], capture_output=True
        )
        if result.returncode == 0:
            return
    raise SystemExit("could not remount the device")


def run_pass(scratch, source, label):
    """Copies COUNT files to <name>.part and renames each onto <name>."""
    loop = GLib.MainLoop()
    state = {"index": 0, "reported_ok": 0}

    def step():
        if state["index"] >= COUNT:
            loop.quit()
            return
        name = f"file_{state['index']:03d}.bin"
        target = scratch.get_child(name)
        partial = scratch.get_child(name + ".part")

        def after_move(obj, res):
            try:
                obj.move_finish(res)
                state["reported_ok"] += 1
            except GLib.Error as err:
                print(f"  move failed loudly: {name}: {err.message}")
            state["index"] += 1
            GLib.idle_add(step)

        def after_copy(obj, res):
            try:
                obj.copy_finish(res)
            except GLib.Error as err:
                raise SystemExit(f"copy failed: {err.message}")
            partial.move_async(
                target,
                Gio.FileCopyFlags.OVERWRITE,
                GLib.PRIORITY_DEFAULT,
                None,
                None,
                after_move,
            )

        source.copy_async(
            partial,
            Gio.FileCopyFlags.OVERWRITE,
            GLib.PRIORITY_DEFAULT,
            None,
            None,
            after_copy,
        )

    GLib.idle_add(step)
    loop.run()
    print(f"{label}: g_file_move reported success {state['reported_ok']}/{COUNT} times")


def count_names(scratch):
    """Returns (renamed, still .part) as the device actually holds them."""
    renamed = stuck = 0
    enumerator = scratch.enumerate_children(
        "standard::name", Gio.FileQueryInfoFlags.NOFOLLOW_SYMLINKS, None
    )
    while True:
        info = enumerator.next_file(None)
        if info is None:
            break
        if info.get_name().endswith(".part"):
            stuck += 1
        else:
            renamed += 1
    return renamed, stuck


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    uri = sys.argv[1].rstrip("/")

    payload = "/tmp/gvfs-mtp-rename-repro-payload.bin"
    with open(payload, "wb") as handle:
        handle.write(b"\0" * PAYLOAD)
    source = Gio.File.new_for_path(payload)

    root = Gio.File.new_for_uri(uri)
    scratch = root.get_child(SCRATCH)
    try:
        scratch.make_directory(None)
    except GLib.Error as err:
        if not err.matches(Gio.io_error_quark(), Gio.IOErrorEnum.EXISTS):
            raise

    run_pass(scratch, source, "pass 1 (targets are free names)")
    remount(uri)
    scratch = Gio.File.new_for_uri(uri).get_child(SCRATCH)
    renamed, stuck = count_names(scratch)
    print(f"  on the device: {renamed} renamed, {stuck} left as .part\n")

    run_pass(scratch, source, "pass 2 (targets already exist)")
    remount(uri)
    scratch = Gio.File.new_for_uri(uri).get_child(SCRATCH)
    renamed, stuck = count_names(scratch)
    print(f"  on the device: {renamed} renamed, {stuck} left as .part")
    print("\nExpected: pass 2 strands files that g_file_move reported as moved.")

    for name in list(count_children(scratch)):
        try:
            scratch.get_child(name).delete(None)
        except GLib.Error:
            pass
    try:
        scratch.delete(None)
    except GLib.Error as err:
        print(f"could not remove {SCRATCH}: {err.message}")


def count_children(scratch):
    enumerator = scratch.enumerate_children(
        "standard::name", Gio.FileQueryInfoFlags.NOFOLLOW_SYMLINKS, None
    )
    while True:
        info = enumerator.next_file(None)
        if info is None:
            return
        yield info.get_name()


if __name__ == "__main__":
    main()
