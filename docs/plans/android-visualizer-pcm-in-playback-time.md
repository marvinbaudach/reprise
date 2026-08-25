---
slug: android-visualizer-pcm-in-playback-time
worktree: /home/marvin/Projects/reprise-android-visualizer-pcm-in-playback-time
branch: feature/android-visualizer-pcm-in-playback-time
phase: shipped
codex_session:
created: 2026-08-24
---
# Der Android-Visualizer liest sein PCM in Wiedergabezeit

## Warum

Der Visualizer auf dem Handy ruckelt sichtbar, und vier Anläufe haben daran
nichts geändert. Der Grund ist, dass alle vier die falsche Größe repariert
haben.

Gemessen am Pixel 10 Pro XL (Release 0.1.43, 120-Hz-Panel, Spektrum-Modus,
Wiedergabe per `dumpsys media_session` als `PLAYING` verifiziert, 2026-08-24):

| Größe | Wert | Urteil |
|---|---|---|
| Renderrate, `dumpsys gfxinfo`, 15,3 s | 1865 Frames = **121,6 fps** | gesund |
| Jank | 2 (0,11 %), p50 11 ms, p99 16 ms, GPU 3 ms | gesund |
| **Balken-Aktualisierung, Videoanalyse** | **~4 Hz** (Median 30–31 Frames = 250–259 ms) | **kaputt** |

Faktor 30 zwischen dem, was gezeichnet wird, und dem, was sich ändert. Die
Zeitreihe eines einzelnen Balkens ist ein Sägezahn: ein langer, glatter
Abkling-Fade über ~30 Frames, dann ein Sprung.

```
98 98 98 97 97 96 95 95 93 ... 78 77 75 │ 85 87 88 88 87 ... 74 74 │ 87 89 96
└──────── Abkling-Fade, 120 Hz ─────────┘└── neue Daten ──┘        └── neue Daten
```

Kontrollarm auf einem zweiten Track: Median-Abstand 30 Frames = 250 ms. Der
Takt ist **musikunabhängig** — also eine feste Blockgröße, nicht der Inhalt.

**Ursache.** Media3s `DefaultAudioTrackBufferSizeProvider` hat den fest
eingebauten Boden `MIN_PCM_BUFFER_DURATION_US = 250000` (250 ms; verifiziert
per `javap` gegen `media3-exoplayer-1.10.1`). Der `TeeAudioProcessor`, der das
PCM für den Visualizer abgreift, sitzt **vor** genau diesem Puffer —
`LivePcmAudio.kt:146-152` baut den `DefaultAudioSink` ohne eigenen
`setAudioTrackBufferSizeProvider(...)`, also greift der Default. ExoPlayer
dekodiert im Schub voraus, bis der Puffer voll ist, ruft dabei
`LivePcmBufferSink.handleBuffer()` in einem Rutsch, und pausiert dann, bis die
Hardware wieder Platz geschaffen hat. Am Gerät sichtbar als `FrmCnt 24000`
@48 kHz (= 500 ms) und `Latency 609 ms` in `dumpsys media.audio_flinger`.

`ingest_pcm_i16` (`visualizer.rs:277-337`) jagt diesen 250-ms-Block **am Stück**
durch CAVA: ein `process_pcm_i16`-Aufruf, ein `SpectrumFrame`, ein sichtbares
Ergebnis. Die 249 ms Audio davor werden mitgerechnet, aber nie gezeigt.

**Ausgeschlossen, jeweils belegt statt vermutet:**

- Audio-Offload — explizit aus, `livePcmAudioOffloadPreferences()` setzt
  `AUDIO_OFFLOAD_MODE_DISABLED`.
- CAVA-Sammelpuffer — `cava.rs:148-206`, `process_into` rechnet bei jedem
  Aufruf komplett durch, `push_samples` ist ein Sliding-Window-Update. Kein
  „warte auf N Samples".
- `dropped_audio_frames` — greift nur bei echter Mutex-Kontention
  (`visualizer.rs:298-303`), stieg gemessen um 1 pro 2,5 s.

**Zweiter Fehler derselben Wurzel.** Der Visualizer zeigt das PCM, sobald es
dekodiert ist — also 250–600 ms **bevor** es hörbar ist. Das Bild eilt dem Ton
voraus. Das ist bisher niemandem aufgefallen, weil bei 4 Hz ohnehin kein
Zusammenhang zwischen Bild und Ton erkennbar war.

