# Spektrogramm-Pipeline: Messung und Vorentscheidung

Gelesen in `/home/marvin/Projects/reprise-mobile` (Branch `feature/mobile-m8`, nur lesend,
nichts verändert). Gemessen mit echten Dateien aus `/home/marvin/Music` (nur gelesen) und der
echten, read-only geöffneten DB `/home/marvin/.local/share/reprise/reprise.db`. Alle
Rust-Zeilennummern beziehen sich auf den reprise-mobile-Checkout; die referenzierten Dateien
existieren identisch im Hauptrepo `/home/marvin/Projects/reprise` (geprüft).

---

## 1. Der billigste ehrliche Erzeuger

**Der bestehende `waveform_peaks`-Erzeuger, konkret:**

- Vertrag: `crates/reprise-core/src/waveform.rs` — `WaveformBackend`-Trait,
  `STORED_PEAK_COUNT = 1000`, plus `WaveformAccumulator` (streaming Sum-of-Squares pro Bucket,
  RMS, dann **pro Track auf das eigene Maximum normalisiert** — `finish_waveform()`,
  Zeile 108–127: `(value / maximum).sqrt() * 255.0`).
- Backend: `crates/reprise-platform-linux/src/waveform.rs`, `GstreamerWaveformBackend`. Pipeline
  `uridecodebin ! audioconvert ! audioresample ! audio/x-raw,F32LE,mono,8000Hz ! appsink`
  (Zeile 17–19). **8 kHz** ist bewusst so niedrig gewählt, weil nur eine Amplitudenhüllkurve
  gebraucht wird, keine Frequenzinformation.
- Angestoßen wird das **verzögert, beim Abspielen, nicht beim Scan**:
  `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs::sync_waveform` (Zeile 307–348).
  Ablauf: DB-Cache-Treffer? sonst `extract_peaks` synchron in einem `one_shot_task`-Worker-Thread,
  dann `set_waveform_peaks` zurückschreiben. Kein Scan-Trigger, keine Warteschlange.
  Es gibt einen vorbereiteten, aber **toten** Hook dafür: `db.rs::pending_waveform_tracks`
  (Zeile 759–770) wird nirgends außer in Tests aufgerufen — offenbar für einen geplanten, nie
  gebauten Hintergrund-Backfill.
- Live-DB-Stichprobe (read-only): `SELECT COUNT(*), SUM(LENGTH(waveform_peaks)) FROM tracks` →
  **1846 Tracks, 1 648 000 Bytes** → exakt 1648 Tracks mit vollem 1000-Byte-Cache (89,3 %),
  Rest `NULL`. Bestätigt: nur gespielte Tracks haben heute Peaks.

**GStreamer `spectrum`-Element — geprüft, nicht das, was tatsächlich läuft:**

`gst-inspect-1.0 spectrum` zeigt das Element ist installiert (`gst-plugins-good` 1.28.5, bereits
Teil der Dependency-Kette). Der Code in `crates/reprise-platform-linux/src/player_effects.rs` und
`player_pipeline.rs` heißt zwar `set_spectrum_messages`/`spectrum_enabled`, **nutzt aber nicht**
`gst::ElementFactory::make("spectrum")` — kein einziger Treffer im ganzen Baum
(`grep -rn 'make("spectrum")'` → leer). Stattdessen tappt ein `tee → queue → audioconvert →
audioresample → appsink` (`player_pipeline.rs:90-96`) rohes PCM ab und füttert es in einen
**reinen Rust-Portierung von CAVA** (`crates/reprise-core/src/playback/cava.rs`,
`playback/cava/bands.rs`, `playback/cava/smoothing.rs`). Der Name „spectrum“ ist ein historisches
Überbleibsel; das eigentliche Backend ist CAVA auf `realfft`.

