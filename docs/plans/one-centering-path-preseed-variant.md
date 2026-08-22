---
slug: one-centering-path-preseed-variant
worktree:
branch:
phase: todo
created: 2026-08-20
related: one-centering-path-for-jump-and-clear
---

# Preseed-Variante für den Zentrierpfad

**Abhängigkeit:** dieser Plan folgt unmittelbar auf
`one-centering-path-for-jump-and-clear.md`. Die Vorgängerin ist bei `phase:
coded` stehen geblieben, weil die Messung (Tasks 1–2–5) ein Problem offenbart
hat, das mit Tasks 3–4 als dort spezifiziert nicht lösbar ist.

## Correction recorded 2026-08-22 — issue #620 is a different occasion

The player-bar title reproduction from #620 was traced end to end. It reaches
the anchor restore through `NavigationIntent::RevealTrack`; Core constructs its
`TrackAnchor` with a `0.0` content offset, so the old target was exactly the
revealed row's top edge. It is not an instance of the four-step fight measured
for search clearing, and the edge snap discussed below has already been removed
as recorded in `centered_scroll_restore.rs:9-36`.

The #620 fix therefore carries an explicit centre-anchor viewport intent into
the existing reload anchor writer. It does not implement Tasks 1–3 of this plan.
Those tasks and this plan's `phase` remain open for the search-clearing
occasion.

## Das Messproblem

Die Control-Arm-Messung aus Task 2 der Vorgängerin registrierte einen **vierstufigen
Kampf** statt der erwarteten Zentrierung:

```
gtk 6460 → hold 482 → hold 2923.5 → … 482 → hold 2923.5
```

Das passiert, weil Tasks 3–4 so implementiert den `AdjustmentHold` nach dem
Modelltausch anziehen (ohne ihn je freizugeben). Der Hold **verteidigt einen
Wert** — genau den Offset, auf dem die geleerte Liste zufällig steht. Die
Zentrierung schreibt dann ihren Zielwert, der Hold schreibt den alten Offset
zurück, Zentrierung versucht es erneut, und so weiter. Das ist kein Bug in der
Hold-Mechanik; das ist eine Anwendung des Hold auf den falschen Wert.

## Warum das wichtig ist

Die Vorgängerin-Messung deckte zwei Root Causes auf:

**(a) unser damaliger Edge-Snap** — `centered_scroll_restore.rs:9-36`
dokumentiert seine Entfernung und den heute verwendeten reproduzierbaren
Zeilenanker.

**(b) GTKs eigene Allokation** — nach dem Modelltausch läuft GTKs Allokationsdurchlauf
und schreibt den alten Adjustment-Offset (einen "remembered"-Wert der GTK-Seite) zurück.

**Die Messung beweist: diese sind keine Alternativen**, sondern interdependent:

- Ohne (a) landet (b) sofort und überschreibt den sauberen Zielwert der Zentrierung.
- Mit (a) ändert er nur das Startloch — der Rand wird einmal angefahren, dann
  zentriert die Zentrierung neu. Das sieht aus wie zwei separate Schritte, die aber
  beide mit ihrem Ziel landen.
- (a) ist quasi die Anker-Funktion, die der Zentrierung sagt: „fang hier an,
  nicht bei GTKs randomem Offset". Es ist **nicht die Lösung**, nur die
  Vorbedingung, dass die Zentrierung überhaupt ansprechbar wird.

**Die tatsächliche Lösung:** GTK muss nicht *danach* korrigiert werden. Er muss
*zum Zeitpunkt seiner Schreiboperation* bereits auf dem richtigen Wert liegen —
der Zentrierung, bevor GTK den alten offset zurückschreibt. Das ist exakt das,
was der Ankerpfad macht (`reload_anchor_scroll.rs:511-568`):

```rust
// 1. Preseed: den Zielwert in die Geometry eintragen
geometry.configure(…);

// 2. Hold installieren
let hold = AdjustmentHold::new(adjustment, …);

// 3. Zentrierformel rechnen
// (hier nicht nötig, Ankerpfad kennt die Position schon)

// 4. Adjustment schreiben
// 5. Hold freigeben
// 6. (Zwischen 4 und 5 läuft GTKs Allokation; die findet den preseedet Wert vor)
```

Der Ankerpfad läuft aber nur auf dem Neustart (`center_loaded_track`). Der
Wiederherstellpfad (`centered_scroll_restore`, die Such-Leerung) braucht
dieselbe Mechanik.

## Die Aufgabe

