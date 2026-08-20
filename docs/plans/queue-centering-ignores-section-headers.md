---
slug: queue-centering-ignores-section-headers
worktree: /home/marvin/Projects/reprise-queue-centering-ignores-section-headers
branch: feature/queue-centering-ignores-section-headers
phase: shipped
codex_session:
created: 2026-08-19
---
# Der Sprungpfad rechnet ohne Sektionsköpfe

Der Wiederherstellpfad zählt die Kopfzeilen seit #576 mit, der Sprungpfad nicht.
Dieser Plan zieht die zweite Hälfte nach und löscht dabei die letzte Kopie des
kopflosen Listenmodells.

**Alle Zeilenangaben gegen `origin/dev` = `45e65480e9` (nach #576).** Der
geteilte Hauptcheckout steht auf `be5f014d3b` und ist damit älter als der Umbau,
den dieser Plan fortsetzt — dort nachzulesen führt in die Irre. `git show
origin/dev:<pfad>`.

## Was gilt

`RevealMotion::Glide` ist der Titelwechsel: die Liste steht, kein Modelltausch,
keine Anker-Wiederherstellung durch GTK. Der Pfad ist

```
track_reveal::reveal_position  (track_reveal.rs:196)
  → scroll_center::centered_scroll_target
      → ListGeometry::settled_row_height          ← nur Zeilenhöhe
      → scroll_center::centered_scroll_value_with_height
            content_height = n_rows * row_height
            target = (position + 0.5) * row_height - page/2
```

Kopfhöhen kommen darin nicht vor. Der Ankerpfad macht es seit #576 anders:
`centered_scroll_restore::centered_anchor` geht über
`list_geometry_layout::ListLayout::row_top`, und `headers_above` zählt die
Kopfzeilen oberhalb der Zielzeile mit.

`ListLayout` weiß das über sich selbst. Sein Doc-Kommentar
(`list_geometry_layout.rs:48-50`) benennt die Gegenseite ausdrücklich:

> Deliberately GTK-free. `scroll_center::centered_scroll_value_with_height`
> still models a list as rows only; it centres rather than anchors and is
> tracked separately — it is the last remaining copy of this model.

Dieser Plan ist das „tracked separately".

**Der Fehler ist nicht theoretisch.** In der Warteschlange löst
`current_track_selection::reveal_policy` bei `AutomaticAdvance` und
`ExplicitTransport` eine Zentrierung aus, und
`visible_position_for_track_in_source` liefert die Position über
`queue_position`. Der Glide-Pfad feuert dort also auf einer sektionierten Liste.

## Korrektur am Befund

Der TODO schreibt: „je weiter unten der Titel steht, desto weiter [daneben]".
**Das stimmt nur, solange die Zeile noch Kopfzeilen passiert.**
`reprise-view/src/queue.rs:275-311` (`compose_virtual`) legt höchstens **drei**
Sektionen an — `NowPlaying`, `PlayNext`, `UpNext` — und jede höchstens einmal.

Der Versatz ist damit `headers_above(position) × header_height` und nach oben
durch `3 × header_height` **gedeckelt**. Ab der ersten Zeile hinter dem letzten
Sektionsanfang ist er konstant, nicht wachsend. Bei der in den Tests geführten
Geometrie (Zeile 34 px, Kopf 36 px, Viewport 249 px) sind das höchstens 108 px —
knapp ein halber Bildschirm, aber ein fester Betrag.

Diese Zahl ist aus dem Code abgeleitet, nicht gemessen. Task 1 misst sie.

## Warum kein bestehender Test das gefunden hat

Zwei Gründe, beide am Quelltext belegt, und beide gehören in den Plan, weil sie
bestimmen, wie Task 1 aussehen muss.

**1. Das Glide-Orakel ist die Produktionsfunktion selbst.**
`current_track_selection_glide_tests::target_for` (Zeile 40-47) ruft
`scroll_center::centered_scroll_target` und misst den Gleitflug dagegen. Der
Test prüft „der Gleitflug erreicht, was die Rechnung sagt" — nicht „die
Rechnung stimmt". Er wäre auch bei drei Kopfhöhen Versatz grün geblieben und
bleibt es nach dem Umbau, weil er neu rechnet. **Er kann der Kontrollarm nicht
sein.**

**2. Keine Glide-Testliste hat Sektionen.**
`current_track_selection_glide_tests.rs:24`, `glide_reload_display_tests.rs:36`
und `source_switch_centering_display_tests.rs:60` bauen alle mit
`queue_sections::QueueViewModel::default`, also ohne Sektionen. Der sektionierte
Sprung ist nirgends abgedeckt.

## Aufgaben

### Task 1 — Den Versatz messen, bevor etwas umgebaut wird

Ein Display-Test in der Warteschlange mit allen drei Sektionen, der einen
Titelwechsel auslöst (also `reveal_position(..., RevealMotion::Glide)`, **nicht**
den Wiederherstellpfad).

**Der Sollwert kommt aus der Widget-Geometrie, nicht aus unserer Arithmetik.**
Der Test misst die tatsächliche Lage der Zielzeile im Viewport — `compute_bounds`
des Zeilenwidgets gegen die `ScrolledWindow` — und prüft, dass die Mitte der
Zeile auf der Mitte des Viewports liegt. Das ist die Zusage, um die es geht
(„der laufende Titel steht in der Mitte"), und sie ist von `centered_value`
*und* von `row_top` unabhängig. `compute_bounds` ist in den Display-Tests des
Repos etabliert; ein Helfer, der zu einer Position das Zeilenwidget findet,
existiert dagegen nicht — die Zellen hängen pro Spalte in eigenen `ListView`s.
Das ist der Handarbeitsanteil dieser Aufgabe.

**Rückfall, falls die Virtualisierung das Widget nicht hergibt:** der Test
rechnet den Sollwert aus `ListLayout::row_top` und **vermerkt im
Testkommentar, dass die Widget-Messung nicht möglich war und warum**. Der
Kontrollarm verliert damit seine Unabhängigkeit von der Geometriequelle, behält
sie aber gegenüber `centered_value` — und das ist die Stelle, die dieser Plan
verändert. `row_top` ist seit #576 durch eigene Unit-Tests gedeckt. Nicht
zulässig ist der dritte Weg: ein Orakel, das wieder die Produktionsrechnung
ist.

Der Test ist zunächst **rot** und benennt den Betrag: erwartet wird eine
Abweichung von genau `headers_above(position) × header_height`.

Warum zuerst: #576 hat teuer gelernt, dass ein Endwerttest die Bewegung nicht
sieht und dass eine Vorentscheidung ohne Kontrollarm die falsche Hälfte des
Problems repariert.

**Akzeptanz:** ein Test, der den Versatz als Zahl nennt und aus dem
Fehlerprotokoll ablesbar macht, wie viele Köpfe er enthält.

### Task 2 — Die Zentrierung zieht in `ListLayout` ein

Neu in `list_geometry_layout.rs`:

```rust
pub(in crate::ui) fn centered_value(
    &self,
    position: u32,
    n_rows: usize,
    page_size: f64,
) -> Option<f64>
```

Sie übernimmt **alle vier** Ablehnungen der alten Funktion unverändert:
`n_rows == 0`, `page_size <= 0.0`, Inhalt passt vollständig in den Viewport,
und `position >= n_rows`. Die letzte ist laut Doc-Kommentar in `scroll_center.rs`
tragend, nicht defensiv — ein veralteter Index klemmt sich sonst in den
gültigen Bereich und liest sich wie eine richtige Antwort.

Der Zielwert ist der **exakte Mittelwert**,
`row_top(position) + row_height/2 - page_size/2`, geklemmt auf
`[0, max_scroll(n_rows, page_size)]`. Ungerundet: der Sprungpfad läuft auf
einer stehenden Liste, schreibt über `ScrollGlide` direkt ins Adjustment und
ruft kein `scroll_to`. Es gibt also keine von GTK übernommene Position, die der
Allokationsdurchlauf zurückschreiben könnte — die Zeilenkante, auf die #576 den
Ankerpfad zwingen musste, ist hier eine Zwangsmaßnahme ohne Zwang.

`content_height` und `max_scroll` gibt es in `ListLayout` bereits und sie
rechnen die Köpfe mit.

**Akzeptanz:** Unit-Tests. Für `rows_only` liefert `centered_value` exakt die
Werte der bisherigen `centered_scroll_value_with_height`-Tests (405.0, beide
Kanten, alle drei `None`-Fälle, der Stale-Index-Fall). Für `sectioned` liegt das
Ergebnis um genau `headers_above(position) × header_height` höher.

### Task 3 — Der Sprungpfad benutzt sie, die alte Kopie fällt

`scroll_center::centered_scroll_target` nimmt statt einer `RowHeight` ein
`ListLayout` entgegen. Die beiden Aufrufstellen:

- `track_reveal.rs:196` (Glide) baut es über
  `track_list_geometry::layout(shared, None, n_rows)` — dieselbe Quelle, aus der
  der Ankerpfad seine Geometrie nimmt, inklusive der Live-Allokationsprüfung
  (`layout_for_live_allocation`), die bei widerlegter Kopfhöhe auf
  `rows_only` zurückfällt.
- `radio/radio_reveal.rs:182` übergibt `ListLayout::rows_only(row_height)`. Die
  Senderliste ist flach; das ist keine Verhaltensänderung, sondern dieselbe
  Rechnung in der neuen Form.

Danach sind `centered_scroll_value_with_height` und die `#[cfg(test)]`-Hülle
`centered_scroll_value` ohne Aufrufer und werden **gelöscht**; der
Doc-Kommentar in `list_geometry_layout.rs:48-50` verliert seinen Gegenstand und
wird entsprechend umgeschrieben.

**Zu beachten:** `track_list_geometry::layout` gibt `Option` zurück. Liefert es
`None` (keine Zeilenhöhe, kein Adjustment), muss der Glide-Zweig sich genau wie
heute verhalten — `placed_provisionally = false`, also Wiederholung über die
`attempts`-Schleife, kein `ensure_visible` vor der Zeit.

**Akzeptanz:** Der Test aus Task 1 wird grün. Die bestehenden Glide-Tests
(`current_track_selection_glide_tests.rs`, `glide_reload_display_tests.rs`)
bleiben grün, ohne dass eine Toleranz gelockert wird.

### Task 4 — Die Domäne des Testorakels festschreiben

`reload_restore::centered_track_scroll_target` ist seit #576 `#[cfg(test)]` und
das Orakel, gegen das `search_viewport_display_tests.rs` (vier Stellen) und
`source_switch_centering_display_tests.rs` den Wiederherstellpfad messen.

**Es bleibt reine Zeilenmathematik.** Das ist keine Nachlässigkeit, sondern
seine Existenzberechtigung: es ist die *unabhängige* Gegenrechnung zum
Wiederherstellpfad. Hebt man es auf `ListLayout`, teilt es die Geometriequelle
mit genau dem Pfad, den es prüfen soll, und wird tautologisch — dieselbe
Krankheit wie `target_for` bei NAV-10b.

Was fehlt, ist die Domäne. Alle fünf heutigen Aufrufstellen bauen flache Listen
(`QueueViewModel::default`); die Falle ist latent, nicht aktiv. Der
Doc-Kommentar (und, wenn es die Lesbarkeit trägt, der Name) sagt künftig
ausdrücklich: **gilt nur für Ansichten ohne Sektionsköpfe.** Der sektionierte
Fall wird durch den Kontrollarm aus Task 1 gedeckt.

**Akzeptanz:** Aus dem Quelltext ist ablesbar, für welche Ansichtsform das
Orakel gilt, und keine Testdatei benutzt es außerhalb davon.

### Task 5 — NAV-10b präzisieren

`docs/ux-rules.md`, Regel **NAV-10b** — keine neue Kennung. NAV-10b trägt die
Zusage bereits („Play from Stopped as well as explicit Previous/Next center the
new track", „Auto-advance centers only if…"), definiert „mittig" aber nirgends
für eine Liste mit Sektionsköpfen. Eine zweite Kennung würde eine Zusage auf
zwei Regeln aufteilen; die Datei konsolidiert erkennbar in die Gegenrichtung
(NAV-10a und NAV-13 sind beide durch NAV-10b *ersetzt*).

Der Zusatz hält zwei Dinge fest:

- Mittig heißt mittig im **Inhalt einschließlich der Sektionsköpfe**, nicht in
  einer gedachten kopflosen Zeilenfolge.
- Der Sprung trifft den exakten Mittelwert; der Wiederherstellpfad trifft die
  nächstgelegene Zeilenkante und weicht deshalb um bis zu eine halbe Zeile ab.
  Das ist eine benannte Toleranz, kein Fehler — und es steht in der Regel,
  damit es nicht später als einer gemeldet wird.

Die neuen Tests heißen weiter `nav_10b_…`, damit
`scripts/check-ux-traceability.sh` die Kennung wiederfindet.

**Akzeptanz:** Traceability-Gate grün, und die Regel nennt den Sektionsfall
ausdrücklich.

### Task 6 — Gegenmessung

Der Kontrollarm aus Task 1 läuft gegen den fertigen Baum und zeigt die Mitte,
nicht bloß „näher dran".

Dazu eine Mutationsprobe: im neuen `centered_value` `row_top(position)` durch
`f64::from(position) * self.row_height.pixels()` ersetzen — **genau ein
Vorkommen** — und belegen, dass Task 1 rot wird. Erst committen, dann mutieren:
die Rücknahme per `git checkout --` stellt HEAD wieder her und verschluckt
Uncommittetes wortlos und mit Exit 0.

## Nicht in diesem Plan

- **Die Senderliste.** `radio_reveal` ist flach; sie wechselt nur die Form der
  Rechnung, nicht ihr Ergebnis.
- **Die Wachstumsbehauptung des TODO.** Sie ist oben korrigiert und wird nicht
  als Verhalten nachgebaut.
- **Die übrigen offenen Befunde** (`stats-hide-more-top-artists-stutters`,
  `visuals-bars-fall-in-from-the-top-on-open`,
  `episode-covers-appear-seconds-after-start`,
  `radio-genre-chip-drops-the-country`,
  `library-doctor-out-of-date-rows-are-unreadable`) — eigene Pläne, keine
  gemeinsamen Dateien.

## Belege

Vor dem Landen:

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace`
- die betroffenen Display-Tests **einzeln** (`$SCRATCH/dt.sh <voller::pfad>`),
  nicht `scripts/check-merge-readiness.sh` — das Sammel-Gate läuft nie durch
- die Mutationsprobe aus Task 6

Die **volle** Display-Suite ist nicht gefordert: sie ist im Rudel bekannt flaky
und für die Abnahme nicht erforderlich. Wer sie freiwillig fährt, fährt einen
einzelnen roten Test einzeln nach, bevor er ihn als Regression zählt.

## Parallelität

**Ein Strang. Der Plan wird nicht geschnitten.**

Task 1 (Testdatei) und Task 2 (`list_geometry_layout.rs`) wären dateimäßig
disjunkt — aber Task 1 ist bis Task 3 rot, Task 3 ist der einzige Aufrufer von
Task 2 und löscht die Altfunktion in derselben Datei, und Task 5s
Regel-Kennung trägt genau der Test aus Task 1. Zwei Worktrees für zwanzig
Zeilen kaufen keine Wanduhr, sie kosten einen Merge.

**Reihenfolge:** 1 → 2 → 3 → 4 → 5 → 6.

**Dateibesitz dieses Strangs:**

```
crates/reprise-gnome/src/ui/list_geometry_layout.rs
crates/reprise-gnome/src/ui/scroll_center.rs
crates/reprise-gnome/src/ui/track_list/track_reveal.rs
crates/reprise-gnome/src/ui/track_list/track_list_geometry.rs
crates/reprise-gnome/src/ui/track_list/reload_restore.rs
crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs
crates/reprise-gnome/src/ui/track_list/source_switch_centering_display_tests.rs
crates/reprise-gnome/src/ui/track_list/current_track_selection_glide_tests.rs
crates/reprise-gnome/src/ui/track_list/glide_reload_display_tests.rs
crates/reprise-gnome/src/ui/radio/radio_reveal.rs   (nur die eine Aufrufstelle)
docs/ux-rules.md
```

`radio_reveal.rs` steht hier, obwohl sich dort nur die Form der Rechnung
ändert — sonst stolpert die Disjunktheitsprüfung des Code-Phase über eine
Datei, die kein Strang beansprucht hat.

**Parallele Nachbarn mit disjunkten Dateien** (eigene Pläne, eigene Worktrees,
beliebige Reihenfolge, kein gemeinsamer Merge-Zwang):
`stats-hide-more-top-artists-stutters` (`ui/stats/*`),
`visuals-bars-fall-in-from-the-top-on-open` (`reprise-core/visuals`,
`playback/cava`), `episode-covers-appear-seconds-after-start` (`ui/podcasts/*`),
`radio-genre-chip-drops-the-country` (`ui/radio/radio_chips.rs`,
`ui/strings_radio.rs` — berührt `radio_reveal.rs` **nicht**),
`library-doctor-out-of-date-rows-are-unreadable` (`ui/library_doctor/*`).

**Post-Merge-Querprüfungen:** keine — es gibt keinen zweiten Strang, dessen
Ergebnis dieser lesen müsste.
