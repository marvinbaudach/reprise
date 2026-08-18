---
slug: repo-tidy-before-going-public
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Repo aufräumen, bevor Marketing läuft und fremde Devs draufschauen

**Auftrag des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„Repo aufräumen. Gibt es Dinge die weg sollten, bevor ich Marketing mache und
andere DEVs drauf schauen? Sprich ist die Struktur des Repos best practise"*

Alle Zahlen unten sind am 16.08.2026 gegen den geteilten Hauptcheckout
(`be5f014d3b`) gemessen. **Nichts wurde geändert** — der Checkout ist mit
anderen Sitzungen geteilt.

## Kurzfassung

Die Struktur ist überdurchschnittlich gut: Lizenz, Verhaltenskodex,
Beitragsleitfaden, ADRs, zweisprachige README mit Architekturbild, Screenshots,
GitHub-Topics, Dependabot, und eine Gate-Kette, die sogar gegen die
GNOME-Circle-Ablehnungsgründe für KI-Beiträge prüft. **Kein Fund von
Zugangsdaten.** Es gibt genau **zwei Dateien, die eindeutig weg müssen**, eine
Verlaufs-Entwarnung, und eine offene Grundsatzfrage zum Prozess-Innenleben in
`docs/`.

## A. Zwei Dateien sind getrackt, obwohl `.gitignore` sie ausschließt

`.gitignore` greift nicht rückwirkend auf bereits getrackte Dateien. Beide
stehen ausdrücklich in der Ignore-Liste und liegen trotzdem im Repo:

| Datei | Größe | Was es ist |
| --- | --- | --- |
| `.pipeline-codex.md` | 1 KB | Bericht des **letzten** Codex-Laufs. `.gitignore` führt sie unter „Pipeline scratch files (codex-run.sh writes these per run)". |
| `.superpowers/sdd/progress.md` | **945 KB**, 1588 Zeilen | Aufgabenprotokoll eines KI-Werkzeugs („My Stats redesign progress", „Task T1: complete (commit …)"). `.gitignore` führt `.superpowers/` unter „Session scratch (not part of the repo)". |

Beides ist Werkzeugausgabe, kein Projektartefakt. Die progress-Datei ist
zugleich die **drittgrößte getrackte Datei des Repos** — größer als jede
Quelldatei, größer als jede Übersetzung. Für einen fremden Dev ist sie das
Erste, was beim Stöbern nach großen Dateien auffällt, und sie erzählt nichts
über das Produkt.

Dazu kommt ein praktischer Schaden, der bereits eingetreten ist:
`.pipeline-codex.md` wird von jedem Pipeline-Lauf neu geschrieben und
konfligiert deshalb bei jedem Rebase.

**Vorschlag:** `git rm --cached .pipeline-codex.md .superpowers/sdd/progress.md`
in einem eigenen Commit. Der Verlauf bleibt, die Dateien verschwinden aus dem
Arbeitsbaum-Index und werden ab dann von `.gitignore` gedeckt.

## B. Entwarnung beim Verlauf — die 1,9 GB sind ein lokales Problem

Der lokale `.git`-Ordner ist **1,9 GB** groß, und im Verlauf liegen die
Bauartefakte eines gelöschten Android-Prototyps:

```
17.9 MB  android-spike/app/build/intermediates/dex/debug/…/classes.dex
16.5 MB  android-spike/app/build/outputs/apk/debug/app-debug.apk
 2.0 MB  .cache/mesa_shader_cache/index      (Testlauf-Cache)
```

