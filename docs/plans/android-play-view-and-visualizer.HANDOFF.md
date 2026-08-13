# Handover: Android-Visuals — was noch zu tun ist

Stand: 2026-08-11, 21:45. **Nichts hängt, nichts ist halb fertig.** Beide Zweige
sind gemerged, die Worktrees geschlossen, der Emulator beendet.

| PR | in `dev` als | Inhalt |
|---|---|---|
| #413 | `996d670a01` | Play-View steht beim Songwechsel still, Queue-Wisch gehört dem Pager |
| #414 | `8b5d42653b` | Geteilter Visualizer im Cover, Nebel mit Bass-Antwort, drehende Coverscheibe |

Gemessene Belege stehen in den PR-Beschreibungen. Die Pläne liegen in
`docs/plans/android-desktop-visualizer.md`,
`docs/plans/android-fog-desktop-bloom-response.md`,
`docs/plans/android-cover-shimmer.md`.

## 1. Bildrate auf dem Pixel nachmessen (zugesagt, Handy fehlte)

Der Takt-Fix füttert die FFI **jeden** Frame statt jeden dritten. Auf dem
Emulator kostet das Bildrate — 59,2 → 43,8/45,5 Ticks/s in der Play-View, über
zwei Stichproben reproduziert. Auf echter Hardware ungemessen.

So messen (dauert ~10 min, Handy per USB, Debugging an):

1. Temporäre Zeile in `android/app/src/main/java/de/reprise/spike/NowPlayingScene.kt`
   im `SceneFrameSink`:
   `android.util.Log.d("visualtick", "${System.nanoTime()} ${bands?.take(6)?.joinToString(",") ?: "null"}")`
2. `ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a ./scripts/android-build.sh`,
   dann `cd android && JAVA_HOME=/usr/lib/jvm/java-21-openjdk ./gradlew assembleDebug`,
   installieren, Track mit Spektrogramm spielen, Visualizer an (Tap aufs Cover).
3. `adb logcat -d -s visualtick:D` auswerten: Ticks/s, Anteil `null`, verschiedene
   Bandvektoren je 50 ms. Zielwerte vom Emulator siehe oben.
4. Zeile wieder entfernen (`git checkout HEAD -- <datei>`).

Wenn die Bildrate auch auf dem Pixel deutlich fällt: entschärfen statt
zurückbauen — nur jeden zweiten Frame nachfüttern, oder die Bänder ohne
`Vec<f32>`-Kopie über die FFI reichen. Eigener Zweig, eigene Messung.

## 2. Strukturelle Glätte der Bänder (unverändert offen)

24 Bänder auf 64 gestreckt heisst ~0,365 Eingangsbänder je Ausgabebalken — keine
Zacke schmaler als ~2,7 Balken, unabhängig vom Audio. Wenn es zu glatt aussieht,
braucht es ein höher aufgelöstes Spektrogramm oder eine Live-FFT auf dem Gerät.
Das kippt Beschluss 1 des Visualizer-Plans und gehört in einen eigenen Zweig.
**Rauschen dazuzurechnen wäre erfundene Messung — nicht tun.**

## 3. Kein wandernder Lichtfleck im Schimmer (bewusst so gelassen)

Die Coverscheibe dreht sich einmal pro Minute, aber ein *wanderndes* Licht ist
nicht messbar: die Scheibe ist eine stark geblurrte Coverkopie und damit fast
rotationssymmetrisch. Auf dem Desktop ist das genauso. Falls das mal stören
sollte, ist der Hebel die Maske (härterer Kern), nicht die Drehzahl.

## Fallen, die hier Zeit gekostet haben

- **Keine Aufnahme dieses Emulators löst 60 Hz auf** — `adb shell screenrecord`
  ~11/s, host-seitiges `adb emu screenrecord` ~27–31/s (mit einem Fling als
  Kontrolle gemessen). Taktbehauptungen gehören per logcat in die App.
- **`-gpu swiftshader_indirect` zeichnet 9 fps**, `-gpu host` ~50.
- **`dumpsys media_session` meldet nach App-Neustart eine veraltete Position** —
  zwei „ab Sekunde 30"-Läufe lagen 38 s auseinander und führten zur falschen
  Schlussfolgerung. Vorher `am force-stop`, Startposition über die Zeitanzeige im
  Bild gegenprüfen.
- **Codex fährt sonst selbst den Emulator** (ein Lauf: 100 Minuten, keine
  Commits). In jeden Android-Plan: kein Gerät, kein adb, kein cua-driver.
- **Gradle meldet BUILD SUCCESSFUL, ohne Tests zu fahren** — XMLs unter
  `android/app/build/test-results/testDebugUnitTest` zählen, mit JDK 21.
