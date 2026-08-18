---
slug: mcp-drop-ai-create
worktree: /home/marvin/Projects/reprise-mcp-drop-ai-create
branch: chore/mcp-drop-ai-create
phase: shipped
created: 2026-08-18
---

# Handover — zwei Stränge, 18.08.2026 nachts

## 1. Showroom — **gelandet**

PR #561 ist per REST-Squash auf `dev` (`11c09573e6`). Enthält den Design-Import,
den behobenen Anzeigefehler, den Frame-Pfad, die Web Vitals und die bedienbaren
Seekbars. Pläne stehen auf `phase: shipped`; die Details stehen in
`showroom-design-import.HANDOFF-3.md` (Abschnitt „Nachtrag").

**Erledigt:** der `dev`-CI-Lauf zu diesem Merge (`32177283493`) wurde von der
GitHub-Nebenläufigkeit abgeräumt, als der MCP-Merge nachrückte — das ist kein Rot.
Gemessen wird der Stapel vom Lauf über `783b49b3e0` (siehe unten). Cross-target und Promotion source sind grün. Lokal
geprüft: `cargo fmt --check` und `cargo clippy -p reprise-platform-linux
-p reprise-gnome --all-targets -- -D warnings` beide sauber — **Achtung**, PRs
überspringen die schweren Suiten per Regel (`ci-paths.sh --suite-skip` gibt bei
`pull_request` immer `true`), das grüne Gate am PR sagt über Rust nichts.

Worktree `/home/marvin/Projects/reprise-showroom-design-import` ist entfernt,
Branch `feature/showroom-design-import` ebenfalls.

Aus dem Showroom weiterhin offen: vier kleine Review-Befunde, die Sprossen bei
390 px, `isHero()` sucht `#hero` statt `#rp-top`, und der Sheen-Wert
`--sheen-peak: 0.62` ist vom Auftraggeber unbeurteilt.

## 2. MCP — `ai:create` gestrichen, **gelandet**

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

**Gelandet:** PR #562 ist am 18.08.2026 um 19:53 UTC per REST-Squash auf `dev`
(`783b49b3e0`); die Desktop-Version wurde vor dem Push gehoben
(`chore: raise the desktop version for the withdrawn MCP surface`).
Der Zweig `chore/mcp-drop-ai-create` und der Worktree
`/home/marvin/Projects/reprise-mcp-drop-ai-create` sind damit erledigt.

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

## Was offen bleibt

Aus dem Showroom-Strang, unverändert offen: die vier kleinen Review-Befunde, die
Sprossen bei 390 px, `isHero()` sucht `#hero` statt `#rp-top`, und der Sheen-Wert
`--sheen-peak: 0.62` wartet auf ein Urteil des Auftraggebers. Alles Weitere aus
beiden Strängen ist auf `dev`.