Ein älteres Planungsdokument im Hauptrepo
(`docs/superpowers/specs/2026-07-22-visualizer-honest-loudness-design.md`) beschreibt noch einen
Zustand mit „GStreamer threshold -80 dB“/„folded[band]“ — das war die **Vorgänger-Implementierung**
vor dem CAVA-Port. Per `git log`: Der Honest-Loudness-Spec ist vom 22.07., der CAVA-Port
(„feat(core): add CAVA logarithmic band planning“ etc.) kam am 26.07. — **danach**, und hat den
GStreamer-`spectrum`-Pfad komplett ersetzt. Das GStreamer-Element ist seither aus dem Live-Pfad
verschwunden.

**Fazit Teil 1: `realfft` (3.5.0, wrapt `rustfft` 6.4.1) ist bereits Abhängigkeit von
`reprise-core`** (`crates/reprise-core/Cargo.toml:59`), dort bewusst **ohne** GStreamer — der
Crate-Kommentar sagt es explizit: „pulls in no gtk/gstreamer/zbus (verify with `cargo tree`)“,
weil `reprise-core` auch auf Android laufen muss. Das ist exakt dieselbe FFT-Maschinerie, die
CAVA schon in Echtzeit auf genau dieser Hardware für 64 Bänder bei 44,1 kHz betreibt. Ein
GStreamer-`spectrum`-Element wäre eine **zweite**, nur auf dem Desktop verfügbare
FFT-Implementierung für dieselbe Aufgabe — unnötig, wenn die portable schon da ist und sich
bereits bewährt hat. **Kein neues FFT-Backend nötig.**

**Ein Durchlauf für beides:** `run_pipeline`/`push_sample` in `waveform.rs` dekodiert bereits
Chunk für Chunk zu F32-Mono-PCM und füttert das in den `WaveformAccumulator`. Der einzige Grund,
warum das heute nicht auch für Bänder reicht: **8 kHz Zielrate ⇒ Nyquist 4 kHz**, das deckt die
geforderten 16 kHz nicht ab. Lösung, ohne zweiten Decode-Durchlauf: Zielrate der bestehenden
Pipeline auf ≥ 32 kHz anheben (schadet den Peaks nicht — die sind ratenunabhängige
RMS-Buckets) und **denselben PCM-Strom** zusätzlich durch eine STFT/Band-Stufe laufen lassen.
Decodieren ist ohnehin der teurere Teil (siehe Teil 2) — zweimal zu dekodieren wäre die
Verschwendung, die im Auftrag schon vermutet wurde, und lässt sich mit dieser einen
Pipelinen-Änderung vermeiden.

---

## 2. Was es wirklich kostet — gemessen

Kein Rust-Neubau während laufendem Codex-Lauf und laufenden Parallel-Builds (Maschine lief heute
Nacht auf 96 °C, aktuell `load average 10.4` auf 8 Kernen von *anderen* Agenten-Builds). Deshalb
mit vorhandenen Bordmitteln gemessen statt einer neuen Cargo-Kompilation:
`ffmpeg` fürs Decodieren (gleicher Kostenblock wie `uridecodebin`/uraufwendiger, da software-only
ohne GPU-Beschleunigung — realistische Obergrenze), `python3`/`numpy` (Single-Thread erzwungen via
`OMP_NUM_THREADS=1` etc.) für STFT+Banding als **pessimistische** Schätzung (ein natives
Rust/`realfft` wird schneller sein).

Drei echte Dateien, bewusst unterschiedlich (dicht/laut vs. dünn/leise):

| Track | Typ | Dauer | Decode (ffmpeg→32 kHz mono f32) | FFT+Banding (Python, 24 Bänder, N=4096, Hop=1600) |
|---|---|---|---|---|
| As I Lay Dying – An Ocean Between Us (mp3) | dicht, metalcore | 253,1 s | 1,297 s | 2,392 s |
| A Day to Remember – Homesick (Acoustic) (mp3) | dünn, akustisch | 247,7 s | 1,107 s | 0,578 s |
| Asking Alexandria – Alone Again (flac) | dicht, modern | 229,0 s | 1,725 s | 1,298 s |

