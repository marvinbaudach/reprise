---
slug: cua-explore-hover-preflight-and-geometry
worktree: /home/marvin/Projects/reprise-cua-explore-night-run-fixes
branch: feature/cua-explore-night-run-fixes
phase: planned
created: 2026-08-11
---

# B2 und B3 — der Preflight muss beweisen, die Geometriebelege müssen überleben

Beide Befunde stammen aus dem Review-Sweep zu `scripts/cua-explore` (Runde 2,
„Vertagt"). Sie blockieren den Nachtlauf M4: ohne B2 bricht seit dem
Treibervertrag **jede** Mission mit `hover`-Capability vor der Aktionsschleife
ab, ohne B3 sind Geometrie- und Layoutbefunde eines Laufs mit Neustart nicht
belastbar.

Gemeinsamer Kern beider Befunde: **etwas Ungeprüftes sah aus wie ein Messwert.**
Genau daran hängt auch die Abnahme unten.

## Rahmen

- Arbeitsverzeichnis ist der bestehende Worktree
  `/home/marvin/Projects/reprise-cua-explore-night-run-fixes` auf
  `feature/cua-explore-night-run-fixes`. Nichts außerhalb anfassen.
- **Kein `cargo`.** Diese Aufgabe ist reines Python unter `scripts/cua-explore/`
  und `scripts/tests/`.
- Fokussierte Commits, **Englisch im Code und in den Commit-Messages**, kein Push.
- Die Testsuite läuft ohne Xvfb und ohne echten Treiber. **Nicht versuchen,
  `cua-driver` gegen einen echten Bildschirm zu fahren** — die dafür nötigen
  Messungen sind unten bereits erledigt und als Fakten aufgeführt.

## B2 — der Hover-Preflight verifiziert nichts

`scripts/cua-explore/driver.py:694-723` (`hover_preflight`), Aufrufer
`scripts/cua-explore/launch.py:150` (`prepare_hover`) und
`scripts/cua-explore/runner.py:24`.

Der Docstring verspricht „Verify desktop-pointer dispatch before spending a
mission action budget". Der Code ruft `move_cursor`, `get_cursor_position` und
`get_window_state` — und **vergleicht sie nie**. Die einzige erkannte Fehlerart
ist eine Exception. Bewegt ein Treiber-Update den echten X11-Zeiger nicht mehr,
sind alle Vorher/Nachher-Bilder identisch, und `hover_oracle.py:136-152` schreibt
für **jedes** Ziel `hover-affordance-missing` als `error` — dem Produkt
zugeschrieben. Eine ganze Hover-Messreihe kippt, ohne dass irgendwo steht „der
Zeiger war nie dort".

### Die Messung ist gemacht — das sind Fakten, keine Annahmen

Eigener Xvfb, eigene D-Bus-Sitzung, `cua-driver 0.19.3`, ein
`gnome-text-editor`-Fenster, Zeigerwahrheit unabhängig über
`xdotool getmouselocation`. Start (100,100), Ziel (640,480), vier Aufrufformen.

**`get_cursor_position` — Eingabeschema kennt nur `session`**
(`additionalProperties: false`):

| Leseform | Antwort | stimmt mit xdotool? |
| --- | --- | --- |
| `{}` (sitzungsfrei) | `{"source":"x11","x":…,"y":…}` | **ja, in allen vier Fällen** — auch dann, wenn der Zeiger *nicht* bewegt wurde |
| `{"session":S}` | `{"code":"desktop_escalation_required","desktop_unlocked":false,"effective_scope":"window",…}` | nie — auch nicht, wenn der Zeiger beweisbar auf dem Ziel stand |
| `{"pid":P,"window_id":W,"session":S}` (heutige Harness-Form) | dasselbe `desktop_escalation_required` | nie |
| `{"pid":P,"window_id":W}` | `{"code":"invalid_arguments","detail":"unknown field \`pid\`, expected \`session\`"}` | — |

**`move_cursor`** (Schema kennt `cursor_id`, `scope`, `session`, `x`, `y`):

| Bewegungsform | Antwort | echter Zeiger bewegt? |
| --- | --- | --- |
| `{"pid","window_id","session","scope":"desktop","x","y"}` (heutige Harness-Form) | `{"effect":"unverifiable","delivery":{"mode":"not_applicable"},"route":"global_input"}` | **ja** |
| `{"scope":"desktop","x","y"}` (sitzungsfrei, schemakonform) | dieselbe Antwort, `route: global_input` | **ja** |
| `{"session","scope":"desktop","x","y"}` (schemakonform **mit** Sitzung) | `desktop_escalation_required` | **nein** |
| `{"session","scope":"window","x","y"}` | `{"effect":"unverifiable",…,"route":"synthetic_events"}` | **nein** |

**Daraus folgt, verbindlich:**

1. **Die Sitzung ist das, was die Eskalationssperre auslöst** — nicht die
   fehlenden `pid`/`window_id`. Sitzungsgebundene Aufrufe sind auf `window`-Scope
   gesperrt; sitzungsfreie Aufrufe sprechen über den echten X11-Desktop.
2. **Die sitzungsfreie Abfrage ist die richtige Auskunft über denselben Zeiger.**
   Sie meldet `source: x11` und deckte sich in allen vier Fällen exakt mit
   `xdotool` — auch in den beiden Fällen, in denen der Zeiger stehen blieb. Sie
   ist damit kein Ersatzwert, sondern die Messung.
3. **Eskalation ist keine Option.** `cua-driver describe escalate_session` sagt
   wörtlich: „Escalation is permanent for that session and disables
   window-scoped tools. To recover window scope, call end_session, then
   start_session with a new session id." Das nähme der Mission genau den
   sitzungsgebundenen `get_window_state`-Pfad, auf dem jeder Snapshot beruht.
   Nebenbei gemessen: `reason` ist ein Enum
   (`ax_tree_pixel_mismatch`, `background_delivery_failed`, `foreground_ineffective`,
   `no_window_target`, `other`); ein Freitext wird mit `invalid_escalation_reason`
   abgelehnt — das ist eine der vier bekannten Fehlerhüllen.
4. **Die Antwort auf `move_cursor` kann niemals als Beweis dienen.** Sie lautet
   auch im erfolgreichen Fall `"effect": "unverifiable"`. Nur das Zurücklesen
   beweist etwas.

### Zu tun

- `hover_preflight` muss **beweisen**, dass der Zeiger sich bewegt hat:
  Position lesen → an eine bekannte, **von der aktuellen verschiedene**
  Koordinate bewegen → erneut lesen → **vergleichen**. Steht der Zeiger nicht
  dort (kleine Toleranz von wenigen Pixeln ist in Ordnung, sie muss benannt
  sein), ist das ein lauter Fehler **vor** dem ersten Aktionsbudget — und
  ausdrücklich **kein** Produktbefund. Der Fall „Zielkoordinate ist zufällig die
  aktuelle Position" muss abgedeckt sein, sonst besteht ein toter Zeiger die
  Prüfung.
- Gelesen wird mit der **sitzungsfreien** Form. Die Begründung aus der Messung
  gehört als Kommentar **in den Code**, nicht nur in die Commit-Message: warum
  die sitzungsgebundene Form hier nichts über den echten Zeiger sagt und warum
  Eskalation ausscheidet.
- Der Preflight muss **dieselbe Aufrufform prüfen, die die Mission später
  benutzt.** Sonst beweist er einen anderen Pfad. Deshalb: Die Nutzlast für
  desktop-scope-Zeigerbewegungen kommt aus **einer** gemeinsamen Stelle, die
  sowohl der Preflight als auch der produktive Hover-Pfad
  (`scripts/cua-explore/hover_probe.py`) benutzt.
- Für diese gemeinsame Stelle ist die schemakonforme, sitzungsfreie Form
  `{"scope":"desktop","x":…,"y":…}` die erste Wahl: sie ist gemessen
  wirkungsgleich mit der heutigen Harness-Form und räumt zugleich das
  Schema-Risiko aus B10 ab (`pid`/`window_id` stehen nicht im Schema von
  `move_cursor`; eine spätere Schemadurchsetzung würde den Lauf abbrechen).
  Wenn sich beim Umbau zeigt, dass der Hover-Pfad die Sitzung aus einem anderen
  Grund braucht, ist die Harness-Form die zulässige Alternative — dann aber mit
  benanntem Grund im Code.
- Der Rückgabewert bleibt Evidenz: `hover-preflight.json` muss hinterher
  erkennbar machen, **was** verglichen wurde (Vorher, Ziel, Nachher, Urteil),
  statt ein rohes Antwortobjekt abzulegen. Bisher landete dort stillschweigend
  das Fehlerobjekt als Messwert.

## B3 — Geometriebelege überleben keinen Restart

`scripts/cua-explore/runner.py:655-663` (der `finally`-Block), Quelle
`runner.py:343` (`launch_executor` baut den `CuaExecutor`), Neuanlage beim
Restart `runner.py:572-588`, Auswertung `scripts/cua-explore/report.py:252` und
`report.py:348-352`. Die Felder liegen auf der Instanz:
`driver.py:116-118` (`geometry_failures`, `geometry_calibration`,
`geometry_resolution`).

`launch_executor` baut bei **jedem** Restart einen neuen `CuaExecutor`, und der
`finally`-Block liest per `getattr` nur den **zuletzt gebundenen**. **Fünf der
sechs Missionen haben einen Restart-Workload.** Zusätzlich wird
`geometry_resolution` bei *jedem* Snapshot überschrieben (`driver.py:480-481`) —
„Measured positions: 194 of 198" (`report.py:348-352`) ist also ein einzelner
Snapshot, präsentiert als Aussage über den ganzen Lauf.

Fehlerszenario: Generation 1 misst gar nichts (AT-SPI-Walk scheitert),
Generation 2 nach dem Restart ist sauber → `geometry_trusted: true`,
`geometry_failures: []`, und die halbe Messreihe ohne belegte Positionen taucht
nirgends auf.

### Zu tun

- Belege müssen über Executor-Generationen **akkumulieren** statt ersetzt zu
  werden. Der Schnitt ist frei (Sammler, der die Generationen überlebt; Übergabe
  beim Neuanlegen; Abmelden vor dem Ersetzen — was am besten passt).
- Die Kennzahl im Report muss den **ganzen Lauf** beschreiben. „Measured
  positions" darf nicht länger ein einzelner Snapshot sein, der wie eine
  Laufaussage aussieht. Wenn sich Generationen unterscheiden, muss das im Report
  sichtbar sein statt gemittelt zu verschwinden.
- Anforderung, an der gemessen wird: Ein Lauf mit Restart kann hinterher
  belegen, was in **jeder** Generation gemessen wurde.

## Verifikation — verbindlich

1. **B2:** Ein Test muss **durch den produktiven Preflight-Pfad** zeigen, dass
   ein Treiber, der den Zeiger *nicht* bewegt, den Lauf laut scheitern lässt —
   und dass daraus **kein** `hover-affordance-missing` entsteht. Ein Test, der
   `hover_preflight` mit handgebauten Argumenten aufruft und die Wirkung auf den
   Lauf nicht abbildet, deckt den Befund nicht ab.
2. **B3:** Ein Test muss einen Lauf mit **mindestens zwei** Executor-Generationen
   abbilden, in der ersten Geometriefehler erzeugen und belegen, dass sie im
   Ergebnis auftauchen. Ein Test mit nur einer Generation deckt den Befund nicht ab.
3. **Mutationsprobe für beide:** Den jeweiligen Fix testweise zurücknehmen, die
   Suite muss **rot** werden. Bleibt sie grün, ist der Test keiner. Beide
   Ergebnisse (Kommando, Exit-Code, Zahl der Fehlerzeilen) gehören in den
   Abschlussbericht. Danach die Rücknahme rückgängig machen und belegen, dass der
   Diff wieder leer ist.
4. `bash scripts/tests/cua-explore.sh` läuft durch. Stand vorher: **427 Tests in
   16 Dateien**, Exit 0, ~18 s. Die **Dateizahl darf nicht sinken** — eine
   ausgeschlossene Suite sieht sonst aus wie ein grüner Lauf. Testzahl und
   Dateizahl im Bericht nennen.
5. `cargo` nicht anfassen.
