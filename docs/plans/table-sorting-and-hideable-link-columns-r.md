---
slug: table-sorting-and-hideable-link-columns-r
worktree: /home/marvin/Projects/reprise-table-sorting-and-hideable-link-columns-r
branch: feature/table-sorting-and-hideable-link-columns-r
phase: shipped
codex_session:
created: 2026-08-15
---

# Strang R — Abschluss: UX-Regeln und Gesamtlauf

## Zweck

Nach dem Zusammenführen von A, B und C: die UX-Regeln nachziehen, die durch die
Umsetzung falsch geworden sind, und einen vollständigen Lauf fahren, der zeigt,
dass die drei Stränge zusammen funktionieren — und nicht nur jeder für sich.

**Vorbedingung:** A, B und C sind **alle drei** gelandet. Prüfe das, bevor du
anfängst:

```
git log --oneline origin/dev | head -20
```

Es müssen Commits für alle drei Stränge sichtbar sein, insbesondere
`crates/reprise-gnome/src/ui/table_columns/single_sort_indicator.rs` (A),
`migrate_v75` in `crates/reprise-core/src/db_concerts.rs` (B) und
`ReleaseColumn::pin()` ohne `Pin::Trailing` (C). Fehlt einer, brich ab und melde
es — die Querprüfungen sind sonst wertlos.

Aufgaben in dieser Reihenfolge: **R-1, R-2.**

---

## Dateibesitz

Dir gehören:

```
docs/ux-rules.md
docs/plans/table-sorting-and-hideable-link-columns*.md
```

**Eine eng begrenzte Ausnahme:** Zeigt eine Querprüfung in R-2 einen echten
Merge-Schaden (ein Test, der in keinem Strangzweig rot war und erst im
zusammengeführten Stand rot ist), darfst du ihn minimal reparieren, egal in
welcher Datei. Halte die Reparatur so klein wie möglich und benenne sie im
Bericht ausdrücklich als Merge-Reparatur. Alles andere ist ein Befund, keine
Änderung.

## Was dir **nicht** gehört

Alles übrige. Insbesondere gilt: fällt dir beim Lesen eine Verbesserung an
Sortierung, Spalten oder Migration auf, ist das ein Befund für den Bericht.
R ist kein zweiter Anlauf auf B oder C.

---

## Aufgabe R-1 — Die UX-Regeln nachziehen

**Ziel:** Keine Regel behauptet noch, was der Code nicht mehr tut.

**Bereich:** `docs/ux-rules.md`.

### Warum das kein Doku-Nachtrag ist

`docs/ux-rules.md` STYLE-10 [active] enthält heute wörtlich (`:3010-3013`):

> A table may declare fixed columns — a leading artwork column, **a trailing
> action column on a surface without a row context menu** — which stay visible,
> keep their position and never appear in the editor

Das ist wörtlich die Begründung, die `ReleaseColumn::pin()` im Code trug.
Strang C hat genau diesen Fall entfernt. Die Regel ist durch die Umsetzung
falsch geworden und muss mitgeändert werden — sie ist Teil der Änderung, nicht
ihre Nachpflege. Die Spec erwähnt `ux-rules.md` überhaupt nicht.

### ID-Vergabe — Achtung

Höchste vergebene IDs sind heute **STYLE-12**, **CONC-16**, **NR-33**.
**NR-34 bis NR-38 sind bereits von Strang 2 der laufenden Arbeit
`updates-concerts-releases-rework` reserviert** und noch nicht in `dev`. Die
neue Releases-Regel dieses Plans heißt deshalb **NR-39**, nicht NR-34. Der
Display-Test aus C-3 heißt entsprechend `nr_39_…` und existiert bereits.

Prüfe vor dem Schreiben, ob jene Arbeit inzwischen gelandet ist:

```
grep -nE '^\- \*\*(NR|CONC|STYLE)-[0-9]+' docs/ux-rules.md | tail -20
```

Sind NR-34…NR-38 inzwischen belegt, bleibt NR-39 trotzdem richtig. Sind sie es
nicht, bleibt NR-39 **ebenfalls** richtig — die Lücke ist bewusst und darf nicht
„aufgefüllt" werden.

### Vorgehen

