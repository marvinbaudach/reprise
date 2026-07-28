# gvfs (MTP) — überschreibendes Rename wird bestätigt, aber nicht ausgeführt

Projekt: https://gitlab.gnome.org/GNOME/gvfs — Backend `mtp`.
Beobachtet mit **gvfs/gvfs-mtp 1.60.1** und GLib 2.88.2 auf Manjaro, Gerät:
Google Pixel 10 Pro XL (Android, MTP über `gvfsd-mtp`).

**Status: formuliert, noch nicht eingereicht** — der Text im letzten Abschnitt
ist fertig zum Einreichen, es fehlt nur ein GNOME-GitLab-Konto.

## Befund

`g_file_move (src, dst, G_FILE_COPY_OVERWRITE, …)` auf einem MTP-Mount

- meldet **Erfolg**,
- entfernt die **vorhandene Zieldatei**,
- **wendet den neuen Namen nicht an** — die Quelle bleibt unter ihrem alten
  Namen liegen.

Zeigt das Ziel dagegen auf einen **freien** Namen, funktioniert dasselbe Rename
zuverlässig. Der Fehler hängt also am Overwrite-Pfad, nicht am Rename an sich.

Gemessen mit `scripts/upstream-repros/gvfs-mtp-overwriting-rename.py`, zweimal
derselbe Ablauf über je 120 Dateien à 7 MB gegen dasselbe Verzeichnis:

| Durchlauf | Ziel existierte vorher | `g_file_move` meldete Erfolg | tatsächlich umbenannt |
| --- | --- | --- | --- |
| 1 | nein | 120/120 | **120** |
| 2 | ja | 120/120 | **87** (33 blieben liegen) |

Sowohl `g_file_move` als auch `g_file_move_async` sind betroffen.

## Einordnung: Folgefehler von !246, kein Duplikat

[#751](https://gitlab.gnome.org/GNOME/gvfs/-/issues/751) („gvfs-mtp's do_move()
doesn't implement file renaming") ist **geschlossen**, behoben durch
[!246](https://gitlab.gnome.org/GNOME/gvfs/-/merge_requests/246) („fix: Add file
rename support in MTP backend move operation", gemergt 2025-08-13). Der Rename
selbst funktioniert seitdem — 1.60.1 enthält den Fix, und Durchlauf 1 oben
belegt ihn.

Offen bleibt genau der Fall, den #751 in seiner Beschreibung bereits als
Randfall benennt: *„handling edge cases where the new filename exists in the
source directory or the old filename exists in the destination"*. !246 fügt in
`do_move` ein

```c
if (g_strcmp0 (src_name, dest_name) != 0) {
  LIBMTP_file_t *file = LIBMTP_Get_Filemetadata (device, src_entry->id);
  if (file != NULL) {
    int ret = LIBMTP_Set_File_Name (device, file, dest_name);
```

ein. MTP duldet keine zwei Objekte gleichen Namens im selben Ordner, und der
Zielname ist zum Zeitpunkt dieses Aufrufs belegt beziehungsweise gerade erst
freigegeben worden — der Rename greift dann nicht, der Job meldet aber Erfolg.
Die genaue Zeile ist nicht verifiziert; die Messung oben ist eine Beobachtung
von außen.

Kein offenes Issue mit Label `5. MTP` deckt das ab (Stand 2026-07-28). Am
nächsten liegt [#648](https://gitlab.gnome.org/GNOME/gvfs/-/issues/648)
(hochgeladene Bilder werden von der Android-Galerie nicht indexiert) — gleiche
Wirkung, andere Ursache.

## Zweitbefund: der Verzeichnis-Cache lügt nach einem Rename

Unmittelbar nach einem Rename beantwortet `g_file_query_exists` beide Namen
falsch, und zwar in beide Richtungen: der alte Name wird noch als vorhanden
gemeldet, der neue je nach Zeitpunkt als fehlend. Im ersten Durchlauf oben
meldete eine sofortige Nachprüfung 120 Fehlschläge, von denen nach einem
Remount **keiner** echt war. Eine Anwendung kann den Erfolg also nicht einmal
zuverlässig selbst nachmessen, ohne den Mount zu erneuern.

## Warum das praktisch weh tut

Das ist genau der Ablauf, den jede Anwendung nutzt, die atomar veröffentlichen
will: erst nach `<name>.part` schreiben, dann auf `<name>` umbenennen. Auf MTP
bleibt die Nutzlast dadurch unter einer Endung liegen, die der Media-Scanner
des Telefons nicht indexiert — die Datei ist vollständig übertragen und für den
Nutzer trotzdem unsichtbar. Da der Aufruf Erfolg meldet, merkt die Anwendung
nichts davon.

In Reprise traf das 173 von 278 übertragenen Titeln eines Laufs; auf dem Handy
erschienen 104. Unsere Gegenmaßnahme (siehe MTP-21): vorhandenes Ziel vor dem
Rename explizit löschen und das Ergebnis danach nachmessen, statt dem
Rückgabewert zu glauben.

## Reproduktion

```
./scripts/upstream-repros/gvfs-mtp-overwriting-rename.py \
    "mtp://<host>/Internal shared storage"
```

Das Skript legt ein Streuverzeichnis an, fährt beide Durchläufe, mountet vor
jeder Zählung neu (wegen des Cache-Befunds oben) und räumt am Ende auf.

---

## Einzureichender Text

Neues Issue unter https://gitlab.gnome.org/GNOME/gvfs/-/issues/new, Skript oben
anhängen.

**Titel:** `MTP: an overwriting g_file_move reports success, drops the destination and never applies the rename`

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
