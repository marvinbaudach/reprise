# Handover — Updates/Concerts/Releases-Umbau, 14.08.2026 19:30

Die Pipeline ist für **alle drei Stränge** durch: `plan → code → check → refactor`.
Nichts ist gepusht, nichts gemergt. Was fehlt, ist die Landung in der
Reihenfolge 1 → 2,3 und danach die Post-Merge-Querprüfungen aus §7 des
Mutterplans.

**Mutterplan:** `docs/plans/updates-concerts-releases-rework.md` (eingefroren,
`strands: 1,2,3`, `merge_order: 1,2,3`, `branch:`/`worktree:` bewusst leer)

| Strang | Branch | Worktree | Stand |
|---|---|---|---|
| 1 `core-concerts` | `feature/updates-concerts-releases-rework-1` | `~/Projects/reprise-updates-concerts-releases-rework-1` | `refactored`, 6 Commits, sauber |
| 2 `updates-popover` | `feature/updates-concerts-releases-rework-2` | `~/Projects/reprise-updates-concerts-releases-rework-2` | `refactored`, 11 Commits, sauber |
| 3 `update-notifications` | `feature/updates-concerts-releases-rework-3` | `~/Projects/reprise-updates-concerts-releases-rework-3` | `refactored`, 10 Commits, sauber |

Alle drei liegen **9 Commits hinter `origin/dev`** (`f24366b269` beim Schreiben
dieses Dokuments). `land.sh` rebast selbst.

---

## Der eine Beschluss, der von §7 abweicht — und warum er die Landung nicht ändert

Der Plan sagt, Strang 2 und 3 sollen auf den **gemergten** Strang 1 rebasen,
bevor sie beginnen. Das hätte bedeutet: Strang 1 codieren, prüfen, refaktorieren,
landen — und erst danach 2 und 3 starten, über mehrere Sitzungen.

Stattdessen zweigen **2 und 3 direkt von Strang 1s Branch ab**
(`52b75db56f`, dem Stand vor Strang 1s Review-Fix). Ihre Vorbedingungen —
`TicketAvailability`, `query::mark_event_seen()`, `ui/feed_footer.rs`, die Spalte
`new_releases.notified_released_at` — lagen damit vor, und beide konnten
gleichzeitig laufen.

**Folgen für die Landung: keine.** Die Reihenfolge bleibt 1 → 2,3; jeder Branch
rebast vor seinem Merge auf `dev`. Der einzige Unterschied ist, dass 2 und 3
Strang 1s Commits mitschleppen, bis Strang 1 gelandet ist — nach dessen Merge
verschwinden sie beim Rebase.

**Folge für Diffs:** Ein Diff von Strang 2 oder 3 gegen `origin/dev` zeigt heute
auch Strang 1s Arbeit. Wer nur die eigene Arbeit sehen will, nimmt
`git diff 52b75db56f...HEAD`.

---

## Was gelandet werden muss

```bash
# 1. Strang 1 zuerst — er ist Vorbedingung für beide anderen
scripts/land.sh <PR> ~/Projects/reprise-updates-concerts-releases-rework-1

# 2. danach 2 und 3, in beliebiger Reihenfolge (land.sh rebast auf das neue dev)
scripts/land.sh <PR> ~/Projects/reprise-updates-concerts-releases-rework-2
scripts/land.sh <PR> ~/Projects/reprise-updates-concerts-releases-rework-3
```

Jeder Branch trägt **seine eigene Strangdatei** mit passender `branch:`-Zeile im
Status-Block; der Mutterplan hat keine. `land.sh` findet also je Branch genau
einen Plan und braucht kein `--plan`.

Die Disjunktheitsprüfung ist gelaufen und **grün**: Strang 2 und 3 — die beiden,
die unabhängig voneinander landen — teilen sich genau eine Datei,
`docs/ux-rules.md`, und dort liegen ihre Änderungen in weit auseinanderliegenden
Abschnitten:

