---
slug: android-playback
worktree: ~/Projects/reprise-android-playback
branch: feature/android-playback
phase: planned
codex_session:
created: 2026-08-03
---
# Android, Paket 2 — die Wiedergabe hinter das Trait

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Android-Wiedergabe läuft über `PlaybackBackend` statt an ihm
vorbei. Damit bekommt die App Warteschlange, Weiter/Zurück und
Hintergrundwiedergabe aus `reprise-core`, statt sie in Kotlin nachzubauen.

**Basis:** `dev` (`bd2c8cddac`, MVP und Welle 7 gemergt).

## Warum das der nächste Schritt ist

Der MVP spielt eine Datei mit `MediaPlayer` direkt aus Kotlin — bewusst so
gebaut, als Verkürzung. Der Preis: keine Warteschlange, kein Weiter/Zurück,
keine Hintergrundwiedergabe, keine Benachrichtigungssteuerung. Das ist der
Unterschied zwischen „die Bibliothek trägt" und „es ist ein Player".

`PlaybackBackend` ist bereits ein sauberes Trait; GStreamer sitzt in
`reprise-platform-linux`, nicht im Kern. Es fehlt nur die zweite
Implementierung.

## Die Entwurfsfrage: Ereignisse fließen rückwärts

`LibrarySource` war einseitig — Rust fragt, Kotlin antwortet. Hier gehen
**Befehle hin und Ereignisse zurück**, und die Rückrichtung ist heute ein
Rust-Closure:

```rust
Box<dyn Fn(PlayerEvent) + Send + Sync>
```

Das überquert UniFFI nicht. Wie ein `PlayerEvent` von Media3s
`Player.Listener` in den Kern kommt, ist die Frage dieses Pakets — und sie
entscheidet mehr als die Befehlsseite, weil an ihr die Warteschlangenlogik
hängt.

Beachte dabei:

- **Media3s Listener feuert auf dem Hauptthread.** Der Kern erwartet
  `Send + Sync`. Wer das übersieht, baut eine Brücke, die auf dem Emulator
  läuft und auf einem echten Gerät unter Last bricht.
- **`StreamGeneration`.** Ereignisse tragen eine Generation, damit ein
  verspätetes Ereignis eines abgelösten Streams nicht als aktuell gilt. Das
  Trait erlaubt die Vorgabe `INITIAL` („Staleness-Erkennung ist für dieses
  Backend schlicht nicht verfügbar, statt falsch"). Entscheide begründet, ob
  Media3 echte Generationen liefern kann oder ehrlich degradiert.
- **`content://` als Eingabe.** `play_uri` akzeptiert heute `http`, `https`
  und `file`. Eine SAF-URI ist keins davon. Prüfe, ob der Doc-Kommentar
  erweitert werden muss oder ob `play` mit einem opaken Bezeichner der
  richtige Weg ist — der Kern hält Pfade als Bezeichner, seit Paket 1.

## Was ehrlich degradieren darf

Das Trait sagt es selbst vor: `set_spectrum_enabled` und
`current_generation` haben Vorgaben, und `set_transition` darf `Crossfade`
wie `Gapless` behandeln („documented degradation, never a failure"). Nutze
das, statt Media3-Funktionen zu erfinden. **Ein Backend, das etwas nicht
kann, sagt es — es täuscht es nicht vor.** Dieselbe Regel, die in der
Storage-Reihe fünf Fehler verhindert hat.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`,
  `bash scripts/tests/gettext-catalogs.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe. Testbilanz nach
  **Schlüsselwort** summieren.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Der Desktop darf sich nicht ändern.** GStreamer bleibt unberührt.
- **`reprise-android-ffi` hängt nur an `reprise-core`.**
- **Keine Vorgabe-Implementierung** für etwas, das nur eine Quelle beantworten
  kann — die Regel aus Paket 3 der Storage-Reihe gilt hier genauso.
- Kein `#[allow(…)]`, keine neue Rust-Abhängigkeit im Kern.

---

## Task 1: Messen, was wirklich gebraucht wird

**Files:** keine Änderung.

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Die Bedarfsanalyse**

Welche `PlaybackBackend`-Methoden braucht ein Player, der Bibliothek zeigt,
abspielt, pausiert, springt und die Warteschlange abarbeitet? Welche kann
Media3 direkt, welche gar nicht? Und **welchen Weg nehmen Ereignisse** — welche
`PlayerEvent`-Varianten muss die Kotlin-Seite überhaupt erzeugen können?

Halte das Ergebnis in Task 2s Commit-Nachricht fest. Es ist die Grundlage für
„so viel wie nötig, nicht mehr".

---

## Task 2: Die Ereignisrichtung

**Files:**
- Modify: `crates/reprise-android-ffi/src/`

- [ ] **Step 1: Die Form**

Wie kommt ein `PlayerEvent` von Kotlin nach Rust? UniFFI trägt keine Closures.
Entscheide begründet und beachte die Thread-Frage oben.

- [ ] **Step 2: Ein Test ohne Gerät**

Ein Rust-Test, der Ereignisse durch die Brücke schickt und prüft, dass der Kern
sie in der richtigen Reihenfolge und mit der richtigen Generation sieht.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 3: Das Backend

**Files:**
- Modify: `crates/reprise-android-ffi/src/`
- Modify: `android/app/src/main/java/…`

- [ ] **Step 1: `PlaybackBackend` auf der Brücke**

- [ ] **Step 2: Media3 in Kotlin**

`ExoPlayer` mit `MediaItem.fromUri(contentUri)`. Ein `MediaSessionService`,
damit Hintergrundwiedergabe und Benachrichtigung funktionieren — ohne das ist
es kein Player, sondern ein Tonabspieler mit Bildschirmzwang.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 4: Die App benutzt den Kern

**Files:**
- Modify: `android/app/src/main/java/…`

- [ ] **Step 1: Warteschlange statt Einzeldatei**

Antippen füllt die Warteschlange aus der aktuellen Liste und startet dort —
wie auf dem Desktop. Weiter/Zurück, Pause, Position.

- [ ] **Step 2: Volle Gates und Commit**

Den Gerätelauf nicht selbst ausführen; der wird getrennt gefahren.

---

## Task 5: Festhalten

- [ ] **Step 1: Was die zweite Brücke über `PlaybackBackend` gesagt hat**

Dieselbe ehrliche Bilanz wie in Phase 5 des MVP: welche Signatur trug, welche
sich verrenkte, was man anders schneiden würde. **Die Storage-Reihe hat fünf
falsche Annahmen erst an der zweiten Quelle gezeigt** — hier ist die Gelegenheit
zu sehen, ob `PlaybackBackend` besser vorbereitet war, und warum.

- [ ] **Step 2: Ledger, Gates, Commit**

---

## Was dieses Paket nicht ist

Keine Suche, keine Alben-Ansicht, keine Playlists, keine Schreibseite. Die
Wiedergabe soll durch den Kern laufen — mehr nicht. Die Ansichten kommen
danach, und dann informiert davon, was Compose tatsächlich doppelt gebaut hat.
