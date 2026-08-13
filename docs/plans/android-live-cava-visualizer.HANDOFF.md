# Handover: Android-Visualizer auf Live-Audio

Stand: 2026-08-12, 07:28. **Achtung: ein Codex-Lauf ist noch in der Luft.**

## Sofort wissen

| | |
|---|---|
| Zweig | `feature/android-live-cava-visualizer`, **nicht gepusht** |
| Worktree | `/home/marvin/Projects/reprise-android-live-cava-visualizer` |
| Plan | `docs/plans/android-live-cava-visualizer.md` (Statusblock: `phase: refactored`) |
| Basis | `origin/dev` = `d038cfa1a0` |
| Commits | 10 (Stand 07:28), Runde 4 läuft noch |
| Auf dem Pixel installiert | Release-Build von **07:05** — das ist **nicht** der aktuelle Zweigstand |

**Der laufende Prozess:** Codex, gestartet 07:11:39, PID 1275711, Auftrag in
`<worktree>/.pipeline-findings.md`, Log `<worktree>/.tmp-codex-r4.log`,
Ergebnis danach in `<worktree>/.pipeline-codex.md`. Ein persistenter Monitor
hängt dran. Wenn du in einer neuen Sitzung übernimmst: erst `kill -0 1275711`
prüfen, dann `git log` im Worktree.

Wake-Lock `codex-pause` ist noch gehalten — nach dem Lauf freigeben
(`wake-lock release codex-pause`).

## Worum es geht

Der Android-Visualizer zeichnete 64 Balken, die aus **24 gespeicherten Bändern
linear interpoliert** waren (`visuals/spectrogram_frame.rs`), bei 20 Hz, ohne
CAVA-Kompensation, mit erfundenem Bassdruck. Ergebnis laut Nutzer: „eine sehr
homogene Masse an Bars". Der Desktop rechnet an derselben Stelle 64 echte
Bänder live aus dem PCM-Strom.

Der Umbau speist Android aus demselben Live-Pfad: Media3 `TeeAudioProcessor` →
Kotlin `LivePcmBufferSink` → UniFFI → `CavaBarProcessor` + `BassPressureDetector`
in `reprise-core`. Nebeneffekt, ausdrücklich gewünscht: **Tracks ohne Analyse
reagieren jetzt auch** — vorher fielen sie auf nicht-reagierendes Wabern zurück.

Wichtig für das Verständnis: Android nutzte die portable `VisualEngine` aus
`reprise-core` **schon vorher**. Nur der Input war schlecht. Der Umbau tauscht
die Quelle, nicht die Engine.

## Gemessen am Gerät (Pixel 10 Pro XL, Android 17, Release)

| Größe | vorher | nachher |
|---|---|---|
| Framezeit Median | 11 ms | **10 ms** |
| 90. / 99. Perzentil | 14 / 18 ms | **12 / 16 ms** |
| Janky frames | 0,20 % | 0,20 % |
| GPU-Zeit | 3 ms | 3 ms |
| `frameRateOverride` | **60 Hz** | **120 Hz** |
| ARR-Kategorie | `Normal` | `High` |

Die Framezeit ist **gesunken**, obwohl je Audiopuffer eine FFT dazugekommen
ist: Die Pro-Frame-Interpolation von 24 auf 64 Bänder samt Kotlin-Hüllkurve ist
ersatzlos entfallen, und die FFT läuft auf dem Audio-Thread ausserhalb des
UI-Locks.

Sichtprüfung durch den Nutzer: **„sieht deutlich besser aus"**.

## Offen

### 1. Runde 4 abwarten und prüfen (läuft)

Vier Befunde aus dem dritten Review, alle in Arbeit:

- **HOCH:** `state.playing` kommt aus `player.isPlaying`, und das ist in Media3
  auch bei `STATE_BUFFERING` false (`Media3PlaybackPort.kt:227-234` bildet das
  auf `PAUSED` ab). Ein Netzwerk-Stall oder Bluetooth-Wechsel ist damit an der
  Rust-Grenze nicht von einer Nutzer-Pause zu unterscheiden — der 500-ms-Verfall
  wird unterdrückt und das Bild friert ein, statt zurückzufallen. Fix: auf
  `playWhenReady` (Absicht) umstellen, plus Deckel für lange Stalls.