**Was die Vorgänger repariert haben** (und es ist repariert): #633 den
UI-Service-Desync, #644 die Szenenuhr, #646 die Per-Frame-Kosten der Brücke.
Die Messung oben reproduziert exakt den „in sync"-Sollwert aus deren
Handoff-Protokoll. Die Renderseite ist gesund. (#654 gehört zu einem anderen
Plan und berührt keine Visualizer-Datei.) Keiner der drei hat die Datenrate je
gemessen — das Abnahmekriterium war immer `Total frames rendered`. Genau das
ist der Fehler, den dieser Plan nicht wiederholen darf.

## Zielbild

Das ankommende PCM wird nicht mehr am Stück analysiert, sondern **in
Wiedergabezeit gelesen**: `ingest_pcm_i16` legt die Samples nur noch in einen
Ringpuffer, und die Analyse entnimmt bei jedem Szenen-Tick genau den
Ausschnitt, der zur verstrichenen Zeit gehört.

```
vorher:  [250 ms Block] ──> CAVA ──> 1 Balkensatz, dann 250 ms Stillstand
                                     └─ 4 Hz, Bild eilt dem Ton voraus

nachher: [250 ms Block] ──> Ringpuffer ──> pro Tick ~8 ms ──> CAVA
                                           └─ Panel-Rate, konstante Verzögerung
```

Der entscheidende Glücksfall: `tick()` (`visualizer.rs:398-404`) berechnet
bereits `elapsed = now - last_visual_tick_at`. Das **ist** die Zeitbasis. Es
braucht keinen `AudioTrack.getTimestamp()` und keine Media3-Position — die
Entnahmemenge ist `elapsed × sample_rate`.

## Entwurf

Die sechs Entscheidungen unten sind gegrillt und verbindlich. Wo eine Variante
verworfen wurde, steht der Grund dabei — nicht neu aufrollen.

### Der Ringpuffer

Er lebt in `LiveAudioState` (`visualizer.rs:82-90`) und hält **Mono-f32-Samples
nach dem Downmix**, nicht die rohen Bytes. Der Downmix ist billig, passiert
ohnehin schon in `process_pcm_i16`, und spart Faktor `channel_count` an
Speicher und an Arbeit auf der Entnahmeseite.

Kapazität: 2 s bei der aktuellen `sample_rate_hz` (96 000 f32 = 384 KB bei
48 kHz). Reichlich über der gemessenen Vorauslauf-Latenz von max. 609 ms.

Bei Kapazitätsüberschreitung wird **vorne verworfen**, nicht hinten — ein
voller Puffer heißt, die Entnahme hängt zurück, und dann ist das Neueste das
Richtige.

### Die Aufteilung der Arbeit

| Thread | vorher | nachher |
|---|---|---|
| Audio (Media3) | Downmix + FFT + Bassdetektor + Zustandsübergabe | Downmix + Ringpuffer schreiben |
| Szene (Tick) | `advance_by(elapsed)` | Entnahme + FFT + Bassdetektor + `advance_by` |

Das entlastet den Audio-Thread deutlich und macht nebenbei den offenen Punkt
**C-4** aus #646 gegenstandslos: der kritische Abschnitt auf der Audioseite
schrumpft auf „Bytes downmixen und anhängen", die Kontention verschwindet
weitgehend, und `dropped_audio_frames` sollte nicht mehr wachsen.

### Analyse bei jedem Tick

**Keine eigene Analyse-Rate.** Jeder Szenen-Tick entnimmt und rechnet: auf dem
120-Hz-Panel 120 Analysen/s, auf einem 60-Hz-Gerät 60 — also immer mindestens
auf Desktop-Niveau (der speist mit 60–75 Hz ein) und automatisch passend zum
Gerät.

Das ist zugleich der einfachste Code: kein Zeit-Akkumulator, kein „ist es schon
Zeit"-Zweig, und die Entnahmemenge bleibt schlicht `elapsed × sample_rate`.
Eine feste 60-Hz-Rate wurde verworfen, weil sie diesen Apparat kostet und auf
diesem Gerät hinter dem Desktop zurückbliebe.

Pro Analyse werden die entnommenen Samples per `push_samples` in CAVAs Sliding
Window geschoben, dann einmal `process_into`. Das FFT-Fenster von 8192 Samples
(`bands.rs:57-66`) bleibt die Analysetiefe und ist davon unberührt.

### Der Füllstandsregler

Tick-Uhr (`SystemMonotonicClock`, CPU-`Instant`) und Zufuhr (Sample-Clock der
Soundkarte) driften real gegeneinander, typisch 10–100 ppm. Ohne Korrektur
läuft der Füllstand über Minuten weg. Ein **P-Regler mit explizitem Sollwert**:

```
soll      = SOLL_FUELLSTAND * sample_rate        // SOLL_FUELLSTAND = 250 ms
nominal   = elapsed * sample_rate
korrektur = (fuellstand - soll) / TAU            // TAU ≈ 30 Analysen
entnahme  = clamp(nominal + korrektur, 0.9 * nominal, 1.1 * nominal)
```

Der ±10-%-Deckel ist die Gegenmaßnahme gegen sichtbares Beschleunigen und
Bremsen der Balken. Ein gleitender Mittelwert als Sollwert wurde verworfen: er
wandert mit der Störung mit, ein dauerhaft zu voller Puffer würde damit zur
neuen Norm statt korrigiert zu werden.

**Überfüllung wird nicht geregelt, sondern geschnitten.** Liegt der Füllstand
über dem Doppelten des Sollwerts, wird vorne hart auf den Sollwert verworfen:

```
if fuellstand > 2 * soll:
    verwirf_vorne(fuellstand - soll)
else:
    regle()
```

Das deckt in einer Regel drei Fälle ab, die der Regler sonst über Sekunden
abbauen müsste: Rückkehr aus dem Hintergrund (`DriveScene` stoppt das Ticken
bei `!runtimeActive`, während der Dienst weiter einspeist), Reste nach einem
Seek, und den Anfangsschub beim Start. Es hält den Regler zugleich testbar,
weil er nie große Auslenkungen sehen muss.

### Ton-Bild-Versatz

`SOLL_FUELLSTAND = 250 ms` ist eine dokumentierte Konstante und der Stellknopf
für die Synchronität. Weil das PCM uns 250–600 ms **vor** dem Ton erreicht,
schiebt ein größerer Sollfüllstand das Bild **näher** an den Ton, nicht weiter
weg.

**Die Feinkalibrierung ist ausdrücklich nicht Teil dieser Abnahme.** Sie
braucht einen Klatsch- oder Impulstest und ist eine eigene, spätere Messung.
Dieser Plan stellt her, dass der Versatz *konstant* ist; wie groß er ist, ist
dann eine Zahl.

### Zustandsübergänge

| Ereignis | Ringpuffer | Regler |
|---|---|---|
| Seek | leeren | zurücksetzen |
| Trackwechsel (`note_track_changed`) | leeren | zurücksetzen |
| `reset_audio_stream` | leeren | zurücksetzen |
| Pause | **behalten** | einfrieren |
| Unterlauf (leer) | — | einfrieren, kein Nachziehen |

Bei Unterlauf wird **nicht** mit Stille aufgefüllt — das würde die Balken
fälschlich auf null ziehen. Stattdessen läuft die vorhandene Fade-Mechanik
weiter, genau wie heute zwischen zwei Blöcken. Der Unterschied ist, dass dieser
Fall künftig die Ausnahme ist statt der Normalfall.