Aufgeschlüsselt über alle Blob-Versionen: **123 MB android-spike/**, **25 MB
`.cache*/`**, 19 MB Binärentwürfe unter `docs/design/`. Nichts davon ist heute
noch im Baum (`git ls-files` findet keinen einzigen dieser Pfade).

**Das rechtfertigt trotzdem keine Verlaufsumschreibung.** GitHub meldet für
das Repository **64 MB** (`gh api … --jq .size` → `64389` KB) — das ist, was
ein fremder Dev tatsächlich klont, und das ist völlig unauffällig. Die 1,9 GB
lokal sind schlicht ein ungepacktes Arbeitsverzeichnis:

```
count: 67208        ← lose Objekte (≈1,1 GB)
packs: 71           ← 71 einzelne Packdateien (≈725 MB)
prune-packable: 4803
```

Ein `git gc` räumt das ohne jede Verlaufsänderung auf. Eine Umschreibung
(`filter-repo`) würde dagegen alle Commit-Hashes ändern, jede
PR-Referenz entwerten und jeden bestehenden Klon brechen — für 64 MB, die
niemanden stören. **Empfehlung: `git gc`, keine Umschreibung.**

## C. Private Pfade sind nach der letzten Aufräumrunde zurückgekommen

`.pipeline-codex.md` protokolliert die Docs-Hygiene-Runde (PR #455):

> „Every surviving tracked document now uses `~` instead of the private home
> prefix. … zero private-home hits in tracked docs"

Heute, drei Tage später: **23 getrackte Dokumente enthalten wieder
`/home/marvin`**, sieben davon zuletzt am 13.08.2026 angefasst — also
zeitgleich mit der Bereinigung. Die Aufräumung war einmalig und wird von
keinem Gate gehalten, obwohl das Repo für Architektur, UX-Regeln,
Barrierefreiheit und KI-Hygiene je ein Skript hat.

Im Code selbst sind die Treffer harmlos und großteils **Absicht**:
`crates/reprise-mcp/tests/leak_matrix.rs` und
`crates/reprise-core/src/diagnostics/tests.rs` benutzen
`/home/marvin/Music/secret-folder/…` als Fixture, um zu beweisen, dass
Diagnoseausgaben private Pfade **nicht** durchlassen. Das ist ein guter Test.
Eine Ausnahme lohnt einen Blick: `library_doctor/remote/diagnostics.rs:7-8`
setzt `/home/marvin/.local/share/reprise/reprise.db` als Vorgabewert eines
`#[ignore]`-Diagnosetests — überschreibbar per `REPRISE_DIAG_DB_URI`, aber ein
fremder Dev liest dort zuerst deinen Benutzernamen.

**Vorschlag:** ein kleines Gate in der bestehenden Kette
(`scripts/check-merge-readiness.sh` listet die Gates), das getrackte Dokumente
auf den privaten Heimatpräfix prüft. Sonst wiederholt sich das alle paar
Wochen.

## D. Die offene Grundsatzfrage: wie viel Prozess-Innenleben bleibt sichtbar?

Das ist der Punkt, den Marketing und fremde Devs am stärksten sehen — und der
einzige, den ich nicht entscheiden kann:

| Ort | Umfang | Charakter |
| --- | --- | --- |
| `docs/plans/` | 102 getrackt (+43 **ungetrackt** im Arbeitsbaum) | Deutsch, mit Grill-Protokollen, `HANDOFF`-Notizen, Codex-Sitzungsfeldern im Frontmatter |
| `docs/superpowers/plans/` | 68 | dito, älterer Jahrgang |
| `AGENTS.md` | 490 Zeilen | Anweisungen an KI-Agenten, prominent im Wurzelverzeichnis |
| `CONTEXT.md` | 141 Zeilen | Kontextdatei fürs Werkzeug |

Zum Vergleich: `CONTRIBUTING.md` hat **51 Zeilen**. Ein fremder Dev findet also
zehnmal mehr Anweisungstext für Maschinen als für sich selbst.

Das ist kein Fehler — es ist eine Positionierungsfrage. Drei gangbare Haltungen:

1. **Offen lassen und dazu stehen.** Die Pipeline ist Teil der Geschichte des
   Projekts. Dann gehört ein Absatz in die README, der erklärt, was
   `docs/plans/` ist — sonst wirkt es wie versehentlich veröffentlichter
   Arbeitsspeicher.
