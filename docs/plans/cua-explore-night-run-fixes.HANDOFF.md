# Übergabe — cua-explore-Reparatur, Stand 2026-08-10 11:40

## Wo es steht

`docs/plans/cua-explore-night-run-fixes.md`, **`phase: refactored`**.
Branch **`feature/cua-explore-night-run-fixes`** in
`/home/marvin/Projects/reprise-cua-explore-night-run-fixes`, HEAD `034c6fc5eb`,
21 Commits über `origin/dev` = `f8c29b00b3`.

Die Pipeline ist damit durch: plan → code → check → refactor. **Offen ist nur
noch die Abnahme am echten Bildschirm** (Nachtlauf), danach PR gegen `dev`.

Der zweite Worktree `…-fixes-ii` ist eingemergt und kann weg:
`git worktree remove ../reprise-cua-explore-night-run-fixes-ii`.

## Schon auf `dev` (fertig)

- **PR #392** — `DEFAULT_WIDTH` 1200 → **1440**, `DEFAULT_HEIGHT` → **900**, und
  ein Collapse vor dem ersten Frame löst keinen Toast mehr aus.
- **PR #393** — der Click-Probe schreibt den Treiberbaum neben jedes Screenshot.

## Verifiziert (selbst gefahren, nicht behauptet)

- **405 Tests in 15 Dateien grün.**
- `bash scripts/tests/cua-explore.sh` läuft **vollständig** durch, Exit 0.
  Das widerlegt §7/§8 des Plans: `unshare`/`dbus-run-session` funktionieren in
  der Codex-Sandbox. Künftige Pläne dürfen Codex mehr zutrauen.

## Was der Review ergab

Alle 13 Befunde stehen in `…findings.md`. **12 behoben, 1 begründet
zurückgewiesen** (K4: `unsupported` heißt „Sektion ohne Suchfeld", nicht
„Sektion fehlt" — der Alternate kostet nur Budget, wenn die Sektion wirklich
fehlt, also genau im Fall, für den er da ist).

Beide Blocker waren dieselbe Krankheit und beide hatten **grüne Tests**: die
Treiber-Nutzlast erreichte nie die Platte (Test setzte `evidence_dir` von Hand),
und ein erschöpftes Budget endete als Abbruch (Test rief
`ensure_run_complete` mit handgebautem Summary). Für den nächsten Durchgang:
**ein Test, der nicht den Weg der Produktion geht, beweist nichts.**

## Nächste Schritte, in dieser Reihenfolge

1. **Nachtlauf (M4 im Plan).** Alle sechs Missionen × Seeds 11 und 29,
   `--profile release`, **strikt sequenziell** und **nicht** parallel zum
   Nightly (`reprise-nightly.timer` feuert 04:33). Frisches Evidenzverzeichnis.
   Dabei die **Laufzeit messen** — erst danach über mehr Seeds entscheiden;
   niemand weiß, wie lange ein Lauf braucht, der sein Budget wirklich
   ausschöpft. Erwartungen stehen in §8/M4 des Plans.
2. **Aggregator umstellen (M5).** `~/.local/bin/reprise-explore-report` wird ein
   dünner Aufruf von `scripts/cua-explore/aggregate_report.py`. Gegenprobe: die
   Geometriezeile muss `164/168`-artige Werte zeigen, nie wieder `164/1129`.
3. **Befunde sichten (M6).** Der Aggregator sortiert jetzt nach
   Reproduzierbarkeit — von oben abarbeiten, was in zwei Seeds derselben Mission
   auftritt, wird Issue.
4. **PR gegen `dev`.**

## Die offene Messung, die alles andere überragt

**F8: die semantische Route wirkt nicht.** Drei Proben, zwei Profile, zwei
verschiedene Knöpfe (`Search all fields`, `Toggle sidebar`), beide mit
`actions=['click']`:

| Probe | ax | px |
| --- | --- | --- |
| `search-probe` | 0.000 | 0.441 |
| `search-probe-sources` | 0.000, 212 → 212 | 0.441, `search box` erscheint |
| `ax-control-probe` | 0.000, 212 → 212 | 0.452, Seitenleiste klappt zu |

Der nächste Schritt ist **kein Issue gegen Reprise**, sondern eine
Trennmessung: dieselbe Probe gegen eine fremde GTK4-Anwendung. Wirkt `ax` dort,
ist es Reprise; wirkt es dort auch nicht, ist es `cua-driver` und gehört als
Upstream-Repro nach `scripts/upstream-repros/`.

Bis das geklärt ist, hält E4 (e) den Harness am Leben: nach drei wirkungslosen
`ax`-Versuchen schaltet ein Lauf auf `px` und setzt **einen** Befund
`semantic-route-unavailable`, der die Beobachtung benennt, nicht die Schuld.

## Weitere echte Produktbefunde aus dem Messlauf

- **11 × `no-accessible-action`** auf den Seitenleistenzeilen: `actions == []`,
  die Navigation ist per Assistenztechnik semantisch nicht bedienbar (F4).
- **`main-loop-stall`**: Antwortlücken 1270 / 1294 / 976 ms bei Basislinie
  973 ms — der Lauf lief allein auf der Maschine.
- **Startzeit**: Fenster nach 1929 ms, benutzbarer AT-SPI-Baum erst nach
  **6770 ms** (F7).
- Rating-Sterne ohne unterscheidbaren Namen (F5), Spaltenkopf ohne Aktion (F6).

## Fallen beim Betrieb

- `run.sh` ruft `cargo` im **aktuellen Arbeitsverzeichnis** auf → vorher
  `cd` in den Worktree.
- `run.sh` verweigert den Start bei **schmutzigem Worktree**.
- Missionen mit `agent: required` (`section-search-isolation`,
  `offline-recovery`, `large-library-stress`) lassen sich **nicht** als
  `--click-probe` fahren; dafür `hover-affordance-sweep` nehmen, dasselbe Profil
  `mixed-sources-128`.
- Profil `mixed-128` hat **keine** Podcasts/YouTube/Radio-Module;
  `mixed-sources-128` schaltet sie ein (`fixtures.py:199-203`).

## Evidenz (außerhalb des Repos, nur lesen)

- `~/.cache/reprise-explore-evidence/2026-08-10/` — der gescheiterte Nachtlauf
- `~/.cache/reprise-explore-evidence/2026-08-10-postfix/` — die Nachmessungen:
  `first-time-exploration-seed-11`, `search-probe`, `search-probe-sources`,
  `ax-control-probe`

## Wake-Lock

`wake-lock release cua-explore-fixes`, sobald nichts mehr läuft.
