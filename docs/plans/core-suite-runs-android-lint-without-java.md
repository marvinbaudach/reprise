---
slug: core-suite-runs-android-lint-without-java
worktree: /home/marvin/Projects/reprise-core-suite-runs-android-lint-without-java
branch: feature/core-suite-runs-android-lint-without-java
phase: shipped
codex_session:
created: 2026-08-18
---
# Plan: Die Core-Suite ruft Android-Lint in einem Container ohne Java

## Der Befund

Seit `6e4de2d99d` („ci(core): provision source quality tooling") ist der CI-Job
**„Core and workspace quality suite"** rot, sobald eine Änderung den `core`-Pfad
auslöst. Gemessen am 18.08.2026 an den Läufen `32143444037` (dev `6297aba86e`)
und `32148826553` (dev `56ac4a6d6d`), beide mit derselben Meldung:

```
== Android source quality ==
> ./gradlew --max-workers=2 :app:lintDebug :lint-contract:lintDebug
ERROR: JAVA_HOME is not set and no 'java' command could be found in your PATH.
```

Ein Re-Run am 18.08. hat den zweiten, gleichzeitig roten Job („Android JVM unit
suite", Auflösungsfehler für `org.jetbrains.kotlin.plugin.compose:2.4.10`) grün
wiederholt — das war Flakiness und ist **nicht** Gegenstand dieses Plans. Die
Core-Suite blieb im Re-Run rot.

## Warum es erst jetzt auffiel

`6e4de2d99d` selbst lief grün, weil der Job für diesen Commit **übersprungen**
wurde: er fasste nur `.github/`-Dateien an, der `core`-Pfad wurde also gar nicht
geroutet. Der erste Merge mit `crates/`-Änderungen (PR #553) hat den latenten
Fehler gezündet. Der „letzte grüne Lauf" ist deshalb kein Gegenbeweis.

## Die Kette

`.github/workflows/ci.yml`, Job `core-suite`, Schritt „Run the complete workspace
gate" → `scripts/ci-quality.sh` → `MERGE_READINESS_BASE_REF=… scripts/check-merge-readiness.sh --no-fetch`
→ darin der Aufruf `scripts/check-project-quality.sh` **ohne Flags**.

Ohne Argumente fährt `check-project-quality.sh` alle drei Bereiche
(`--project --showroom --android`). Der Android-Bereich ruft
`npm --prefix android run lint` → `./gradlew :app:lintDebug :lint-contract:lintDebug`.

Der `core-suite`-Job läuft im Container `archlinux:latest` und richtet Node und
uv ein, aber **kein Java** — `actions/setup-java@v5` gibt es nur im Job
`android-unit-suite`.

## Warum Java nachrüsten der falsche Fix wäre

`lintDebug` braucht zusätzlich die generierten UniFFI-Kotlin-Typen; genau dafür
gibt es im `android-unit-suite`-Job einen eigenen Bindgen-Schritt (PR #551, der
Kommentar steht in `ci.yml` direkt über „Run the Android JVM unit suite"). Ein
`setup-java` im `core-suite`-Job würde also nur den nächsten Fehlschlag
freilegen, und dazu Gradle ein zweites Mal fahren.

## Warum das Entfernen KEINE Deckung kostet

Beide Bereiche laufen bereits in eigenen Jobs, und zwar dort, wo die Werkzeuge
vorhanden sind:

- `base-contracts`: `scripts/check-project-quality.sh --project --showroom`
  (mit Node und uv)
- `android-unit-suite`: `scripts/check-project-quality.sh --android`
  (mit Java 21 und den generierten Bindings)

Der Aufruf ohne Flags in der Core-Suite ist reine Dopplung — und die einzige,
die nicht laufen kann.

## Aufgaben

### 1. Den Android-Bereich aus dem CI-Aufruf der Sammelprüfung nehmen

`scripts/check-merge-readiness.sh` ist **auch** die lokale Vollprüfung; lokal
ist Java da und der Android-Bereich gehört dort weiterhin dazu. Die Änderung
darf die lokale Deckung also nicht anfassen.

Deshalb: eine ausdrücklich benannte Umgebungsvariable, die nur der CI-Pfad
setzt, z. B. `MERGE_READINESS_SKIP_ANDROID_QUALITY`.

- In `scripts/check-merge-readiness.sh` an der Stelle des Aufrufs
  `scripts/check-project-quality.sh`: ist die Variable auf `1`/`true` gesetzt,
  stattdessen `scripts/check-project-quality.sh --project --showroom` aufrufen,
  sonst unverändert ohne Flags.
- In `scripts/ci-quality.sh` die Variable beim Aufruf von
  `check-merge-readiness.sh` mitgeben, direkt neben `MERGE_READINESS_BASE_REF`.
- An beiden Stellen ein Kommentar, der sagt **warum**: der Container hat kein
  Java und keinen Bindgen-Schritt, und `--android` läuft im Job
  `android-unit-suite`, `--project --showroom` im Job `base-contracts`.
  Ohne diese Begründung liest die nächste Person das als vergessene Deckung.

### 2. Die Stille absichern

Es darf nicht passieren, dass diese Abkürzung später unbemerkt zur einzigen
Ausführung wird. Wenn die Variable greift, eine Zeile ausgeben, die das sagt —
etwa „skipping the Android area here; it runs in the android-unit-suite job".
Ein stiller Skip ist genau das Muster, das diesen Fehler erst versteckt hat.

### 3. Nichts anderes anfassen

- Keine Änderung an `android/build.gradle.kts` und am Pin
  `org.jetbrains.kotlin.plugin.compose` — der Fehlschlag dort war Flakiness und
  im Re-Run grün.
- Keine Änderung am Pfad-Routing (`.github/scripts/ci-paths.sh`).
- Keine Änderung an `scripts/check-project-quality.sh` selbst. Insbesondere darf
  der Android-Bereich dort **nicht** anfangen, ein fehlendes Java stillschweigend
  zu überspringen — das würde die Deckung im Job, der ihn wirklich fahren soll,
  unbemerkt aushebeln.
- Kein Versions-Bump (das macht `land.sh`).

## Abnahme

- `scripts/check-shell.sh` grün (ShellCheck über alle getrackten Skripte).
- `bash -n` bzw. ShellCheck sauber für die beiden geänderten Skripte.
- Ein Trockenlauf, der belegt, dass die Verzweigung greift: mit gesetzter
  Variable erscheint die Skip-Zeile und der Android-Bereich läuft nicht; ohne
  sie läuft er wie bisher. Das lässt sich ohne die volle Sammelprüfung zeigen,
  indem der Aufruf isoliert nachgestellt wird — die Sammelprüfung selbst darf
  **nicht** gestartet werden (sie terminiert praktisch nie).
- Der eigentliche Beweis ist der `dev`-Lauf nach dem Merge: „Core and workspace
  quality suite" muss grün werden.
