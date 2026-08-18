---
slug: showroom-seek-track-measured
worktree: /home/marvin/Projects/reprise-showroom-seek-track
branch: feature/showroom-seek-track
phase: shipped
codex_session:
created: 2026-08-18
---

# Die Seek-Leiste im Showroom bekommt eine gemessene Spur

**Ziel.** Der Design-Import bringt eine Spectral-Seek-Sektion mit, deren
Formung ein echter Port von `crates/reprise-view/src/waveform.rs` und
`spectral_colour.rs` ist — deren **Eingabe** aber erfunden war: der Entwurf
würfelte Pegel und Centroid aus geseedetem Fraktal-Rauschen (`buildTrack()`,
214 s Fantasie-Song). Die Seite behauptet daneben, die Leiste zeige die echte
Formung. Dieser Plan ersetzt die Erfindung durch eine Messung, genau wie es
`docs/plans/showroom-plate-plays-the-visualizer.md` für den Visualizer getan
hat.

**Herkunft.** Abgespalten vom Gesamt-Import der Design-Seite (18.08.2026). Die
Entscheidung „messen statt würfeln" hat der Auftraggeber getroffen, nachdem der
Befund vorlag.

---

## 1. Der Pfad ist schon da

`RenderDataBackend::extract_render_data(path, buckets)` liefert beides in einem
Durchgang — Pegel **und** Spektrogramm, ohne DB-Umweg:

- Die Schnittstelle steht in `crates/reprise-core/src/waveform.rs:61`,
  `STORED_PEAK_COUNT` ist dort `1000`.
- Die echte Implementierung ist `GstreamerWaveformBackend` in
  `crates/reprise-platform-linux/src/waveform.rs:47` (`RenderRequest::PeaksAndBands`).
- Die Farbkurve kommt aus `TrackSpectrogram::centroid_curve(buckets)`
  (`crates/reprise-core/src/spectrogram.rs:96`) — dieselbe Funktion, die
  `waveform_cache.rs:66` für den Player ruft.

Es wird also **nichts nachgebaut**: der Extraktor ruft dieselben zwei
Funktionen wie die App und schreibt heraus, was sie zurückgeben.

## 2. Das Asset

`showroom/public/media/showroom/seek-track.bin`, feste Größe:

| Bytes | Inhalt |
|---|---|
| 0–3 | Spieldauer in Millisekunden, `u32` little-endian |
| 4–1003 | 1000 Pegelwerte (`waveform_peaks`), je ein `u8` |
| 1004–2003 | 1000 Centroid-Werte (`centroid_curve(1000)`), je ein `u8` |

**2004 Bytes.** Das Format ist bewusst stumpf: eine Länge, zwei gleich lange
Blöcke. Die Web-Seite liest es mit einem `DataView` und braucht keinen Parser.

Die Spieldauer muß echt sein — aus demselben Dekodierlauf, wenn der sie
hergibt, sonst aus den Metadaten der Datei. Sie trägt die Anzeige `0:00` /
`−3:34` und die Fensterbreiten der Glättung (`smoothCentroid` rechnet in
Sekunden).

## 3. Aufgaben

1. **Extraktor** als `#[test] #[ignore]` dort, wo der echte Backend erreichbar
   ist (`crates/reprise-platform-linux`): Quelldatei aus `REPRISE_SEEK_SOURCE`,
   `GstreamerWaveformBackend::extract_render_data(path, STORED_PEAK_COUNT)`,
   Pegel und `centroid_curve(1000)` heraus, dazu die Spieldauer.
2. **`scripts/render-showroom-seek-track.sh`**: nimmt den Pfad zur Audiodatei,
   startet den ignorierten Test, packt die 2004 Bytes nach
   `showroom/public/media/showroom/seek-track.bin` und **meldet** Größe und
   Spieldauer.
3. **Asset erzeugen** aus
   `/home/marvin/Music/Lorna Shore/…And I Return to Nothingness (2021)/01 To the Hellfire.flac`
   — dieselbe Aufnahme, aus der die Bandspur des Visualizers stammt.
4. **`showroom/tests/seek-track.test.mjs`**: das Asset existiert, ist **genau**
   2004 Bytes lang, seine Spieldauer liegt zwischen 60 s und 20 min, und die
   beiden Blöcke sind nicht konstant (eine Spur aus lauter Nullen wäre grün,
   ohne etwas zu zeigen).

## 4. Was dieser Plan nicht tut

- **Er zeichnet nichts.** Die Seek-Sektion selbst — Canvas, Port der Formung,
  Legende, Modusschalter, Tastaturbedienung — entsteht im Design-Import auf
  `feature/showroom-design-import`. Dieser Zweig liefert nur die Daten.
- **Er faßt `showroom/src/` nicht an.** Dort arbeitet der Import parallel.
- Der Zweig ist von `feature/showroom-plate-plays-the-visualizer` geschnitten,
  nicht von `origin/dev` — beide landen nacheinander.
