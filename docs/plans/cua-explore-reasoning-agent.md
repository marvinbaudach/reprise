---
slug: cua-explore-reasoning-agent
worktree:
branch:
phase: planned
created: 2026-08-06
---
# Reasoning-Agent und Hover-Abnahme für `scripts/cua-explore`

## 0. Vorbemerkung — bitte zuerst lesen

**Branch: von `test/cua-exploratory-agents` (HEAD `b168486e64`) abzweigen, NICHT von `dev`.**
Nur auf diesem Branch existieren `scripts/cua-explore/`, die Missionen und die
zugehörigen Tests. Ein Feature-Branch von `dev` hat weder Harness noch Tests und läuft
sofort ins Leere.

```sh
git switch test/cua-exploratory-agents
git switch -c feat/cua-explore-reasoning-agent
```

Vorgeschlagener Branchname: `feat/cua-explore-reasoning-agent`.

**Commit 1 blockiert alles andere.** Der Harness startet die App heute in einem
User-Namespace, in dem sie keine Session-Bus-Verbindung bekommt; damit ist AT-SPI tot und
jeder Snapshot enthält genau ein Element. Solange das so ist, ist keine der hier geplanten
Fähigkeiten real messbar. Details in §4.

**Codex kann die Display-Seite nicht selbst verifizieren.** Die Codex-Sandbox hat kein
Xvfb, keine verschachtelten Namespaces und keinen `cua-driver`. Was Codex beweist und was
der Maintainer danach nachfährt, steht getrennt in §11. Alles, was aus einem echten
AT-SPI-Baum kommt (Rollen-Vokabular, `value` am Suchfeld, `selected` an Sidebar-Zeilen,
Fensterkoordinaten), ist **heute ungemessen** — der bisherige Probelauf lieferte nur
degradierte Snapshots. Der Code ist deshalb defensiv zu bauen: primär Label-Text, Rollen
nur als Zusatzsignal, und **jede** Rollen-/Wertannahme an genau einer Stelle
(`scripts/cua-explore/ui_vocabulary.py`, Commit 2), damit die Nachmessung sie ohne Umbau
korrigieren kann.

---

## 1. Ziel

Drei Ergebnisse, in dieser Reihenfolge wertvoll:

1. **Der Harness funktioniert wieder** — die App im Explorationslauf hat einen
   Session-Bus, AT-SPI liefert einen vollen Baum, MPRIS lebt.
2. **Hover-Abnahme**: Buttons und Links werden systematisch daraufhin geprüft, ob sie beim
   Überfahren sichtbar reagieren (UX-Regelwerk `docs/ux-rules.md` Sektion W, BTN-1/3/4).
   Neue Aktion `hover`, neues Orakel `hover-affordance`, neue agent-freie Mission
   `hover-affordance-sweep`.
3. **Reasoning-Agent** für die drei agent-pflichtigen Missionen — ein ausführbares
   Programm hinter `--agent-command-json`, das
   `section-search-isolation`, `offline-recovery` und `large-library-stress`
   unbeaufsichtigt und wiederholbar durchspielt: alle `workloads` real ausgeführt, jeder
   Checkpoint vom Runner-Audit (`workload_audit.audit_action_workload`, bei `batch-edit`
   zusätzlich `fixtures.audit_batch_edit`) als `complete: true` bestätigt, Lauf endet mit
   `finish`, `summary.json` zeigt `mission_complete: true`.

Gleichrangiges Zweitziel des Agenten: **echte GUI-Fehler sichtbar machen**, nicht nur
Checkpoints abhaken. Er erzeugt eigene, benannte Beobachtungen zu Zusicherungen, die die
Orakel nicht abdecken (Scope-Leaks, nicht wiederhergestellte Zustände, verschwundene
Affordances).

## 2. Nicht-Ziele

- **Kein Produktcode unter `crates/`.** Wo das Produkt eine Affordance nicht hat, wird das
  berichtet, nicht umgangen. Der einzige geplante Produkt-Eingriff wäre die fehlende
  Auswahlanzahl — die wird in §5 als UX-Befund dokumentiert und ausdrücklich **nicht** hier
  gebaut.
- **Kein LLM in der Agentenschleife**, kein Netzwerk, keine Credentials (Begründung E1).
- **Keine Aufnahme in `.github/workflows`.** `scripts/tests/cua-explore.sh` bleibt der
  einzige CI-Berührungspunkt und startet die App nicht.
- Kein Ersatz des eingebauten `explorer.py`; `first-time-exploration` und
  `pointer-layout-reachability` bleiben inhaltlich unverändert.
- Keine weiteren neuen Aktionsarten außer `hover`.

---

## 3. Commit-Reihenfolge

Ein Codex-Auftrag, sieben Commits, strikt in dieser Reihenfolge. Jeder Commit ist für sich
grün (`scripts/tests/cua-explore.sh`), englische Commit-Message.

| # | Commit-Message | Inhalt | blockiert |
| --- | --- | --- | --- |
| 1 | `fix(cua-explore): give the sandboxed app a working session bus` | `--map-root-user` → `--map-current-user`, Vertragstest nachziehen, Regressionstest | alles |
| 2 | `refactor(cua-explore): one source for the UI role vocabulary` | neues `ui_vocabulary.py`, Rollen-Normalisierung, Dedupe runner/oracles | 3–7 |
| 3 | `fix(cua-explore): split Podcasts and YouTube into separate routes` | Missionen, Fixtures (YouTube-Quelle), Explorer, Tests | 7 |
| 4 | `fix(cua-explore): accept the tag dialog title as a selection marker` | `has_selection_marker`, adversarialer Test, README-Satz | 7 |
| 5 | `feat(cua-explore): add a hover action and the hover-affordance oracle` | Protokoll, Driver, PNG-Diff, Geometrie, Rauchtest, Runner/`run.sh`-Flags | 6, 7 |
| 6 | `feat(cua-explore): add the hover-affordance-sweep mission` | Workload-Art `hover-sweep`, Audit, Explorer-Erweiterung, Mission | — |
| 7 | `feat(cua-explore): add the deterministic exploration agent` | `agents/*`, FakeWorld, Agenten-Suite, README, Notiz-Evidenz | — |

---

## 4. Commit 1 — der Namespace-Fix (Blocker, empirisch belegt)

### 4.1 Befund

`runner.py:125-129` startet die App so:

```python
["unshare", "--user", "--map-root-user", "--net", "--", str(self.app_binary)]
```

`--map-root-user` bildet den aufrufenden Nutzer auf uid 0 **innerhalb** des Namespace ab.
Die App meldet dem Session-Bus per SASL `EXTERNAL` genau diese 0. Der Bus erwartet die uid
des Bus-Besitzers und lehnt ab. Folge: AT-SPI kommt nie hoch, jeder Snapshot enthält genau
ein Element (das Fenster), `degraded_reason: x11_property_fallback_partial`, MPRIS fällt
ebenfalls aus.

Gegenprobe auf diesem Host:

```sh
# scheitert: "Did not receive a reply"
dbus-run-session -- unshare --user --map-root-user --net \
  dbus-send --session --print-reply --dest=org.freedesktop.DBus \
  /org/freedesktop/DBus org.freedesktop.DBus.ListNames

# antwortet sauber
dbus-run-session -- unshare --user --map-current-user --net \
  dbus-send --session --print-reply --dest=org.freedesktop.DBus \
  /org/freedesktop/DBus org.freedesktop.DBus.ListNames
```

Beide Varianten haben eine eigene netns-Inode — **die Netzisolation bleibt erhalten**, sie
war nie an `--map-root-user` gebunden.

### 4.2 Änderung

1. `runner.py`: neue, testbare Modulfunktion statt Inline-argv:

   ```python
   APP_NAMESPACE_ARGV: tuple[str, ...] = ("unshare", "--user", "--map-current-user", "--net", "--")

   def app_launch_argv(app_binary: pathlib.Path) -> list[str]:
       """Private network namespace without breaking the session bus (EXTERNAL auth)."""
       return [*APP_NAMESPACE_ARGV, str(app_binary)]
   ```

   `AppLifecycle.start` ruft `app_launch_argv(self.app_binary)` auf.