```
S3  Abschnitt H   Zeilen 1342–1362
S2  Abschnitt R   Zeilen 2211–2524   + genau EINE Zeile bei 5068 (CONC-7-Marker)
S1  Abschnitt AE  Zeilen 4972–5126
```

Die eine Zeile bei 5068 ist die in §7 namentlich vorgesehene Ausnahme und liegt
**zwischen** Strang 1s Hunks, nicht auf ihnen.

---

## Vier genehmigte Besitzausnahmen — sie sind keine Verstöße

Wer die Diffs prüft, wird auf Dateien stoßen, die laut §7 einem anderen Strang
gehören. Alle vier sind bewusst freigegeben und im jeweiligen Commit begründet:

1. **Strang 1 → `ui/updates/concerts_section.rs`** (Strang 2), zwei Zeilen im
   `#[cfg(test)]`-Helfer: `use reprise_core::concerts::TicketAvailability;` und
   `availability: TicketAvailability::Unknown,`. `ConcertRow` bekam ein
   Pflichtfeld, und dies ist die einzige Konstruktionsstelle außerhalb von
   `ui/concerts/**`. Ohne sie wäre der `reprise-gnome`-Testbau nach Strang 1s
   Merge rot geblieben, bis Strang 2 landet — und §7 verlangt ausdrücklich, dass
   Strang 1 allein lieferbar ist.
2. **Strang 1 → `db_recent_migration_tests.rs`**, die erwartete Spalte
   `notified_released_at` in `assert_new_releases_schema()`. Die Datei gehört
   keinem Strang; die neue Spalte stammt aus Strang 1s eigener `migrate_v74`.
3. **Strang 2 → `ui/feed_footer.rs`** (Strang 1), ein additiver Zugang
   `apply_with_copy(state, copy)`. Strang 1 hatte `FeedFooterCopy` und
   `presentation_with_copy()` gebaut, aber der einzige Weg ans Widget war
   `apply(state)` mit fest verdrahteter Concerts-Copy; `apply_presentation()` war
   privat. `apply()` ist unverändert.
4. **Strang 2 → `releases/releases_failure_ui.rs`**, Umbenennung zweier
   Testfunktionen `nr_21_…` → `nr_21a_…`. Die Datei gehört keinem Strang, und
   `NR-21a` ist Strang 2s eigene Regel aus Abschnitt R.

### Die Regel, die dem Plan fehlte

Codex hat zweimal ~45 Minuten ohne Commit verbraucht, weil er **jede nicht
gelistete Datei als fremd** las. Die Besitzlisten in §7 trennen die drei Stränge
voneinander; über den Rest des Repos sagen sie nichts. Ab dem dritten Lauf galt
deshalb explizit: *nicht gelistete Datei + rein mechanischer Folgefehler aus der
eigenen Arbeit = eigene Zuständigkeit, mit Nennung im Commit.* Danach kam kein
Stopp dieser Art mehr. **Für künftige Strang-Pläne gehört dieser Satz in §7.**

Betroffen waren dadurch legitim: sieben `db_*_migration_tests.rs`,
`db_recent_test_support.rs`, `reprise-core/src/lib.rs`, `ui/mod.rs`,
`ui/style/theme.rs`, `main.rs`, `ui/file_open.rs`.

---

## Review: 4 Befunde, 3 überlebt, 3 behoben

Reviewer je Strang (`rust-reviewer` auf den `.rs`-Diff, generischer Sonnet-Agent
auf `docs/ux-rules.md` und `po/POTFILES.in`), danach je Befund ein Skeptiker mit
dem Auftrag, ihn zu **widerlegen**.