1. **STYLE-10 → STYLE-13.** STYLE-10 bekommt nach etablierter Konvention
   (`docs/ux-rules.md:20`) den Marker `[replaced by STYLE-13]`; sein Text bleibt
   **wörtlich stehen**. Neu geschrieben wird **STYLE-13** [active] mit
   demselben Text, aber:
   - die Aufzählung fester Spalten schrumpft auf „a leading artwork column";
     der Halbsatz über die trailing action column entfällt;
   - neuer Satz: eine Tabelle zeigt **genau einen** Sortierindikator — der der
     aktuellen Primärspalte; die Indikatoren aller anderen Spalten sind
     unsichtbar, ihre Breite bleibt reserviert, damit Header nicht springen;
   - neuer Satz: ein Header ohne Sortierfeld trägt **keinen** Sorter und wirkt
     deshalb nicht klickbar; ein Header, der sortiert, ordnet **seine eigene**
     Spalte;
   - die Sätze über Füller (STYLE-9) und über den Sortier-Rückfall beim
     Verstecken der sortierten Spalte bleiben unverändert;
   - **Test rule** bleibt: one rule-named display test per table, plus a
     measured filler test.

   Genannte Tests für STYLE-13:
   `style_10_hiding_the_sorted_column_keeps_a_visible_sort_indicator`
   (`ui/table_columns/registry.rs`),
   `nr_39_the_column_editor_lists_status_and_link_and_hides_them`
   (`ui/releases/releases_view_tests.rs`),
   `conc_16_the_source_column_is_available_but_off_by_default`
   (`ui/concerts/concerts_view_tests.rs`),
   `two_concert_sorts_leave_one_indicator`,
   `two_release_sorts_leave_one_indicator`,
   `only_the_ticket_header_carries_no_sorter`,
   `the_cover_status_and_link_headers_carry_no_sorter`,
   `hiding_venue_by_default_moves_the_filler_to_the_artist_column`.

   **`style_10_…` wird nicht umbenannt.** Der Testname bleibt, obwohl die Regel
   ersetzt wird: `scripts/check-display-tests.sh --rule-named` bildet Regel-IDs
   nicht auf Testnamen ab, sondern filtert Tests nach dem Muster
   `<präfix>_<zahl>_` und fährt sie — der Test läuft also weiter mit. Eine
   Umbenennung wäre Bewegung in einer Datei, die alle drei gelandeten Stränge
   berührt haben, ohne Gewinn. Im Repo gibt es dafür Präzedenz (`nr_11_…`,
   `conc_2_…`): Testnamen wandern hier nicht mit jeder Ersetzung mit.

   > **AUFGEHOBEN am 15.08.2026, vom Nutzer freigegeben.** Der Absatz oben
   > beruht auf einer falschen Prämisse: `check-display-tests.sh --rule-named`
   > ist nicht das bindende Skript. Das ist `scripts/check-ux-traceability.sh`,
   > Zeile 70 von `scripts/check-merge-readiness.sh`. Dessen Regel 2 lautet
   > „No test references an ID that is missing from the document or marked
   > `[replaced ...]`", und die Fehlermeldung schreibt die Auflösung selbst vor:
   > `ERROR: test references replaced rule STYLE-10 — re-point it`. Der genannte
   > Präzedenzfall existiert unter diesem Gate nicht mehr.
   >
   > **Was stattdessen gilt:** die betroffenen Testnamen werden eng begrenzt auf
   > `style_13_…` bzw. `conc_17_…` umgezeigt, und STYLE-13 wie CONC-17 bekommen
   > je einen gleichnamigen Test (Richtung 1 desselben Gates: jede
   > `[active]`-Regel braucht Deckung). Die Zusicherungen der Tests bleiben
   > unverändert — umgezeigt wird der Name, nicht die Messung. Der Nebengrund
   > („Bewegung in einer Datei, die alle drei Stränge berührt haben") trägt
   > hier nicht: R landet zuletzt, nach ihm rebased kein Strang mehr.

2. **NR-30** [active] bleibt inhaltlich richtig, solange die Spalte sichtbar
   ist. Ein Halbsatz kommt dazu: die Spalte ist ausblendbar und verschiebbar;
   „trailing" beschreibt die **Vorgabeposition**, keine Garantie.

3. **NR-33** [active] — die Spaltenliste `Cover · Date · Release · Artist ·
   Type · Status · Link` ist jetzt die **Vorgabe**, nicht die feste Ordnung.
   Gleicher Halbsatz. Der Satz über den Cover-Pin bleibt: Cover ist weiterhin
   führend gepinnt.

4. **NR-39** [active] neu, in Abschnitt R: Releases' `Status`- und `Link`-Spalte
   sind gewöhnliche Spalten des freien Bandes — ausblendbar, verschiebbar und im
   Spalten-Editor sichtbar, per Vorgabe eingeblendet. Nur die Cover-Spalte
   bleibt fest. Wer beide ausblendet, verliert den sichtbaren Weg zum
   Verstecken einer Release und zum Kaufweg; das Kopfzeilen-Popover holt beide
   zurück. Ein Layout aus der Zeit vor dieser Änderung behält sie sichtbar; ein
   Layout, das sie nie erwähnt hat, startet ohne sie. Test:
   `nr_39_the_column_editor_lists_status_and_link_and_hides_them`.

5. **CONC-16** bleibt **unverändert** gültig — Source ist weiterhin per Vorgabe
   aus. Nicht anfassen. (Dass Source jetzt zusätzlich **sortierbar** ist, ändert
   an CONC-16 nichts; es steht in CONC-17.)

6. **CONC-17** [active] neu, in Abschnitt AE: die Concerts-Vorgabespalten sind
   `Artist · Date · City · Distance · Tickets` sichtbar, `Venue` und `Source`
   aus. Sortierbar sind Date, Artist, City, Venue, Distance und Source; die
   Tickets-Spalte trägt keinen Sorter, weil ihre Zelle ein Knopf ist. Die
   Umstellung verwirft **einmalig** gespeicherte Concerts-Layouts (Migration
   v75); gespeicherte Spaltenbreiten bleiben erhalten. Tests:
   `the_default_concert_layout_leads_with_the_artist_and_hides_venue_and_source`,
   `every_sortable_concerts_header_orders_its_own_column`,
   `only_the_ticket_header_carries_no_sorter`,
   `v75_drops_the_stored_concerts_column_layout_and_keeps_the_widths`.

### Konvention

Neue Regeln werden zuerst als `[planned]` mit
`<!-- REVIEW: rule proposal -->` eingetragen und **im selben Commit** auf
`[active]` gezogen, sobald der benannte Test grün ist. Ersetzte Regeln behalten
ihren Text und bekommen nur den Marker.

### Nachweis

```
scripts/check-display-tests.sh --rule-named > $SCRATCH/r1.log 2>&1
grep -E "^test result|running [0-9]+ tests|FAILED|panicked" $SCRATCH/r1.log
```

**Der Filter allein ist kein Beleg.** `--rule-named` bildet Regel-IDs nicht auf
Tests ab; er sammelt alle `#[ignore]`-Tests, deren Name mit
`<präfix>_<zahl>_` beginnt, und fährt sie. Eine Regel ohne Test fällt ihm nicht
auf. Führe den Nachweis deshalb zusätzlich per Hand: für jede in R-1 genannte
ID und jeden genannten Testnamen

```
grep -rn "fn <testname>" --include='*.rs' crates/
```

und halte das Ergebnis im Bericht fest. Alle oben genannten Testnamen müssen
existieren; findest du einen nicht, hat der zuständige Strang ihn anders
benannt — dann korrigierst du die **Regel**, nicht den Test.

---

## Aufgabe R-2 — Post-Merge-Querprüfungen und vollständiger Durchlauf

**Ziel:** Zeigen, dass A, B und C zusammen funktionieren. Jede der folgenden
Prüfungen liest oder ändert eine Datei, die der jeweilige Strang **nicht**
besaß — kein Strang konnte über seine Besitzgrenze hinweg verifizieren.

### Die sechs Querprüfungen

1. **`bind_view_column_keys` panickt nicht** (`ui/table_columns/registry.rs`,
   war Besitz A). Nach C-3 müssen alle sieben Releases-Spalten binden: Cover
   ohne ID als führender Pin, die anderen sechs mit ID. Prüfung: einen
   Releases-Display-Test fahren, der die Ansicht **baut** —
   `nr_39_the_column_editor_lists_status_and_link_and_hides_them` —, und zwar
   auf dem zusammengeführten Stand. Ein Panic hier liest sich als
   `invalid … column binding: …` oder
   `pinned column must not expose an editable id`.

2. **`sort_fallback` bleibt korrekt** (`registry.rs`, war Besitz A). Grün:
   `hiding_primary_sort_chooses_first_visible_sortable_free_column`
   (`registry.rs:597`) und
   `style_10_hiding_the_sorted_column_keeps_a_visible_sort_indicator`
   (`registry.rs:612`) — mit dem neuen `ReleaseColumn`-Pinning **und** dem
   extrahierten Ein-Pfeil-Helfer gleichzeitig geladen. `Status` und `Buy` sind
   jetzt frei und sichtbar, tragen aber keinen Sorter; `sort_fallback` muss sie
   überspringen.

3. **Die Musikbibliothek hat keinen Kollateralschaden** (`ui/track_list/**`,
   `ui/style/**`, war Besitz A). Strang A hat `track_list_header_style` seine
   Konstanten und Funktionen abgenommen. Grün müssen sein:
   - `inactive_sort_columns_render_no_arrow` — **die Pixelmessung**, der harte
     Beleg, dass der Umzug wirkungsgleich war
   - `marking_targets_only_the_track_table_root`
   - `mapped_column_title_uses_the_subtle_foreground_alpha`
   - `header_style_is_subtle_and_scoped_away_from_song_cells`
   - `column_headers_update_sort_state_and_reload_once`
   - `sorting_a_new_column_replaces_the_previous_sort_key`
   - `contrast_3_secondary_surfaces_use_verified_level` (`ui/style/theme.rs:515`)
     — liest `track_list_header_style::css()` und sucht darin `> header label`;
     bricht, wenn die Extraktion zu viel mitgenommen hat

4. **Ein Pfeil in beiden Tabellen zugleich.**
   `two_concert_sorts_leave_one_indicator` (aus B) und
   `two_release_sorts_leave_one_indicator` (aus C) in **einem** Lauf. Jeder
   Strang konnte nur seinen eigenen zeigen.

5. **Die UX-Regeln** — Aufgabe R-1 vollständig, plus der Handprüfung oben.

6. **Voller Lauf** — siehe unten. Die Teilläufe der Stränge sind **kein**
   Ersatz: eine grüne Bilanzzeile aus einem Lauf, in dem eine Suite gar nicht
   startete, ist in diesem Repo schon einmal als Beleg durchgegangen.

### Der vollständige Durchlauf

```
cargo test -p reprise-core   > $SCRATCH/core.log    2>&1
cargo test -p reprise-view   > $SCRATCH/view.log    2>&1
cargo test -p reprise-gnome  > $SCRATCH/gnome.log   2>&1
scripts/check-display-tests.sh > $SCRATCH/display.log 2>&1
```

Auswertung per

```
grep -E "^test result|running [0-9]+ tests|FAILED|panicked" $SCRATCH/*.log
```

nicht per `cat`. **Vier** Logdateien müssen entstanden sein und **vier** Suiten
müssen tatsächlich gelaufen sein — prüfe je Datei die Zeile `running N tests`
mit N > 0, bevor du eine grüne Bilanzzeile als Beleg nimmst.

Rote Display-Tests gegen den bekannten `dev`-Stand halten, bevor du sie dieser
Arbeit zuschreibst: `dev` hat bekannte rote Display-Tests, und ein einzeln roter
Test im Rudel ist kein Beleg für einen Fehler.

### Zum Abschluss

`phase: shipped` in den Frontmattern von Mutterplan und allen vier
Strangdateien, sobald alles grün ist.

---

## Randfälle

**RF-R1 — Ersetzte Regeln sind Protokoll, kein Ballast.** `NR-25`
(`[replaced by NR-31]`) und `NR-31` (`[replaced by NR-33]`) beschreiben beide
noch die „fixed trailing action column". Sie **bleiben wörtlich stehen**.
Ersetzte Regeln dokumentieren, was einmal galt; ein Sweep über alle Vorkommen
des alten Verhaltens würde genau diesen Zweck zerstören. Nur **aktive** Regeln
werden nachgezogen: STYLE-10 (→ STYLE-13), NR-30, NR-33.

**RF-R2 — Abschnittskonflikt mit fremder laufender Arbeit.** Strang 2 von
`updates-concerts-releases-rework` schreibt in Abschnitt R (NR-34…NR-38, NR-21a,
Statusmarker auf NR-5b/10a/21/22/23) und **eine** Zeile in Abschnitt AE
(Statusmarker auf CONC-7). Du schreibst in Abschnitt R (NR-30, NR-33, neu
NR-39), Abschnitt AE (neu CONC-17) und Abschnitt S (STYLE-10 → STYLE-13). Die
IDs überschneiden sich nicht — NR-39 ist genau deshalb gewählt —, aber ein
Textkonflikt in denselben Abschnitten ist wahrscheinlich. Auflösung: **beide
Seiten übernehmen**, Regeln sind additiv. Nach der Auflösung prüfen, dass keine
ID doppelt vorkommt:

```
grep -oE '^\- \*\*[A-Z]+-[0-9]+[a-z]?\*\*' docs/ux-rules.md | sort | uniq -d
```

Die Ausgabe muss leer sein.

**RF-R3 — `--rule-named` ist ein Lauf-Filter, keine Vollständigkeitsprüfung.**
Siehe den Nachweis in R-1. Eine Regel ohne Test fällt dem Skript nicht auf; nur
die Handprüfung per `grep` zeigt es.

**RF-R4 — Ein grüner Lauf ohne Suite ist kein Beleg.** `running 0 tests` endet
ebenfalls mit `test result: ok`. Das ist in diesem Repo schon einmal als Beleg
durchgegangen. Je Logdatei die Zeile `running N tests` lesen.

**RF-R5 — Ein freier Füller, der nicht füllen kann.** Nach C-3 kann der Nutzer
alle Textspalten von Releases ausblenden; `filler_for` wählt dann die erste
sichtbare freie Spalte — im Extremfall `Status` oder `Buy`, beide mit fester
Breite und `resizable(false)`. Die Tabelle expandiert dann eine Aktionsspalte.
Kein Absturz, aber hässlich. **Außerhalb des Umfangs dieser Spec.** Nicht
reparieren, nicht als neue Regel schreiben — nur im Abschlussbericht als
bekannter Rand benennen, damit es beim nächsten Bericht nicht als neuer Fehler
gilt.

---

## Testdisziplin

**Unit** (kein Display, im Rudel unkritisch):

```
cargo test -p reprise-core   <filter>
cargo test -p reprise-view   <filter>
cargo test -p reprise-gnome  <filter>
```

Fallen, die in diesem Repo schon Zeit gekostet haben und hier gelten:

- `-p reprise-gnome --lib` findet **nichts** — das Paket hat kein `lib`-Target
  unter diesem Namen. Immer ohne `--lib` filtern.
- `--exact` in Kombination mit einem Modulpfad läuft ins Leere. Filter als
  Teilstring angeben.
- Die Ergebniszeile allein ist kein Beleg: `running 0 tests` endet ebenfalls mit
  `test result: ok`. Nach jedem Lauf die Zeile `running N tests` prüfen und
  gegen die erwartete Zahl halten.
- Ausgabe nach `$SCRATCH/<name>.log` umleiten und per `grep`/`wc` auswerten,
  nicht ins Terminal spülen. Auswertungsmuster:
  `grep -E "^test result|running [0-9]+ tests|FAILED|panicked" $SCRATCH/<name>.log`

**Display**:

```
scripts/check-display-tests.sh
scripts/check-display-tests.sh --rule-named        # nur die regelbenannten
```

Das Skript zieht alle `#[ignore]`-Tests aus `reprise-gnome`, startet
`dbus-run-session` + `xvfb-run` je Worker mit isolierten XDG-Roots,
`GSK_RENDERER=cairo`, `GDK_BACKEND=x11`, leerem `WAYLAND_DISPLAY`. Neue
display-gebundene Tests tragen deshalb zwingend:

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
```

und beginnen mit
`let _main_context = crate::ui::test_main_context::lock_main_context();`
gefolgt von `gtk4::init().unwrap();`. Ohne den Lock sind sie im Rudel flaky.

Ein einzeln roter Display-Test in einem Rudel-Lauf ist **kein** Beleg für einen
Fehler. Bei Rot: den einzelnen Test isoliert erneut fahren

```
xvfb-run -a cargo test -p reprise-gnome <name> -- --ignored --nocapture
```

und erst dann urteilen. Ebenso gilt: `dev` hat bekannte rote Display-Tests — Rot
in einer fremden Datei ist nicht die Schuld dieser Arbeit.
