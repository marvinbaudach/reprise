---
slug: lyrics-scan-should-ride-along-with-the-library-scan
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-18
---
# TODO: Der Lyrics-Scan braucht beim Start keinen eigenen Fortschrittsbalken

**Befund, kein Plan.** Gemeldet am 18.08.2026: *„auch dieser Lyrics-Scan zu
Beginn braucht es nicht. Das sollte einfach mit dem Scan für neue Musikdateien
zusammen passieren. Ich muss da nicht einzeln noch ne Progressbar sehen bei
jedem Start."*

## Symptom

Bei jedem App-Start läuft „Checking missing lyrics…" als eigener sichtbarer
Job mit eigener Fortschrittsanzeige, zusätzlich zum Scan nach neuen
Musikdateien und zum Cover-Durchlauf. Aus Nutzersicht ist das dritte Balken für
eine Sache, die zusammengehört.

## Gewünschtes Verhalten

Das Nachladen fehlender Lyrics läuft **als Teil des Datei-Scans** mit — kein
eigener Startjob, keine eigene Fortschrittszeile beim Start. Ein explizit vom
Nutzer angestoßener Lauf (Einstellungen → Plugins) darf weiterhin sichtbar
sein; die Unterscheidung automatisch/explizit ist die gleiche wie beim
Bild-Ladepfad.

## Fundstellen (lokaler Stand, gegen `origin/dev` gegenprüfen)

| Sache | Stelle |
| --- | --- |
| Startkette: Lyrics-Batch hängt hinter dem Cover-Batch | `ui/progress_subscribers.rs:113` (`start_after_cover_callback`) |
| GTK-Adapter des Batchs | `ui/lyrics/lyrics_batch.rs` |
| Fortschrittsdarstellung | `ui/lyrics/lyrics_batch_progress.rs` |
| Sichtbarer Text | `ui/strings_news.rs:185` (`LYRICS_BATCH_CHECKING`) |
| Job-Zeile im Scan-Chrome | `ui/scan/scan_controls.rs:569`, `:584`, `ui/scan/scan_chrome.rs:269` |

## Zu klären

1. Zusammenlegen heißt was genau — der Lyrics-Durchlauf als Stufe **innerhalb**
   des Datei-Scans (ein Balken, eine Restzeit), oder weiterlaufen lassen und
   nur die eigene Startzeile entfernen? Der Wortlaut („zusammen passieren")
   deutet auf Ersteres.
2. Was passiert bei ausgeschaltetem Online-Lyrics-Modul und bei einem Start
   ohne neue Dateien? Dann darf gar nichts erscheinen.
3. Vorher prüfen, ob der alte Befund
   [[reprise-lyrics-scan-never-finishes]] (Job hängt bei 0 % und startet sich
   endlos neu, 09.08.2026) noch gilt — er ist ein Teil desselben Ärgernisses
   und könnte beim Zusammenlegen mit auffallen.
4. Berührt [lyrics-batch-to-core.md](lyrics-batch-to-core.md) — Phase dort vor
   dem Planen prüfen.