| # | Strang | Schwere | Befund | Fix |
|---|---|---|---|---|
| 1 | 1 | medium | `reprise_view::columns::ConcertColumn` war nach dem Umbau tot **und falsch** (6 Spalten statt 7, kein `Source`, `Tickets` angepinnt), weil die Tabelle auf ein lokales `ConcertTableColumn` umgestellt wurde. `pub` + re-exportiert ⇒ kein `dead_code`-Warnhinweis. | `6eeee39b6e` — geteilte Definition wieder einzige Wahrheit, `ConcertTableColumn` entfernt. `concerts_column_layout.rs` steht danach netto **+2 Zeilen** über `dev`. |
| 2 | 3 | high | `concerts_announced: Cell<bool>` war ein Latch über die **ganze Prozesslaufzeit**: nach der ersten Concerts-Meldung blieb der Zweig für den Rest der Sitzung still, auch bei neuen Terminen. Kein Test fasste den Closure-Zustand an. | `a5a215416a` — reine, testbare `ConcertAnnouncementState::observe(count)`; schreibt den Zustand **nur nach erfolgreichem Versand** fort, also wird ein fehlgeschlagener Versand erneut versucht. |
| 3 | 2 | high | Tooltip und `sensitive(false)` saßen auf demselben Knopf. GTK4 pickt mit `GTK_PICK_DEFAULT`, `GTK_PICK_INSENSITIVE` ist optional ⇒ insensitive Widgets nehmen am Hit-Testing nicht teil. `No ticket or event link available` war gesetzt und trotzdem nie sichtbar. Der Test las nur die Property, nie ein Hover. | `18fe422661` — Tooltip am sensiblen Zeilen-Wrapper, einheitlich für beide Zustände, mit Kommentar; `nr_38` prüft jetzt Ort **und** Sensitivität. |
| 4 | 2 | high | **Widerlegt.** Behauptung: die Zurückziehung von CONC-7/NR-22 breche `check-ux-traceability.sh`. | Der Skeptiker hat das Skript laufen lassen, den roten Exit reproduziert — und belegt, dass genau dieser Zwischenzustand geplant ist: die verweisenden Tests liegen in fremdem Besitz, §7 führt den vollständigen Lauf als **Post-Merge**-Prüfung. Kein Fix nötig. |

Ein weiterer Befund (S1, low: `CONC-11a` zitiere die zurückgezogene `CONC-4b`)
wurde ebenfalls widerlegt — der Mutterplan hält für `CONC-4b → CONC-4c` eine
wortgleiche Neuausstellung fest.

---

## Was NICHT bewiesen ist

1. **Screenshot 8 (Strang 3) fehlt.** CUA bekam die verschachtelte GNOME-Shell
   weder über X11 (schwarzes Bild) noch über Wayland (kein
   `zwlr_screencopy_manager_v1`, Portal-Fallback ohne Aufnahme) aufgenommen; die
   echte Desktop-Sitzung wurde regelkonform nicht benutzt. **Coverdarstellung und
   Klickgeste der Benachrichtigung sind visuell unbestätigt.**
2. **Strang 1s Hover-Aufnahme** scheiterte an `desktop_escalation_required`.
3. **Ein Kontrollarm hat eine Planannahme widerlegt.** Strang 1 fand das
   Listenende auch im exakt zurückgerollten Altcode bereits direkt unter der
   dritten Zeile — die im Plan behauptete alte Mittenposition war **nicht
   reproduzierbar**. Der Fix behebt hier möglicherweise etwas, das so nie kaputt
   war. Beleg als Screenshot im Codex-Lauf.
