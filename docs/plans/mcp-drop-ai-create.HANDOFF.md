---
slug: mcp-drop-ai-create
worktree: /home/marvin/Projects/reprise-mcp-drop-ai-create
branch: chore/mcp-drop-ai-create
phase: coded
created: 2026-08-18
---

# Handover — zwei Stränge, 18.08.2026 nachts

## 1. Showroom — **gelandet**

PR #561 ist per REST-Squash auf `dev` (`11c09573e6`). Enthält den Design-Import,
den behobenen Anzeigefehler, den Frame-Pfad, die Web Vitals und die bedienbaren
Seekbars. Pläne stehen auf `phase: shipped`; die Details stehen in
`showroom-design-import.HANDOFF-3.md` (Abschnitt „Nachtrag").

**Offen:** der `dev`-CI-Lauf zu diesem Merge lief beim Abbruch der Sitzung noch
(`gh run view 32177283493`). Cross-target und Promotion source sind grün. Lokal
geprüft: `cargo fmt --check` und `cargo clippy -p reprise-platform-linux
-p reprise-gnome --all-targets -- -D warnings` beide sauber — **Achtung**, PRs
überspringen die schweren Suiten per Regel (`ci-paths.sh --suite-skip` gibt bei
`pull_request` immer `true`), das grüne Gate am PR sagt über Rust nichts.

Worktree `/home/marvin/Projects/reprise-showroom-design-import` steht noch und
ist sauber — kann weg (`git worktree remove`, Branch `feature/showroom-design-import`).

Aus dem Showroom weiterhin offen: vier kleine Review-Befunde, die Sprossen bei
390 px, `isHero()` sucht `#hero` statt `#rp-top`, und der Sheen-Wert
`--sheen-peak: 0.62` ist vom Auftraggeber unbeurteilt.

## 2. MCP — `ai:create` gestrichen, **noch nicht gepusht**

Ein Commit `c98cadbd6a` auf `chore/mcp-drop-ai-create`, Basis `origin/dev` nach
dem Showroom-Merge. 21 Dateien, −1478 Zeilen.

Weg: `music_create_instrumental`, `music_get_job_status`, deren DTOs, Datenschicht
und Fixtures; `agent.capability.ai:create` samt Default, Live-Abfrage,
Startup-Schnappschuss und Effektiv-Gate; die Staging-Verkabelung des Crates;
die AI-Tests und 147 Zeilen Worker-Helfer.

Bewusst geblieben: `Capability::AiCreate` in `reprise-runtime` (bewacht
`Command::Job` des Laufzeitprotokolls — anderer Strang, das Merkmal lebt),
Instrumental in Core/CLI, und der erzählende Teil der Planungsdokumente.

Grün: `cargo fmt`, `cargo clippy -p reprise-mcp --all-targets -- -D warnings`,
`cargo test -p reprise-mcp` (108 Tests, 16 Binaries, `TEST_EXIT=0`).

**Nächster Schritt:** pushen, PR gegen `dev`, Quality gate abwarten, REST-Merge
(`gh api -X PUT repos/marvinbaudach/reprise/pulls/<PR>/merge -f merge_method=squash`).
Version: `scripts/bump-version.sh --base origin/dev` vor dem Push laufen lassen —
der Zweig fasst `crates/*` an, hebt also die Desktop-Version.

## Fallen dieser Sitzung

- **`heavy-run` verschluckt die Ausgabe des Kindes.** Die Umleitung muss an
  `cargo` selbst hängen, nicht an `heavy-run`. Sonst: Exit-Code ohne eine Zeile
  Text.
- **`cargo` schreibt hier keine sichtbare stderr.** Diagnosen bekommt man über
  `--message-format=json` (das geht nach stdout und wird zuverlässig gefangen).
- **`heavy-run` braucht `--`** vor dem Kommando, sonst Usage + Exit 2.
- **CDP-Mauskoordinaten sind Viewport-Koordinaten.** Ein Bedienelement unter dem
  Falz muss erst per `scrollTo({behavior:'instant'})` ins Fenster, sonst gehen
  Klicks ins Leere und es sieht wie ein toter Regler aus.
- **`naturalWidth` ist bei `srcset` dichte-normiert** — Speicherzahlen daraus
  sind falsch; die echte Dateibreite kommt aus dem gewählten Dateinamen.
