# Manche Songs haben kein Spektrum (Android-Seekbar)

Untersuchung 2026-08-31. Symptom: Im Android-Player zeigt die Seekbar bei
manchen Titeln nur die flache Linie statt der Spektralbalken — beim selben
Titel **immer**. Beispiel aus dem Screenshot: „A Lot Like Vegas" / Until We Die
= Track 1666.

## Ergebnis

Der Sync will diese Titel in einen Ordner schreiben, der sich vom Ordner auf dem
Handy **nur in der Groß-/Kleinschreibung** unterscheidet. Das Dateisystem des
Telefons ist case-insensitiv, MTP führt aber beide Schreibweisen als eigene
Objekte — das Anlegen des „neuen" Ordners scheitert daher dauerhaft mit
`creating the destination directory failed: libmtp error: Could not send object
info`. Für Track 1666, wörtlich aus der Datenbank:

```
device_files  (liegt auf dem Handy):  Until We Die/Count Your Blessings_ an Encore/04 A Lot Like Vegas.mp3
sync_events   (Lauf 82 wollte):       Until We Die/Count Your Blessings_ An Encore/04 A Lot Like Vegas.mp3
                                                                        ^^
```

Die Analyse-Sidecar wird aus **demselben gewünschten Pfad** abgeleitet
(`mirror.rs::plan_analysis_sidecars` nimmt `desired.device_path`). Sie zielt
also in denselben nicht anlegbaren Ordner und scheitert bei jedem Lauf aufs
Neue. Android rechnet die Analyse **nie selbst** — fehlt die Sidecar, bleibt
die Seekbar für diesen Titel dauerhaft flach.

Das ist die zweite Wirkung des bereits dokumentierten Phantom-/Case-Variant-
Problems aus `device-sync-mtp-phantom-objects-findings.md`.

## Die Kette

1. Desktop-Scan berechnet `tracks.waveform_peaks` + `track_spectrograms`.
2. Device-Sync schreibt eine `.reprise-analysis`-Sidecar neben die Audiodatei
   (`mirror.rs::plan_analysis_sidecars` → `device_sync_effects.rs::copy_analysis_sidecar`).
3. Der Scan auf dem Handy registriert gefundene Sidecars
   (`scanner_mobile_sync.rs::register_analysis_sidecars`).
4. Beim Abspielen importiert Android sie einmalig
   (`mobile_import.rs::import_analysis_for_track`).
5. `track_render_bars` liefert die Balken, sonst `PlainSeekTrack`.

Der Bruch sitzt in Schritt 2.

## Belege

**Desktop-DB ist lückenlos** (read-only gemessen, 1999 präsente Tracks): kein
Track ohne `waveform_peaks`, keiner ohne Spektrogramm, keiner, der das
Frische-Prädikat von `get_track_spectrogram` verfehlt. 1666 und 1670 frisch.

**Das Lauf-Protokoll `sync_runs` nennt den Fehler wörtlich.** Lauf 82
(2026-08-30 21:25, `failed`, 349 geplant, 335 kopiert, 13 gescheitert) endet mit

```
could not copy analysis sidecar: creating the destination directory failed:
device I/O failed: libmtp error:  Could not send object info.
```

Lauf 76 endet ebenfalls an einer Analyse-Sidecar. Seit Lauf 70 (2026-08-29) ist
**kein Lauf mit tatsächlicher Arbeit mehr durchgelaufen**: 71–82 `failed`,
83/84 `cancelled`; die `completed`-Läufe davor hatten `planned = 0`.

**Der Schreibweisen-Konflikt ist gemessen, nicht vermutet.** 7 der 17
gescheiterten Transfers unterscheiden sich vom Pfad auf dem Gerät ausschließlich
in der Schreibweise — Track 512, 808, 1276, **1666**, 1667, 2151, 2500. Und die
Inventartabelle `device_files` führt 7 Albumordner gleichzeitig in zwei
Schreibweisen:

```
Emmure/Speaker Of The Dead        | Emmure/Speaker of the Dead
Emmure/Slave To The Game          | Emmure/Slave to the Game
Chelsea Grin/Desolation Of Eden   | Chelsea Grin/Desolation of Eden
Carnifex/GRAVESIDE CONFESSIONS    | Carnifex/Graveside Confessions
Lorna Shore/I Feel The Everblack… | Lorna Shore/I Feel the Everblack…
Bring Me The Horizon/Count Your…  | Bring Me the Horizon/Count Your…
Fight The Fade/APOPHYSITIS (Del…) | Fight the Fade/APOPHYSITIS (del…)
```

Einschränkung: Dass die Sidecar **genau für 1666** scheiterte, ist nicht
protokolliert — es gibt für Analyse-Fehlschläge keine `sync_events`-Zeile (siehe
unten). Es folgt aber aus dem gescheiterten Transfer desselben Tracks in
denselben Ordner und dem Sidecar-Fehler desselben Laufs.

## Ausgeschlossen

- **Ladefehler / Race auf dem Handy.** „Bleibt immer flach" entlastet
  `SpectralSeekTrack`s Nachladepfad und den `revision`-Retry.
- **Auswahl-Lücke.** 1666 hängt an der Playlist „Like Lorna Shore". (Nebenbefund:
  296 der 796 Gerätedateien hängen nur an der Smart-Liste „Top rated" — verlässt
  ein Track deren Mitgliedschaft, bleibt das Audio liegen, eine Sidecar bekommt
  er nie mehr.)
- **Pfadkollisionen** beim Ersetzen der Endung: keine.
- **Reihenfolge-Starvation.** Die Sidecars stehen zwar hinter allen
  Audiotransfers (`machine.rs:501`), aber Lauf 82 hat die Phase erreicht.

## Der eigentliche Mangel: der Fehlpfad ist stumm

Auch mit dem Protokoll bleibt **unbekannt, welche und wie viele** Sidecars
scheiterten:

- `sync_events.kind` erlaubt nur
  `skipped|failed|deleted|conversion_fallback|playlist_write_failed` — für eine
  gescheiterte Analyse-Sidecar gibt es **keine Art**, also keine Zeile.
- `Event::AnalysisWritten(Err)` überschreibt nur `terminal_error`
  (`machine.rs:374`); von N Fehlschlägen überlebt genau der letzte als Text.
- `device_sync_compact.rs:286` überspringt einen Track ohne Meldung
  (`Ok(None) => continue`).
- Auf der Android-Seite ist jeder Fehlschlag ein wortloses `Missing`
  (`mobile_import.rs:70,73,88`); `read_analysis_sidecar` verschluckt `NotFound`.
  **`adb logcat` taugt hier nicht als Unterscheider.**
- Der grobe Größenvergleich in `mirror.rs:397` sorgt dafür, dass eine
  beschädigte Sidecar gleicher Länge von keinem künftigen Sync überschrieben
  wird.

## Was zu tun ist

**Damit die Spektren erscheinen**, in dieser Reihenfolge:

1. Phantom-Objekte auflösen (aus dem Phantom-Bericht):
   ```
   adb shell content call --uri content://media/ --method scan_volume \
       --arg external_primary
   gio mount -u mtp://<device>/ && gio mount mtp://<device>/
   ```
2. Einen Sync **bis zum Ende** laufen lassen.
3. Auf dem Handy die Bibliothek neu scannen — erst dieser Scan trägt die
   Sidecars in `track_analysis_sidecars` ein. `register_analysis_sidecars`
   läuft über *alle* gefundenen Titel, ein normaler Scan genügt also.
4. Den Titel erneut abspielen; die Balken erscheinen beim nächsten Abspielen
   nach dem Scan, nicht sofort.

**Im Code, zwei getrennte Arbeiten:**

- *Ursache:* Der Sync darf einen Zielpfad nicht als neu behandeln, wenn er sich
  vom residenten Pfad nur in der Schreibweise unterscheidet — sonst plant er
  denselben Transfer bis in alle Ewigkeit gegen ein Dateisystem, das beide
  Namen für dieselbe Sache hält. Das trifft Audio und Sidecar gleichermaßen.
- *Sichtbarkeit:* Eine gescheiterte oder übersprungene Analyse-Sidecar muss
  zählbar werden — eine eigene `sync_events`-Art, damit ein Lauf sagen kann
  „23 Titel haben keine Analyse bekommen", statt dass es Monate später an einer
  flachen Seekbar auffällt.
