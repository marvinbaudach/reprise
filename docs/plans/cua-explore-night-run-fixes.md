---
slug: cua-explore-night-run-fixes
worktree: /home/marvin/Projects/reprise-cua-explore-night-run-fixes
branch: fix/cua-explore-night-run-fixes
phase: shipped
created: 2026-08-10
---

# Der Nachtlauf soll wieder etwas messen — Reparatur von `scripts/cua-explore`

## 0. Vorbemerkung — bitte zuerst lesen

**Basis: `origin/dev` = `cfd2b76039`.** Darin steckt bereits der Produktteil dieser Arbeit
(PR #392, „A fresh window no longer snaps its own layout shut"): `DEFAULT_WIDTH` ist von 1200
auf **1440** gestiegen, `DEFAULT_HEIGHT` von 800 auf **900**, und ein Collapse **vor dem ersten
Frame** löst keinen Toast mehr aus. Dieser Plan baut darauf auf und ändert **nichts** mehr unter
`crates/`.

```sh
git switch -c fix/cua-explore-night-run-fixes origin/dev
```

**Zwei Evidenzbestände**, beide außerhalb des Repos, beide nur lesend zu benutzen:

| Pfad | Was drinsteht |
| --- | --- |
| `~/.cache/reprise-explore-evidence/2026-08-10/` | der gescheiterte Nachtlauf: 12 Läufe, Fenster 1200 px, Seitenleiste zu |
| `~/.cache/reprise-explore-evidence/2026-08-10-postfix/` | die Nachmessung auf `cfd2b76039`: Fenster 1440 px, Seitenleiste offen, Suchprobe |

**Codex kann die Display-Seite nicht selbst verifizieren.** Die Sandbox hat kein Xvfb, keine
verschachtelten Namespaces und keinen `cua-driver`. §8 trennt deshalb, was Codex beweist, von
dem, was der Maintainer nachfährt. Anders als im ersten Entwurf sind die kritischen Messungen
aber **schon gemacht** — die Fixtures dieses Plans stammen aus echten Läufen, nicht aus
Annahmen.

**Zwei Annahmen aus der Auftragslage sind falsch.** §1.1 und §1.4 führen den Gegenbeweis. Wer
nur die Commit-Liste liest, verpasst genau die Stellen, an denen dieser Plan von der
Vordiagnose abweicht.

---

## 1. Befund — was gemessen wurde

### 1.1 Es gibt keine doppelten Knoten. `listitem.scroll-to` ist die Wurzel.

Die vermuteten Duplikate sind **zwei verschiedene Tabellenzeilen**. Aus
`2026-08-10/first-time-exploration-seed-11/states/step-0001-before.json`:

```
element_index 118  role='grid cell'  label='Fixture Album 34'  y=649  x=812
element_index 139  role='grid cell'  label='Fixture Album 34'  y=739  x=812
```

90 px Abstand = zweimal die Zeilenhöhe von 45 px. Es sind die Album-Spaltenzellen zweier
verschiedener Tracks desselben Albums (`Writable Batch 0035` und `Writable Batch 0099`). Ein
Screenreader liest hier nichts doppelt. Derselbe Baum enthält aus demselben harmlosen Grund
`'3:00'` 12×, `'Fixture Artist 00'` 4×, `'★'` 24× und `'☆'` 21×.

**Die wirkliche Ursache ist eine Klassifikationsentscheidung des Harness.** GTK4 hängt auf jede
`GtkColumnViewCell` und jede Listenzeile die AT-SPI-Aktion `listitem.scroll-to`. Seit PR #335
liest `atspi_geometry._action_names` diese Namen und hängt sie an das Treiber-Element
(`driver.py:552-574`). Zwei Stellen behandeln jede nicht-leere Aktionsliste als Bedienangebot:

- `oracles.py:99-101` — `Element.actionable = bool(self.actions) or role in ACTIONABLE_ROLES`
- `driver.py:631-638` — `carrying = [item for item in matches if item.get("actions")]`, und bei
  `len(carrying) > 1` der Abbruch

Damit ist jede Tabellenzelle „actionable", und die erste Label-Kollision in der Tabelle beendet
den Lauf: **83 `actionable_labels`, davon 52 Tabellenzellen.** Der agentfreie Explorer wählt
sein Ziel per `_rank = sha256(f"{seed}:{label}")` (`explorer.py:406-408`) aus genau dieser
Menge — er *musste* in eine Zelle laufen.

**Vollständige Auszählung der Aktionsnamen** über alle **1020** aufgezeichneten Snapshots des
Nachtlaufs — das ist das ganze Universum, keine Stichprobe:

| Aktion | Vorkommen | Rollen |
| --- | --- | --- |
| `listitem.scroll-to` | 3458 | `grid cell` (2996), `row` (462) |
| `click` | 2127 | `button` (1890), `toggle button` (237) |
| 21 × `win.*`, 3 × `window.*`, `default.activate` | je 34 | ausschließlich `window` |
| `list.select-all`, `list.unselect-all` | je 34 | ausschließlich `list` |

Genau **ein** invocable Name (`click`), 26 strukturelle. `app.*` kam **nie** vor.

**Derselbe Fehler erzeugt den einzigen „bestätigten Befund" des Nachtlaufs.** Die 2×
`suspected-no-handler` auf `'Title Artist Album Year Length Rating'` treffen die **Kopfzeile**
der Tabelle (`element_index 5`, `role='row'`, `actions=['listitem.scroll-to']`). Sie wurde
aktiviert, weil sie eine Aktion „trägt", hat erwartungsgemäß nichts getan, und die App wurde
dafür angeklagt. Fehlalarm, verschwindet mit Paket A.

### 1.2 Bei 1200 px war die Seitenleiste zu — behoben, und nachgemessen

`crates/reprise-gnome/.../responsive_side_panels.rs:13` definiert `CONSTRAINED_WIDTH = 1400`;
`session.rs` definierte `DEFAULT_WIDTH = 1200`. Jedes frische Profil startete unterhalb der
eigenen Schwelle, der Breakpoint griff beim ersten Frame, beide Seitenpaneele schlossen sich,
und der Toast „Side panels were closed to fit the window" erschien. Alle sechs
agentgesteuerten Läufe scheiterten daran an Checkpoint 0: die Sektionsnamen kamen im Baum
**gar nicht vor**.

**Nachgemessen auf `cfd2b76039`** (`2026-08-10-postfix/first-time-exploration-seed-11`):

```
Fensterrechteck 1440x900, size_matches_list_windows: true
kein Toast-Element im Baum
'Music'           role='list item'  selected=true   actions=[]
'Queue'           role='list item'  selected=false  actions=[]
'Recently played' role='list item'  selected=false  actions=[]
'Top rated'       role='list item'  selected=false  actions=[]
'Recently added'  role='list item'  selected=false  actions=[]
'My Stats'        role='list item'  selected=false  actions=[]
```

Damit sind zwei offene Fragen des Entwurfs beantwortet: **Seitenleistenzeilen melden
`selected`** (der Audit darf sich darauf stützen), und ihre Rolle ist `list item`, die
`ui_vocabulary.ROLE_ALIASES` bereits auf `row` faltet.

`Podcasts`, `YouTube` und `Radio` fehlen in diesem Baum, weil `first-time-exploration` das
Profil `mixed-128` fährt — dort sind diese Module gar nicht angelegt. `mixed-sources-128`
setzt sie (`fixtures.py:199-203`: `online-sources-enabled`, `module.podcasts.enabled`,
`module.youtube.enabled`, `module.radio.enabled`).

### 1.3 Ein ungültiger JSON-Frame vom Treiber

`2026-08-10/pointer-layout-reachability-seed-29` starb an `driver.py:81`,
`cua-driver get_window_state returned invalid JSON`. Der zugehörige `cua-driver.log` enthält
nur die üblichen Overlay-Warnungen und keinen Hinweis auf den Inhalt der kaputten Antwort —
**die Nutzlast wurde nie aufbewahrt**, deshalb ist die Ursache heute nicht bestimmbar.

`CliTransport.call` (`driver.py:61-84`) hat keinen Retry, fängt `subprocess.TimeoutExpired`
nicht ab und wirft die Nutzlast weg. Ein einzelnes verstümmeltes stdout beendet damit einen
20-Minuten-Lauf, ohne eine Information darüber zu hinterlassen, was da stand.

### 1.4 Die Geometrieauflösung ist **nicht** eingebrochen — der Aggregat-Report rechnet falsch

„164 von 1129 (~15 %)" stammt nicht aus dem Harness. Jeder einzelne `report.md` sagt:

```
Measured positions: 164 of 168 driver elements (0.9762)
```

Über alle 12 Läufe liegt `resolved_ratio` zwischen **0.9759 und 0.9774**; PR #335 hatte 0.978;
die Nachmessung auf `cfd2b76039` liefert **0.9798**. Es ist derselbe Wert.

Die 1129 kommen aus `~/.local/bin/reprise-explore-report`, Zeile 40:

```python
total = sum(v for v in resolution.values() if isinstance(v, int)) or None
```

Das addiert *alle* Integer-Felder des Records: `168 + 467 + 164 + 61 + 103 + 161 + 3 + 1 + 1 =
1129`. Für die 173er-Läufe ergibt dieselbe Summe 1177, für die 162er 1115 — exakt die Zahlen
aus dem Report. Ein Rechenfehler im Aggregator, kein Messwert.

**Und es hat nicht dieselbe Wurzel wie §1.1** — auch dann nicht, wenn es echte Duplikate gäbe.
Die Gruppierung nach (Rolle, Label, Breite, Höhe) in
`atspi_geometry.resolve_driver_geometry:316-367` scheitert nicht an gleichen Schlüsseln,
sondern nur an **ungleichen Anzahlen** auf beiden Seiten. Der Nachtlauf zeigt genau das: 103
bzw. 111 Paarungen in Walk-Reihenfolge, `subset_violations: 0`.

**Konsequenz: `atspi_geometry.py` wird nicht angefasst.** Repariert wird der Aggregator
(Paket D) — eine Zahl, die der Maintainer um 03:26 liest, muss aus getestetem Code kommen.

### 1.5 Was der Lauf findet, sobald er nicht mehr stirbt

Die Nachmessung auf `cfd2b76039` kam 13 Schritte weit — weiter als jeder Nachtlauf — und lieferte
sofort echte Produktbefunde, die vorher niemand sehen konnte:

- **11 × `no-accessible-action` (error, Konfidenz 0.9):** *„'Music' offers assistive technology
  no action to invoke."* Die Seitenleistenzeilen tragen `actions=[]`. Semantisch ist die
  Navigation für Assistenztechnik nicht bedienbar.
- **`main-loop-stall` (warning):** Antwortlücken von 1270, 1294 und 976 ms bei einer Basislinie
  von 973 ms. (Der Lauf lief allein auf der Maschine; der Nightly startete erst danach.)
- **2 × `missing-waiting-feedback`.**
- **Startup:** Fenster nach 1929 ms, benutzbarer AT-SPI-Baum erst nach **6770 ms**.

Der Lauf endete trotzdem an `workload evidence incomplete: 0` — mit 87 von 100 Aktionen
ungenutzt. Das ist der Abbruch, den E6 entfernt.

### 1.6 Bilanz

| Symptom | Läufe | Wurzel | Ort |
| --- | --- | --- | --- |
| `more than one node labelled …` | 5 | `listitem.scroll-to` gilt als Affordanz | `ui_vocabulary.py`, `oracles.py`, `driver.py` |
| `workload evidence incomplete: 0` | 6 + Nachmessung | Abbruch statt Befund | `runner.py` |
| ↳ Fenster 1200 px < Breakpoint 1400 px | 6 | **behoben in `cfd2b76039`** | — |
| `returned invalid JSON` | 1 | kein Retry, Nutzlast verworfen | `driver.py` |
| „Geometrie 164/1129" | 12 | Rechenfehler im externen Aggregator | `aggregate_report.py` (neu) |
| bestätigter `suspected-no-handler` | 2 | Fehlalarm aus §1.1 (Tabellen-Kopfzeile) | fällt mit Paket A weg |
| 4 × `agent-search-scope-leak` | 2 | Assertion ohne Vorbedingungsprüfung | `agents/assertions.py` |

---

## 2. Ziel

1. **Ein Lauf stirbt nicht mehr an einer Mehrdeutigkeit.** Der Harness wählt deterministisch,
   protokolliert die Wahl samt Alternativen und macht aus der Mehrdeutigkeit einen *Befund*.
2. **`listitem.scroll-to` und Verwandte zählen nicht mehr als Bedienangebot.**
3. **Die Fenstergröße ist eine erklärte Missionseigenschaft**, kein Zufall des App-Defaults.
4. **Der gebündelte Agent kommt an das Suchfeld** und meldet keine Befunde ohne Vorbedingung.
5. **Ein unvollständiger Checkpoint beendet den Lauf nicht mehr.** Er wird Befund; der Lauf
   spielt sein Budget zu Ende.
6. **Ein einzelner kaputter Treiber-Frame beendet den Lauf nicht mehr** — begrenzter Retry für
   lesende Aufrufe, Nutzlast aufbewahrt.
7. **Der Aggregat-Report rechnet richtig, ist getestet und ordnet Befunde nach
   Reproduzierbarkeit.**
8. **Ein Orakel, das nie auswertet, fällt auf.**

Abnahme: ein echter Lauf aller sechs Missionen × zwei Seeds, in dem *kein* Lauf mit
`exploratory run failed:` endet und jede Mission entweder `outcome: complete` meldet oder mit
einem benannten, evidenzgestützten Befund `incomplete` bleibt.

## 3. Nicht-Ziele

- **Keine Änderung an `atspi_geometry.py`.** Die Geometrie ist gesund (§1.4).
- **Keine Änderung unter `crates/`.** Der Produktteil ist mit `cfd2b76039` erledigt.
- **Kein neues Orakel-Thema, keine neue Mission.**
- **Kein Zieladressieren per `element_index` oder `stable_key` im Protokoll.**
  `protocol.ActionGateway._target` (`protocol.py:440-456`) beschränkt `target` hart auf
  `{label}`; eine deterministische Reihenfolge-Regel (E2) löst das ohne Protokollbruch.
- **Kein LLM, kein Netz, keine Credentials** — der gebündelte Agent bleibt ein
  deterministischer Zustandsautomat.
- **Kein Fix für `cua-driver`.** Nur Robustheit im Harness plus ggf. ein Upstream-Repro (F3).
- **Der Harness kommt nicht in `.github/workflows`.**

---

## 4. Entscheidungen

### E1 — Aktionsnamen bekommen zwei Klassen: *invocable* und *strukturell*

Neu in `scripts/cua-explore/ui_vocabulary.py`, an genau einer Stelle, wie bei `ROLE_ALIASES`:

```python
# Measured over all 1020 recorded snapshots of the 2026-08-10 exploratory run
# (GTK 4.22, Reprise edd458e8df): 27 distinct action names, exactly one of them
# invocable. Structural means assistive technology may call it, but it is not a
# user affordance - GTK4 puts listitem.scroll-to on *every* row and cell of a
# ColumnView, list.select-all on every list, and the win.*/window.*/default.*
# GActions on the window itself.
STRUCTURAL_ACTION_PREFIXES = ("listitem.", "list.", "win.", "window.", "default.")
MEASURED_INVOCABLE_ACTIONS = frozenset({"click"})

def is_structural_action(name: str) -> bool: ...
def invocable_actions(names: Iterable[str]) -> tuple[str, ...]: ...
def unknown_action_names(names: Iterable[str]) -> tuple[str, ...]: ...
```

`app.` steht **nicht** in der Liste — der Präfix kam in keinem einzigen Snapshot vor, und der
Herkunftskommentar behauptet eine Messung.

**Unbekannte Namen fallen als *invocable* durch** — dieselbe Philosophie wie bei den
Rollenschreibweisen: lieber sichtbar falsch als still blind. Der Preis ist seit E2 nur noch
Rauschen, kein Abbruch. Jeder unbekannte Name wird pro Lauf in `summary.json` unter
`unknown_action_names` gezählt **und erzeugt einen Befund `unknown-action-name` (warning)** —
ein Zähler in einer JSON-Datei liest um 03:26 niemand, ein Befund steht im Report.

Gemessene Wirkung auf `2026-08-10/section-search-isolation-seed-11/states/step-0001-before.json`:
`actionable_labels` **83 → 31**, Kollisionen über Tabellenzellen vollständig weg. Übrig bleiben
zwei mehrdeutige Labels: `'★'` und `'☆'` — echte Buttons mit echtem `click`.

### E2 — Eine Mehrdeutigkeit ist ein Befund, kein Abbruch

**Bei einer Messung ist Verweigern richtig** — eine Position, die man nicht beweisen kann, darf
man nicht erfinden. Deshalb bleibt `atspi_geometry` streng. **Bei einer Navigation gibt es
keine falsche Antwort, nur eine unprotokollierte.** Wenn 27 Buttons `'★'` heißen, ist „welchen
meint der Nutzer" nicht die Frage des Harness — sie *ist* der Befund. Ein Screenreader-Nutzer
steht vor derselben Mehrdeutigkeit.

Neue Regel in `driver.CuaExecutor._target`:

1. `matches` = alle Knoten mit diesem Label.
2. `carrying` = davon die mit **invocable** Aktionen. Genau einer → fertig.
3. Mehrere → sortiere nach `(frame.y, frame.x, element_index)`, nimm den ersten
   (Leserichtung), **und hinterlege eine Mehrdeutigkeitsnotiz**.
4. Keiner → wie bisher: Rolle in `ACTIONABLE_ROLES`, sonst `matches[0]`.
5. Kein Treffer → wie bisher `DriverError("fresh snapshot no longer exposes target")`.

`_execute` wandelt jede *neue* Notiz in einen Befund:

```python
Finding(
    "ambiguous-accessible-name", "warning", 0.8,
    f"{count} nodes share the accessible name '{label}'; "
    "assistive technology cannot tell them apart.",
    {"target": label, "role": role, "count": count,
     "chosen": chosen_frame, "alternatives": other_frames[:8]},
    blocks_gate=False,
)
```

Höchstens einmal pro `(role, label)` und Lauf. Kein Endlosrisiko: der Explorer merkt sich
getroffene Ziele in `_tried` (`explorer.py:42`, `109`, `115`).

**Bekannte Divergenz, dokumentieren statt reparieren:** `oracles.normalize_snapshot:255-270`
vergibt `stable_key = role|label|occurrence` nach einer Sortierung über `(label, role, x, y)` —
also *spaltenweise*, nicht in Leserichtung. E2 wählt in Leserichtung. Beides zu vereinheitlichen
säße in `normalize_snapshot` und verschöbe jede Elementidentität im ganzen Harness; das gehört
zu F1, nicht hierher. Der README hält die Divergenz fest.

### E3 — Die Fenstergröße wird von der Mission erklärt

`missions/*.json` bekommt ein optionales Feld:

```json
"window": {"width": 1600, "height": 1000}
```

Der Runner setzt es **einmal nach `lifecycle.start()` und nach jedem `lifecycle.restart()`**,
vor `resolve_window_origin`, über das vorhandene `transport.resize_window`. Es ist Aufbau,
keine Agentenaktion — die `resize`-Capability wird dafür **nicht** benötigt.

**Fünf Missionen erklären `1600×1000`. `pointer-layout-reachability` erklärt `1200×800`.**
Diese Mission trägt die Persona *„impatient pointer user on a small display"* und das Ziel
*„across wide and narrow windows"*; nachdem der App-Default auf 1440 gestiegen ist, wäre sie
sonst die einzige Abdeckung des constrained-Zweigs, die verlorenginge. Sie startet bei 1440
(Paneele offen) und wird auf 1200 verkleinert — das ist eine echte Nutzeraktion, der Undo-Toast
erscheint dabei erwartungsgemäß und ist **kein** Defekt. Damit testet diese Mission ab sofort
absichtlich, was bisher nur zufällig getestet wurde. *(Anmerkung: die Mission deklariert die
Capability `resize`, hat aber keinen `resize`-Workload — sie hat nie selbst die Größe geändert.)*

`run.sh:226` steigt von `1600x900x24` auf `1920x1200x24`, damit ein 1600er Fenster mitsamt
Openbox-Platzierung sicher auf den Schirm passt.

Der Runner **misst nach** — über `transport.wmctrl_geometry(window_id)`, die vorhandene Naht,
die `resolve_window_origin` schon benutzt; ein `list_windows` gibt es im `Transport`-Protokoll
**nicht** (`driver.py:37-42`: `call`, `resize_window`, `set_connectivity`, `wmctrl_geometry`).
Das Ergebnis landet in `summary.json` unter
`window_setup = {"requested": …, "achieved": …, "honoured": bool}`. Weicht es um mehr als 2 px
ab, gibt es den Befund `window-size-not-honoured` (warning) — kein Abbruch, aber sichtbar.

### E4 — Der Agent erkämpft fehlende Flächen, statt an ihnen zu verhungern

*(wird nach der Suchfeld-Messung eingesetzt)*

### E5 — Keine Assertion ohne beobachtete Vorbedingung

`agents/assertions.py` bekommt Vorbedingungen. `agent-search-scope-leak` wird nur noch
ausgesprochen, wenn (a) der zugehörige `open-<Sektion>`-Schritt eine Zustandsänderung bewirkte
und (b) der getippte Wert in der Folgebeobachtung an einem Element mit Rolle aus `ENTRY_ROLES`
sichtbar wurde. Sonst: `agent-precondition-unmet:<step>` mit der Angabe, welche Vorbedingung
fehlte.

Regel für den README: **Eine Assertion, deren Vorbedingung nicht beobachtet wurde, ist kein
Befund über die App, sondern einer über den Lauf.** Der Nachtlauf hätte sonst vier erfundene
Scope-Leaks in den Report geschrieben.

### E6 — Drei Klassen von Ende, aber nur zwei Exit-Codes

| Klasse | Beispiel | Verhalten | Exit |
| --- | --- | --- | --- |
| **Beobachtung** | mehrdeutiges Label, fehlende Sektion, unvollständiger Checkpoint, Treiber-Frame nach Retry heil | Befund, Lauf läuft weiter | — |
| **Unvollständig** | Budget verbraucht, `mission_complete: false` | Report vollständig, Evidenz gültig | **0** |
| **Abgebrochen** | App gestorben, Treiber dauerhaft unbrauchbar, Profil/Isolation kaputt | Report so weit wie möglich, `abort_reason` gesetzt | **1** |

Der Exit-Code beantwortet nur die Frage, die eine Shell beantworten kann: *hat das Werkzeug
funktioniert?* Ob eine Mission ihr Ziel erreicht hat, steht in `summary.json → outcome` und im
Aggregat-Report. Das ist bewusst so gewählt: `~/.local/bin/reprise-explore-night` protokolliert
jeden Nicht-Null-Code als `FEHLGESCHLAGEN` und schreibt ihn in die `skipped`-Liste — mit
0/1/2 hätte jede legitime unvollständige Mission dort gestanden. So bleibt das Sweep-Skript
unverändert, und die Liste enthält nur echte Abbrüche.

Konkret entfällt der Abbruch bei `runner.py:797-800`
(`raise RunError(f"workload evidence incomplete: …")`). Stattdessen: Audit aufbewahren, Befund
`workload-incomplete` (error, `blocks_gate=True`), **`gateway.confirm_workload` nicht** aufrufen
und **`report.add_step`** für diesen Checkpoint **nicht** aufrufen (sonst lügt
`completed_workload_indices`). Der Lauf geht zur nächsten Phase. `mission_complete` bleibt über
`audits_complete` (`report.py:181-188`) korrekt `false`. `ensure_run_complete`
(`runner.py:622-626`) wirft nicht mehr bei `mission_complete is not True`, sondern nur noch,
wenn der Lauf ohne `finish` endete.

`summary.json` bekommt `outcome: "complete" | "incomplete" | "aborted"` und `abort_reason`.

### E7 — Retry nur für lesende Treiberaufrufe

`CliTransport.call` bekommt einen begrenzten Retry — **niemals für Aktionen**. Ein zweites
`click` oder `type_text` wäre eine zweite Nutzereingabe und würde den Lauf verfälschen.

```python
RETRYABLE_TOOLS = frozenset({"get_window_state", "get_cursor_position", "get_screen_size"})
```

Zwei Wiederholungen, 250 ms und 500 ms Pause, nur bei `json.JSONDecodeError` und
`subprocess.TimeoutExpired`. Nicht-Null-Exit bleibt sofortiger Fehler.

Jeder Fehlversuch wird aufbewahrt — das ist der eigentliche Gewinn:
`evidence/driver-faults.jsonl` mit `{tool, attempt, returncode, stdout_head (2000 Zeichen),
stderr_head (2000)}`, gezählt in `summary.json` unter `transport_faults`, und einmal pro Lauf
als Befund `driver-transport-fault` (warning). Damit ist die nächste Instanz von §1.3
diagnostizierbar statt nur benannt.

Für die Testbarkeit ohne `cua-driver` wird der Prozessaufruf als eigene Methode
`CliTransport._run(command) -> subprocess.CompletedProcess` herausgezogen; der Test überschreibt
nur diese eine Methode.

### E8 — Orakel und Aggregator liefern Signal, nicht nur Zeilen

Zwei Ergänzungen, die der Entwurf nicht hatte und die dem eigentlichen Zweck dienen — Fehler
finden, nicht nur überleben:

**(a) Orakel-Aktivität wird gezählt.** `summary.json` führt heute nur `finding_codes` und
`finding_counts` — also nur, was gefunden *wurde*. Ein Orakel, das nie zur Auswertung kommt,
sieht im Report exakt aus wie ein sauberes Produkt. Neu: `oracle_activity` mit
`{name: {"evaluated": n, "fired": m}}` je deklariertem Orakel. Ein deklariertes Orakel mit
`evaluated == 0` erzeugt den Befund `oracle-never-evaluated` (warning).

**(b) Der Aggregator ordnet nach Reproduzierbarkeit.** Er sieht ohnehin alle Läufe und gruppiert
Befunde nach `(code, target)` mit der Zahl der Läufe, Missionen und Seeds, in denen sie
auftraten — absteigend sortiert. Damit wird aus M7-Lesearbeit eine Liste, in der oben steht,
was in zwei Seeds derselben Mission reproduziert (= Issue-Kandidat) und unten der Einzelfall.

### E9 — Neue Tests laufen gegen echte Läufe, nicht gegen Handarbeit

Der bestehende `scripts/tests/fixtures/hover-sweep-observe.json` ist gegenüber der Pipeline
**veraltet**: 180 Elemente, davon **0 mit `actions`-Schlüssel**, aufgezeichnet am 2026-08-07 vor
der Aktionsinjektion aus PR #335. Genau deshalb konnte die Mehrdeutigkeitsfalle grün durch die
Suite gehen und dann fünf Läufe töten. Er bleibt liegen — er deckt weiter den Rollenpfad ab —
aber der Integritätstest hält fest, dass er aus der Zeit *vor* der Aktionsinjektion stammt,
damit ihn niemand mehr für aktuell hält.

Neue Fixtures, **verbatim** aus echten Läufen, nur ganze Schlüssel entfernt, nie ein
Elementinhalt verändert:

| Ziel | Quelle |
| --- | --- |
| `night-2026-08-10-ambiguous-cells.json` | `2026-08-10/first-time-exploration-seed-11/states/step-0001-before.json` (168 Elemente, `Fixture Album 34` zweimal) |
| `night-2026-08-10-music-collapsed.json` | `2026-08-10/section-search-isolation-seed-11/states/step-0001-before.json` (179 Elemente, Seitenleiste zu) |
| `postfix-2026-08-10-sidebar-open.json` | `2026-08-10-postfix/first-time-exploration-seed-11/states/step-0001-before.json` (198 Elemente, Seitenleiste offen, `selected`-Flags) |
| *(Suchfeld-Fixture — siehe E4)* | |

Zu entfernen (ganze Schlüssel, maschinenspezifisch bzw. groß): `pid`, `window_id`,
`snapshot_id`, `screenshot_file_path`, `screenshot_mime_type`, `tree_markdown`.
Zu ergänzen: `_source` und `_note` im Stil der bestehenden Fixtures, mit dem **entscheidenden
Hinweis**:

> Dies ist der Snapshot **nach** `CuaExecutor.with_measured_geometry`, also exakt das, was
> `_target`, `normalize_snapshot`, der Explorer und der Agent sehen. cua-driver selbst liefert
> weder `actions` noch diese `frame`-Werte.

Ein Integritätstest verhindert das erneute Wegdriften: die neuen Fixtures müssen `actions` an
mindestens einem Element tragen; `…-ambiguous-cells.json` muss mindestens zwei Knoten mit
gleichem Label und nicht-leeren `actions` enthalten; `…-music-collapsed.json` darf **kein**
Element namens `Music`, `Podcasts`, `Radio` oder `YouTube` enthalten;
`…-sidebar-open.json` **muss** `Music` mit `selected` enthalten.

**Was diese Fixtures nicht können:** Der Walk (`GeometryNode`-Liste) wird heute nirgends
aufbewahrt, deshalb lässt sich `resolve_driver_geometry` daraus nicht verbatim antreiben. Da die
Geometrie gesund ist, ist das folgenlos; das Aufbewahren des Walks ist F2.

---

## 5. Arbeitspakete, Dateizuständigkeit und Ausführung

Zwei **parallele Codex-Ströme**, dateidisjunkt:

| Strom | Pakete | Alleinige Dateizuständigkeit |
| --- | --- | --- |
| **I** | A (Vokabular, Zielauflösung, Retry) + D (Aggregator) | `ui_vocabulary.py`, `oracles.py`, `driver.py`, **neu** `aggregate_report.py`, `scripts/tests/cua-explore-target-resolution.py`, `scripts/tests/cua-explore-aggregate.py` |
| **II** | B (Fenster/Missionen) + C (Agent) + E (Abbruchpolitik) | `protocol.py`, `missions/*.json`, `run.sh`, **neu** `window_setup.py`, `agents/*`, `runner.py`, `report.py`, `scripts/tests/cua-explore-window.py`, `-agent.py`, `-outcome.py` |

Paket 0 (Fixtures) liegt **vor** beiden Strömen und wird vom Maintainer gelegt, weil es reine
Kopien aus der Evidenz sind.

**Eine echte Kopplung zwischen den Strömen:** Paket A schreibt fest, dass `actionable_labels`
auf 31 benannte Labels schrumpft; Paket C testet den Agenten gegen dieselbe Beobachtung. Diese
Erwartung liegt **genau einmal** in `scripts/tests/cua_explore_expectations.py` (die Konvention
existiert: `cua-explore-agent.py:21` hängt `TEST_ROOT` in `sys.path`). Beide Ströme importieren
daraus; keiner schreibt die Menge ein zweites Mal ab.

Gemeinsam am Ende, von wem auch immer zuletzt: `scripts/cua-explore/README.md` und
`scripts/tests/cua-explore.sh`. Beides Anhänge, keine Logik — Konflikte dort sind trivial.

---

## 6. Commits

### Strom I

**I-1 — Aktionsnamen bekommen zwei Klassen.** `ui_vocabulary.py`.
Inhalt: E1 mit Herkunftskommentar.
Test (`cua-explore-target-resolution.py`): `listitem.scroll-to`, `list.select-all`, `win.about`,
`window.close`, `default.activate` sind strukturell; `click` ist invocable; `foo.bar` ist
invocable **und** taucht in `unknown_action_names` auf.
Abnahme: über `night-2026-08-10-ambiguous-cells.json` hat kein `grid cell` mehr eine invocable
Aktion, alle Sterne-Buttons haben eine.

**I-2 — `actionable` heißt wieder „bedienbar".** `oracles.py`.
Inhalt: `Element.invocable_actions` als Property; `Element.actionable` (99-101) wird
`bool(invocable_actions(self.actions)) or self.role in ACTIONABLE_ROLES`; `Snapshot` sammelt
`unknown_action_names`; `ActionEvidence.target_has_action` bezieht sich auf invocable Aktionen
(Kommentar bei `oracles.py:174-176` mitziehen).
Test: über `night-2026-08-10-music-collapsed.json` schrumpft `actionable_labels` von 83 auf die
31 Labels aus `cua_explore_expectations.py`; keine Zellen-Labels mehr dabei; Zeilen-Labels
bleiben (Rolle `row` ist weiter in `ACTIONABLE_ROLES`).

**I-3 — Mehrdeutigkeit wird zum Befund.** `driver.py` (`_target` 618-644,
`target_carries_action` 646-660, `_execute` 320-415).
Inhalt: E2. `_target` bekommt `self._ambiguity_notes: dict[tuple[str, str], dict]`; `_execute`
hängt neue Notizen als `ambiguous-accessible-name` an die Schrittbefunde. Das
`raise DriverError("more than one node labelled …")` entfällt ersatzlos.
Test: `_target(raw, 'Fixture Album 34')` liefert deterministisch `element_index 118` und
**keinen** Befund (keine invocable Aktion → Rollenpfad); `_target(raw, '☆')` liefert den
obersten/linkesten Stern und genau **einen** Befund mit `count`, `chosen`, ≤8 `alternatives`;
ein zweiter Aufruf erzeugt keinen zweiten Befund; ein aufzeichnender Fake-Transport zeigt, dass
der Explorer über `propose` → `ActionGateway` → `_target` → `click` durchläuft, ohne zu werfen.
Abnahme: ein simulierter Lauf über die Fixture erreicht ≥10 Schritte, wo er heute bei 0 stirbt.

**I-4 — Begrenzter Retry und aufbewahrte Treiber-Nutzlast.** `driver.py` (`CliTransport`, 45-127).
Inhalt: E7.
Test (Unterklasse mit überschriebenem `_run`): `get_window_state` liefert erst Müll, dann
gültiges JSON → Aufruf gelingt, ein Eintrag in `driver-faults.jsonl`, `transport_faults == 1`;
dreimal Müll → `DriverError`, Nutzlast steht in der Datei; **`click` liefert Müll → sofort
`DriverError`, `_run` genau einmal gerufen** (die wichtigste Zusicherung: eine Eingabe wird nie
wiederholt); `subprocess.TimeoutExpired` wird wie Müll behandelt; der `✅`-Sonderfall
(`driver.py:79-80`) bleibt unverändert.

**I-5 — Der Aggregat-Report rechnet richtig und ordnet nach Reproduzierbarkeit.**
**neu** `aggregate_report.py`, `scripts/tests/cua-explore-aggregate.py`.
Inhalt: Der Aggregator zieht ins Repo. `health_line` benutzt `resolution["driver_elements"]` als
Nenner, nicht die Summe aller Integer. Er liest `outcome`/`abort_reason` aus `summary.json`
statt Abbruchgründe aus `*.log` zu raten, meldet `transport_faults`, `unknown_action_names` und
`oracle_activity`, und gruppiert Befunde nach E8(b).
Test: gegen zwei eingecheckte, gekürzte `summary.json`-Kopien liefert `health_line` `164/168`
und `173/177` — **nicht** `164/1129`; plus ein Regressionstest, der genau die Summenformel
ausschließt; plus ein Test, der aus vier Läufen mit demselben `(code, target)` eine Gruppe mit
`runs=4, missions=2, seeds=2` bildet und vor den Einzelfall sortiert.

### Strom II

**II-1 — Die Mission erklärt ihr Fenster.** `protocol.py` (`MISSION_FIELDS` 99-115 plus
Validierung), **neu** `window_setup.py`, `missions/*.json` (alle sechs), `run.sh:226`.
Inhalt: E3. `window_setup.apply_window_size(transport, *, window_id, requested) -> dict` kapselt
Resize plus Nachmessung über `wmctrl_geometry` und liefert den `window_setup`-Record; sie kennt
weder Runner noch Report. Validierung: `width`/`height` ganzzahlig, `600 ≤ w ≤ 3840`,
`400 ≤ h ≤ 2160`; fehlendes Feld ist erlaubt (dann kein Resize).
Test (`cua-explore-window.py`): `load_mission` akzeptiert das Feld und weist Unfug ab (`0`,
`"1600"`, negativ, unbekannter Unterschlüssel); ein Fake-Transport zeichnet Resize und
Nachmessung auf und liefert `honoured: False` bei 4 px Abweichung; fünf Missionsdateien erklären
`1600×1000`, `pointer-layout-reachability` erklärt `1200×800`; `run.sh` enthält `1920x1200x24`.
Abnahme: `run.sh --validate-only` für alle sechs Missionen grün.

**II-2 — Der Agent erreicht Seitenleiste und Suchfeld.** *(Inhalt nach E4)*

**II-3 — Keine Assertion ohne Vorbedingung.** `agents/assertions.py`, `agents/agent_core.py`.
Inhalt: E5.
Test: die Spur aus `2026-08-10/section-search-isolation-seed-11/trajectory.jsonl` (Sektion nie
geöffnet, Wert nie übernommen) erzeugt **null** `agent-search-scope-leak` und stattdessen
`agent-precondition-unmet:search-Music`; eine Spur mit erfüllter Vorbedingung und 14 Zeilen
erzeugt weiterhin `agent-search-scope-leak`.

**II-4 — Ein unvollständiger Checkpoint beendet den Lauf nicht mehr.**
`runner.py` (796-801, 622-626, 938-950), `report.py` (`write`, 165-231).
Inhalt: E6 plus die neuen Felder `outcome`, `abort_reason`, `transport_faults`,
`unknown_action_names`, `oracle_activity`, `window_setup` in `summary.json`.
Test (`cua-explore-outcome.py`): ein `RunReport` mit einem unvollständigen Audit liefert
`outcome: "incomplete"`, `mission_complete: false`, und `completed_workload_indices` enthält den
Index **nicht**; ein vollständiger Lauf liefert `"complete"`; ein gesetzter `abort_reason`
liefert `"aborted"`; `main()` bildet einen abgebrochenen Lauf auf **1** und eine unvollständige
Mission auf **0** ab.

**II-5 — Orakel-Aktivität zählen.** `runner.py`, `oracles.py` (nur die Zählstelle), `report.py`.
Inhalt: E8(a).
Test: ein Lauf, in dem ein deklariertes Orakel nie zur Auswertung kommt, erzeugt
`oracle-never-evaluated`; ein Orakel, das auswertet und nichts findet, erzeugt ihn **nicht**.

**II-6 — Fenster im Runner verdrahten.** `runner.py` (nach `lifecycle.start()` bei ~703 und im
Restart-Zweig bei ~828), `report.py`.
Inhalt: `window_setup.apply_window_size` an beiden Stellen, **vor** `resolve_window_origin`;
Ergebnis in `report.set_window_setup`; Befund `window-size-not-honoured` bei Abweichung.
Test: Fake-Transport im Runner-Test; die Reihenfolge Start → Resize → `wmctrl_geometry` →
`CuaExecutor` wird zugesichert; bei fehlendem `window`-Feld wird kein Resize gerufen.

### Gemeinsam

**Z-1 — README und Testliste.** `scripts/cua-explore/README.md`, `scripts/tests/cua-explore.sh`.
Neue Abschnitte: *Was als Aktion zählt* (E1), *Mehrdeutige Namen* (E2, mitsamt der
`stable_key`-Divergenz), *Die Mission erklärt ihr Fenster* (E3), *Wann ein Lauf endet* (E6),
*Treiberfehler* (E7), *Stumme Orakel* (E8a), plus die Fixture-Herkunftsnotiz aus E9 und die
Warnung, dass die Fixtures **nach** `with_measured_geometry` aufgezeichnet sind. Die neuen
Testdateien in `scripts/tests/cua-explore.sh` eintragen.

---

## 7. Testplan

| Datei | Deckt ab |
| --- | --- |
| `cua-explore-fixture-integrity.py` | Die Fixtures driften nicht zurück |
| `cua-explore-target-resolution.py` | E1, E2, E7 — Vokabular, Zielwahl, Retry |
| `cua-explore-window.py` | E3 — Missionsfeld, Resize, Nachmessung |
| `cua-explore-aggregate.py` | §1.4 und E8(b) — Geometriezeile und Gruppierung |
| `cua-explore-outcome.py` | E6, E8(a) — `outcome`, Exit-Codes, Checkpoint, stumme Orakel |
| `cua_explore_expectations.py` | geteilte Erwartungen beider Ströme (kein Test) |

Erweitert: `cua-explore-agent.py` (E4, E5), `cua-explore-real-snapshot.py` (neue
`actionable_labels`-Menge), `cua-explore.py` (Befundtypen).

Alle neuen Tests fahren gegen aufgezeichnete Treiberausgaben oder Trajektorien. **Keine
handgeschriebene Elementliste.**

Sammellauf: `scripts/tests/cua-explore.sh`. **Achtung für Codex:** dieses Skript endet mit
`unshare --user --map-current-user --net` und `dbus-run-session`; in einer Sandbox ohne
verschachtelte Namespaces schlägt es dort fehl. Codex führt die Python-Dateien einzeln aus und
meldet ausdrücklich, ab welcher Zeile das Shell-Skript nicht mehr lief.

---

## 8. Was Codex beweist — und was der Maintainer nachfährt

### Codex beweist (ohne Display, ohne Treiber, ohne App)

- Alle Testdateien aus §7 grün, einzeln aufgerufen.
- Dass `listitem.scroll-to` und Verwandte im aufgezeichneten Snapshot nicht mehr als Affordanz
  zählen und `actionable_labels` auf die 31 benannten Labels schrumpft.
- Dass `_target` bei `'Fixture Album 34'` und `'☆'` **nicht** mehr wirft, deterministisch
  dasselbe Element wählt und höchstens einen Befund pro Label erzeugt.
- Dass ein simulierter Lauf über die Fixture ≥10 Schritte weit kommt, wo er heute bei 0 stirbt.
- Dass eine Eingabeaktion nach einem kaputten Treiber-Frame **niemals** wiederholt wird und die
  Nutzlast in `driver-faults.jsonl` landet.
- Dass ein unvollständiger Checkpoint keinen `RunError` mehr auslöst, `mission_complete` trotzdem
  `false` bleibt und `completed_workload_indices` den Index nicht enthält.
- Dass `aggregate_report.health_line` gegen echte `summary.json`-Kopien `164/168` liefert.
- Dass alle sechs Missionen `--validate-only` bestehen und ihre Fenstergröße erklären.
- Dass jede vom Agenten erzeugte Aktion von einer echten `ActionGateway`-Instanz akzeptiert wird.

### Codex kann **nicht** beweisen

- Dass ein 1600 px breites Fenster unter Xvfb wirklich zustande kommt.
- Ob `Shift+F10` im Kontextmenü „Edit tags" liefert (`plan_batch_edit`).
- Ob `listitem.scroll-to` die einzige strukturelle Aktion bleibt — `listitem.select` kann bei
  anderen Ansichten auftauchen.
- Ob ein `ax`-Klick auf eine ColumnView-**Zeile** überhaupt etwas auslöst.
- Was `cua-driver` beim Invalid-JSON-Vorfall ausgegeben hat.

### Der Maintainer fährt nach

**M1 — Fenster.** Kurzlauf `first-time-exploration`, Seed 11. Prüfen:
`summary.json → window_setup.honoured == true`, `achieved.width == 1600`. Schlägt das fehl, ist
die Xvfb-/wmctrl-Seite dran, nicht der Agent.

**M2 — Vokabular-Nachmessung.** Aus demselben Lauf: steht `unknown_action_names` leer? Jeder
Eintrag gehört einmalig nach `STRUCTURAL_ACTION_PREFIXES` oder `MEASURED_INVOCABLE_ACTIONS`.

**M3 — Zeilenaktivierung.** `--click-probe` auf ein Zeilenlabel. Ändert nur die Pixel-Route
etwas, gehört die Zeilenaktivierung in den Missionen auf `px` umgestellt (Folgeaufgabe).

**M4 — Voller Nachtlauf.** Alle sechs Missionen × Seeds 11 und 29, `--profile release`,
sequenziell, frisches Evidenzverzeichnis, **nicht** parallel zum Nightly (der Timer feuert
04:33). Erwartung:
- **kein** Lauf endet mit `exploratory run failed:`;
- `outcome` ist überall `complete` oder `incomplete`, nie `aborted`;
- `geometry_resolution.resolved_ratio ≥ 0.97` in jedem Lauf;
- `transport_faults` und `driver-faults.jsonl` durchsehen;
- **kein** `suspected-no-handler` mehr auf `'Title Artist Album Year Length Rating'`;
- `ambiguous-accessible-name` für `'★'`/`'☆'` — der neue, echte Produktbefund.

Dabei die **Laufzeit messen**: erst danach wird über mehr Seeds entschieden. Wie lange ein Lauf
braucht, der sein Budget wirklich ausschöpft, weiß heute niemand.

**M5 — Aggregator umstellen.** `~/.local/bin/reprise-explore-report` wird ein dünner Aufruf von
`scripts/cua-explore/aggregate_report.py`. Gegenprobe: die Geometriezeile zeigt `164/168`-artige
Werte.

**M6 — Befunde sichten.** Die nach Reproduzierbarkeit sortierte Liste des Aggregators von oben
abarbeiten; alles, was in zwei Seeds derselben Mission reproduziert, wird Issue.

---

## 9. Folgeaufgaben, ausdrücklich nicht in diesem Plan

**F1 — Optionaler `key` im Zielobjekt.** `protocol.ActionGateway._target` erlaubt heute nur
`{label}`. Ein optionaler `key` (`role|label|occurrence`, bereits als `element["key"]` in jeder
Beobachtung vorhanden) gäbe den Sternen eine eindeutige Adresse. Erst sinnvoll, wenn ein Lauf
zeigt, dass die Leserichtungsregel das falsche Element trifft — und dann zusammen mit der in E2
beschriebenen Vereinheitlichung der Occurrence-Reihenfolge.

**F2 — Den AT-SPI-Walk aufbewahren.** Heute wird die `GeometryNode`-Liste nie geschrieben,
deshalb lässt sich `resolve_driver_geometry` nicht verbatim aus echter Evidenz antreiben — genau
die Lücke, die eine echte Geometrie-Regression unbemerkt lassen würde. Vorschlag: einmal pro
Start `states/walk-<generation>.json` (~467 Knoten, ~60 KB).

**F3 — Upstream-Bugreport `cua-driver`.** Erst nachdem `driver-faults.jsonl` eine Nutzlast
enthält. Ist ein Muster erkennbar, gehört ein minimales Repro nach `scripts/upstream-repros/`.
Ohne Nutzlast kein Report.

**F4 — Issue: Seitenleistenzeilen ohne Aktion.** Gemessen am 2026-08-10 auf `cfd2b76039`:
`Music`, `Queue`, `Recently played`, `Top rated`, `Recently added`, `My Stats` tragen alle
`actions=[]`. Der Harness meldet 11 × `no-accessible-action` (error). Die Navigation ist für
Assistenztechnik semantisch nicht bedienbar. Kandidat für
`scripts/check-accessibility-semantics.sh`.

**F5 — Issue: Rating-Sterne ohne unterscheidbaren Namen.** 27 Buttons namens `'★'` und 23 namens
`'☆'` in einem Tabellenausschnitt. Ein Screenreader liest 50 × „Stern, Schaltfläche".

**F6 — Issue: Spaltenkopf ohne Aktion.** Die Kopfzeile kommt als ein einziges `row` mit dem
zusammengesetzten Namen `'Title Artist Album Year Length Rating'` und ohne invocable Aktion.

**F7 — Startzeit.** Fenster nach 1929 ms, benutzbarer AT-SPI-Baum erst nach 6770 ms. Gemessen,
nicht bewertet — gehört gegen die bestehende Startzeit-Messstrecke gehalten.

---

## 10. Risiken

**R1 — Sidebar-Zeilen ohne Aktion blockieren die Sektionsnavigation (hoch).** Die Zeilen tragen
`actions=[]`; ein `ax`-Klick hat im Nachmessungslauf nichts ausgelöst (11 × `no-accessible-action`).
Dann erreicht auch ein 1600er Fenster die Sektionen semantisch nicht. E4 beantwortet das; die
Rückfallebene ist die Pixel-Route, die im Click-Probe nachweislich wirkt.

**R2 — 1600 px bringen mehr Elemente und damit mehr Zeit pro Snapshot (mittel).** Der Nachtlauf
brauchte für 31 Schritte 155 s; mit Seitenleiste und `stress-100k` kann das Sekundenbudget knapp
werden. `large-library-stress` hat 3600 s, die anderen 900–1800 s. Beobachten, nicht vorsorglich
erhöhen.

**R3 — Die Leserichtungsregel trifft den falschen Stern (niedrig).** Sie trifft immer den ersten.
Die Wahl steht mitsamt Rechteck in der Evidenz; F1 ist die Eskalation.

**R4 — `strict_roles` bei `type` blockiert einen bisher funktionierenden Pfad (niedrig).** Falls
irgendwo absichtlich in ein Nicht-Entry getippt wurde, fällt das jetzt auf statt still zu wirken.
Das ist die gewünschte Richtung.

**R5 — Die beiden Ströme driften bei der Zahl 31 auseinander (mittel).** Mitigation: die
Erwartung liegt genau einmal in `cua_explore_expectations.py`; der Integrationsdurchgang fährt
die **gesamte** Suite am Stück, bevor `/check` läuft.
