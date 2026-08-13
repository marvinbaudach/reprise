# Angenommene Review-Befunde — alle 13, vom Nutzer am 2026-08-10 bestätigt

Worktree: `/home/marvin/Projects/reprise-cua-explore-night-run-fixes`
Branch: `feature/cua-explore-night-run-fixes`, Basis `f8c29b00b3`.
Alle 377 Tests waren vor dem Refactor grün; `bash scripts/tests/cua-explore.sh` lief Exit 0
vollständig durch. **Das muss danach wieder gelten.**

## Blocker

**B1 — Die aufbewahrte Treiber-Nutzlast landet nie auf der Platte.**
`driver.py`: `CliTransport.__init__(evidence_dir=None)`, und `_retain_fault` (~104-124) steigt
bei `None` aus, bevor `driver-faults.jsonl` geschrieben wird. Die einzige Produktionsstelle,
`runner.py:423`, übergibt kein `evidence_dir`. Damit besteht der Defekt aus Plan §1.3 fort, den
E7 beheben sollte. Fix in `runner.py` (Evidenzverzeichnis durchreichen) **und** ein Test, der
die Konstruktion aus dem Runner heraus prüft — nicht nur einen Test-Transport, der es von Hand
setzt.

**B2 — Ein erschöpftes Budget endet als Abbruch statt als „unvollständig".**
`protocol.py:418-421` wirft `ContractError("action|time budget exhausted")`; die Aktionsschleife
`runner.py:528` fängt das nicht, `main()` (~750-762) bildet es auf **1** ab. Zusätzlich wirft
`ensure_run_complete` (`runner.py:346-350`) unbedingt, sobald `finished=False`, unabhängig vom
bereits korrekt berechneten `summary["outcome"]`. Die E6-Tabelle verlangt für diese Zeile Exit
**0**. Fix: Budgeterschöpfung ist ein reguläres Ende (`outcome: "incomplete"`, Exit 0), nur echte
Abbrüche liefern 1. Test muss den echten `run()`-Pfad mit erschöpftem Budget fahren, nicht
`ensure_run_complete` mit handgebautem Summary.

## Wichtig

**W1 — `"actions": null` bringt `_target` zum Absturz.** `driver.py:679` und `:721` nutzen
`.get("actions", ())`; der Default greift nur bei fehlendem Schlüssel, nicht bei `None` →
ungefangener `TypeError`, der den Lauf beendet. `explorer.py:404/408` schützt sich an derselben
Datenquelle bereits mit `or ()`. Fix am besten in `ui_vocabulary.invocable_actions` /
`unknown_action_names` selbst, damit es nur eine Stelle gibt.

**W2 — Ein kaputtes JSON-Artefakt tötet den Aggregat-Report für alle Läufe.**
`aggregate_report.py:154` (`json.loads` in `load_run`) und `:144` (`_trajectory_findings`) sind
ungefangen; `discover_runs:168` propagiert bis `main:199`. Reproduziert mit einem abgeschnittenen
`summary.json`. Fix: kaputten Lauf als Lücke vermerken, die gesunden berichten.

**W3 — Die Aggregator-Tests fassen den Ladepfad nie an.** `cua-explore-aggregate.py` baut
`RunRecord`-Objekte im Speicher; `discover_runs`, `load_run`, `_trajectory_findings`,
`render_report`, `main` und der Fallback „Findings im Summary vs. `trajectory.jsonl`" sind
ungetestet. Der Plan (I-5) verlangte **eingecheckte, gekürzte `summary.json`-Kopien**. Diese
Fixtures anlegen und den echten Glob-/Lade-/Parse-Pfad fahren.

**W4 — `oracle-never-evaluated` feuert falsch ohne externen Agenten.** `runner.py:700-705`
knüpft `oracle_activity.supersede("accessibility", …)` allein an die Datei
`agent-home/dispatch-policy.json`, die nur `agents/agent_core.py` schreibt. Bei
`agent: optional`-Missionen ohne `--agent-command-json` läuft `DeterministicExplorer`, wählt
`px`, das Accessibility-Orakel wird nie ausgewertet und bekommt einen Fehlalarm. Der
`getattr(explorer, "dispatch_policy", None)`-Fallback in `:702` fließt in den Report, aber
**nicht** in die `supersede()`-Bedingung.

**W5 — E4 (a) hat die Notiz vergessen.** `explorer.py:390-421` und `agents/steps.py:30-53`
bleiben bei fehlender Geometrie korrekt bei `ax` — aber ohne jede Notiz. Der Plan verlangt „bleibt
es bei `ax` **plus Notiz**".

**W6 — E5 prüft den falschen Wert.** `agents/assertions.py:47-55` prüft nur, ob *irgendein*
nicht-leerer String im Suchfeld steht, nicht ob es der getippte Token ist — `assertion_codes`
bekommt weder `TokenLearner` noch Token übergeben. Ein Altwert aus der Vorquelle (Escape leert
nicht zuverlässig, siehe `agent_core.py:632-647`) kann die Vorbedingung fälschlich erfüllen.

## Klein

**K1** `driver-faults.jsonl` deckelt 2000 Zeichen pro Zeile, nicht die Zeilenzahl —
Gesamtgrenze ergänzen.
**K2** `oracles.py:285` lowercased Aktionsnamen, `driver.py:679/721` nicht; beide urteilen auf
demselben Snapshot. Symmetrisch machen.
**K3** `window_setup.apply_window_size:38-39` ruft `resize_window`/`wmctrl_geometry` ohne Retry;
E3 lässt sie jetzt doppelt so oft laufen, und jeder transiente `wmctrl`-Hänger bricht den Lauf ab.
**K4** `agents/plans.py:64-77` hängt den Sidebar-Toggle-Alternate auch an `unsupported-*`-Schritte,
die bewusst erwarten, dass die Sektion fehlt — bis zu 5 Aktionen Budget pro Eintrag umsonst.
**K5** Der Docstring von `cua-explore-aggregate.py` behauptet einen eingecheckten Report, den es
nicht gibt (löst sich mit W3 auf).