Befehle:
```
ffmpeg -y -nostdin -i "<datei>" -ac 1 -ar 32000 -f f32le track.pcm -loglevel error
OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 MKL_NUM_THREADS=1 nice -n 15 python3 band_bench.py
```
(`band_bench.py` im Scratchpad: Hann-Fenster, `np.fft.rfft`, 24 log-Bänder 20 Hz–16 kHz, dB-Fenster
→ `u8`.)

**Hochgerechnet auf 1846 Titel** (Summe Decode+FFT je Track, Mittel der drei Samples ≈ 2,80 s):
`1846 × 2,80 s ≈ 5170 s ≈ 86 Minuten`, einzelner Kern, mit dem **pessimistischen** Python-FFT-Anteil.
Geschätzt: Ein natives `realfft`-Backend (das exakt diese Bandzahl in Echtzeit auf derselben
Maschine schafft) drückt den FFT-Anteil auf einen Bruchteil; dann dominiert allein das Decodieren
(~1,1–1,7 s/Track), macht **geschätzt ~46 Minuten** für einen kompletten Erstlauf,
einzelner Kern — auf 8 Kernen parallelisierbar auf wenige Minuten, falls gewünscht.

**Tatsächliche Größe nach zstd — widerlegt die Auftragsannahme:**

Roh-Bytes (24 Bänder × 20 fps): 118,6–121,4 KiB — **das trifft die geschätzten „~115 KB“ genau.**
Die geschätzten „20–30 KB gepackt“ (Faktor 4–5×) **halten nicht**:

| Track | roh | zstd -19 | Ratio | zstd -19, band-major transponiert | Ratio | + Delta + xz -9e | Ratio |
|---|---|---|---|---|---|---|---|
| AILD (dicht) | 121 440 B | 107 085 B | 1,13× | 95 659 B | 1,26× | 89 588 B | 1,35× |
| ADTR (leise) | 118 848 B | 103 798 B | 1,14× | 94 636 B | 1,25× | 86 672 B | 1,37× |
| AA (dicht) | 109 848 B | 100 473 B | 1,09× | 90 399 B | 1,21× | 79 288 B | 1,38× |

Befehle: `zstd -q -19 -o out.zst in.raw`, `xz -9e -k -c in.delta > in.delta.xz`. Byte-Entropie der
rohen Zellen (Shannon, gemessen über `np.unique`): **7,1–7,35 Bit/Byte** von 8 möglichen — die
dB-skalierten Zellen sind fast maximal-entropisch. Auch mildes 3-Frame-Glätten (150 ms,
psychoakustisch plausibel) drückt die Entropie nur auf 6,98–7,35 Bit/Byte und die Ratio nur auf
1,31–1,45×. **Bestmöglich gemessen: ~1,4× (Delta+xz), realistisch mit einfachem zstd: ~1,1–1,3×.**

Grund: Ein log-/dB-skaliertes Magnitudenspektrum echter Musik sieht auf Byte-Ebene wie Textur/
Rauschen aus, nicht wie ein Bild mit großen flachen Flächen — Spektrogramme sind bekannt schlecht
komprimierbar, anders als z. B. Wellenform-Hüllkurven mit viel Redundanz. Das war schon an der
Größenordnung erkennbar, bevor ich komprimiert habe: die Zielgröße von „115 KB roh“ war schon
korrekt geschätzt, aber die Annahme, dass ein Allzweck-Kompressor daraus 20–30 KB macht, war es
nicht.

**zstd ist nicht in der Dependency-Kette** (`grep -n '^name = "zstd'` in `Cargo.lock` → leer;
auch kein `lz4`/`brotli`; nur `flate2`/`miniz_oxide` als transitive Abhängigkeiten anderer Krates,
nicht direkt nutzbar ohne eigene Einbindung). Jede Kompression hier wäre eine **neue**
Abhängigkeit für einen Gewinn von 10–30 %.