Pause behält den Puffer, weil `set_engine_playing`/`retain_paused_live_shape`
die stehende Form bereits bewusst hält (#432).

`live_audio_is_current`/`LIVE_AUDIO_STALE_AFTER` (500 ms) bleiben unverändert
gültig — aber der Bezugspunkt wandert: `last_live_audio_at` wird künftig bei der
**Entnahme** gesetzt, nicht beim Eintreffen des Blocks. Sonst gilt Live-Audio
als frisch, während der Puffer längst leer ist.

## Aufgaben

### T1 — Ringpuffer in `LiveAudioState`

`crates/reprise-android-ffi/src/visualizer.rs`

Ringpuffer fester Kapazität (2 s bei der aktuellen `sample_rate_hz`) als Feld
von `LiveAudioState`. Schreibseite verwirft bei Überlauf vorne. `reset()` leert
ihn.

Tests (`visualizer_tests.rs`): Überlauf verwirft die ältesten Samples; `reset`
leert; die Kapazität folgt der Abtastrate.

### T2 — `ingest_pcm_i16` schreibt nur noch

`crates/reprise-android-ffi/src/visualizer.rs:277-337`

Downmix bleibt, alles danach entfällt auf diesem Pfad: kein `process_into`,
kein `pressure_detector.observe`, kein `state.engine.ingest`. Der Aufruf hängt
die Mono-Samples an den Ringpuffer und kehrt zurück.

`has_live_audio`/`last_live_audio_at` werden hier **nicht** mehr gesetzt (das
wandert nach T3), aber der Stream-Generations-Abgleich bleibt, damit Samples
eines abgelösten Streams nicht in den Puffer geraten.

Tests: ein Aufruf ändert die Balken *nicht* mehr sofort; die Samples liegen
danach im Puffer; ein Aufruf mit falscher Generation landet nicht im Puffer;
die bestehenden Validierungen (Kanalzahl, `byte_count`, Vielfaches von
`frame_bytes`) gelten unverändert.

### T3 — Entnahme im Tick

`crates/reprise-android-ffi/src/visualizer.rs:398-404`

`tick()` entnimmt anhand von `elapsed`, rechnet CAVA und den Bassdetektor, und
übergibt das Ergebnis wie bisher an `state.engine.ingest`. Hier werden
`has_live_audio`, `last_live_audio_at` und `live_pressure` gesetzt.

Der Rückgabewert von `tick()` bleibt „hat sich etwas geändert" — er muss jetzt
auch dann `true` sein, wenn nur ein neuer Balkensatz eingespeist wurde.

**Sperrreihenfolge:** `tick()` hält heute `state`. Die Entnahme braucht
zusätzlich `live_audio`. Beide in **derselben** Reihenfolge nehmen wie
`ingest_pcm_i16` (erst `live_audio`, dann `state`), sonst entsteht ein
Deadlock-Fenster.

Tests: nach N simulierten Ticks sind N Balkensätze entstanden; ein Tick ohne
Puffervorrat ändert die Balken nicht; die Entnahmemenge folgt `elapsed`
(Testuhr benutzen, `with_clock` existiert); bei konstanter Einspeisung ändern
sich die Balken bei jedem Tick.

### T4 — Füllstandsregler

`crates/reprise-android-ffi/src/visualizer.rs`

P-Regler nach der Formel oben, inklusive des harten Schnitts bei > 2× Soll.
`SOLL_FUELLSTAND` und `TAU` als benannte Konstanten mit Kommentar, warum sie
diese Werte haben.

Tests: dauerhafte Überfüllung baut sich ab, ohne dass die Entnahmemenge um mehr
als 10 % vom Nominalwert abweicht; der harte Schnitt greift ab 2× Soll und
lässt genau `soll` stehen; dauerhafte Unterfüllung führt nicht zu Nachziehen
ins Leere; der Regler konvergiert bei konstanter Zufuhr.

### T5 — Zustandsübergänge

`crates/reprise-android-ffi/src/visualizer.rs`

Puffer und Regler nach der Tabelle oben an `note_track_changed`,
`reset_audio_stream`, `reset_audio_history` und den Seek-Pfad hängen.

Tests je Übergang: nach Trackwechsel enthält der Puffer keine Samples des
Vorgängers; Pause behält; Unterlauf zieht die Balken nicht auf null.

### T6 — Messskript für die Datenrate

`scripts/android-visualizer-data-rate.sh`

Das Rezept aus der Abnahme als Skript, damit der Beleg wiederholbar ist und
nicht wieder an „den Menschen" weitergereicht wird. Es tritt **neben**
`scripts/android-scene-framerate.sh`, ersetzt es nicht: jenes misst die
Renderrate, dieses die Datenrate.

Ohne Terminal fahrbar sein (`REPRISE_SCENE_ASSUME_READY=1`-Äquivalent) — das
bestehende Skript liest seine Bestätigung von `/dev/tty` und ist damit aus
einer Agentensitzung heraus nicht startbar; genau das hat die Messung, aus der
dieser Plan entstand, zuerst blockiert.

Es muss den Bildschirm wachhalten (`adb shell svc power stayon usb`) und ihn
danach zurücksetzen — sonst bricht `screenrecord` mit `UNASSIGNED_LAYER_STACK`
ab, sobald das Display einschläft.

## Risiken

**FFT auf dem Tick-Pfad erzeugt Jank.** Das ist die eine Stelle, an der dieser
Plan die gesunde Renderseite gefährden kann: zwei FFTs (main + bass) pro Tick
im Frame-Budget von 8,3 ms, geschätzt 100–200 µs — geschätzt, nicht gemessen.
Deshalb steht die Renderrate mit im Abnahmekriterium. Fällt sie durch, ist der
Ausweg zweistufig: erst die Analyse auf jeden zweiten Tick drosseln (Einzeiler),
dann — nur wenn das nicht reicht — einen Analyse-Thread nachrüsten, für den der
Ringpuffer bereits die richtige Entkopplung ist. **Nicht vorsorglich bauen.**

**Der Regler schwingt.** Ein zu aggressiver Regler erzeugt sichtbares
Beschleunigen und Bremsen. Der ±10-%-Deckel ist die Gegenmaßnahme; die
Gerätemessung muss auf die Verteilung schauen, nicht nur auf einen Mittelwert.

**Wiedergabe ohne Live-PCM.** Der Fallbackpfad `ingest_bands` (gespeichertes
Spektrogramm, 20 Hz) bleibt unangetastet und behält seine Interpolation
(`SceneDriver.bandsForTick()`/`motionBandsWithin()`). Die Vorrangregel „Live
schlägt gespeichert" gilt weiter, hängt aber jetzt an der Entnahme statt am
Eintreffen — das ist in T3 der Punkt, an dem `last_live_audio_at` gesetzt wird,
und ein Fehler dort lässt den Fallback nie oder immer greifen.

**Media3 ändert seine Puffergröße.** Der Plan macht sich von der konkreten
250 ms unabhängig — der Ringpuffer verträgt jede Blockgröße von einem Sample
bis 2 s. Ein Media3-Update kann diesen Fix nicht entwerten.

## Abnahme

Zwei Zahlen am echten Gerät, mit Kontrollarm. Der Beleg, den alle vier
Vorgänger an „den Menschen" weitergereicht haben und den keiner erbracht hat.

**Datenrate** (das eigentliche Ziel):

```
adb shell svc power stayon usb          # sonst bricht screenrecord mit UNASSIGNED_LAYER_STACK ab
# Wiedergabe per dumpsys media_session als PLAYING verifizieren, Spektrum-Modus, App im Vordergrund
adb shell screenrecord --time-limit 10 --bit-rate 40000000 /sdcard/viz.mp4
adb pull /sdcard/viz.mp4
ffmpeg -i viz.mp4 -filter:v "crop=660:230:210:880,scale=64:1:flags=area" \
       -pix_fmt gray -fps_mode passthrough -f rawvideo bars.raw
```

Die 64 Spalten sind die 64 Balken. `-fps_mode passthrough` ist **Pflicht**:
screenrecord schreibt 90k tbn, ohne den Schalter dupliziert ffmpeg auf ~900 000
Frames. Dann auf der Spalte mit der größten Varianz die Anstiege
(`s[i] - s[i-1] >= 6`) finden und die Abstände dazwischen histogrammieren.

Gemessen wird **nicht** der Median, sondern die Stillstands-Lücke. Der Median
taugt hier nicht: nach dem Fix laufen die Balken kontinuierlich statt in
Sprüngen, und eine Sprung-Zählung könnte einen gelungenen Fix als Misserfolg
ausweisen. Die Lückenlänge misst dagegen genau den Defekt.

| | Baseline (gemessen 2026-08-24) | Ziel |
|---|---|---|
| Anteil der Abstände ≥ 15 Frames | ~77 % (33 von 43) | **0 %** |
| 95. Perzentil des Abstands | 33 Frames | **≤ 8 Frames** |
| Verteilung | zwei Populationen: 1–3 und 30–33 | eine Population, kein Puffer-Takt |
| Kontrollarm | zweiter Track: 30 Frames Median | zweiter Track im selben Band |

Verbleibende Lücken dürfen nur noch aus leisen Musikstellen stammen, nicht aus
dem Puffer — erkennbar daran, dass sie mit dem Track wechseln statt bei
250 ms zu klumpen.

**Der Desktop ist der eigentliche Maßstab.** Die Vorgabe lautet „es soll so in
etwa reagieren wie beim Desktop", und das ist messbar statt Geschmackssache:
derselbe Track, dieselbe Auswertung, einmal auf dem Desktop-Visualizer
aufgenommen (Bildschirmaufnahme der laufenden GTK-App, Crop auf dessen
Balkenbereich, dieselbe `scale=64:1`-Kette und dieselbe Lückenstatistik).

Das ergibt eine Referenzverteilung, und die Abnahme fordert: **die
Lückenverteilung des Handys liegt im selben Band wie die des Desktops** — nicht
nur unter einer abstrakten Schwelle. Fällt der Desktop selbst schlechter aus
als die Zielwerte oben, gilt der Desktop-Wert, nicht die Tabelle: gleichwertig
zum Desktop ist erfüllt, besser als der Desktop ist nicht gefordert.

Diese Referenzmessung wird **vor** dem Codex-Lauf erhoben, damit die Zielzahl
feststeht, bevor jemand am Code dreht — eine Referenz, die erst nach dem Fix
entsteht, lässt sich unbewusst passend wählen.

**Renderrate** (darf nicht bezahlen):

`dumpsys gfxinfo <pkg> reset`, 15 s, auslesen. Baseline 121,6 fps / 0,11 %
Jank / p50 11 ms. Ziel: **≥ 110 fps, Jank ≤ 1 %**.

**Audio** (darf nicht bezahlen): `dumpsys media.audio_flinger` zeigt für die App
keine neuen `Underruns` gegenüber einem Lauf vor dem Fix.

**Regressionsschutz:** `dropped_audio_frames` wächst nach dem Fix nicht mehr
(T2 nimmt die Arbeit vom Audio-Thread) — wenn doch, ist die Sperrreihenfolge aus
T3 falsch.

Ein Lauf zählt nur, wenn Wiedergabe **und** Vordergrund am Anfang und am Ende
geprüft sind. Mehrere Läufe der Vorgänger waren wertlos, weil die
Benachrichtigungsleiste offen war (`Total frames rendered: 0`) oder der
Bildschirm einschlief.

### Auflagen aus dem Review (2026-08-25)

Vier Funde des Reviews sind keine Code-Fehler, sondern Bedingungen an genau
diese Abnahme. Ohne sie kann ein Lauf grün aussehen und trotzdem nichts sagen.

**A1 — Ein Lauf ist kein Beweis.** Das Skript misst pro Aufruf eine
Kombination und aggregiert nichts; die Tabelle oben verlangt aber Ziel- **und**
Kontrollarm im selben Band. Also zwei Aufrufe, zwei `analysis.txt`, und beide
Zahlen kommen in den Bericht. Ein einzelner zitierter Lauf zählt nicht.

**A2 — Die Capture-Rate gehört ins Protokoll.** `long_gap ≥ 15` und
`p95 ≤ 8` sind nur bei ~120 fps die 125 ms und 67 ms, die hier gemeint sind.
Das Skript misst die tatsächliche Bildrate der Aufnahme nie. Drosselt der
Encoder, stehen dieselben Frame-Zahlen für eine andere Wanduhrzeit und die
Läufe sind nicht mehr vergleichbar. Also die Rate aus der Datei ablesen und
mitschreiben, bevor die Schwellen gelten.

**A3 — Belegen, dass die Spektrum-Szene gemessen wurde.** Resumed, Focused und
PLAYING werden erzwungen, die Szene selbst hängt an einer Nachfrage, die
`REPRISE_SCENE_ASSUME_READY=1` überspringt. Unbeaufsichtigt kann der Crop auf
irgendein anderes animiertes Element zeigen. Ein Einzelbild aus dem
beschnittenen Bereich gehört zu den Belegen.

**A4 — `dropped_audio_frames` misst nicht, was der Regressionsschutz
behauptet.** Oben steht, ein Wachsen zeige eine falsche *Sperrreihenfolge*. Das
trifft nicht: Vorher lief die FFT auf dem Audio-Thread etwa 4×/s, jetzt auf dem
Tick-Thread bis zu 120×/s, und sie hält dabei beide Sperren, während
`ingest_pcm_i16` nur `try_lock`t und bei Kollision verwirft. Die Mechanik ist
Haltedauer × Frequenz. Wächst der Zähler, ist die richtige Frage, wie lange
`tick()` die Sperre hält — nicht, in welcher Reihenfolge es sie nimmt.

## Parallelität

**Kein Schnitt. Ein Strang.**

T1 bis T5 ändern alle `crates/reprise-android-ffi/src/visualizer.rs`, und zwar
überwiegend dieselben Funktionen: T2 nimmt aus `ingest_pcm_i16` heraus, was T3
in `tick()` wieder einsetzt, T4 verändert die Entnahmemenge, die T3 einführt,
T5 hängt an Feldern, die T1 anlegt. Es gibt keine disjunkte Dateigruppe.

Ein Schnitt „Rust / Messskript" (T1–T5 gegen T6) wäre formal disjunkt, ist aber
sinnlos: T6 ist ein Hilfsskript von wenigen Dutzend Zeilen, und seine einzige
Existenzberechtigung ist, den Fix aus T1–T5 zu belegen. Zwei Worktrees und zwei
Codex-Läufe für diese Menge Arbeit kosten mehr Wall-Clock, als sie sparen.

Der Aufwand liegt ohnehin nicht in der Codemenge, sondern in der Gerätemessung,
und die ist seriell.