2. **In ein eigenes Repo oder einen `internal/`-Zweig auslagern.** Der
   Hauptzweig zeigt Produkt und Beitragsweg, das Innenleben bleibt
   nachvollziehbar, aber nicht im Weg. Die Release-Archive tun das seit #455
   ohnehin schon: `docs/plans/` und `docs/superpowers/plans/` sind dort
   ausgeschlossen.
3. **`CONTRIBUTING.md` aufwerten.** Unabhängig von 1 oder 2: 51 Zeilen sind
   für ein Projekt dieser Größe knapp. Das ist der Text, an dem sich ein
   fremder Dev entscheidet.

Sicherheitshalber notiert, weil Marketing daran hängt: `scripts/check-ai-hygiene.sh`
prüft laut Kopfkommentar „the four shapes the GNOME Circle committee names as
rejection reasons for AI-generated submissions" — aber nur `src=crates`.
Wer das Repo unter GNOME-Circle-Gesichtspunkten anschaut, liest zuerst
`AGENTS.md` und `docs/plans/`, und die deckt das Gate nicht ab.

## E. 43 ungetrackte Planungsdokumente schweben im geteilten Checkout

`git status` zählt 43 ungetrackte Dateien unter `docs/plans/`, darunter die
drei aus dieser Sitzung. Im geteilten Hauptcheckout gehen solche Dateien
erfahrungsgemäß verloren. Vor dem Aufräumen entscheiden: einchecken oder
bewusst verwerfen — nicht liegen lassen.

**Der Schaden ist bereits eingetreten, und zwar messbar.**
`docs/plans/android-ci-gates.md` ist ungetrackt und trug bis zum 16.08.2026
`phase: planned` — obwohl die Arbeit am **14.08.2026 in #471 gelandet** und auf
`dev` grün ist. Ein ungetrackter Plan bekommt keinen Statusnachtrag, weil der
PR, der ihn erledigt, ihn nicht anfassen kann. Der nächste Leser hätte eine
fertige CI-Kette ein zweites Mal gebaut. Genau dafür ist das Frontmatter da —
und es funktioniert nur, wenn die Datei im Repo liegt.

## F. Was ausdrücklich gut ist

Damit die Aufräumliste nicht den falschen Eindruck hinterlässt:

- **Keine Zugangsdaten gefunden.** Mustersuche nach `api_key|secret|token|password`
  mit ≥16-stelligen Literalen: null Treffer außerhalb von Testfixtures.
- **README trägt.** Zweisprachig, Statushinweis („active alpha"), Architekturbild,
  Crate-Tabelle mit „Owns / Must not own" — das liest sich wie ein Projekt mit
  Haltung, nicht wie ein Hobbyordner.
- **OSS-Hygiene vollständig:** `LICENSE` (GPL-3.0), `LICENSING.md`, `LICENSES/`,
  `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `RELEASING.md`, `TESTING.md`,
  `reprise.doap`, ADRs, PR-Vorlage, Dependabot.
- **GitHub-Metadaten gepflegt:** Beschreibung und sieben Topics gesetzt.
  (`homepage` ist leer — der Showroom unter marvinbaudach.github.io/reprise
  gehört dort hinein, das ist ein Einzeiler mit Marketingwirkung.)
- **Screenshots liegen bereit:** vier unter `data/screenshots/`.

## Reihenfolge, wenn es losgeht

1. `git rm --cached` für die beiden Werkzeugdateien (A) — klein, unstrittig.
2. `git gc` lokal (B) — keine Verlaufsänderung, keine Absprache nötig.
3. `homepage` auf GitHub setzen (F) — ein Feld, sofort sichtbar.
4. Entscheidung zu D treffen, **bevor** Marketing läuft.
5. Gate gegen private Pfade (C) nachrüsten, damit die Bereinigung hält.