---

## 3. Wohin es gespeichert wird

Reale Zahlen mit den widerlegten Kompressionswerten: **~90–121 KB/Track, nicht 20–30 KB.**
Bei 1846 Titeln: **~170–195 MB unkomprimiert, ~165–185 MB mit zstd** (kaum ein Unterschied) —
nicht die im Auftrag angenommenen 40–55 MB.

DB-Struktur, read-only gemessen:
```
sqlite3 "file:...reprise.db?mode=ro" "PRAGMA page_size; PRAGMA page_count;"
→ 4096 / 1657   (Datei aktuell 6,5 MB)
sqlite3 ... "SELECT name, SUM(pgsize), COUNT(*) FROM dbstat GROUP BY name ORDER BY 2 DESC LIMIT 10;"
→ tracks 2 347 008 B über 573 Seiten (≈ 4096 B/Seite, dicht gepackt)
```
`tracks` hat durchschnittlich ≈ 1272 Bytes/Zeile (2 347 008 / 1846) — Text-Spalten plus die
bereits vorhandenen `waveform_peaks` (Ø 893 B, da nur 89 % gefüllt). Ein 1000-Byte-Blob passt bei
Seitengröße 4096 noch **inline** in die Haupt-B-Tree-Seite (SQLites Overflow-Schwelle liegt nahe
`page_size − 35`, deutlich über 1000 B) — deshalb tut `waveform_peaks` der Tabelle heute nicht weh,
und die Projektion in `TRACK_COLUMNS`/`track_projection`
(`crates/reprise-core/src/queries/clauses.rs:125-164`) wählt es ohnehin nie mit aus (22 explizite
Spalten, kein `SELECT *`) — jede Fensterabfrage überspringt es bereits heute.

Ein Spektrogramm-Blob von ~100 KB **überschreitet** diese Inline-Schwelle massiv und würde in
SQLites Overflow-Seiten wandern (verkettete Zusatzseiten außerhalb der Haupt-B-Tree-Seite der
Zeile) — das schützt zwar weiterhin jede Abfrage, die die Spalte nicht selektiert (genau wie
heute bei `waveform_peaks`), aber es ändert etwas anderes wesentlich: die **Dateigröße** der DB
selbst wächst von 6,5 MB auf potenziell ~180 MB. Das trifft:
- `VACUUM`/Backup-Zeit,
- die geplante MTP/WLAN-Synchronisation (die diese Datenmenge über die Leitung bringen muss —
  nicht mein Thema hier, aber die Zahl gehört dorthin),
- den Erstlauf-Migrationsschreibvorgang (1846 einzelne ~100-KB-`UPDATE`s statt eines Batch-Inserts
  in eine neue Tabelle).

**Empfehlung: eigene Tabelle, keine Spalte auf `tracks`.** Nicht weil `waveform_peaks` als Spalte
falsch wäre (bei 1 KB ist es das nicht), sondern weil der Sprung auf ~100 KB zwei Größenordnungen
ausmacht und weil es im Schema bereits ein Präzedenzmuster für „optionale, ableitbare Pro-Track-
Daten“ gibt: `track_audio_analysis` (v18, jetzt entfernt) war exakt so gebaut — eigene Tabelle,
`track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE`. Eine eigene Tabelle
`track_spectrograms(track_id, sample_rate_hz, band_count, frame_count, data BLOB, …)` folgt diesem
bestehenden Muster, hält `tracks` so schlank wie heute, lässt sich unabhängig droppen/neu aufbauen
(reine Ableitung, kein Verlust an Wahrheit) und macht einen künftigen Vollbackfill zu einem reinen
Insert-Batch statt 1846 Row-Updates auf der Haupttabelle.

---

## 4. Die Bandaufteilung, konkret

