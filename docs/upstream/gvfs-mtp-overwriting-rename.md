# gvfs (MTP) — an overwriting rename is confirmed but never carried out

Project: https://gitlab.gnome.org/GNOME/gvfs — `mtp` backend.
Observed with **gvfs/gvfs-mtp 1.60.1** and GLib 2.88.2 on Manjaro, device:
Google Pixel 10 Pro XL (Android, MTP via `gvfsd-mtp`).

**Status: drafted, not yet submitted** — the text in the last section is ready
to submit, only a GNOME GitLab account is missing.

## Finding

`g_file_move (src, dst, G_FILE_COPY_OVERWRITE, …)` on an MTP mount

- reports **success**,
- removes the **existing destination file**,
- **does not apply the new name** — the source stays behind under its old
  name.

If the destination points at a **free** name instead, the same rename works
reliably. So the bug hangs off the overwrite path, not off the rename itself.

Measured with `scripts/upstream-repros/gvfs-mtp-overwriting-rename.py`, the
same sequence twice over 120 files of 7 MB each against the same directory:

| Pass | Destination existed before | `g_file_move` reported success | actually renamed |
| --- | --- | --- | --- |
| 1 | no | 120/120 | **120** |
| 2 | yes | 120/120 | **87** (33 left behind) |

Both `g_file_move` and `g_file_move_async` are affected.

## Classification: a follow-up bug of !246, not a duplicate

[#751](https://gitlab.gnome.org/GNOME/gvfs/-/issues/751) ("gvfs-mtp's do_move()
doesn't implement file renaming") is **closed**, fixed by
[!246](https://gitlab.gnome.org/GNOME/gvfs/-/merge_requests/246) ("fix: Add file
rename support in MTP backend move operation", merged 2025-08-13). The rename
itself has worked since then — 1.60.1 contains the fix, and pass 1 above
demonstrates it.

What remains open is exactly the case that #751 already names as an edge case in
its description: *"handling edge cases where the new filename exists in the
source directory or the old filename exists in the destination"*. !246 inserts
into `do_move`:

```c
if (g_strcmp0 (src_name, dest_name) != 0) {
  LIBMTP_file_t *file = LIBMTP_Get_Filemetadata (device, src_entry->id);
  if (file != NULL) {
    int ret = LIBMTP_Set_File_Name (device, file, dest_name);
```

MTP tolerates no two objects with the same name in the same folder, and at the
time of this call the destination name is occupied, or has only just been
freed — the rename then does not take effect, but the job reports success. The
exact line is not verified; the measurement above is an observation from the
outside.

No open issue with the label `5. MTP` covers this (as of 2026-07-28). The
closest is [#648](https://gitlab.gnome.org/GNOME/gvfs/-/issues/648)
(uploaded images are not indexed by the Android gallery) — same effect,
different cause.

## Second finding: the directory cache lies after a rename

Immediately after a rename, `g_file_query_exists` answers incorrectly for both
names, and in both directions: the old name is still reported as present, the
new one — depending on the moment — as missing. In the first pass above, an
immediate re-check reported 120 failures, of which **none** were real after a
remount. So an application cannot even reliably measure the success itself
without renewing the mount.

## Why this hurts in practice

This is exactly the sequence used by every application that wants to publish
atomically: first write to `<name>.part`, then rename to `<name>`. On MTP the
payload is thereby stranded under an extension that the phone's media scanner
does not index — the file is fully transferred and still invisible to the user.
Because the call reports success, the application notices nothing of it.

In Reprise this affected 173 of 278 transferred tracks in one run; 104 showed up
on the phone. Our countermeasure (see MTP-21): explicitly delete an existing
destination before the rename and re-measure the result afterwards instead of
believing the return value.

## Reproduction

```
./scripts/upstream-repros/gvfs-mtp-overwriting-rename.py \
    "mtp://<host>/Internal shared storage"
```

The script creates a scratch directory, runs both passes, remounts before every
count (because of the cache finding above) and cleans up at the end.

---

## Text to submit

New issue at https://gitlab.gnome.org/GNOME/gvfs/-/issues/new, attach the script
above.

**Title:** `MTP: an overwriting g_file_move reports success, drops the destination and never applies the rename`

**Labels:** `1. Bug`, `5. MTP`

> ### Summary
>
> On an MTP mount, `g_file_move()` with `G_FILE_COPY_OVERWRITE` onto an
> **existing** destination returns success, deletes the previous destination
> file, and leaves the source under its original name. The same move onto a
> **free** name in the same directory works reliably.
>
> This is a follow-up to #751 / !246 rather than a duplicate: renaming itself
> works since !246 (verified on 1.60.1). What is left is the edge case #751
> already anticipated — "handling edge cases where the new filename exists in
> the source directory or the old filename exists in the destination".
>
> ### Version
>
> gvfs and gvfs-mtp 1.60.1, GLib 2.88.2, Manjaro Linux.
> Device: Google Pixel 10 Pro XL (Android).
>
> ### Steps to reproduce
>
> The attached script runs the identical workload twice against the same
> directory on the device: copy a file to `<name>.part`, then
> `g_file_move(partial, target, G_FILE_COPY_OVERWRITE)`. Pass 1 renames onto
> free names, pass 2 renames onto the names pass 1 just created. It remounts
> before counting — see "Additional finding" below for why that is necessary.
>
> ```
> ./gvfs-mtp-overwriting-rename.py "mtp://<host>/Internal shared storage"
> ```
>
> ### Results
>
> 120 files of 7 MB each per pass:
>
> | Pass | Destination existed | `g_file_move` reported success | actually renamed |
> | --- | --- | --- | --- |
> | 1 | no | 120/120 | 120 |
> | 2 | yes | 120/120 | 87 — 33 left as `.part` |
>
> Both `g_file_move` and `g_file_move_async` are affected.
>
> ### Expected
>
> Either the rename is applied, or the call fails with an error. Reporting
> success while the destination is gone and the source keeps its old name is
> the damaging combination: the caller has no way to notice.
>
> ### Additional finding: the directory cache misreports names after a rename
>
> Immediately after a rename, `g_file_query_exists()` answers incorrectly for
> both names, in both directions — the old name is still reported as present,
> the new one as missing. In pass 1 above, an immediate check reported 120
> failures of which **none** were real after a remount. An application
> therefore cannot even verify the result itself without remounting, which
> makes the silent-success bug considerably worse.
>
> ### Why this matters in practice
>
> This is the standard publish-atomically pattern: write to `<name>.part`, then
> rename to `<name>`. On MTP the payload is stranded under an extension the
> phone's media scanner does not index — fully transferred, and invisible to
> the user. Because the call reports success, the application records the file
> as delivered and no later run sees anything to repair.
>
> In our case (a music player syncing to a phone) this affected 173 of 278
> transferred tracks in a single run; 104 showed up on the device.
>
> ### Possibly relevant
>
> !246 added to `do_move`:
>
> ```c
> if (g_strcmp0 (src_name, dest_name) != 0) {
>   LIBMTP_file_t *file = LIBMTP_Get_Filemetadata (device, src_entry->id);
>   if (file != NULL) {
>     int ret = LIBMTP_Set_File_Name (device, file, dest_name);
> ```
>
> MTP does not allow two objects with the same name in one folder, and with
> `G_FILE_COPY_OVERWRITE` the destination name is occupied — or has only just
> been freed — when this runs. We have not verified the exact line; the
> measurements above are an outside observation.