2. `scripts/tests/cua-explore.sh`, **beide** Stellen:
   - die Schleife `for required in 'unshare' '--map-root-user' '--net'` wird zu
     `'unshare' '--map-current-user' '--net'`;
   - zusätzlich eine neue Negativprüfung: `--map-root-user` darf in `run.sh` und
     `runner.py` **nicht mehr** vorkommen (mit Begründungskommentar „breaks D-Bus EXTERNAL
     auth");
   - die Namespace-Gegenprobe nutzt `unshare --user --map-current-user --net readlink
     /proc/self/ns/net` und vergleicht weiter gegen `readlink /proc/self/ns/net`;
   - neu, weich gegatet (`command -v dbus-run-session` und `command -v dbus-send`,
     `timeout 20`): eine Prüfung, dass die Bus-Verbindung im Namespace **antwortet**.
     Fehlt eines der Programme, wird die Prüfung übersprungen und das laut gemeldet — sie
     darf den Vertragstest nicht auf fremden Hosts rot machen.

3. Regressionstests in `scripts/tests/cua-explore.py`:
   - `test_app_launch_argv_keeps_a_private_network_namespace` — `--net` vorhanden.
   - `test_app_launch_argv_does_not_map_root_because_dbus_external_auth_rejects_it` —
     `--map-current-user` vorhanden, `--map-root-user` nicht.
   - `test_app_launch_argv_puts_the_binary_after_the_argument_separator` — `--` steht
     direkt vor dem Binary.

Kein anderes Verhalten wird in diesem Commit angefasst.

---

## 5. UX-Befund: es gibt keine sichtbare Auswahlanzahl

**Beobachtung.** `strings_news::tracks_selected` (`"{count} tracks selected"`) ist
definiert, hat aber **keine Aufrufstelle**. Die Statusleiste zeigt bewusst immer die ganze
Bibliothek, die Filterzeile zählt Treffer, nicht Auswahl. Die Frage „Wie viele Zeilen habe
ich gerade ausgewählt?" ist im Produkt nur beantwortbar, indem man den Tag-Dialog öffnet
(`TAG_EDIT_TITLE_MULTI` = `"Edit {count} Tracks"`) oder den Speichern-Knopf liest
(`TAG_SAVE_COUNT` = `"Save {count}"`).

**Bewertung.** Das ist ein echter UX-Mangel, kein Harness-Problem: Mehrfachauswahl ohne
Rückmeldung über den Umfang ist eine stumme Operation auf potenziell tausenden Zeilen. Der
Befund gehört als eigenes Produkt-Ticket erfasst (Vorschlag: Zähler in der Filterzeile
oder Statusleiste, sichtbar sobald `selected_count > 1`). **Er wird in diesem Plan nicht
implementiert** — Nicht-Ziel §2.

**Konsequenz für den Harness (Commit 4).**
`workload_audit._audit_batch.has_selection_marker` verlangt heute ein Label, das die Zahl
**und** `"select"` enthält. Ein solches Label existiert nicht; das Prädikat ist
unerfüllbar und macht `large-library-stress` strukturell ungewinnbar. Es wird minimal
aufgeweicht:

```python
SELECTION_MARKER_NOUNS = ("select", "track")

def has_selection_marker(trace: ActionTrace) -> bool:
    return any(
        selection_pattern in label
        and any(noun in label.casefold() for noun in SELECTION_MARKER_NOUNS)
        for label in trace.after_labels
    )
```

Damit zählt `"Edit 512 Tracks"` als Beleg — die App hat die Zahl 512 beim Öffnen *und*
beim Speichern nachweislich gekannt. Ein nacktes `"512"` oder `"Save 512"` reicht weiter
**nicht**.

Zusätzlich, damit die Aufweichung nicht stillschweigend zur Norm wird:

- Der Agent erzeugt die Notiz `agent-missing-selection-count`, sobald er 512 Zeilen
  selektiert hat und **kein** Label mit der Zahl außerhalb eines Dialogs sichtbar ist.
- `scripts/cua-explore/README.md` bekommt im Abschnitt zur Stress-Mission genau einen
  Satz: „Reprise zeigt keine Auswahlanzahl außerhalb des Tag-Dialogs; das Batch-Audit
  akzeptiert deshalb den Dialogtitel als Beleg — die fehlende Anzeige ist ein offener
  UX-Befund, keine Harness-Eigenschaft."

---

## 6. Entscheidungen

### E1 — Deterministischer Zustandsautomat, kein LLM in der Schleife

**Position: rein deterministisch, seed-gesteuert. Kein Modell, kein Transport, keine
Advisor-Naht.**

Begründung, in der Reihenfolge der Härte:

1. **Der Agent bekommt keine Credentials und darf keine bekommen.**
   `ExternalAgent.__enter__` filtert die Umgebung auf `PATH`, `HOME`, `LANG`, `LC_ALL`,
   `PYTHONPATH` und setzt `HOME` auf ein Wegwerfverzeichnis. Ein API-Key käme nur über
   `argv` (landet in Logs und in `ps`) oder über eine Datei im Wegwerf-`HOME`. Beides
   zerstört genau die Eigenschaft, die README und Review-Tests dem Harness zuschreiben:
   „no credential contract".
2. **Reproduzierbarkeit ist Pflicht.** `report.confirm_findings` zählt einen Befund erst,
   wenn er in zwei frischen Läufen auftritt. Ein nichtdeterministischer Agent macht diese
   Funktion faktisch wirkungslos.
3. **Die Workload-Muster sind formal, nicht kreativ.** `workload_audit.py` verlangt exakte
   Reihenfolgen (Refresh unmittelbar vor dem Connectivity-Verlust; Chips einzeln und je
   mit Zeilenänderung; Suche zuletzt; Down-Scroll vor dem Edit, Up-Scroll danach). Das ist
   ein Parser-Problem. Ein Automat kodiert die Regeln einmal und verletzt sie nie.
4. **Fail-closed-Ökonomie.** Ein einziger Ausrutscher — ein Label, das gerade nicht in
   `actionable_labels` steht, ein Token-Tippfehler, eine Antwort >30 s — beendet den
   ganzen Lauf per `ContractError`/`AgentError`. Bei 3600 s Budget und einem
   100k-Zeilen-Profil ist die Wiederholung teuer.
5. **Der Fehlerfindungswert sitzt nicht im Aktionswähler.** Die Orakel analysieren *jede*
   Aktion, unabhängig davon, wer sie gewählt hat. Ein LLM erhöht die Streuung der besuchten
   Zustände, nicht die Auflösung der Detektoren; Streuung ist über Seeds billiger und
   reproduzierbar zu haben (E3).

Gegenargument, ernst zu nehmen: Ein Automat „sieht nur, was er kennt". Antwort: Die
eingebaute `DeterministicExplorer`-Mission deckt die code-blinde Breitensuche ab und läuft
weiter. Der neue Agent ist absichtlich der zielgerichtete Gegenpart; E3 gibt ihm zusätzlich
ein ungerichtetes Sondierungsbudget.

**Keine Advisor-Naht.** Anders als der Entwurf: kein `Advisor`-Protocol, kein
`NullAdvisor`. Eine vorbereitete Naht lädt genau die Erweiterung ein, die die
Credential-Freiheit strukturell aufgibt. Wer später ein Modell will, schreibt einen
zweiten Agenten hinter dasselbe JSONL-Protokoll — dafür ist das Protokoll da.

### E2 — Architektur: Phasenkette aus deklarativen Steps, spät aufgelöst

**Position: eine pro Mission generierte Liste von `Phase`-Objekten; jede Phase liefert
`Step`-Objekte; ein `Step` wird erst im Moment des Sendens gegen die aktuelle Beobachtung
zu einer Aktion aufgelöst.**

Kern-Invarianten:

- **Kein Zustand über Beobachtungen hinweg außer dem Phasenindex.** Nie ein
  `element_index`, nie ein `key`, nie ein Label aus einer älteren Beobachtung speichern.
  Das Protokoll adressiert über Labels (`ActionGateway._target` prüft gegen
  `observation["actionable_labels"]`, `CuaExecutor._target` löst den Index aus einem
  frischen Snapshot unmittelbar vor dem Dispatch auf). Der Stale-Index-Fehler ist damit
  harnessseitig ausgeschlossen; verbleibende Gefahr ist ein Label, das zwischen Beobachtung
  und Dispatch verschwindet. Der Agent minimiert das, indem er nur Labels adressiert, die
  in der *gerade übergebenen* Beobachtung `actionable: true` sind, und nach
  zustandsverändernden Aktionen nichts „auf Vorrat" plant.
- **`state_id` immer aus der aktuellen Beobachtung**; jede Aktion trägt
  `schema_version: 1` und `state_id: observation["state_id"]`.
- **Ein Request → genau eine Aktion.** Keine Warteschlange gesendeter, unbestätigter
  Aktionen; die Phase rückt erst vor, wenn die nächste Beobachtung die Nachbedingung
  erfüllt (oder die Recovery-Leiter greift).

Datenfluss pro Request:

```
request {mission, observation, recent_history, instruction}
  → AgentSession.next_action(observation, history)
      1. Erstaufruf: plans.build_phases(mission, seed); budget.plan_budget(mission)
                     — bei Unmöglichkeit sofort finish
      2. Trace fortschreiben: observation_to_trace(previous, observation, last_action,
                              history[-1]["finding_codes"])
      3. TokenLearner.observe(observation, last_action)
      4. assertions.evaluate(...) → notes
      5. phase.next_step(observation, trace, learner)
         - None → Phase fertig → complete-workload → nächste Phase
      6. Falls Phase nichts liefert und Budget erlaubt: probes.next_probe(...)
      7. steps.step_to_action(step, observation)
         - nicht auflösbar → Recovery-Leiter (E4)
      8. self_gate(action, observation, mission)
      9. ledger.spend(action["kind"])
```

**Label-Auflösung.** Die Beobachtung liefert `actionable_labels` und `elements[]` mit
`key`, `label`, `role`, `enabled`, `visible`, `focused`, `selected`, `value`, `actionable`,
`frame`. Filter:

```python
@dataclass(frozen=True)
class LabelMatcher:
    exact: tuple[str, ...] = ()
    contains: tuple[str, ...] = ()
    roles: tuple[str, ...] = ()          # nur Zusatzsignal, nie alleiniges Kriterium
    require_actionable: bool = True
    require_enabled: bool = True
    def candidates(self, observation: Mapping[str, Any]) -> tuple[str, ...]
    def resolve(self, observation: Mapping[str, Any]) -> str | None
```

`resolve` gibt ein **Label** zurück (kein Index). Stabile Ordnung bei mehreren Kandidaten:
exakte Treffer vor `contains`-Treffern, dann kleinste `frame.y`, dann Label alphabetisch —
deterministisch und unabhängig von der Snapshot-Reihenfolge.

**Rollen sind weich.** Weil das Rollen-Vokabular ungemessen ist (§0), gilt: `roles` ist
ein *Bonus*-Kriterium. Ist die Kandidatenliste mit Rollenfilter leer, wird ohne
Rollenfilter erneut gesucht und die Notiz `agent-role-vocabulary-mismatch:<matcher>`
erzeugt. Alle Rollenmengen kommen aus `ui_vocabulary.py`.

**`complete-workload` erst nach eigener Vorprüfung.** Der Agent führt einen
`ActionTrace`-Spiegel (dieselbe Dataclass, direkt aus `workload_audit` importiert) und ruft
vor dem Checkpoint `audit_action_workload(index, workload, self.traces, learned_tokens)`
auf. Der Spiegel ist bewusst nur Vorprüfung, keine zweite Wahrheit: Der Runner baut
`before` aus einem frischen Pre-Action-Snapshot, der Agent sieht als „before" die letzte
settled Beobachtung — `state_changed` kann minimal abweichen; Fixture-Werte kennt der Agent
nur, soweit gelernt (wertabhängige Prädikate werden im Spiegel übersprungen).
Regel: Spiegel `true` → Checkpoint senden. Spiegel `false` → Reparaturschritte bis
`repair_budget`, danach Checkpoint **trotzdem** senden (E4).

**Token-Lernen.** `_mission_for_agent` sendet `fixture_tokens` als sortierte Namensliste
**ohne Werte**. Der Agent kennt `"MUSIC_ONLY_NEEDLE"`, nicht `"Writable Batch 0042"`.
`TokenLearner` liest nach jeder `type`-Aktion das `value` des Zielelements aus der
Folgebeobachtung und merkt sich `token -> value`. Fällt das aus (Entry exponiert kein
`value`), arbeitet der Agent strukturell weiter (Zeilenanzahl, Chip-Präsenz) und notiert
`agent-token-value-unknown`.

**Phasen je Workload-Art** (`plans.py`), jeweils reine Funktion
`plan_<kind>(workload, index, rng) -> Phase`:

`section-search` (Mission `section-search-isolation`, Workload 0). Die Reihenfolge der
Quellen ist **nicht** permutierbar: `_audit_section_search` läuft mit vorwärtslaufendem
`cursor` über `route_tokens` in JSON-Reihenfolge. Pro Quelle S:
1. Ist S laut Beobachtung schon `selected`: erst neutrale Zwischenstation (`"Queue"`)
   aktivieren — sonst erzeugt das Aktivieren von S kein `state_changed` und das Audit
   findet keinen `source_index`.
2. `activate S`, `dispatch` per Seed aus `("ax", "px")`, `expect_effect: "required"`.
3. **Hover-Stichprobe** (nur wenn `"hover" in capabilities` und Budget): ein `hover` auf
   das erste buttonartige Label der Sektion.
4. optionale Sonde (keine andere Section-Aktivierung — das würde die Audit-Suche nach dem
   `type` abbrechen).
5. `type` mit dem Token von S in `"Search all fields"`. Fehlt das Entry in S: Notiz
   `agent-missing-affordance:search@S`, Phase geht weiter (das Audit weist S als `false`
   aus — das ist der Befund).
6. Nachbedingung: genau eine `row`-Zeile; S weiterhin `selected`. Mehr als eine Zeile oder
   eine Zeile ohne gelernten Needle → Notiz `agent-search-scope-leak` mit den sichtbaren
   Zeilenlabels. **Das ist der eigentliche Missionszweck.**
7. Suche leeren: `press escape` mit `target = "Search all fields"`; Nachbedingung
   `value == ""`, sonst Notiz `agent-search-not-cleared`.
Danach `unsupported`: `activate "My Stats"` (muss `state_changed` erzeugen) und prüfen,
dass `"Search all fields"` **nicht** in den `actionable`-Labels steht; ist es doch da:
Notiz `agent-fake-search-affordance`.

`restart` (beide Missionen).
- Variante `preserve: ["section"], clear: ["transient-search"]`: `activate <section>`
  (falls nicht aktiv), `type <search_token>`, Nachbedingung: `value` des Entries ist in der
  Beobachtung unmittelbar vor dem Restart nicht leer (das Audit liest `before_values`),
  dann `restart` mit sprechendem `reason`. Danach: Section wieder `selected`? Suchfeld
  leer? Sonst Notizen `agent-section-not-preserved` /
  `agent-search-not-cleared-after-restart`.
- Variante `connectivity: "offline", status_label: "No connection"`:
  `set-connectivity offline`, dann in die Radio-Section (dort erscheint
  `RADIO_NO_CONNECTION_RETRY` = `"No connection · Retry"`, erfüllt den Substring-Test),
  `wait expect_status=true` bis das Label steht, `restart`, danach prüfen, dass das Label
  wieder da ist.

`offline-transition` (Mission `offline-recovery`, Workload 0). Harte Reihenfolge aus
`_audit_offline`:
1. Online-Phase: jede Quelle einmal besuchen (`Podcasts`, `YouTube`, `Radio`), damit
   Cache-Zeilen sichtbar sind.
2. **Unmittelbar** vor dem Verlust: `activate` auf ein Label mit `"refresh"`
   (`PODCAST_REFRESH_NOW` = `"Refresh now"`). `traces[offline_at - 1]` muss genau dieses
   Activate sein: **keine Sonde, kein Wait dazwischen.** Der Sequencer markiert den Schritt
   `atomic_with_next=True`.
3. `set-connectivity offline`.
4. Offline-Phase: jede Quelle **genau einmal** aktivieren; die letzte Offline-Beobachtung
   je Quelle muss den gelernten Needle **genau einmal** in `after_labels` enthalten.
   Doppelte Zeilen → `agent-duplicate-cached-row`.
5. Ein `activate` auf ein Label mit `"retry"` während offline.
6. `set-connectivity online`, dann jede Quelle erneut genau einmal, wieder mit
   Einmal-Vorkommen des Needles.
7. `complete-workload 0` **vor** dem Restart-Workload — Workload 1 setzt die Connectivity
   erneut auf `offline` und würde spätere Traces einmischen.

`batch-edit` (Mission `large-library-stress`, Workload 0). Anker-Reihenfolge aus
`_audit_batch`: Down-Scroll *vor* dem Edit-Öffnen, Up-Scroll *nach* dem Speichern, und die
*letzte* qualifizierende Up-Scroll-Spur im gesamten Trace zählt. Weil Workload 3
(`scroll-sweep`) später 40 Seiten hoch scrollt, **muss `complete-workload 0` gesendet
werden, bevor der Scroll-Sweep beginnt.** Workload-Reihenfolge damit fix: 0 → 1 → 2 → 3,
jeweils mit sofortigem Checkpoint.
1. `type WRITABLE_BATCH` in `"Search all fields"` → 512 Zeilen.
2. `activate` auf die erste sichtbare Zeile (Fokus in die Liste).
3. `scroll down` 1 Seite (liefert `before_rows`-Anker), dann `scroll up` 1 Seite.
4. `hotkey ["ctrl","a"]`. Danach prüfen, ob irgendein Label die Zahl 512 trägt; wenn nicht
   → `agent-missing-selection-count` (§5).
5. `hotkey ["shift","f10"]` → Kontextmenü, dann `activate "Edit tags…"`. Alternate:
   `press f10`; danach `agent-missing-affordance:context-menu`.
6. Im Dialog `type BATCH_GENRE` in `"Genre"`, `type BATCH_YEAR` in `"Year"`. Beide
   `type`-Traces brauchen `state_changed` und müssen zwischen Edit- und Apply-Index liegen.
7. `activate` auf ein Label mit `"save"`/`"apply"` → `"Save 512"`.
8. `wait duration_ms=2000, expect_status=true`, wiederholt bis der Dialog verschwunden und
   das Schreiben durch ist (512 FLAC-Kopien werden per SHA-256 geprüft). Fehlt jede
   Fortschrittsanzeige, erzeugt das Orakel `missing-waiting-feedback` — das Audit
   akzeptiert das ausdrücklich, der Befund bleibt trotzdem im Report.
9. `scroll up` 1 Seite (letzter Up-Scroll vor dem Checkpoint) → Ankerprüfung.
10. `complete-workload 0`.

`sort-cycle` (Workload 1): 24 erfolgreiche `activate` auf Spaltenköpfe, rotierend über
`["title","artist","album","year","rating"]` (Seed permutiert nur Startposition und
Rotationsrichtung, nicht die Abdeckung). Jede Aktivierung muss `before_rows != after_rows`
erzeugen; sonst erneut klicken (Toggle asc/desc), bei erneutem Ausbleiben
`agent-sort-without-reorder`.

`combined-filter` (Workload 2), Reihenfolge aus `_audit_filter`:
1. Suche vorher leeren.
2. Je Facette einzeln: `activate "Add filter"` → Facette → Wert, so dass der Chip exakt
   `active_labels[facet]` ergibt. Das Audit verlangt: Chip **nicht** in `before_labels`,
   **in** `after_labels`, und `row_labels(before) != row_labels(after)`. Strikt eine
   Facette pro Aktion.
3. Erst wenn alle drei Chips stehen: `type SEARCH_NEEDLE`; danach genau eine Zeile, alle
   drei Chips weiter sichtbar. Verschwindet ein Chip beim Suchen →
   `agent-filter-dropped-by-search`.

`scroll-sweep` (Workload 3): `pages: 40` je Richtung, `amount` maximal 10 → mindestens 4
Aktionen abwärts, 4 aufwärts. Der Seed variiert die Stückelung (10/10/10/10 vs. 7/10/10/9
vs. 5×8); jede Aktion muss `state_changed` und geänderte Zeilen liefern und darf keinen
Scroll-Befund tragen (`scroll-direction-mismatch`, `wrong-scroll-direction`, `scroll-jump`,
`scroll-lost-selection`); sonst wird nachgelegt.

### E3 — Fehlerfindung trotz Determinismus

**Position: Determinismus je Seed, Variation über Seeds; freies Budget geht in Sonden und
agent-eigene Zusicherungen.**

1. **Seed** über `argv` (`--seed N`), weil die Umgebung auf die Allowlist beschränkt ist.
   `run.sh --seed` steuert nur den eingebauten Explorer und wird **nicht** an den Agenten
   durchgereicht; der Aufrufer gibt ihn im `--agent-command-json` mit. Der Agent schreibt
   seinen Seed in die `finish`-Begründung, damit er in `trajectory.jsonl` auftaucht.
   Der Seed steuert: `ax`/`px`-Dispatch pro Aktivierung, Scroll-Stückelung, Wartedauern
   (250–2000 ms), Auswahl unter mehreren passenden Chips/Zeilen, Einfügepunkte der Sonden,
   Startposition der Sortierrotation.
2. **Reihenfolge-Permutation, wo erlaubt.** Nicht permutierbar: Quellenreihenfolge in
   `section-search` (Cursor), Refresh→Offline (Adjazenz), Facetten-Einzelschritte vor der
   Suche (Cursor), Workload-Reihenfolge im Stress-Lauf (Anker). Permutierbar:
   Online-Vorbesuche, Sondierungen, Sortierreihenfolge, Scroll-Stückelung, Reihenfolge der
   `unsupported`-Prüfungen. `plans.py` markiert das über `Phase.order_locked: bool`; ein
   Test stellt sicher, dass gesperrte Phasen bei wechselndem Seed identische Aktionsfolgen
   liefern.
3. **Zwischen-Sondierungen** (`probes.py`), nur aus dem Überschussbudget:
   - *Idle-Sonde*: `wait` mit `expect_status=false` in gesetztem Zustand → `layout-shift`
     meldet unerbetene Bewegung.
   - *Idempotenz-Sonde*: aktive Section erneut aktivieren mit
     `expect_effect: "idempotent"`.
   - *Escape-Sonde*: nach transienter Fläche `press escape`, prüfen, ob der vorherige
     Zustand exakt zurückkehrt.
   - *Leerergebnis-Sonde*: `NO_MATCH` in die Suche, Leerzustand prüfen, danach Löschen
     stellt die Zeilenzahl wieder her.
   - *Rundlauf-Sonde*: Filterchip setzen und entfernen, Zeilenzahl muss zurückgehen.
   - *Hover-Sonde*: ein `hover` auf ein zufälliges buttonartiges Label der aktuellen
     Fläche (nur wenn `"hover" in capabilities`).
   Sonden dürfen nie ein laufendes Audit-Muster brechen; `Sequencer` fragt vorher
   `phase.probe_allowed(step_kind, target_label)`.
4. **Agent-eigene Zusicherungen** (`assertions.py`), die kein Orakel abdeckt:
   `agent-search-scope-leak`, `agent-search-not-cleared`, `agent-section-not-preserved`,
   `agent-fake-search-affordance`, `agent-duplicate-cached-row`,
   `agent-offline-status-stuck`, `agent-filter-dropped-by-search`,
   `agent-filter-not-restored`, `agent-sort-without-reorder`,
   `agent-row-count-changed-by-sort`, `agent-missing-selection-count`,
   `agent-missing-affordance:<name>`, `agent-scroll-anchor-lost`,
   `agent-role-vocabulary-mismatch:<matcher>`, `agent-token-value-unknown`.
   Jede Notiz ist ein `Note(code, summary, evidence)`; `evidence` enthält nur Labels und
   Zahlen (keine Pfade, keine URLs — `report._sanitize` greift sonst ohnehin).

**Transport der Notizen.** Der Adapter liest `stderr` nur beim abnormalen Ende und nur 400
Bytes; die Pipe wird nie geleert. Deshalb: **maximal 8 KB nach stderr**, sonst blockiert
der Agent beim Vollaufen des 64-KB-Puffers. Notizen gehen (a) vollständig als JSONL nach
`$HOME/agent-notes.jsonl` und (b) verdichtet in die `finish`-Begründung (`reason`, auf 400
Zeichen gekürzt: Codes mit Zähler). `runner.py` kopiert am Ende
`profile_root/agent-home/*.jsonl` nach `evidence_dir/agent/` — ohne das löscht `run.sh`
den Scratch-Root im `trap` und die Datei ist weg.

### E4 — Fehlerverhalten: nie schweigend überspringen, nie abstürzen

**Position: dreistufige Recovery-Leiter, danach weitermachen und melden. Der Agent beendet
einen Lauf nie mit einem Prozess-Exit und verschluckt nie ein fehlendes Element.**

Leiter für einen nicht auflösbaren `Step`:

1. **Neu beobachten.** `wait duration_ms=500, expect_status=false`, im nächsten Request
   erneut auflösen. Maximal 2× (aus der Recovery-Reserve).
2. **Alternativroute.** Jeder `Step` kann `alternates` deklarieren: Suchfeld per
   `hotkey ["ctrl","f"]` statt Klick; Kontextmenü per `press f10` statt
   `hotkey ["shift","f10"]`; Section per Tastatur statt Klick. In deklarierter Reihenfolge,
   je einmal.
3. **Benannte Notiz, kein Abbruch.** `agent-missing-affordance:<step.name>` mit den zum
   Zeitpunkt sichtbaren `actionable_labels` (auf 40 Einträge gekürzt), Schritt als
   `skipped` markiert, **die Phase läuft weiter**.

Endet eine Phase mit übersprungenen Pflichtschritten, sendet der Agent den Checkpoint
**trotzdem**. Begründung: Beide Alternativen enden im Fehlschlag des Laufs — ein `finish`
vor vollständigen Checkpoints wird mit `workloads incomplete: [...]` abgelehnt
(`ContractError`), ein `complete-workload` mit unvollständiger Evidenz mit
`workload evidence incomplete: N` (`RunError`). Der Unterschied ist die Diagnosequalität:
nur der Checkpoint-Pfad ruft `report.add_workload_audit(audit)` auf, und dieser Audit
landet über den `finally`-Block in `summary.json` mit **genau benannten** Teilprädikaten
(`route_results`, `selection_observed`, `refresh_before_loss`, …). Genau das braucht ein
Mensch, um zu entscheiden, ob Produkt oder Agent versagt hat. Deshalb: **Bei Zweifel den
Checkpoint senden und den Lauf laut scheitern lassen.**

Hart abgebrochen (sofortiges `finish` in Aktion 1–2) wird nur bei Vertragswidersprüchen,
die nichts mit dem Produkt zu tun haben: unbekannte `workload.kind`, ein Workload, der eine
nicht in `capabilities` enthaltene Aktionsart braucht, ein Budget, das die Pflichtschritte
nicht trägt (E5), `schema_version != 1`. Reason-Präfix `agent-contract-mismatch: …`.

**Nie ein Gateway-Reject provozieren.** `self_gate(action, observation, mission)` prüft vor
dem Senden dieselben Regeln wie `ActionGateway`: `kind in capabilities`;
`target.label in observation["actionable_labels"]`; keine destruktiven Wörter
(`delete/remove/forget/eject/trash/erase`), wenn `"delete" in forbidden`; keine
Externallink-Phrasen; `fixture_token in mission["fixture_tokens"]`;
`key in protocol.ALLOWED_KEYS`; Hotkey mit Modifier und 2–3 Tasten; `scroll.amount` 1–10;
`duration_ms` 100–5000; `workload_index` im Bereich. Die Konstanten werden aus `protocol`
**importiert**, nicht kopiert. Verletzt der eigene Plan das Gate, wirft der Agent intern
`AgentGateError`, fängt sie in `next_action`, notiert `agent-self-gate-blocked:<grund>` und
liefert eine sichere Ersatzaktion (`wait 250 ms`).

**Der Agent antwortet immer mit einer gültigen Aktion und beendet sich nie selbst.** Der
Entrypoint fängt jede unerwartete Exception, antwortet mit `finish` und
`reason="agent-internal-error: <typ>: <kurztext>"` und beendet danach die Schleife sauber.
Ein stiller Exit erzeugt nur „agent closed stdout without an action" — diagnostisch
wertlos.

### E5 — Budgets: `finish` ist reserviert, nicht erhofft

```python
@dataclass(frozen=True)
class BudgetPlan:
    total_actions: int
    mandatory_per_workload: tuple[int, ...]
    checkpoint_actions: int                   # == len(workloads)
    finish_reserve: int = 1
    recovery_reserve: int = 0                 # ceil(0.10 * total), min 4
    probe_allowance: int = 0                  # Rest

def mandatory_step_count(workload: Mapping[str, Any]) -> int
def plan_budget(mission: Mapping[str, Any]) -> BudgetPlan   # raises BudgetTooSmall
```

`mandatory_step_count` konkret:
- `section-search`: `4 * len(route_tokens) + len(unsupported)` (Park/Activate/Type/Clear)
- `restart`: 3 (+1 wenn `connectivity` gesetzt)
- `offline-transition`: `2 * len(source_tokens)` (online) `+ 1` (refresh) `+ 1`
  (set-offline) `+ len(source_tokens)` (offline) `+ 1` (retry) `+ 1` (set-online)
  `+ len(source_tokens)` (recovery)
- `batch-edit`: 12
- `sort-cycle`: `repetitions + len(columns)`
- `combined-filter`: `2 * len(facets) + 2`
- `scroll-sweep`: `len(directions) * ceil(pages / 10)`
- `hover-sweep`: `len(sections) * (1 + min_targets_per_section)`

Vorabprüfung:
`sum(mandatory) + len(workloads) + finish_reserve + recovery_reserve <= budgets["actions"]`.
Schlägt sie fehl, sendet der Agent **in der ersten Aktion** `finish` mit
`agent-contract-mismatch: budget cannot cover mandatory steps (need N, have M)` — der Lauf
scheitert in Sekunden statt nach einer Stunde. Gegenrechnung mit den korrigierten Missionen
(§7): `section-search-isolation` ≈ 17+1+3+2+4 = 27 von 80; `offline-recovery` ≈ 16+4+2+4 =
26 von 90; `large-library-stress` ≈ 12+29+8+8+4+4 = 65 von 130. Alle tragen komfortabel.

Laufender Governor (`BudgetLedger`):
- `spend(kind)` nach jeder gesendeten Aktion; `restarts_spent` separat gegen
  `budgets["restarts"]`.
- `may_probe()` nur, wenn `remaining_actions - remaining_mandatory -
  remaining_checkpoints - finish_reserve > safety_margin` (Default 3).
- Zeit-Governor: eigene Wanduhr ab dem ersten Request (`clock()`-Injektion für Tests). Ab
  `0.80 * budgets["seconds"]` keine Sonden mehr, ab `0.92` auch keine Reparaturversuche;
  danach nur noch der kürzeste Weg durch die Pflichtschritte.
- `remaining_actions == remaining_checkpoints + 1` → nur noch Checkpoints und `finish`.
  `remaining_actions == 1` → `finish`, egal was offen ist.

**Budget-Erschöpfung ohne `finish` darf strukturell nicht vorkommen** — Test 17.

### E6 — Tests: reine Funktionen plus eine Miniatur-Reprise, ohne App

**Position: TDD mit einer scriptbaren `FakeWorld`; zentrales Akzeptanzkriterium ist, dass
die vom Agenten erzeugte Spur das *echte* `workload_audit` besteht.**

Vorbild: `scripts/tests/cua-explore.py`, `cua-explore-review.py`,
`cua-explore-audit-adversarial.py` — `unittest`, `sys.path.insert(0, EXPLORE_ROOT)`, keine
externen Abhängigkeiten, keine Prozesse außer im Adapter-Test.

`FakeWorld` modelliert genau so viel Reprise, wie die Audits sehen: Sections mit
`selected`-Zustand, ein Suchfeld mit `value`, eine Zeilenliste mit Rolle aus
`ui_vocabulary.CANONICAL_ROW_ROLE` und `frame.y`, Filterchips, ein Tag-Dialog mit
Genre/Year/„Save N", ein Connectivity-Zustand mit Statuszeile, ein Scroll-Offset,
Podcast-/YouTube-/Radio-Cachezeilen, buttonartige Elemente mit Hover-Reaktion.

```python
class FakeWorld:
    def __init__(self, *, profile: str, tokens: Mapping[str, str],
                 quirks: frozenset[str] = frozenset())
    def observation(self) -> dict[str, Any]     # exakt das Schema von CuaExecutor._observation
    def apply(self, action: Mapping[str, Any]) -> None
    def restart(self) -> None

def drive(session, world, *, max_actions: int) -> tuple[list[dict], list[ActionTrace]]
```

`quirks` schaltet Fehlermodi zu: `"search-leaks-music"`, `"search-survives-restart"`,
`"no-selection-count"`, `"no-podcast-section"`, `"no-youtube-section"`,
`"chip-dropped-by-search"`, `"scroll-anchor-drift"`, `"offline-status-stuck"`,
`"duplicate-cached-row"`, `"sort-does-not-reorder"`, `"context-menu-missing"`,
`"rows-report-table-row-role"`, `"entry-has-no-value"`, `"button-without-hover"`.

Die letzten drei sind absichtlich dabei: sie modellieren genau die ungemessenen Annahmen
aus §0 und beweisen, dass der Agent daran nicht zerbricht, sondern eine Notiz erzeugt.

### E7 — Hover-Abnahme: Aktion, Orakel, Mission

Ausgangspunkt ist das Regelwerk, nicht der Geschmack:
- **BTN-1** — jeder klickbare Button hat vier unterscheidbare Zustände; **Hover** hebt die
  Fläche (Icon-Buttons: Hintergrund weiß ~8 %), Cursor `pointer`, Transition auf dem
  Micro-Token (150 ms). Keine Ausnahme.
- **BTN-3** — Lautstärke ist eine Stufe: Primär (Accent-Fläche, stärkster Hover), Standard
  (flach, Hintergrund-Hover), **Tertiär (Menüeinträge, Listenzeilen): nur
  Hintergrund-Hover, kein Scale** — eine Zeile darf nicht unter dem Cursor springen.
- **BTN-4** — Zustände zentral in `ui/style/buttons.rs`; und: bei
  `gtk-enable-animations = false` fallen Scale und Transition weg, **die Zustandsänderung
  bleibt**.
- Links unterstreichen als Affordanz (NAV-2-Umfeld, `docs/ux-rules.md` ~Z. 140).

**E7.1 — Warum die neue Aktion nicht `click` mit halber Kraft ist.** `move_cursor` mit
`scope: "window"` bewegt **nur den vom Treiber gezeichneten Agenten-Cursor**, ein Overlay —
das erzeugt kein `enter-notify` und damit keinen Hover. Ein echter Hover braucht
`move_cursor` mit `scope: "desktop"` (bewegt den realen OS-Zeiger, Koordinaten in
Desktop-Pixeln). Das ist der einzige Weg und deshalb eine eigene Aktionsart, kein
Klick-Derivat.

**E7.2 — Messverfahren.** Ein `hover` ist eine zusammengesetzte Messung, ausgeführt von
`CuaExecutor.execute_hover`, in genau dieser Reihenfolge:

1. Agenten-Cursor der Session einmalig abschalten
   (`set_agent_cursor_enabled {session, enabled: false}`) — er würde selbst Pixel
   verändern. Idempotent, beim ersten Hover je Executor.
2. Zeiger auf den **Parkpunkt** stellen: Fensterursprung + `HOVER_PARK_MARGIN_PX` in beiden
   Achsen (linke obere Ecke, außerhalb jeder Trefferfläche). Kurze Beruhigung.
3. Baseline-Snapshot (`step-NNNN-hover-before`, PNG + JSON).
4. Zeiger auf die Mitte des Elementrechtecks (`frame.x + w/2`, `frame.y + h/2`), umgerechnet
   in Desktop-Koordinaten.
5. `HOVER_SETTLE_MS` warten.
6. Hover-Snapshot (`step-NNNN-hover-after`).
7. Pixel-Vergleich **genau des Elementrechtecks** zwischen beiden PNGs.
8. Zeiger zurück auf den Parkpunkt (sonst bleibt ein Widget dauerhaft im Hover-Zustand und
   verfälscht alle späteren Snapshots und das `layout-shift`-Orakel).

Der reguläre `OracleEngine.analyze` läuft zusätzlich über beide Snapshots — Layout-Shift
beim bloßen Überfahren ist ein eigener, wertvoller Befund.

**E7.3 — Schwellen, benannt und begründet** (`hover_oracle.py`):

```python
HOVER_SETTLE_MS = 250            # > 150 ms Micro-Token aus BTN-1, mit Reserve
HOVER_MIN_CHANNEL_DELTA = 6      # pro Kanal, 0..255
HOVER_MIN_CHANGED_RATIO = 0.02   # Anteil geänderter Pixel im Elementrechteck
HOVER_CURSOR_EXCLUSION_PX = 48   # Kantenlänge der um den Zeiger ausgeschlossenen Box
HOVER_PARK_MARGIN_PX = 2
HOVER_MIN_RECT_PX = 6            # kleinere Rechtecke gelten als nicht messbar
```

Begründung `HOVER_MIN_CHANNEL_DELTA = 6`: BTN-1 fordert weiß ~8 % über `currentColor`. Auf
einer dunklen Fläche (~27/255) hebt das auf ~45 — Delta ≈ 18, dreifach über der Schwelle.
Auf hellen Paletten ist der Kontrast geringer, aber weiterhin > 10. Unterhalb von 6 liegt
nur, was Font-Antialiasing und Dithering ohnehin erzeugen; PNG ist verlustfrei, echtes
Rauschen gibt es nicht.

Begründung `HOVER_MIN_CHANGED_RATIO = 0.02`: Ein echter Flächen-Hover färbt 60–100 % des
Rechtecks. 2 % liegt weit darunter (fängt also auch schwache Umsetzungen), aber deutlich
über dem, was ein einzelner Fokusring-Pixelsaum oder ein durchscheinendes Nachbarelement
erzeugt.

**E7.4 — Strenge nach Rolle.**

| Klasse | Rollen (aus `ui_vocabulary`) | ohne messbare Änderung |
| --- | --- | --- |
| strikt | `BUTTON_ROLES` (`button`, `push button`, `toggle button`, `link`, `check box`, `radio button`, `menu item`) | `hover-affordance-missing`, **severity `error`** |
| weich | `SOFT_HOVER_ROLES` (`row`, `cell`, `tab`, `list item`, Chips, Cover-Kacheln) | `hover-affordance-weak`, severity `warning` |
| übersprungen | `enabled: false`, `visible: false`, Rechteck < `HOVER_MIN_RECT_PX`, Rolle in keiner Klasse | `hover-skipped`, severity `info` |

`error` bei Buttons und Links ist Absicht: BTN-1 kennt keine Ausnahme. `warning` bei Zeilen
und Kacheln ebenfalls: BTN-3 verlangt dort bewusst nur einen Hintergrund-Hover, und ob eine
Zeile in der aktuellen Farbwelt sichtbar reagiert, ist eine Ermessensfrage für einen
Menschen — kein Gate. Kein Befund setzt `blocks_gate`.

Kann nicht gemessen werden (PNG fehlt, Farbtiefe nicht unterstützt, Rechteck außerhalb des
Bildes), ist das **nie** ein Fehlbefund, sondern `hover-unmeasurable` (severity `info`) mit
dem Grund im `evidence`.

**E7.5 — Zwei Fallen, ausdrücklich behandelt.**

*(a) Der Agenten-Cursor verändert selbst Pixel.* Der Treiber zeichnet für eine Session ein
Cursor-Overlay; `driver.py` schaltet ihn heute nirgends ab. Zwei Maßnahmen, beide:
`set_agent_cursor_enabled {enabled: false}` einmal je Executor **und** eine
`HOVER_CURSOR_EXCLUSION_PX`-Box um die Zeigerposition, die aus dem Vergleich
herausgerechnet wird. Wird die Box größer als 50 % des Elementrechtecks, gilt die Messung
als `hover-unmeasurable` statt als bestanden — sonst würde ein großer Cursor jede Aussage
verschlucken.

*(b) Pixel-Dispatch hat auf diesem Host `cua-driver` schon mit einem evdev-Assert
abgerissen.* Deshalb **erst ein Rauchtest, dann eine Sweep-Mission**:

- `hover_preflight(transport, *, pid, window_id, session, origin) -> dict` in
  `driver.py`: genau ein `move_cursor` (Desktop-Scope, Fenstermitte), danach
  `get_cursor_position` und `get_window_state`. Antwortet der Treiber auf beides, ist der
  Pfad tragfähig; sonst `DriverError("hover dispatch is unsafe on this driver build")`.
- `runner.run` ruft den Preflight **vor** der Aktionsschleife auf, sobald `"hover" in
  mission.capabilities`. Er kostet kein Aktionsbudget und schreibt
  `evidence_dir/hover-preflight.json`.
- `run.sh --hover-smoke MISSION.json OUTPUT_DIR` fährt Sitzung, App und Preflight und
  beendet sich danach — der isolierte Einzelschuss, den ein Mensch vor dem ersten
  Sweep-Lauf fährt.

**E7.6 — BTN-4 als zweiter, deterministischerer Messmodus: ja, und zwar als Primärmodus.**
Mit `gtk-enable-animations = false` fallen Transition und Scale weg, die Zustandsänderung
bleibt und **schaltet hart**. Damit entfällt die einzige Zeitabhängigkeit der Messung
(150-ms-Token) und der Pixelvergleich wird ein sauberer Sprung statt einer Momentaufnahme
einer laufenden Interpolation. Deshalb:

- `run.sh`/`runner.py` bekommen `--gtk-animations on|off` (Default `on`, damit bestehende
  Missionen unverändert laufen). `off` schreibt vor dem App-Start
  `"$XDG_CONFIG_HOME/gtk-4.0/settings.ini"` mit `[Settings]\ngtk-enable-animations=0` in das
  private Profil — kein Eingriff in die Nutzerkonfiguration, weil `XDG_CONFIG_HOME` bereits
  auf den Scratch-Root zeigt.
- Der autoritative Sweep-Lauf fährt mit `--gtk-animations off`.
- Ein zweiter Vergleichslauf mit `on` ist optional und dient genau einer Frage: Ein Element,
  das **nur mit** Animationen einen Hover zeigt, verletzt BTN-4 („die Zustandsänderung
  bleibt"). `scripts/cua-explore/hover_compare.py` nimmt zwei `summary.json` und listet
  diese Elemente als `hover-animation-only` — ein kleines, reines Skript, kein Runner-Teil.

**E7.7 — Mission `hover-affordance-sweep` (agent-frei).** Sie läuft mit dem eingebauten
`DeterministicExplorer`, damit sie sofort und unabhängig vom Agenten nutzbar ist.

```json
{
  "schema_version": 1,
  "id": "hover-affordance-sweep",
  "goal": "Prove that every button and link in every section answers the pointer with a visible state change (ux-rules W, BTN-1/BTN-3/BTN-4).",
  "persona": "reviewer who points at everything before clicking anything",
  "mode": "discovery",
  "agent": "optional",
  "profile": "mixed-sources-128",
  "budgets": {"actions": 220, "seconds": 1800, "restarts": 0},
  "capabilities": ["activate", "hover", "scroll", "wait", "complete-workload", "finish"],
  "fixture_tokens": {"SEARCH_NEEDLE": "Writable Batch 0042"},
  "oracles": ["clean-runtime", "feedback", "layout-shift", "pointer-reachability", "accessibility", "hover-affordance"],
  "workloads": [
    {"kind": "hover-sweep",
     "sections": ["Music", "Queue", "Playlists", "Podcasts", "YouTube", "Radio", "My Stats"],
     "min_targets_per_section": 3,
     "roles": ["button", "toggle button", "link", "check box", "tab"]}
  ],
  "success": [
    {"kind": "hover", "description": "Every button and link changes visibly under the pointer"},
    {"kind": "scope", "description": "List rows answer with a background wash and never with a scale"}
  ],
  "forbidden": ["delete", "external-url", "real-library", "real-account", "foreground-desktop"]
}
```

Budgetrechnung: 7 Sektionen × (1 Activate + bis zu 28 Hover) + 1 Checkpoint + 1 Finish
passt in 220 Aktionen; `MAX_HOVER_TARGETS_PER_SECTION = 28` deckelt den Explorer.

Der Explorer arbeitet die Sektionen in der Reihenfolge der `sections`-Liste ab, aktiviert
jede und hovert danach jedes Element der aktuellen Beobachtung, dessen Rolle in `roles`
liegt, das `actionable`, `enabled` und `visible` ist und ein nichtleeres Label hat —
sortiert nach `(frame.y, frame.x, label)`, dedupliziert über `(section, label)`.

**Report nennt Element und Sektion.** `_audit_hover_sweep` ordnet jede Hover-Spur der
zuletzt aktivierten Sektion zu und gibt zurück:

```python
{
  "kind": "hover-sweep",
  "complete": bool,
  "sections_visited": {section: bool},
  "hovered_per_section": {section: int},
  "measured_per_section": {section: int},
  "hover_findings": [{"section": str, "label": str, "role": str, "codes": [str]}],
}
```

`complete` = jede deklarierte Sektion besucht (Activate mit `state_changed`) **und**
`hovered_per_section[s] >= min_targets_per_section` **und** `measured_per_section[s] >= 1`.
Bewusst **nicht** „keine Befunde": Befunde sind das Ergebnis der Mission, kein Gate.

**E7.8 — Hover-Stichprobe in den drei agent-pflichtigen Missionen.** Sie bekommen `"hover"`
in `capabilities`, `"hover-affordance"` in `oracles` und je +10 Aktionen Budget (§7). Der
Agent hovert pro besuchter Sektion höchstens ein buttonartiges Element und, im
Stress-Lauf, zusätzlich den Speichern-Knopf des Tag-Dialogs. Mehr nicht — der systematische
Teil ist die Sweep-Mission.

### E8 — Getrennte Routen „Podcasts" und „YouTube"

„Podcasts / YouTube" existiert im Produkt nicht. `strings_podcasts.rs:11-12` definiert
`PODCASTS = "Podcasts"` und `YOUTUBE = "YouTube"` getrennt; `sidebar_rebuild.rs:245-260`
baut zwei Zeilen mit `NavIcon::Podcasts` bzw. `NavIcon::Youtube`. Folge heute:
`ActionGateway._target` verwirft das Label, `_audit_section_search` vergleicht
`target_label == source` exakt — die Missionen sind unspielbar.

**Entscheidung: zwei getrennte Routen, nicht eine umbenannte.** Die Missionen erhalten
`"Podcasts"` **und** `"YouTube"` als eigene Quellen mit je eigenem Needle-Token. Das ist
mehr Abdeckung (die YouTube-Sektion ist heute überhaupt nicht abgedeckt) zum Preis weniger
Aktionen. Keine Alias-Tabelle: ein Alias, der ein nicht existierendes Label auf ein
existierendes umbiegt, verdeckt genau den Fehler, den der Lauf finden soll.

**Fixtures säen die YouTube-Quelle direkt in die DB — ohne Netz.** YouTube nutzt dieselben
Tabellen wie Podcasts, unterschieden über `kind` (`db_podcasts_radio.rs:6-43`,
`podcast_subscriptions.kind`). `fixtures._seed_source_rows` bekommt deshalb:

```sql
INSERT INTO podcast_subscriptions (id, kind, feed_url, title, author, added_at)
VALUES (2, 'youtube', 'https://fixture.invalid/channel', 'Fixture Channel', 'Fixture Author', 1);

INSERT INTO podcast_episodes (id, subscription_id, guid, title, audio_url, published_at, duration_secs, first_seen_at)
VALUES (2, 2, 'fixture-youtube-needle', 'Fixture YouTube Needle',
        'https://fixture.invalid/video.flac', 2, 60, 2);
```

plus `module.youtube.enabled = '1'` in der Settings-Schleife (Schlüsselformat aus
`modules.rs:236-238`, `enabled_key(&YOUTUBE_MODULE) == "module.youtube.enabled"`).
`FixturePlan` bekommt `youtube_episode_count: int = 0`, `mixed-sources-128` setzt es auf 1;
`prepare_profile` ruft `_seed_source_rows` weiterhin auf, wenn irgendeine Quellenzahl > 0
ist. Der private Namespace bleibt intakt — es wird nichts geladen, nur eingefügt.

`explorer.SURFACE_PRIORITY` ersetzt `"Podcasts / YouTube"` durch die beiden Einträge
`"Podcasts"`, `"YouTube"` (an derselben Position).

Neuer Vertragstest in `scripts/tests/cua-explore.py`:
`test_every_mission_route_label_exists_in_the_known_section_vocabulary` — prüft jeden
Schlüssel aus `route_tokens`/`source_tokens` **und** jeden `unsupported`-Eintrag **und**
`hover-sweep.sections` gegen `ui_vocabulary.KNOWN_SECTION_LABELS` (gespiegelt aus
`sidebar_rebuild.rs`) und schlägt bei Unbekanntem fehl.

### E9 — Ein Vokabular-Modul statt drei Kopien

`oracles.py` definiert `ACTIONABLE_ROLES`, `BUSY_ROLES`, `BUSY_WORDS`, `OFFLINE_WORDS`;
`runner._trace_from_observations` definiert `busy_roles`/`busy_words` **noch einmal** und
filtert Zeilen hart auf `role == "row"`; `runner._snapshot_has_busy_state` ein drittes Mal.
Drei Kopien derselben Entscheidung sind genau das Muster, das in diesem Projekt schon
zweimal hörbare Bugs erzeugt hat.

Commit 2 zieht alles nach `scripts/cua-explore/ui_vocabulary.py`:

```python
ROLE_ALIASES: Mapping[str, str]        # "table row"/"list item"/"tree item" -> "row", ...
CANONICAL_ROW_ROLE = "row"
ROW_ROLES: frozenset[str]
BUTTON_ROLES: frozenset[str]
SOFT_HOVER_ROLES: frozenset[str]
ENTRY_ROLES: frozenset[str]
VALUE_BEARING_ROLES: frozenset[str]
ACTIONABLE_ROLES: frozenset[str]       # zieht aus oracles.py um
BUSY_ROLES / BUSY_WORDS / OFFLINE_WORDS
SEARCH_ENTRY_LABEL = "Search all fields"
KNOWN_SECTION_LABELS: tuple[str, ...]  # gespiegelt aus sidebar_rebuild.rs

def canonical_role(role: str) -> str
def is_row(role: str) -> bool
def is_buttonish(role: str) -> bool
def is_entry(role: str) -> bool
def hover_strictness(role: str) -> str   # "strict" | "soft" | "skip"
```

`oracles.normalize_snapshot` wendet `canonical_role` an; `runner`, `workload_audit`,
`hover_oracle` und `agents/vocabulary.py` importieren ausschließlich von hier. **Das ist
die eine Stelle aus §0**, an der die Nachmessung Rollen korrigiert.

Ein Test hält das fest: `test_no_module_redefines_the_busy_role_table` sucht per
`rg`-freiem Quelltext-Scan nach einer zweiten Definition der Mengen in
`scripts/cua-explore/*.py`.

---

## 7. Änderungen an den bestehenden Missionen

| Mission | Änderung |
| --- | --- |
| `section-search-isolation` | `route_tokens`: `{"Music": MUSIC_ONLY_NEEDLE, "Podcasts": PODCAST_ONLY_NEEDLE, "YouTube": YOUTUBE_ONLY_NEEDLE, "Radio": RADIO_ONLY_NEEDLE}`; neuer Token `"YOUTUBE_ONLY_NEEDLE": "Fixture YouTube Needle"`; `capabilities` += `hover`; `oracles` += `hover-affordance`; `budgets.actions` 70 → 80 |
| `offline-recovery` | `source_tokens`: `{"Podcasts": …, "YouTube": …, "Radio": …}`; neuer Token wie oben; `capabilities` += `hover`; `oracles` += `hover-affordance`; `budgets.actions` 80 → 90 |
| `large-library-stress` | `capabilities` += `hover`; `oracles` += `hover-affordance`; `budgets.actions` 120 → 130 |
| `first-time-exploration`, `pointer-layout-reachability` | unverändert |
| **neu** `hover-affordance-sweep` | siehe E7.7 |

Mitzuziehen (heutige Literale `"Podcasts / YouTube"`):
`scripts/tests/cua-explore.py:470`, `scripts/tests/cua-explore-review.py:579,582,586`,
`scripts/cua-explore/explorer.py:18`.

---

## 8. Dateiplan

**Neu**

```
scripts/cua-explore/ui_vocabulary.py            # Commit 2 — die eine Rollen-/Label-Stelle
scripts/cua-explore/pngdiff.py                  # Commit 5 — abhängigkeitsfreier PNG-Leser + Rechteck-Diff
scripts/cua-explore/hover_geometry.py           # Commit 5 — Fensterursprung, Desktop-Koordinaten, Parkpunkt
scripts/cua-explore/hover_oracle.py             # Commit 5 — Schwellen, Klassifikation, Findings
scripts/cua-explore/hover_compare.py            # Commit 5 — zwei summary.json → hover-animation-only
scripts/cua-explore/missions/hover-affordance-sweep.json   # Commit 6
scripts/cua-explore/agents/__init__.py          # Commit 7
scripts/cua-explore/agents/reprise_ux_agent.py  # Commit 7 — ausführbarer JSONL-Entrypoint
scripts/cua-explore/agents/agent_core.py        # AgentSession, self_gate, Note, TokenLearner
scripts/cua-explore/agents/sequencer.py         # Phase, Sequencer, Recovery-Leiter
scripts/cua-explore/agents/steps.py             # Step, step_to_action
scripts/cua-explore/agents/plans.py             # plan_<workload-kind>, build_phases
scripts/cua-explore/agents/vocabulary.py        # LabelMatcher + Matcher-Tabellen (dünn über ui_vocabulary)
scripts/cua-explore/agents/budget.py            # BudgetPlan, BudgetLedger, mandatory_step_count
scripts/cua-explore/agents/probes.py            # seeded Sonden
scripts/cua-explore/agents/assertions.py        # agent-eigene Zusicherungen → Notes
scripts/cua-explore/agents/probe_agent.py       # Wegwerfagent für den Vokabular-Dump (§11)
scripts/tests/cua_explore_png.py                # Test-Helfer: 8-Bit-RGB-PNG encodieren
scripts/tests/cua_explore_fake_world.py         # Testdouble
scripts/tests/cua-explore-hover.py              # Commit 5/6 — Hover-Suite
scripts/tests/cua-explore-agent.py              # Commit 7 — Agenten-Suite
```

**Geändert**

```
scripts/cua-explore/runner.py          # C1 argv; C5 Hover-Dispatch, Preflight, --gtk-animations; C7 Notiz-Evidenz
scripts/cua-explore/oracles.py         # C2 Rollen aus ui_vocabulary + canonical_role
scripts/cua-explore/workload_audit.py  # C4 has_selection_marker; C6 _audit_hover_sweep
scripts/cua-explore/protocol.py        # C5 hover-Aktion; C5 ALLOWED_ORACLES; C6 hover-sweep-Workload
scripts/cua-explore/actions.py         # C5 HoverAction
scripts/cua-explore/driver.py          # C5 execute_hover, disable_agent_cursor, hover_preflight
scripts/cua-explore/explorer.py        # C3 Sektionsliste; C6 hover-sweep
scripts/cua-explore/fixtures.py        # C3 YouTube-Quelle, FixturePlan-Feld
scripts/cua-explore/run.sh             # C5 --hover-smoke, --gtk-animations; Hilfetext
scripts/cua-explore/README.md          # C4 UX-Satz; C5/C6 Hover-Abschnitt; C7 Agenten-Abschnitt
scripts/cua-explore/missions/section-search-isolation.json
scripts/cua-explore/missions/offline-recovery.json
scripts/cua-explore/missions/large-library-stress.json
scripts/tests/cua-explore.sh           # C1 Namespace-Vertrag; C5–C7 neue Suiten + Vertragsprüfungen
scripts/tests/cua-explore.py           # C1 argv-Tests; C3 Label-Vertrag
scripts/tests/cua-explore-review.py    # C3 Literale
scripts/tests/cua-explore-audit-adversarial.py  # C4 zwei neue Fälle
```

**Kein Produktcode unter `crates/`.**

---

## 9. Schrittfolge (TDD — erst der Test, dann der Code, dann `scripts/tests/cua-explore.sh` grün)

**Schritt 0 — Branch und Baseline.** Branch wie in §0 anlegen,
`scripts/tests/cua-explore.sh` einmal grün sehen (Baseline dokumentieren).

**Schritt 1 (Commit 1) — Namespace.** Tests aus §4.2 zuerst, dann `app_launch_argv`, dann
`cua-explore.sh` nachziehen.

**Schritt 2 (Commit 2) — `ui_vocabulary.py`.** Tests zuerst:
`test_role_aliases_map_table_row_to_the_canonical_row_role`,
`test_normalize_snapshot_canonicalises_row_roles`,
`test_no_module_redefines_the_busy_role_table`,
`test_known_section_labels_contain_podcasts_and_youtube_separately`.
Dann Modul anlegen und `oracles.py`/`runner.py`/`workload_audit.py` darauf umstellen. Keine
Verhaltensänderung außer der Rollen-Normalisierung.

**Schritt 3 (Commit 3) — Routen und Fixtures.** Tests zuerst:
`test_every_mission_route_label_exists_in_the_known_section_vocabulary`,
`test_mixed_sources_profile_seeds_a_youtube_subscription_and_episode` (sqlite-Assertion
gegen ein temporär vorbereitetes Profil ohne Seed-Binary — nur `_seed_source_rows` gegen
eine In-Memory-DB mit minimalem Schema),
`test_section_search_mission_covers_podcasts_and_youtube_separately`.
Dann Missionen, `fixtures.py`, `explorer.SURFACE_PRIORITY` und die drei Testdateien mit den
alten Literalen.

**Schritt 4 (Commit 4) — Auswahl-Marker.** Tests zuerst in
`cua-explore-audit-adversarial.py`: `test_batch_rejects_a_bare_count_without_a_noun`
(`("512",)` und `("Save 512",)` reichen nicht),
`test_batch_accepts_the_multi_tag_dialog_title` (`("Edit 512 Tracks",)` reicht). Der
bestehende `test_batch_rejects_unbracketed_tokens_and_scroll_anchor` bleibt grün. Dann
`has_selection_marker` ändern und den README-Satz aus §5 ergänzen.

**Schritt 5 (Commit 5) — Hover-Mechanik.** Reihenfolge innerhalb des Commits:

1. `pngdiff.py` (rein, ohne I/O-Abhängigkeit außer Datei lesen). Tests zuerst mit dem
   Helfer `scripts/tests/cua_explore_png.py`:
   - `test_reads_an_eight_bit_rgb_png_round_trip`
   - `test_reads_an_eight_bit_rgba_png_and_ignores_alpha`
   - `test_rejects_sixteen_bit_images_as_unsupported`
   - `test_rejects_interlaced_images_as_unsupported`
   - `test_rect_change_ratio_ignores_pixels_below_the_channel_delta`
   - `test_rect_change_ratio_excludes_the_cursor_box`
   - `test_rect_outside_the_image_is_reported_as_unmeasurable`
   API: `read_rgb(path) -> Image`, `rect_change_ratio(before, after, rect, *, channel_delta,
   exclude=None) -> ChangeStats(changed_pixels, total_pixels, ratio, max_delta, mean_delta)`.
   Unterstützt Bit-Tiefe 8, Farbtyp 2 und 6, ohne Interlace; alles andere
   `UnsupportedImage`.
2. `hover_oracle.py`. Tests:
   - `test_button_without_any_change_is_an_error_finding`
   - `test_button_with_an_eight_percent_white_wash_is_accepted`
   - `test_link_without_change_is_an_error_finding`
   - `test_list_row_without_change_is_only_a_warning`
   - `test_disabled_or_invisible_element_is_skipped`
   - `test_tiny_rect_is_unmeasurable_not_missing`
   - `test_cursor_box_covering_most_of_the_rect_is_unmeasurable`
3. `hover_geometry.py`. Tests mit Fake-Transport:
   - `test_window_origin_prefers_the_list_windows_record`
   - `test_window_origin_falls_back_to_wmctrl_geometry`
   - `test_window_origin_failure_is_a_driver_error_not_a_silent_zero`
   - `test_desktop_point_is_the_element_centre_plus_the_window_origin`
   - `test_park_point_sits_inside_the_window_but_outside_any_element`
4. `actions.HoverAction`, `protocol.ALLOWED_ACTIONS += "hover"`,
   `ALLOWED_ORACLES += "hover-affordance"`, `_parse_hover`. Tests:
   - `test_hover_action_is_rejected_when_the_mission_lacks_the_capability`
   - `test_hover_action_requires_an_actionable_target`
   - `test_hover_action_rejects_unknown_fields`
   - `test_hover_action_respects_the_forbidden_target_words`
5. `driver.CuaExecutor.execute_hover` + `disable_agent_cursor` + `hover_preflight`. Tests
   mit einem aufzeichnenden Fake-Transport:
   - `test_execute_hover_disables_the_agent_cursor_once_per_executor`
   - `test_execute_hover_parks_the_pointer_before_the_baseline_snapshot`
   - `test_execute_hover_returns_the_pointer_to_the_park_point`
   - `test_execute_hover_records_a_finding_when_nothing_changed`
   - `test_hover_preflight_fails_loudly_when_the_driver_stops_answering`
6. `runner.py`: Hover-Dispatch, Preflight-Aufruf bei `"hover" in capabilities`,
   `--gtk-animations`. Tests:
   - `test_runner_writes_a_gtk_settings_file_when_animations_are_disabled`
   - `test_runner_runs_the_hover_preflight_before_the_action_loop`
7. `run.sh`: `--hover-smoke`, `--gtk-animations`, Hilfetext. `cua-explore.sh` prüft, dass
   der Hilfetext beide Flags nennt.
8. `hover_compare.py` + `test_hover_compare_lists_elements_that_only_hover_with_animations`.

**Schritt 6 (Commit 6) — Sweep-Mission.** Tests zuerst:
- `test_hover_sweep_workload_rejects_an_empty_section_list`
- `test_hover_sweep_workload_rejects_unknown_fields`
- `test_hover_sweep_explorer_activates_every_section_before_hovering_it`
- `test_hover_sweep_explorer_hovers_only_actionable_enabled_visible_elements`
- `test_hover_sweep_explorer_caps_targets_per_section`
- `test_hover_sweep_audit_requires_one_measured_hover_per_section`
- `test_hover_sweep_audit_names_element_and_section_for_every_finding`
- `test_hover_sweep_mission_validates_and_is_listed_by_the_runner`
Dann `protocol._validate_workloads`, `workload_audit._audit_hover_sweep`,
`explorer._next_workload_action` (Signatur bekommt die Beobachtung), Mission-JSON,
`cua-explore.sh`-Missionsliste.

**Schritt 7 (Commit 7) — Agent.** In dieser Reihenfolge, jeweils Test zuerst:

1. `budget.py` — Tests 16, 17 (§10).
2. `steps.py` + `agent_core.self_gate` — Tests 7–11, mit handgeschriebenen Beobachtungen,
   ohne `FakeWorld`.
3. `cua_explore_fake_world.py` + Selbsttests
   (`test_fake_world_matches_the_observation_schema`,
   `test_fake_world_quirks_change_only_the_intended_predicate`). Das Schema muss exakt
   stimmen: `schema_version`, `state_id`, `state_signature`, `window`, `degraded`,
   `actionable_labels`, `elements[*].{key,label,role,enabled,visible,focused,selected,
   value,actionable,frame}`.
4. `sequencer.py` + `plans.plan_section_search` + `plans.plan_restart` +
   `agent_core.AgentSession` + `TokenLearner` — Test 1.
5. `plans.plan_offline_transition` — Test 2 inklusive Adjazenz-Assertion.
6. `plans.plan_batch_edit`, `plan_sort_cycle`, `plan_combined_filter`, `plan_scroll_sweep`
   — Test 3, Workload-Ordnung 0→1→2→3 erzwungen.
7. `probes.py`, `assertions.py`, Recovery-Leiter, Zeit-Governor — Tests 5, 6, 12–15, 18.
8. `reprise_ux_agent.py`: `argparse` (`--seed`, `--notes-dir` Default `$HOME`,
   `--probe-ratio` Default 1.0), zeilenweise JSONL-Schleife, `flush()` nach jeder Antwort,
   globaler Exception-Fang, stderr-Deckel — Tests 14, 19–21.
9. `runner.py` kopiert `profile_root/agent-home/*.jsonl` nach `evidence_dir/agent/` —
   `test_runner_retains_agent_notes_as_evidence`.
10. `probe_agent.py` (nur `wait`/`activate`, schreibt jede Beobachtung nach
    `$HOME/observations/NNN.json`, nach 15 Aktionen `finish`).
11. README-Abschnitt „Der mitgelieferte Reasoning-Agent" + `cua-explore.sh`:
    Suite-Aufruf, `agents/reprise_ux_agent.py` existiert und ist ausführbar, README nennt
    `--agent-command-json` zusammen mit dem Agentenpfad.

---

## 10. Testplan

Alle neuen Tests: `unittest`, keine externen Abhängigkeiten, kein X11, keine App, kein
Netz, keine Prozesse außer dem einen Adapter-Test. Zielgröße Gesamtlaufzeit unter 15 s.

Einhängung in `scripts/tests/cua-explore.sh`, in dieser Reihenfolge, direkt nach
`cua-explore-audit-adversarial.py`:

```sh
python3 scripts/tests/cua-explore-hover.py
python3 scripts/tests/cua-explore-agent.py
```

### Agenten-Suite (`scripts/tests/cua-explore-agent.py`)

*Ende-zu-Ende, gesunde Welt*
1. `test_section_search_mission_satisfies_the_real_audit` — Agent + `FakeWorld` fahren
   `section-search-isolation`; die gesammelten `ActionTrace`s durch
   `audit_action_workload(0|1, …)` → beide `complete: true`; letzte Aktion ist `finish`.
2. `test_offline_mission_satisfies_the_real_audit` — analog, plus Assertion, dass
   `traces[offline_at-1]` ein `activate` mit `"refresh"` ist.
3. `test_stress_mission_satisfies_all_four_audits` — alle vier Workloads, plus Assertion,
   dass `complete-workload 0` vor der ersten `scroll-sweep`-Aktion liegt.
4. `test_mission_never_exceeds_its_action_budget` — alle drei Missionen.

*Determinismus und Variation*
5. `test_same_seed_produces_the_same_action_sequence`
6. `test_different_seeds_keep_locked_order_but_vary_probes`

*Selbst-Gate*
7. `test_agent_never_targets_a_label_missing_from_actionable_labels`
8. `test_agent_never_targets_destructive_or_external_labels`
9. `test_agent_only_uses_declared_capabilities_and_fixture_tokens`
10. `test_agent_hotkeys_always_carry_a_modifier_and_allowed_keys`
11. `test_every_emitted_action_is_accepted_by_the_real_action_gateway` — jede Aktion durch
    eine echte `ActionGateway(load_mission(...))`-Instanz; `ContractError` ist Testfehler.

*Fehlerverhalten*
12. `test_missing_section_is_reported_and_never_faked` — `quirks={"no-youtube-section"}`:
    Notiz `agent-missing-affordance:*` vorhanden, Checkpoint trotzdem gesendet, Audit meldet
    `route_results["YouTube"] is False`, keine erfundene Aktion mit dem fehlenden Label.
13. `test_scope_leak_is_recorded_as_a_note` — `quirks={"search-leaks-music"}`.
14. `test_agent_answers_every_request_even_after_an_internal_error` — injizierter
    Planungsfehler → `finish` mit `agent-internal-error`, kein Exit.
15. `test_agent_recovers_via_alternate_route_before_reporting_missing`.

*Budget*
16. `test_budget_shortfall_finishes_immediately_with_a_contract_reason`
17. `test_last_action_is_always_finish_under_a_truncated_budget` (parametrisch über Budgets
    von `mandatory+2` bis `total`)
18. `test_probes_stop_after_the_soft_time_deadline` (injizierte `clock`)

*Transport*
19. `test_agent_process_answers_one_json_object_per_line` — echter Subprozess über
    `ExternalAgent`, inklusive Wegwerf-`HOME`.
20. `test_agent_writes_less_than_eight_kilobytes_to_stderr`
21. `test_agent_response_stays_below_the_bounded_transport_size` (< 64 000 Bytes)

*Vokabular und Robustheit gegen ungemessene Annahmen*
22. `test_agent_notes_a_role_mismatch_instead_of_stalling` — `quirks={"rows-report-table-row-role"}`
    (nach Commit 2 normalisiert; der Test beweist, dass auch ein unbekannter Rollenname nur
    eine Notiz erzeugt).
23. `test_agent_works_without_entry_values` — `quirks={"entry-has-no-value"}`: Notiz
    `agent-token-value-unknown`, Mission läuft trotzdem durch.
24. `test_mission_fixture_tokens_are_referenced_by_at_least_one_plan`
25. `test_hover_sample_is_skipped_when_the_mission_lacks_the_capability`

### Hover-Suite (`scripts/tests/cua-explore-hover.py`)

Die in Schritt 5/6 genannten Fälle, gruppiert nach `pngdiff`, `hover_oracle`,
`hover_geometry`, `protocol`, `driver`, `runner`, `explorer`, `workload_audit`,
`hover_compare`.

### Zusammenfassung

| Ebene | Wo | Was |
| --- | --- | --- |
| Rein funktional | `cua-explore-agent.py` | LabelMatcher, Steps, Budget, Self-Gate |
| Rein funktional | `cua-explore-hover.py` | PNG-Diff, Schwellen, Klassifikation, Geometrie |
| Ende-zu-Ende ohne App | Agenten-Suite + `cua_explore_fake_world.py` | drei Missionen gegen das echte `workload_audit` |
| Vertragsecht | Agenten-Suite | jede Aktion durch eine echte `ActionGateway` |
| Fehlermodi | `quirks` | Scope-Leak, fehlende Section, fehlender Zähler, Ankerdrift, fremde Rollen, fehlende Values |
| Transport | Agenten-Suite | echter Subprozess, stderr-Deckel, Antwortgröße |
| Audit-Härtung | `cua-explore-audit-adversarial.py` | nackte Zahl ohne Substantiv wird abgelehnt |
| Shell-Vertrag | `cua-explore.sh` | Namespace-Vertrag, Missionsliste, Hilfetext, Dateien ausführbar |
| Manuell | §11 | Rauchtest, Vokabular-Dump, Echtläufe mit zwei Seeds |

---

## 11. Was Codex beweist — und was der Maintainer danach nachfährt

### Codex beweist (ohne Display, ohne Treiber, ohne App)

- Alle Unit- und Vertragstests aus §10, `scripts/tests/cua-explore.sh` grün.
- Dass jede vom Agenten erzeugte Aktion von einer **echten** `ActionGateway`-Instanz
  akzeptiert wird.
- Dass die vom Agenten in der `FakeWorld` erzeugte Spur das **echte**
  `workload_audit.audit_action_workload` besteht.
- Dass `app_launch_argv` den Namespace privat lässt und `--map-root-user` nicht mehr
  vorkommt.
- Dass der PNG-Diff, die Schwellenlogik und die Rollenklassifikation auf synthetischen
  Bildern das Erwartete tun.
- Dass `execute_hover` die Reihenfolge Parken → Baseline → Bewegen → Messen → Parken
  einhält (aufzeichnender Fake-Transport).

### Codex kann **nicht** beweisen (Sandbox ohne Xvfb, ohne verschachtelte Namespaces, ohne `cua-driver`)

- Dass der Session-Bus im Namespace tatsächlich antwortet und AT-SPI hochkommt.
- Welche Rollen `cua-driver` für ColumnView-Zeilen, Sidebar-Zeilen, das Suchfeld und die
  Busy-Anzeigen liefert.
- Ob `value` am Suchfeld und `selected` an Sidebar-Zeilen überhaupt gesetzt sind.
- Ob `list_windows` einen Fensterursprung liefert (auf diesem Host lieferte es soeben
  `{"windows": []}`) und ob der `wmctrl`-Fallback greift.
- Ob `move_cursor` mit `scope: "desktop"` einen echten GTK-Hover auslöst.
- Ob der Treiber den Pixel-Dispatch übersteht (evdev-Assert).
- Ob 512 FLAC-Schreibvorgänge ins Zeitbudget passen.

### Der Maintainer fährt nach, in dieser Reihenfolge

**M1 — Bus und AT-SPI.** Einen Kurzlauf `first-time-exploration` fahren und in
`states/*.json` prüfen: `degraded` ist `false`, `elements` enthält mehr als das Fenster.
Das ist die Abnahme von Commit 1. Schlägt sie fehl, ist alles Weitere sinnlos.

**M2 — Vokabular-Dump.** `probe_agent.py` gegen `first-time-exploration` laufen lassen:

```sh
scripts/cua-explore/run.sh \
  scripts/cua-explore/missions/first-time-exploration.json \
  "$PWD/target/cua-explore-evidence/probe-1" \
  --agent-command-json '["/usr/bin/python3","/ABS/scripts/cua-explore/agents/probe_agent.py"]'
```

Aus `target/.../states/*.json` die tatsächlichen Rollen, Labels und Values ablesen und
`ui_vocabulary.py` **einmal** korrigieren (mehr ist nicht nötig, weil alles von dort
importiert). Ergebnis als kurzer Abschnitt in der PR festhalten.

**M3 — Hover-Rauchtest.** `scripts/cua-explore/run.sh --hover-smoke
scripts/cua-explore/missions/hover-affordance-sweep.json "$PWD/target/cua-explore-evidence/hover-smoke"`.
Erwartung: `hover-preflight.json` mit `cursor_position` und lebendem Treiber. Stürzt der
Treiber ab, wird die Sweep-Mission **nicht** gefahren; stattdessen Bug-Report an cua-driver
(Ledger-Eintrag, vgl. Upstream-Regel) und Hover-Teil als blockiert führen.

**M4 — Hover-Sweep.** Zweimal:
`--gtk-animations off` (autoritativ) und `--gtk-animations on` (Vergleich), danach
`hover_compare.py` über beide `summary.json`. Erwartetes Ergebnis: Liste von Elementen mit
`hover-affordance-missing` (Regelverstoß BTN-1) und ggf. `hover-animation-only`
(Regelverstoß BTN-4).

**M5 — Agenten-Echtläufe.** Nacheinander, jeweils frisches Evidence-Verzeichnis:
`section-search-isolation` (Seed 11, dann 29), `offline-recovery` (Seed 11, dann 29),
`large-library-stress` (`--profile release`, Seed 11, dann 29). Prüfen: `mission_complete:
true`, `finished: true`, alle `workload_audits[*].complete: true`. **Zwei verschiedene
Seeds sind Pflicht** — zwei Läufe mit demselben Seed liefern identische Spuren und sind
für `report.confirm_findings` keine unabhängige Bestätigung.

**M6 — Befunde sichten.** `evidence_dir/agent/agent-notes.jsonl` und `summary.json`
zusammenfassen; UX-Befunde (fehlende Auswahlanzahl aus §5, fehlende Hover-Zustände aus M4)
als eigene Tickets anlegen.

---

## 12. Risiken und offene Fragen

**R1 — Rollen-/Value-Mapping ist ungemessen (hoch).** Kommen ColumnView-Zeilen nicht als
`row` an, sind `before_rows`/`after_rows` überall leer und *jedes* zeilenbasierte Audit
scheitert — unabhängig vom Agenten. Mitigation: Commit 2 macht daraus eine
Ein-Zeilen-Korrektur (`ROLE_ALIASES`), M2 misst sie. Der Agent behandelt Rollen ohnehin nur
als Zusatzsignal und erzeugt bei Abweichung eine Notiz statt eines Stillstands.

**R2 — Fensterursprung unbekannt (hoch, betrifft nur Hover).** `list_windows` liefert auf
diesem Host `{"windows": []}` (bekannte Mutter-Discovery-Schwäche des Treibers). Der
`wmctrl -lG`-Fallback ist geplant, aber ungetestet gegen die private Sitzung. Scheitert
beides, ist der Hover-Sweep blockiert und `hover_geometry.resolve_window_origin` wirft
laut, statt still auf `(0,0)` zu raten. Zusatzoption für die Nachmessung:
`run.sh --hover-smoke` akzeptiert `--window-origin X,Y` als manuelle Übersteuerung.

**R3 — evdev-Assert im Treiber (mittel).** Pixel-Dispatch hat `cua-driver` auf diesem Host
schon abgerissen; das sieht aus wie ein App-Hänger, ist aber keiner. Deshalb der Rauchtest
M3 vor jeder Sweep-Mission und der Preflight im Runner. Nie eine 200-Aktionen-Mission auf
einen ungeprüften Zeigerpfad setzen.

**R4 — Hover-Schwelle (mittel).** `HOVER_MIN_CHANGED_RATIO = 0.02` bei
`HOVER_MIN_CHANNEL_DELTA = 6` ist begründet (E7.3), aber nicht an echten Reprise-Pixeln
kalibriert. Erste Auswertung in M4: Wenn Elemente, die für das Auge klar reagieren, als
`missing` gemeldet werden, ist die Schwelle zu hoch; wenn tote Buttons durchrutschen, zu
niedrig. Beide Konstanten stehen an genau einer Stelle.

**R5 — Aufgeweichter Auswahl-Marker (mittel).** §5 senkt die Evidenz vom Listenzähler auf
den Dialogtitel. Das ist bewusst und dokumentiert; die eigentliche Lösung ist ein
Produkt-Ticket. Sollte das Review die Aufweichung ablehnen, ist `large-library-stress` bis
zum Produkt-Fix nicht gewinnbar — der Agent führt den Workload dann trotzdem aus und meldet
den Checkpoint, damit `summary.json` maschinenlesbar `selection_observed: false` ausweist.

**R6 — `Ctrl+A` und `Shift+F10` (mittel).** Die Auswahl von 512 Zeilen hängt an GTKs
`list.select-all` auf dem `MultiSelection`-Modell, das Kontextmenü an der Menütaste. Beide
Tasten sind in `ALLOWED_KEYS`/`ALLOWED_MODIFIERS` vorhanden, aber `hotkey` wird ohne
Zielelement dispatcht — der Fokus muss vorher in der Liste liegen. Der Plan setzt deshalb
vor `Ctrl+A` ein `activate` auf eine Zeile. Reicht das nicht, ist die nächste Stufe ein
`press` mit `target`. Scheitert auch das, ist der Batch-Workload mit dem heutigen
Aktionsvokabular unerreichbar und bräuchte eine weitere Aktionsart — bewusst außerhalb
dieses Plans, weil das die Sicherheitsfläche des Gateways erweitert.

**R7 — 512 FLAC-Schreibvorgänge im Zeitbudget (mittel).** `fixtures.audit_batch_edit` prüft
512 geänderte Dateien per SHA-256. Der Agent wartet über wiederholte
`wait expect_status=true`, aber `duration_ms` ist auf 5000 gedeckelt und jede Wartung kostet
Aktionsbudget. Kalkuliert sind bis zu 6 Wartungen (30 s). Dauert es länger, nimmt der Agent
weitere Wartungen aus der Reserve; beobachten in M5.

**R8 — Anker-Toleranz 6 px (mittel).** `scroll_anchor_restored` verlangt, dass eine
gemeinsame Zeile nach Down-/Up-Scroll innerhalb von 6 px wieder an derselben `y`-Position
steht. Bei ~40 px Zeilenhöhe ist das streng; animiertes Scrollen kann das verfehlen. Der
Agent setzt vor der Ankermessung eine Beruhigungs-`wait` und notiert bei Verfehlung
`agent-scroll-anchor-lost` mit dem gemessenen Delta, damit ein Mensch zwischen „Animation
nicht ausgelaufen" und „Position verloren" unterscheiden kann.

**R9 — `gtk-enable-animations` per `settings.ini` (niedrig, aber prüfen).** Der Weg über
`$XDG_CONFIG_HOME/gtk-4.0/settings.ini` ist der dokumentierte; ob GTK4 ihn in der
Sandbox-Sitzung liest, ist ungeprüft. M4 verifiziert das, indem der Sweep mit `off` und
`on` unterschiedliche Ergebnisse liefert; liefert er identische, wurde die Einstellung nicht
gelesen — dann ist der Vergleichsmodus wertlos und nur der Primärmodus (`on`) bleibt.

**R10 — Missionsdateien sind jetzt Verträge (niedrig).** Nach E8 hängen die Pläne an
konkreten Labelstrings in JSON. Ändert das Produkt ein Sidebar-Label, scheitert der Lauf mit
`agent-missing-affordance` — laut, aber erst nach Minuten. Der Vertragstest aus E8 fängt es
früher, solange `KNOWN_SECTION_LABELS` gepflegt wird. Optionale Folgeaufgabe: die Liste aus
`crates/reprise-gnome/src/ui/strings*.rs` generieren statt pflegen.

**R11 — Icon-Buttons ohne Accessible-Name sind für den Hover-Sweep unsichtbar (niedrig).**
`CuaExecutor._observation` filtert Elemente ohne Label heraus. Ein namenloser Icon-Button
kann also nicht gehovert werden — er ist aber ohnehin schon ein
Accessibility-Befund, den das bestehende `accessibility`-Orakel meldet. Der README-Abschnitt
zum Sweep hält diese Grenze ausdrücklich fest.

**Offene Fragen — keine.** Die vier Fragen des Entwurfs sind entschieden: kein
LLM-Advisor (E1), zwei getrennte Routen mit eigener YouTube-Fixture (E8), Auswahl-Marker
aufgeweicht plus UX-Befund (§5), Agentennotizen werden in die Evidenz kopiert (§9,
Schritt 7.9).
