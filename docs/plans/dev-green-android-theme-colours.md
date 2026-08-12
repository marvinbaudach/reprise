---
slug: dev-green-android-theme-colours
worktree: /home/marvin/Projects/reprise-dev-green-android-theme-colours
branch: feature/dev-green-android-theme-colours
phase: shipped
codex_session:
created: 2026-08-11
---
# origin/dev wieder grün: drei rohe Compose-Farben aus dem Visualizer

`scripts/check-android-theme.sh` bricht den Quality-Gate ab, bevor er die
Display-Tests überhaupt erreicht. Drei Stellen aus #414 bauen Compose-Farben
außerhalb von `android/app/src/main/java/de/reprise/spike/ui/theme/`:

```
NowPlayingScene.kt:296  private fun Int.toComposeColor(): Color = Color(
NowPlayingScene.kt:576  drawRect(Color.Black.copy(alpha = safeOpacity), …)
VisualizerScene.kt:175  val color = Color(red = values[index + 1]…)
```

Die Regel ist eng gemeint und bleibt eng: Farben entstehen im Theme-Verzeichnis,
nirgends sonst. Kein Ausnahme-Mechanismus, keine Lockerung des Lints.

## Was schon da ist

Im Theme-Verzeichnis liegen die passenden Werkzeuge bereits:

- `SpectralColour.kt` → `internal fun spectralColour(red, green, blue, alpha)`,
  dokumentiert als „the sole Compose conversion for RGB channels already
  selected by Rust". Genau der Fall von `VisualizerScene.kt:175`.
- `NocturneTheme.kt` → `internal val AmbientTrueBlack = Color(0xFF000000)`.
  Genau der Fall des Scrims in `NowPlayingScene.kt:576`.

## Was zu tun ist

**Der Look darf sich nicht ändern.** Alle drei Stellen liefern heute exakt die
Farbe, die sie liefern sollen — nur am falschen Ort. Es geht um den Ort, nicht
um den Wert.

1. `VisualizerScene.kt:175` benutzt `spectralColour(...)`. Die Kanäle sind dort
   `Float`, die Funktion nimmt `Double`; entscheide anhand des Aufrufers, ob
   eine `Float`-Überladung im Theme-Verzeichnis sauberer ist als `.toDouble()`
   an der Aufrufstelle. Das `coerceIn(0f, 1f)` steckt bereits in
   `spectralColour`; doppeltes Klemmen ist überflüssig, aber der
   Alpha-Faktor `* opacity.coerceIn(0f, 1f)` muss erhalten bleiben.
2. `NowPlayingScene.kt:576` benutzt `AmbientTrueBlack.copy(alpha = safeOpacity)`
   statt `Color.Black.copy(...)`. Derselbe ARGB-Wert, nur benannt.
3. `NowPlayingScene.kt:296` — `Int.toComposeColor()` wandert als `internal`
   Helfer ins Theme-Verzeichnis und wird dort importiert. Der Konverter macht
   aus einem ARGB-`Int`, der aus dem Core kommt, eine Compose-`Color`; er
   gehört damit zur selben Familie wie `spectralColour`. Leg ihn dorthin, wo er
   inhaltlich hingehört, und nicht in eine neue Datei nur für ihn, wenn eine
   bestehende passt.

Ändere sonst nichts an den beiden Szenen: keine ARGB-Werte, keine
Alpha-Faktoren, keine Komposition.

## Abnahme

```
scripts/check-android-theme.sh
```

muss `Android theme lint passed` melden. Dazu die Android-Suite:

```
cd android && ./gradlew :app:testDebugUnitTest
```

Läuft nur unter JDK 21 — unter neueren JDKs stirbt Robolectric an
„major version 70". `scripts/check-merge-readiness.sh` nicht starten, keine
Display-Tests.