- **MITTEL:** `@Synchronized` in `LivePcmAudio.kt:58-86` koppelt den
  Render-Thread an den Main-Thread (JNI-Aufruf unter gehaltenem Monitor).
- **MITTEL:** Jeder Pause→Resume blitzt kurz auf die Ersatzdarstellung, weil
  `reset_audio_stream()` auch `has_live_audio = false` setzt.
- **KLEIN:** Implizite Listener-Reihenfolge zwischen `livePcmSink` und
  `Media3PlaybackPort` — nur kommentieren.

**Nach dem Lauf noch einmal den Reviewer ansetzen.** Das ist die vierte Änderung
an derselben Zustandsmaschine, und jede der drei bisherigen hat ein neues Loch
aufgemacht, das die Tests nicht gesehen haben. Der gezielte Auftrag lautet
jedes Mal: „prüfe, ob die Reparatur der Vorrunde überlebt hat" — nur so kam der
Buffering-Fehler heraus.

### 2. Danach: bauen, installieren, gegenprüfen

```
export ANDROID_HOME=/home/marvin/.local/share/android-sdk
export ANDROID_NDK_HOME=/opt/android-ndk
ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a ./scripts/android-build.sh
cd android && JAVA_HOME=/usr/lib/jvm/java-21-openjdk ./gradlew assembleRelease
adb install -r android/app/build/outputs/apk/release/app-release.apk
```

Am Gerät zu prüfen: Pause (kein Nachschlagen), Resume (kein falscher erster
Ausschlag), ein Titel der nachpuffert (fällt zurück statt einzufrieren),
Trackwechsel, Kopfhörer abziehen.

### 3. Dann erst: PR

Nichts ist gepusht. Wenn Runde 4 sauber durch ist und das Gerät bestätigt,
ist der Zweig PR-reif.

## Fallen, die hier Zeit gekostet haben

- **Framezeiten aus einem Debug-Build sind wertlos.** 97,7 % Jank im Debug
  wurden im Release zu 0,20 %, bei identischem Code. Ich habe daraus zuerst
  „der UI-Thread ist am Anschlag, der Renderer muss umgebaut werden"
  geschlossen — komplett falsch. Dazu kamen zwei eigene `Log.d`-Zeilen pro
  Frame, die selbst Teil des Messobjekts waren.
- **`gfxinfo` immer direkt vor dem Fenster zurücksetzen.** Ein Lauf ohne Reset
  enthält App-Start und Bibliotheksnutzung und lieferte 93 ms Median statt 11.
- **`worktree.sh` verzweigt vom lokalen `dev`, nicht von `origin/dev`.** Das
  lokale `dev` hängt hier hinterher. Nach dem Anlegen die Basis prüfen.
- **Das SDK liegt unter `~/.local/share/android-sdk`**, `android-build.sh` rät
  `~/Android/Sdk` und stirbt mit „SDK not found". `android/local.properties`
  existiert in einem frischen Worktree nicht und muss angelegt werden.
- **`FrameRateCategory.High` liefert 120 Hz**, nicht die 90, die
  `frameRateCategoryRate {normal=60.0, high=90.0}` nennt. Die Tabelle ist eine
  Untergrenze, keine Zuteilung — nach der Änderung neu messen statt vorhersagen.
- **Hintergrund-Beobachter per `Bash run_in_background` wurden hier zweimal von
  aussen abgeräumt**, der beobachtete Lauf lief jeweils weiter. Für lange Läufe
  den persistenten `Monitor` nehmen.
- **Codex-Zusammenfassungen sind Behauptungen.** „Keine Desktop-Renderpfade
  verändert" stimmte wörtlich, während `reprise-core/src/playback/cava.rs` um
  43 Zeilen geändert wurde — also der Code, aus dem der Desktop seine Bänder
  zieht. (Der Review hat das dann als äquivalent nachgewiesen, inklusive eines
  Tests, der alten und neuen Pfad byteweise vergleicht. Aber geprüft werden
  musste es.)
