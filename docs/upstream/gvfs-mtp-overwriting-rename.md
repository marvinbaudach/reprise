# gvfs (MTP) — überschreibendes Rename wird bestätigt, aber nicht ausgeführt

Projekt: https://gitlab.gnome.org/GNOME/gvfs — Backend `mtp`.
Beobachtet mit gvfs/GLib **2.88.2** auf Manjaro, Gerät: Google Pixel 10 Pro XL
(Android, MTP über `gvfsd-mtp`).

**Status: formuliert, noch nicht eingereicht.**

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