24 logarithmische Kanten, 20 Hz–16 kHz (`edge[i] = 20 · (16000/20)^(i/24)`, `i = 0..24`):

```
20.0 · 26.4 · 34.9 · 46.1 · 60.9 · 80.5 · 106.4 · 140.5 · 185.7 · 245.3 · 324.1 · 428.2 ·
565.7 · 747.4 · 987.4 · 1304.6 · 1723.5 · 2277.1 · 3008.5 · 3974.7 · 5251.4 · 6938.0 ·
9166.3 · 12110.4 · 16000.0   (Hz)
```

**Skalierung der `u8`-Zelle: dB-Fenster, keine per-Track-Normalisierung.** Nicht linear in der
Amplitude (macht leise Passagen schwarz, exakt wie im Auftrag vermutet) und **auch nicht** wie
`WaveformAccumulator::finish_waveform` per Track auf das eigene Maximum normiert — das ist für
eine Seek-Leiste richtig (dort geht es nur um die Form innerhalb eines Tracks), aber falsch für
ein Dataset, das laut Auftrag auch die *Lautstärke* zeigen soll (Seek-Amplitude = Bandsumme):
zwei Tracks mit 25 dB Lautstärkeunterschied würden nach Track-Normalisierung gleich hell aussehen.

Die „honest loudness“-Entscheidung existiert im Schema bereits — aber nicht mehr dort, wo die
Spec sie ursprünglich verortete. Das Dokument
`docs/superpowers/specs/2026-07-22-visualizer-honest-loudness-design.md` beschreibt ein festes
dB-Fenster (dort: −70…−12 dB) + Pink-Tilt (+3 dB/Oktave) + kein AGC — geschrieben gegen den
damaligen GStreamer-`spectrum`-Analyzer. Der wurde vier Tage später vom CAVA-Port abgelöst
(s. Teil 1), und CAVA bringt sein **eigenes** Autosensitivity-/AGC (`playback/cava/smoothing.rs`,
`autosensitivity: u32 = 1` per Default) — die Bars, die man heute im Visualizer sieht, sind
also wieder AGC-normalisiert, nicht „honest“. Das Prinzip überlebte trotzdem, an einer anderen
Stelle: `crates/reprise-core/src/playback/bass_pressure.rs`
(„Absolute bass-pressure detection“, Commit `5adf9636a4 fix(visuals): drive the bass glow from
absolute level, not CAVA bars“). Zitat aus dem Doc-Kommentar: „CAVA's auto-sensitivity keeps
re-normalizing them so the tallest column fills the canvas. That makes them useless as a ‚how
loud is this‘ signal.“ Der Detektor misst deshalb PCM direkt in echtem dBFS, mit festem Fenster
(`PRESSURE_FLOOR_DBFS = -30`, `PRESSURE_CEIL_DBFS = -10`), kalibriert an zwei echten Referenztracks.

**Für das Spektrogramm gilt dasselbe Argument, nicht das der CAVA-Bars:** feste
Boden-/Decken-dBFS-Werte (Ausgangspunkt in der Messung hier: −70/−6 dB, kalibrierbar), keine
Pro-Track-Normalisierung, kein AGC. Das ist keine Neuerfindung, sondern derselbe Entwurf, den
`bass_pressure.rs` schon für „absolute statt relative Lautstärke“ etabliert hat — nur über das
volle Spektrum statt nur 30–150 Hz.

**Auflösungsproblem, real gemessen, nicht nur behauptet:** Band 0 ist nur 6,4 Hz breit
(20–26,4 Hz). Bei 32 kHz und N = 4096 (die FFT-Größe, mit der ich oben gemessen habe) beträgt die
Bin-Breite 32000/4096 ≈ 7,8 Hz — **gröber als das Band selbst.** Die unteren 3–5 Bänder (bis
~80–100 Hz) kollabieren damit auf denselben ein bis zwei FFT-Bins, sind also mit dieser
Fenstergröße gar nicht sauber trennbar. Das ist exakt das Problem, das CAVAs `BandPlan`
(`playback/cava/bands.rs`) schon gelöst hat: ein zweites, doppelt so langes `bass_fft` ausschließlich
für Frequenzen unter 100 Hz, während der Rest die kürzere FFT nutzt — bei gleichem Hop/gleicher
Framerate. Diese Zweiteilung sollte für den Spektrogramm-Erzeuger übernommen werden (nicht neu
erfunden), z. B. N = 4096 (7,8 Hz/Bin) für die oberen ~19 Bänder, N = 16384 (≈2 Hz/Bin) für die
unteren ~5 — das kostet kaum mehr Rechenzeit (die längere FFT läuft nur für den unteren
Bandbereich), löst aber Band 0 tatsächlich auf.

---

## 5. Die Grenze zur entfernten Analyse

`crates/reprise-core/src/db_drop_audio_analysis_mix.rs` (Schema v27) räumt zwei Feature-Familien
weg:
- **v18 `track_audio_analysis`**: pro Track genau ein Ergebnis mit `loudness_rms`,
  `dynamic_range`, `spectral_centroid_hz`, `spectral_rolloff_hz`, `spectral_flux`, `onset_rate`,
  `tempo_bpm` (+confidence), `intensity`/`brightness`/`dynamicity`/`rhythmicity` (je + confidence)
  — **skalare, zusammenfassende Merkmale** für Ähnlichkeitssuche/Empfehlung (Similar Mix, Related
  Artists).
- **v23 `mix_drafts`/`mix_draft_tracks`**: die daraus gebauten Misch-Vorschläge selbst.

Der Kommentar im Migrationscode ist explizit: „`tracks.waveform_peaks` is a separate column on
`tracks`, deliberately untouched: the seek-bar waveform still reads it.“ — die Grenze wurde also
schon einmal genau hier gezogen: **Rohdaten fürs Rendern bleiben, verdichtete Merkmale fürs
Empfehlen gehen.**

Das hier vorgeschlagene Spektrogramm ist eine Zeitreihe roher Bandmagnituden — näher verwandt mit
`waveform_peaks` (Darstellung) als mit `track_audio_analysis` (ein einzelner zusammenfassender
Merkmalssatz pro Track). Es berechnet **kein** Tempo, keine Helligkeit/Rhythmizität/Dynamik-Skalar,
keine Ähnlichkeits-Fingerprints — nichts, worauf eine Empfehlungs- oder Mix-Engine aufsetzen
könnte, ohne selbst noch einmal eine komplette Merkmalsextraktion obendrauf zu bauen. Die einzige
Versuchung, die im eigenen Vorschlag genau an diese Grenze grenzt: `spectral_centroid_hz` (v18)
und die künftige „bandindex-gewichtete Mittelwert“-Farbe der Seek-Leiste sind mathematisch
dieselbe Operation (ein gewichteter Schwerpunkt über die Bänder). Das bleibt diesseits der Grenze,
solange dieser Wert **nur pro Frame zur Anzeige** berechnet und nirgends persistiert oder für
Ähnlichkeitsvergleiche zwischen Tracks verwendet wird — persistierte man ihn zusätzlich als
Track-Skalar, wäre man exakt wieder bei `track_audio_analysis` Spalte 1.

---

## 6. Was fehlt, wenn Daten fehlen

Wiedergabe hängt in diesem Code strukturell **nie** an `waveform_peaks`/einer künftigen
Spektrogramm-Spalte: `sync_waveform`
(`crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs:307-348`) läuft in einem
`one_shot_task`-Hintergrund-Thread und liefert das Ergebnis erst asynchron über
`glib::spawn_future_local` an die UI zurück; ein `waveform_generation`-Zähler
(`player_controller.rs:402-405`) verwirft veraltete/verspätete Resultate bei schnellem
Trackwechsel. Der Kommentar an Zeile 341-343 verweist bereits auf einen bestehenden
Skeleton-/Platzhalterzustand („the mini waveform … shows the real shape + progress, not the
skeleton“) — d. h. es gibt heute schon eine definierte Vorher-Ansicht, bevor Peaks eintreffen.

Für das Spektrogramm gehört die Unterscheidung an dieselbe Stelle, nach demselben Muster: ein
`sync_spectrogram` neben `sync_waveform`, mit einem eigenen `get_spectrogram(db, track_id) ->
Option<…>` neben `get_waveform_peaks` (`db.rs:746-754`). Erkannt wird „keine Daten“ schlicht über
`NULL`/`None` — kein zusätzliches Flag nötig, exakt wie heute bei `waveform_peaks`. Playback selbst
bleibt unberührt: Play-Aufruf und Datenabruf sind in diesem Code bereits entkoppelte Pfade, die
„Titel bleibt sofort abspielbar“-Garantie ergibt sich aus der bestehenden Architektur, nicht aus
etwas, das neu gebaut werden müsste. Der Visualizer-Fallback (grau/nur-live) gehört analog in
`VisualEngine` (`crates/reprise-core/src/visuals/engine.rs`) — dessen Moduldoc sagt bereits
„Signal processing ends at `SpectrumFrame`; this module does not remap … a second time“, das ist
exakt die Stelle, die zwischen „habe präkomputiertes Spektrogramm → zeige auch Zukunft“ und „habe
keins → nur Live-CAVA wie heute“ unterscheiden müsste.

**Wichtig für den geplanten Hintergrund-Backfill (siehe Teil 1):** Die *heutige* Lazy-on-Play-
Strategie reicht für den mobilen Anwendungsfall nicht. Ein Track, der nie auf dem Desktop
abgespielt wurde, hat auch nie ein Spektrogramm — würde aber trotzdem synchronisiert. Für „das
Handy soll für jeden übertragenen Track etwas zum Zeichnen haben“ braucht es einen echten
Hintergrund-Worker über `pending_waveform_tracks`-artige Abfragen (die Query-Funktion existiert
schon, nur der Treiber/Thread dahinter nicht) — das ist eine zusätzliche Bauaufgabe, keine, die
sich aus dem bestehenden Code von selbst ergibt.

---

## Empfehlung

**Eine Tabelle, ein Erzeuger, kein zstd, dB-Fenster statt Track-Normalisierung, CAVAs
Zweite-FFT-Größe für die Bässe übernehmen:**

1. Neue Tabelle `track_spectrograms(track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE
   CASCADE, sample_rate_hz, band_count, frame_rate_hz, frame_count, data BLOB)` — nicht auf
   `tracks` selbst, aus den unter Teil 3 gemessenen Größengründen (~100 KB/Track vs. heute 1 KB;
   ~28× Dateigrößenwachstum, wenn es eine Spalte wäre).
2. Erzeuger: die bestehende `waveform.rs`-Pipeline auf ≥ 32 kHz Zielrate anheben und **einen**
   Decode-Durchlauf zwei Konsumenten füttern lassen — den bestehenden `WaveformAccumulator` und
   einen neuen STFT/Band-Akkumulator, gebaut auf dem bereits vorhandenen `realfft`
   (`crates/reprise-core/Cargo.toml:59`, 3.5.0/`rustfft` 6.4.1). Bandaufteilung: die zwei
   FFT-Größen aus `playback/cava/bands.rs` übernehmen (kurze FFT für Bänder oberhalb ~100 Hz,
   lange FFT nur für die untersten Bänder), keine neue Signalverarbeitungs-Idee erfinden.
3. Kein zstd (und keine neue Kompressions-Abhängigkeit): gemessen 1,1–1,4× Ratio bei ~90–121 KB
   Rohgröße/Track — der Gewinn rechtfertigt keine neue Dependency plus Persistenzlogik dafür.
   Rohes `u8`-Grid speichern.
4. Skalierung: festes dB-Fenster (Boden/Decke kalibrieren, Ausgangspunkt ~−70/−6 dB), keine
   Pro-Track-Normalisierung — dasselbe Prinzip, das `bass_pressure.rs` bereits für „absolute statt
   AGC-relative Lautstärke“ etabliert hat, nicht das AGC der aktuellen CAVA-Bars und nicht die
   Track-Maximum-Normalisierung von `waveform_peaks`.
5. Der bestehende `pending_waveform_tracks`-Hook ist der richtige Ausgangspunkt für einen
   Hintergrund-Backfill, aber der Treiber dahinter muss neu gebaut werden — Lazy-on-Play allein
   deckt den mobilen Anwendungsfall nicht ab.

**Verworfene Alternativen:**

- **GStreamer-`spectrum`-Element**: verworfen, weil es nur auf dem Desktop existiert und
  `reprise-core` bewusst gstreamer-frei gehalten wird (Android-Portabilität); der Live-Visualizer
  selbst nutzt es trotz irreführendem Namen längst nicht mehr, sondern den portablen
  `realfft`-Pfad — ein zweites FFT-Backend für dieselbe Aufgabe wäre reine Redundanz.
- **`waveform_peaks`-Spalte als Vorbild („Spalte statt Tabelle“)**: verworfen anhand der
  gemessenen Größenordnung — 1 KB passt inline in die B-Tree-Seite und tut nicht weh, ~100 KB tut
  das nicht mehr; das bereits im Schema vorhandene Muster für „optionale Pro-Track-Ableitung“
  (`track_audio_analysis`, jetzt entfernt) war schon eine eigene Tabelle, aus genau diesem Grund.
- **zstd -19 wie im Auftrag angenommen**: verworfen, weil an drei echten Tracks gemessen nur
  1,09–1,14× (auch mit Transposition/Delta/xz nur bis 1,45×) erreichbar sind, nicht die
  angenommenen 4–5×. Byte-Entropie der Zellen liegt bei 7,1–7,35 Bit/8 Bit — die Daten sind fast
  maximal-entropisch, weil log-/dB-skalierte Musikspektren wie Rauschen aussehen, nicht wie ein
  Bild mit großen redundanten Flächen.
- **Track-Maximum-Normalisierung wie bei `waveform_peaks`**: verworfen, weil sie genau die
  Eigenschaft zerstört, die laut Auftrag gebraucht wird (Lautstärke soll über Tracks hinweg
  vergleichbar bleiben) — zwei Tracks mit 25 dB Unterschied sähen danach gleich hell aus.

**Widerlegte Auftragsannahme:** „~115 KB roh, 20–30 KB gepackt“ — die Rohgröße stimmt (118,6–121,4
KiB gemessen), die Kompressionsannahme nicht. Real: ~90–121 KB, kaum kleiner als roh. Das ändert
die Hochrechnung in Teil 3 von 40–55 MB auf ~170–195 MB für die volle Bibliothek — ein Faktor 4,
der die „eigene Tabelle“-Entscheidung (Teil 3) noch deutlicher macht, als der Auftrag selbst nahelegt.

Die im Auftrag als Beispiel genannte zweite mögliche Fehlannahme („24 Bänder auf 412 dp Breite
nicht auflösbar“) **trägt dagegen nicht**: Bänder liegen auf der Höhen-, nicht der Breitenachse
eines Vorschaubands (Zeit wird auf Anzeigebreite heruntergerechnet, nicht die Bandzahl) — 24
Pixel/dp Höhe sind auf jedem realen Bildschirm trivial auflösbar, und selbst der kürzeste Track in
der echten Bibliothek (29,8 s) liefert bei 20 fps noch 596 Frames, mehr als jede
Vorschaubreite in Pixeln braucht.