4. **Eine Datei liegt bei 799 Zeilen** (Strang 3), einen Strich unter der Grenze.
5. Alle Gate-Angaben („fmt/clippy/test grün") stammen aus den Codex-Läufen. Sie
   sind **Behauptungen**, nicht nachgemessen. Selbst geprüft wurden nur die
   Besitzfragen, die Zeilenbereiche in `docs/ux-rules.md` und die Diffs der
   Review-Fixes.

---

## Eine Entscheidung, die reversibel ist

**Dismiss entfernt die Zeile sitzungslokal aus dem gehaltenen Delta** und ruft
zusätzlich `mark_event_seen()`.

Der Plan enthält hier einen Widerspruch: Aufgabe 7 erklärt die Reihenfolge
*rendern → stempeln → Badge neu rechnen* (NR-9c) für unantastbar — beim Öffnen
wird also der **ganze** Concerts-Stapel gestempelt. Danach ist
`mark_event_seen()` für eine einzelne Zeile wirkungslos (`WHERE seen_at IS NULL`),
der Klick hätte keinen sichtbaren Effekt.

Gewählt wurde die einzige Lesart, die keine harte Planregel bricht: der Klick
wirkt auf den gehaltenen Delta, der Kern bleibt unangetastet, der
`mark_event_seen()`-Aufruf bleibt wie vorgeschrieben drin (idempotent, korrekt
für noch ungestempelte Zeilen) und trägt einen Kommentar.

**Wenn Dismiss ein eigener, persistenter Zustand werden soll**, ist das ein
Nachtrag in Strang 2 plus eine Spalte — also Schema-Arbeit, die nach §7 Strang 1
gehört hätte. Das wäre ein neuer Plan, kein Nachbessern hier.

---

## Post-Merge-Querprüfungen (§7 des Mutterplans, nach dem letzten Merge)

Keine davon kann ein Strang allein grün bekommen:

1. `scripts/check-ux-traceability.sh` über die ganze `docs/ux-rules.md` —
   erwartet: jede neue `[active]`-Regel hat ihren Test, **kein** Test zeigt mehr
   auf `CONC-3/4b/5a/7/10/11`, `NR-5b/10a/21/22/23`. (Vor dem letzten Merge ist
   dieser Lauf planmäßig rot, siehe Befund 4.)
2. **Ein Wort, zwei Flächen:** dasselbe Event zeigt in der Concerts-Tabelle
   (S1) und in der Popover-Zeile (S2) denselben Status.
3. **Ein Zeitstempel, drei Fußzeilen:** `git grep -n 'Updated .*ago'` über
   `crates/reprise-gnome/src/ui` liefert **keinen** Treffer mehr.
4. **Dieselbe URL:** die Benachrichtigung (S3) öffnet für ein Release exakt die
   URL, die dessen Popover-Zeile (S2) öffnet.
5. **Geometrie-Parität:** Screenshot-Paar Popover-Zeile gegen Tabellenzeile —
   gleiche 44×44-Kachel, gleiche 2px-Akzentmarke, gleiche Tag-Typografie.
6. **Migrationskette am Stück:** eine v72-Datenbank durch **beide** Migrationen,
   danach `PRAGMA user_version == 74` und beide Spalten prüfen.
7. Die Gesamt-Gates: `cargo fmt --check`, `clippy --all-targets --workspace
   -- -D warnings`, `cargo test --workspace`, `cargo audit`, Kernreinheit,
   `every_icon_name_the_app_asks_for_can_be_drawn`, 800-Zeilen-Grenze.
8. **Die Abnahme aus §5** in voller Länge.

---

## Betriebsnotizen

- Der Wake-Lock `pipeline-ucr` ist **freigegeben**. Für einen langen
  Landungs- oder Gate-Lauf einen neuen nehmen.
- Alle Codex-Läufe liefen über `heavy-run medium` — der Lastregler war
  zeitweise mit 6/6 Slots belegt und hat die Läufe eingereiht. Das ist normal
  und kein Hänger.
- Die Prompt-Dateien liegen als `.pipeline-task.md` in den drei Worktrees, die
  Codex-Zusammenfassungen als `.pipeline-codex.md`. Letztere ist **getrackt** —
  sie wurde in allen drei Worktrees auf den `dev`-Stand zurückgesetzt, damit die
  Bäume sauber sind.
- Kosten: **10 Codex-Läufe** — Strang 1: 3 Code + 1 Refactor, Strang 2: 3 Code
  + 1 Refactor, Strang 3: 1 Code + 1 Refactor. Vier der sieben Code-Läufe endeten
  ohne Commit an einer Besitz- oder Entwurfsfrage. Dazu 2 Review-Workflows mit
  11 Agenten und ~837k Subagent-Tokens.
