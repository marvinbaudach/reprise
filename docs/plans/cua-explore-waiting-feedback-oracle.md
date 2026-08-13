# `missing-waiting-feedback` — 93 % Orakel, 7 % Verdacht

Status: `draft` · Stand 2026-08-11, 19:55 · Vorlauf zu M6-Nacharbeit

Die 194 Vorkommen aus der Übergabe sind **von Hand nachgesehen**, wie dort
gefordert. Ergebnis: der Befund ist zum weit überwiegenden Teil ein Artefakt des
Orakels. Er darf so nicht als Produkt-Issue gemeldet werden, und er verzerrt
jede Triage, in der Häufigkeit als Gewicht gilt.

## Gemessen, nicht behauptet

Auszählung über `~/.cache/reprise-explore-evidence/2026-08-11-m4b/*/trajectory.jsonl`
(alle 194 Vorkommen stammen aus diesem Lauf), unabhängig zweimal gefahren —
einmal vom Analyse-Agenten, einmal selbst mit eigenem Skript, gleiches Ergebnis:

| Aktionsart | Anzahl | davon mit erklärendem Begleitbefund |
| --- | --- | --- |
| `activate` | 146 | **146 / 146** (`no-accessible-action` 139, `click-no-visible-effect` 7) |
| `scroll` | 34 | 10 (nur `main-loop-stall`) |
| `wait` | 14 | 8 (nur `main-loop-stall`) |

Verteilung: `large-library-stress` 48 + 48, `hover-affordance-sweep` 17 + 17,
`first-time-exploration` 27, `offline-recovery` 22, Rest 15.

## Die Auslösebedingung

`scripts/cua-explore/oracles.py:626-634`:

```
waited_without_feedback = (action.expect_effect == "required"
                           and first_change_ms is None
                           and app_observation_ms >= SILENT_WAIT_MS)   # 750 ms
if (action.expect_status or waited_without_feedback) and not waiting_visible:
    → missing-waiting-feedback
```

`expect_effect: str = "required"` ist der **Default jeder** Aktion
(`oracles.py:189`). Anders als `_click_findings` gatet dieses Orakel nicht auf
eine zugestellte Aktion.

## Die drei Wurzeln

1. **Doppelmeldung auf ein bereits erklärtes Nichts (146 Fälle, 75 %).**
   Jede einzelne `activate`-Meldung steht neben `no-accessible-action` oder
   `click-no-visible-effect`. Das Ziel bot der Assistenztechnik gar keine Aktion
   an — es gab also nie eine Operation, über deren Dauer zu informieren gewesen
   wäre. Das Timing-Orakel wertet die Abwesenheit einer Wirkung als fehlende
   Rückmeldung.
2. **Scroll ist strukturell blind (34 Fälle, 17,5 %).** `state_signature`
   (`oracles.py:159-169`) trägt `stable_key/value/enabled/visible/focused/selected`
   — **keine Geometrie**. Ein reiner Scroll kann `first_change_ms` damit nie
   setzen, egal wie schnell die App reagiert. `ActionEvidence.scroll()`
   (`oracles.py:219`) erbt trotzdem `expect_effect="required"`.
3. **Transportzeit wird als App-Zeit gebucht.** In
   `large-library-stress-seed-11`, Step 5: `dispatch_ms=5857` gegen
   `app_observation_ms=5870`. Praktisch die gesamte „Wartezeit" ist die
   Rundlaufzeit des Kommandos, nicht Denkzeit der App. `harness_ms` rechnet nur
   `settle_delay_ms + snapshot_ms` heraus.

## Der Rest, der übrig bleibt

14 `wait`-Aktionen mit **ausdrücklich** von der Mission gesetztem
`expect_status: true` (kein Default, `driver.py:173-178`) — in
`large-library-stress` seed 11 **und** seed 29 an denselben Stellen (Steps 19-24,
`state-91/96/101/106/111/116`, 2000 ms plus 4 × 5000 ms), jeweils nach einer
Suchinteraktion. Über 2 bis 5 Sekunden erscheint kein Busy-Widget.

Das ist ein **Verdacht mit zwei unabhängigen Seeds**, kein Beweis: Spinner und
Fortschrittsbalken existieren laut Codegrep in Podcasts, Concerts, Scan,
Device-Sync und Rhythmbox-Import, nicht aber in der allgemeinen
Library-/Such-/Sortieransicht. Bevor daraus ein Issue wird, gehört **ein** Fall
mit laufender App am Bildschirm bestätigt — genau die Auflage, an der der Befund
schon einmal hängen geblieben ist.

## Zu tun

1. `_timing_findings` (`oracles.py:585`) auf `action.dispatched` gaten, so wie
   `_click_findings` es tut (`oracles.py:453`), und zusätzlich unterdrücken,
   wenn im selben Schritt `no-accessible-action`, `click-no-visible-effect` oder
   `suspected-no-handler` steht. Diese Befunde erklären das Ausbleiben der
   Änderung bereits vollständig.
2. `ActionEvidence.scroll()` bekommt ein eigenes Kriterium (Zeilen-Frames wie in
   `_scroll_findings`) oder `expect_effect="none"` — der geerbte Default ist
   strukturell unerfüllbar.
3. Dispatch-Zeit in `harness_ms` einrechnen, damit Transportlatenz nicht als
   App-Beobachtungszeit zählt.

## Falsifizierung — verbindlich

- Fix 1 + 2 anwenden, **dieselbe Evidenz** neu aggregieren (kein neuer Nachtlauf
  nötig, die Trajektorien liegen vor): fällt die Zahl von 194 auf ~14 und sind
  alle Verbliebenen `expect_status`-Waits, ist die Diagnose bestätigt.
- Mutationsprobe: Gate testweise zurücknehmen, die Suite muss **rot** werden.
- `bash scripts/tests/cua-explore.sh` grün, Dateizahl darf nicht sinken.

## Abhängigkeit

Berührt `scripts/cua-explore/oracles.py` — **dieselbe Datei**, die der laufende
A/B/C-Auftrag anfasst. Erst nach dessen Abschluss und Abnahme starten.