**Ein Pfad, preseed-Edition:** `centered_scroll_restore::schedule` wird zu einer
Preseed-Variante:

- Berechne den Zielwert wie heute.
- **Neu:** Schreib ihn in die Geometry statt oder vor die erste Adjustment-Schreiboperation.
- Installiere einen `AdjustmentHold` über die Zentrierung (wie der Ankerpfad).
- Rufe `reveal_position(shared, position, attempts, RevealMotion::Instant)` auf.
- Hold wird freigegeben, sobald die Zentrierung ihr Ziel geschrieben hat oder
  endgültig aufgibt.

**Geometrie-Preseed:** Der Ankerpfad nutzt
`ListGeometryLayout::configure(…)` um den scrolled-Wert geometrisch zu
verankern. Das ist ein Aufruf nach dem Model-Tausch (vor der Zentrierung),
`list_geometry.rs:72-93`. Mit Preseed muss es so aussehen:

```rust
// Nach dem Modelltausch, vor `centered_scroll_restore::schedule`:
if let Some(target) = centered_scroll_target(…) {
    geometry.configure(target);
}
centered_scroll_restore::schedule(shared, …);
```

Das ist der einzige neue Schreiber auf der Geometry-Seite. Der Hold-Timing
ist identisch mit Task 4 der Vorgängerin, aber der **Haltzielwert ist
von Anfang an der richtige**, nicht der zufällige Offset der geleerten Liste.

## Aufgaben für diesen Plan

### Task 1 — Preseed vor der Zentrierung

`track_list_reload.rs` (beide Anlässe: `CenterPlayingElsePreSearch` und
`center_loaded_track`): nach dem Modelltausch, vor
`centered_scroll_restore::schedule`, wird der Zielwert geometrisch preseedet.
Das ist ein zusätzlicher `geometry.configure(target)` für den Such-Anlass.
Der Startanlass hat es schon (via `center_loaded_track` → `apply()` → `:68-73`),
aber nicht im Hold-Pfad.

Beide Anlässe schreiben dann über Task 2 einen Hold an.

### Task 2 — Hold deckt die Zentrierung ab

Wie Task 4 der Vorgängerin, aber mit Preseed: statt `hold.release_now()` vor
der Zentrierung wird der Hold über sie gezogen, der Zentrier-Schreiber ist
erlaubter Schreiber, und die Freigabe erfolgt nach dem Schreiben oder dem
Aufgeben.

(Damit entfällt auch die Notwendigkeit, Task 3–4 der Vorgängerin komplett
umzustoßen — die Preseed-Lösung setzt auf dem vorhandenen Code auf.)

### Task 3 — Aktualisierung des Vorgänger-Plans

`one-centering-path-for-jump-and-clear.md` wird vom Status `phase: coded` mit
Messfunden auf die neue Situation hingewiesen:

- `phase: blocked` (nicht `cancelled`, sondern blockiert bis Preseed-Plan landet)
- Abschnitt `## Stand 19.08.2026 — Messfunde` hinzufügen
- `## Folgepläne` → auf diesen Plan hier verweisen

## Akzeptanz (identisch mit Vorgängerin, außer dem Punkt Preseed)

- **Kontrollarm einstufig** für beide Anlässe (Suche leeren, App-Start) —
  *nach* dieser Implementierung gemessen.
- Der `RevealMotion`-Enum wird **nicht** eingeführt (Task 3 der Vorgängerin
  wird nicht gebaut) — Preseed braucht ihn nicht.
- `Instant`-Bewegung nach Modelltausch ohne neuen Enum: das ist bereits heute
  das Verhalten von `centered_scroll_restore::schedule`.
- Alle anderen Grün-Garantien der Vorgängerin bleiben gültig.

## Nicht in diesem Plan

- Task 6 der Vorgängerin (Seitenleisten-Zentrierung) bleibt verschoben; sie
  setzt auf dem `RevealMotion`-Enum auf, der hier nicht eingeführt wird.
- Task 3 der Vorgängerin (Pfad-Vereinheitlichung via RevealMotion) entfällt;
  stattdessen bleibt die Geometrie der gemeinsame Ankerpunkt.

## Abhängigkeitsauflösung

Sind Tasks 1–2 dieses Plans erledigt und die Messung grün, wird die
Vorgängerin-Plandatei auf `phase: obsolete` gesetzt (Messansätze sind jetzt
in diesem Plan, Preseed hat sich bewährt). Tasks 3–4–6 der Vorgängerin fallen
komplett weg — die Preseed-Lösung leistet dasselbe mit weniger Komplexität.
