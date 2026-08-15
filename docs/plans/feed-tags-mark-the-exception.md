---
slug: feed-tags-mark-the-exception
worktree: /home/marvin/Projects/reprise-feed-tags-mark-the-exception
branch: feature/feed-tags-mark-the-exception
phase: coded
codex_session:
created: 2026-08-15
---
# Der Feed markiert die Abweichung, die Tabelle vergleicht

**Ziel.** Im Updates-Popover trägt eine Concert-Zeile eine Pille nur dann,
wenn ihr Ticketstand von der Erwartung abweicht: `Off sale` und `Unknown`
bekommen einen Tag, `On sale` bleibt ohne. Wo beide Flächen einen Tag zeigen,
zeigen sie dieselbe Pille — die drei Töne des Popovers sind zeichengleich mit
den drei Klassen der Concerts-Tabelle. Danach ist die letzte offene
Entwurfsfrage des Updates/Concerts/Releases-Reworks entschieden und dessen
Anforderung R4 per datiertem Nachtrag geschlossen.

**Herkunft.** Nachlese zu `docs/plans/updates-concerts-releases-rework.md`
(`phase: shipped`). Dessen §9, Prüfung 2, hat die Abweichung gemessen und
ausdrücklich als „offene Entwurfsfrage, kein Regelverstoß" stehen lassen;
§9 „Was offen bleibt" nennt sie und die überholte Kachel-Hälfte von R4 als die
beiden verbliebenen Punkte. Beide werden hier erledigt.

> **Lesestand.** Alle Aussagen unten sind gegen `origin/dev` @ `b6be7cdc61`
> geprüft. Der geteilte Hauptcheckout steht auf einem fremden Stand
> (`be5f014d3b`) und taugt nicht als Basis: lesen per
> `git show origin/dev:<pfad>`, arbeiten in einem eigenen Worktree.
> Wo eine Zeilennummer nicht mehr passt, gilt der genannte Bezeichner —
> nachziehen, nicht raten.

> **Sprache.** Dieser Plan ist deutsch, wie der Mutterplan. `AGENTS.md`
> („NON-NEGOTIABLE safety rules") verlangt Englisch für Code, Kommentare,
> UI-Strings und Commit-Botschaften. Daraus folgt für die Umsetzung: **jede**
> Zeile Code, jeder Kommentar, jeder Regeltext in `docs/ux-rules.md`, jede
> Commit-Botschaft und jeder Branch-Name ist englisch. Deutsch bleibt allein
> dieses Planungsdokument und der Nachtrag im ebenfalls deutschen Mutterplan.

> **Grill-Protokoll (15.08.2026).** Sechs Entscheidungen, alle vom Eigentümer
> bestätigt: (1) **vier** Tag-Töne statt drei — Spiegelung und unangetasteter
> Upcoming-Chip schlagen die Ersparnis einer Enum-Variante; (2) **NR-39** als
> eigenständige Regel, keine NR-36-Nachfolge, weil sonst NR-36s Test auf eine
> Regel umgehängt würde, die er nicht misst; (3) das Kriterium des
> Anzeigetests steht **vorab** fest (Geometrie + drei Farbstichproben, ±1),
> kein Byte-Vergleich und keine Laufzeitentscheidung; (4) das Sondenskript
> wird getrackt, die **Bilder nicht** — die Zahlen tragen das Manifest;
> (5) **ein** Strang; (6) die lokale Gate-Liste ist die berührte Teilmenge
> plus die Brandstellen der letzten Runde, nicht die volle Kette.

---

## 0. Ausgangslage, belegt

**0.1 Die Tabelle nennt alle drei Werte.**
`ticket_presentation` in
`crates/reprise-gnome/src/ui/concerts/concerts_status_cells.rs` gibt für jeden
`TicketAvailability` ein Wort und eine Klasse zurück: `OnSale` →
`strings::CONCERTS_ON_SALE` / `"on-sale"` ohne Tooltip, `OffSale` →
`strings::CONCERTS_OFF_SALE` / `"off-sale"` mit
`strings::CONCERTS_OFF_SALE_TOOLTIP`, `Unknown` → `strings::CONCERTS_UNKNOWN` /
`"unknown"` ohne Tooltip. `ticket_column` hängt zusätzlich die Basisklasse
`reprise-concert-ticket-tag` an und tauscht die drei Modifikatorklassen beim
`bind`.

**0.2 Das Popover taggt nur `OffSale`.**
`delta_presentations` in
`crates/reprise-gnome/src/ui/updates/concerts_section.rs`:

```rust
tag: (row.availability == TicketAvailability::OffSale).then(|| feed_row::Tag {
    text: strings::text(strings::CONCERTS_OFF_SALE),
    tone: feed_row::TagTone::Neutral,
}),
```

`OnSale` und `Unknown` erzeugen kein `Tag`, also kein Label.

**0.3 Zwei Töne, zwei Klassen.**
`enum TagTone { Accent, Neutral }` in
`crates/reprise-gnome/src/ui/updates/feed_row.rs`; `build` bildet in seinem
`match` auf `"updates-tag-accent"` bzw. `"updates-tag-neutral"` ab und hängt
die Basisklasse `updates-tag` an.

**0.4 Die einzige echte optische Abweichung ist die Füllung.**
`.updates-tag-neutral` (in `crate::ui::updates::css::css`) und
`.reprise-concert-ticket-tag.off-sale` (in `crate::ui::concerts::css::css`)
haben denselben Rahmen `alpha(@window_fg_color, 0.20)` und dieselbe Textfarbe
`@reprise_secondary_fg_color`; die Tabelle füllt mit
`alpha(@window_fg_color, 0.08)`, das Popover mit `transparent`.
`.updates-tag-accent` und `.on-sale` stimmen bereits in Rahmen, Farbe und
Füllung überein. Das Grundmaß der Pille (`border-radius: 999px;
padding: 2px 8px; font-size: 11px`) ist auf beiden Flächen gleich.
`.unknown` ist der leiseste Ton: Rahmen `alpha(@window_fg_color, 0.12)`,
Textfarbe `@reprise_hint_fg_color`, keine Füllung. Diesen Ton hat das Popover
heute gar nicht.

**0.5 Ein zweiter Nutzer des neutralen Tons.**
In `release_row.rs` wählt der Chip aus `chip_presentation`:
`ChipPresentation::Upcoming(copy)` → `TagTone::Neutral`,
`ChipPresentation::Released` → `TagTone::Accent`. Der Upcoming-Chip hängt also
am selben Ton wie heute der Concerts-Tag.

**0.6 Kein Bild-Widget in der Concerts-Tabelle.**
`git grep -nE 'Picture|gtk::Image|Avatar|Texture|Pixbuf' origin/dev --
crates/reprise-gnome/src/ui/concerts/` liefert null Treffer; `ConcertColumn`
(in `reprise-view`) kennt keine Cover-Spalte. Der Mutterplan hält dasselbe in
seiner Prüfung 5 fest.

**0.7 Regellage.** NR-36 (`docs/ux-rules.md`, Abschnitt R) regelt, dass der
Trailing-Slot Status-Tag und Dismiss-Knopf dauerhaft nebeneinander hält, dass
der Knopf ein Geschwister der Aktivierungsfläche ist und beide per Tab
erreichbar sind — **nicht**, welche Werte einen Tag bekommen. CONC-12
(Abschnitt AE, `[active] [core]`) regelt nur die Herkunft der Werte
(Ticketmaster-/Bandsintown-Mapping, nie „Sold out"). R4 steht nicht in
`docs/ux-rules.md`, sondern als Anforderung in der Post-Merge-Querprüfung 5
des Mutterplans.

**0.8 Wie die bestehende Tag-Logik heute getestet wird.**
`git grep -n 'delta_presentations\|ticket_presentation' origin/dev -- crates/`
findet genau zwei Testorte:
`concerts_status_cells.rs::tests::ticket_status_copy_is_source_faithful` prüft
Wort, Klasse und Tooltip aller drei Tabellenwerte;
`concerts_section.rs::tests::concert_fields_and_target_follow_the_shared_row_contract`
prüft für ein `OffSale`-Event unter anderem `tag.text == "Off sale"`. Für
`OnSale` und `Unknown` gibt es im Popover heute **keinen** Test — die heutige
Anzeige ist unbelegt, nicht bewusst.

---

## 1. Beschlüsse

### Beschluss 1 — Der Feed taggt die Abweichung, die Tabelle vergleicht

`Off sale` und `Unknown` bekommen im Popover einen Tag, `On sale` nicht.

Begründung: die beiden Flächen beantworten verschiedene Fragen. Die Tabelle
ist eine **Vergleichsfläche** — dort steht eine Spalte, und eine Spalte, die
bei jeder dritten Zeile leer bleibt, liest sich als fehlender Wert, nicht als
guter Zustand. Deshalb füllt sie alle drei Werte. Das Popover ist ein
**Ereignis-Feed**: die Pille markiert dort, was von der Erwartung abweicht.
Bei einem frisch angekündigten Konzert *ist* `On sale` die Erwartung; die
Pille trüge dann keine Information und würde nur die Zeile verbreitern. Die
Gegenprobe gilt genauso: `Unknown` ist keine Beruhigung, sondern die Warnung,
dass der Zustand nicht bekannt ist — und die gehört in den Feed.

Sichtbare Folge: `Unknown` **gewinnt** eine Pille, die es heute nicht hat;
`Off sale` behält seine; `On sale` bleibt wie heute ohne. Der Feed wird also
nicht ärmer, sondern ehrlicher.

### Beschluss 2 — Wo die Regel wohnt: neue Regel NR-39, kein NR-36-Nachfolger

Die Regel wird **NR-39 `[active] [gtk]`**, angehängt am Ende von Abschnitt R
(„New releases"), nach dem heute letzten Eintrag NR-21a. Sie löst keine Regel
ab.

Begründet aus der Hausordnung von `docs/ux-rules.md` („Process rules"):

1. **„IDs sind append-only … Ändert sich die Bedeutung, ersetzt eine neue
   (Unter-)Regel die alte."** NR-36s Bedeutung ändert sich nicht: der
   Trailing-Slot hält weiterhin Tag und Dismiss-Knopf dauerhaft nebeneinander,
   der Knopf bleibt Geschwister der Aktivierungsfläche. Eine Nachfolge wäre
   die Behauptung einer Bedeutungsänderung, die es nicht gibt.
2. **„Tests gegen abgelöste Regeln werden im selben Commit auf die neue Regel
   umgehängt."** NR-36s Test heißt
   `nr_36_dismissing_a_row_never_opens_its_link` und misst genau das:
   Dismiss öffnet keinen Link. Auf eine Regel über die Tag-*Auswahl* umgehängt,
   wäre er falsch benannt — die Traceability-Prüfung bliebe grün und der
   Nachweis wäre trotzdem kaputt. Das ist der teuerste Preis der
   Nachfolge-Variante und der Hauptgrund gegen sie.
3. **Warum NR und nicht CONC.** Es gibt im Repo genau die Präzedenz, die hier
   zählt: **NR-35 „replaces CONC-7"** hat die Regel über den
   Concerts-Abschnitt des Popovers aus Abschnitt AE nach Abschnitt R geholt.
   NR-38 spricht ebenfalls über beide Feeds und nennt Concerts beim Namen. Die
   Popover-Fläche wohnt also unter NR, auch wenn der Inhalt Concerts ist.
   CONC-12 ist zudem `[core]` und regelt die *Werteherkunft*; unsere Aussage
   ist `[gtk]` und regelt die *Darstellung*. Sie an CONC-12 zu hängen, würde
   zwei Ebenen in einer Regel mischen.
4. **Teststufe.** „Getestet wird auf der niedrigsten Stufe, die die Regel
   widerlegen kann." Die Auswahl geschieht in der reinen Funktion
   `delta_presentations` — also `[gtk]` als Ebene, aber ein anzeigefreier
   Test. Die Kennzeichnung `[gtk]` ist trotzdem korrekt: die Regel gilt für
   eine GTK-Fläche, und `check-ux-traceability.sh` liest die Ebene nur, um
   `[manual]` von den übrigen zu trennen.
5. **Testverweis.** Aktive Regeln tragen im Repo einen Verweis der Form
   ``Test: `name` (`pfad`)``, bei Inline-Modulen mit dem Zusatz
   `` `#[cfg(test)]` `` (so bei CONC-12 und CONC-15). NR-39 bekommt deshalb:
   ``Test: `nr_39_the_feed_tags_only_the_exception`
   (`ui/updates/concerts_section.rs`, `#[cfg(test)]`).``
6. **Einreihung am Abschnittsende**, nicht in numerischer Position: so sind in
   Abschnitt AE zuletzt CONC-4c, CONC-5b und CONC-11a **nach** CONC-16
   angehängt worden, und in Abschnitt R steht NR-21a heute nach NR-38.

Der Regeltext steht wörtlich in Aufgabe 1.

### Beschluss 3 — Die Töne spiegeln die Tabelle; der Upcoming-Chip bleibt, wie er ist

**Die Rechnung, offen hingelegt.** Gefordert sind drei Dinge:
(a) genau ein dritter Ton, (b) exakte 1:1-Spiegelung der drei Tabellenklassen,
(c) der Upcoming-Chip der Releases verändert sich optisch nicht. Diese drei
sind zusammen nicht erfüllbar. Der heutige `.updates-tag-neutral` unterscheidet
sich von `.off-sale` durch die Füllung und von `.unknown` durch Rahmenstärke
(0.20 gegen 0.12) und Textrolle (`@reprise_secondary_fg_color` gegen
`@reprise_hint_fg_color`) — er ist also **keine** der drei Tabellenklassen.
Wer bei drei Tönen bleibt, muss entweder den Upcoming-Chip füllen (verletzt c)
oder ihn leiser machen (verletzt c) oder `Unknown` im Popover nur ungefähr wie
in der Tabelle zeichnen (verletzt b).

**Entscheidung: (b) und (c) gewinnen, bezahlt mit einem vierten Ton.** Das Ziel
dieser Runde ist, dass die Pille auf beiden Flächen dieselbe ist; und der
Upcoming-Chip ist ausdrücklich unantastbar. Also:

| Fläche | Ton heute | Ton nachher | Klasse |
|---|---|---|---|
| Releases „Released" | `Accent` | `Accent` | `.updates-tag-accent` — unverändert, spiegelt `.on-sale` bereits heute |
| Releases „in N days" | `Neutral` | **`Neutral`** | `.updates-tag-neutral` — **Deklaration bleibt unangetastet** |
| Concerts `Off sale` | `Neutral` | `NeutralFilled` | `.updates-tag-neutral-filled` — neu, zeichengleich mit `.off-sale` |
| Concerts `Unknown` | kein Tag | `Quiet` | `.updates-tag-quiet` — neu, zeichengleich mit `.unknown` |
| Concerts `On sale` | kein Tag | kein Tag | — |

**Warum der Upcoming-Chip sein Aussehen behält:** er trägt weiterhin
`TagTone::Neutral`, und der CSS-Block `.updates-tag.updates-tag-neutral` wird
in dieser Runde **nicht angefasst** — kein Selektor, keine Deklaration, kein
Zeichen. Das ist Erhaltung per Konstruktion, nicht per Augenmaß, und deshalb
auch ohne Screenshot beweisbar: `git diff` zeigt an diesem Block keine
Änderung.

**Umbenennung wurde geprüft und verworfen.** Die Alternative wäre gewesen,
`Neutral` die Füllung zu geben (dann = `.off-sale`) und den heutigen
transparenten Ton unter dem neuen Namen `Quiet` weiterlaufen zu lassen, den
dann der Upcoming-Chip trägt. Das ergäbe drei Töne und einen unveränderten
Chip — aber `Unknown` im Popover wäre dann der Releases-Ton und nicht
`.unknown`, die Spiegelung wäre gebrochen. Kosten der Umbenennung obendrein:
ein repoweiter Sweep über `feed_row.rs` (`enum`, `match`), `release_row.rs`
(zwei `TagTone`-Nennungen), `concerts_section.rs` (eine), `updates/css.rs`
(Deklaration, die Selektorliste in `css_covers_every_new_release_class`, der
Eintrag in `contrast_1_text_classes_consume_roles_without_local_dimming`) —
neun Fundstellen, die alle nur eine Umbenennung sind und dabei genau die eine
Fläche gefährden, die nicht wackeln darf. Der vierte Ton kostet weniger und
riskiert nichts.

**Warum kein Tooltip am Popover-Tag.** Die Tabelle hängt an `Off sale` den
erklärenden `CONCERTS_OFF_SALE_TOOLTIP`. Im Popover bleibt der Tag ohne
Tooltip: `feed_row::build` setzt den Zeilentooltip („Opens {source}", NR-38)
auf die Zeilenwurzel, und ein Tooltip am Tag würde ihn genau über der Pille
verdrängen — die Zeile hörte dort auf, ihre Quelle zu nennen. TIP-3 ist
gewahrt, weil die Erklärung in der Concerts-Tabelle hoverfrei erreichbar
bleibt.

### Beschluss 4 — R4 wird per datiertem Nachtrag geschlossen, nicht umgeschrieben

Der Mutterplan ist ein abgeschlossenes Protokoll (`phase: shipped`). Sein
Anforderungstext R4 in der Post-Merge-Querprüfung 5 bleibt **wörtlich
unangetastet**; ihn nachträglich zu ändern fälschte den Nachweis. Statt dessen
kommt ein datierter Nachtrag ans Ende von Abschnitt 9. Er stellt fest: die
Kachel-Hälfte war per Konstruktion gegenstandslos, die Akzentmarken-Hälfte war
schon in Prüfung 5 erfüllt, die Tag-Hälfte ist erfüllt und wird durch diese
Runde verschärft. Eine neue normative Regel in `docs/ux-rules.md` für R4
entsteht **ausdrücklich nicht**. Wortlaut in Aufgabe 3.

---

## 2. Aufgaben

Jede Aufgabe ist einzeln abnehmbar: sie übersetzt, sie hält die Gates aus
Abschnitt 5 (mindestens `cargo fmt --check`, `cargo clippy --locked
--all-targets --workspace -- -D warnings`, `cargo test --locked --workspace`),
und sie schreibt einen Eintrag in `.superpowers/sdd/progress.md`, wie es die
Einträge der letzten Runde vormachen.

### Aufgabe 1 — `The feed tags the exception, in the table's tones`

**Dateien**
- `crates/reprise-gnome/src/ui/updates/feed_row.rs`
- `crates/reprise-gnome/src/ui/updates/css.rs`
- `crates/reprise-gnome/src/ui/updates/concerts_section.rs`
- `docs/ux-rules.md`

**Warum das ein Commit ist.** Die Hausordnung von `docs/ux-rules.md` sagt:
„A rule switches to `[active]` **in the same commit** that implements the
behavior or proves it with a test — never after the fact." Regel und
Verhalten dürfen also nicht auf zwei Commits verteilt werden. Zusätzlich
technisch erzwungen: neue `TagTone`-Varianten, die noch niemand konstruiert,
sind toter Code und lassen `clippy -D warnings` auflaufen — Enum, CSS und
Aufrufer müssen gemeinsam landen.

**Änderung 1 — `feed_row.rs`.** `enum TagTone` bekommt zwei Varianten und einen
erklärenden Doc-Kommentar (englisch), der festhält, dass `Neutral` der
Releases-eigene, ungefüllte Ton ist und `NeutralFilled`/`Quiet` die
Concerts-Tabelle spiegeln:

```rust
pub(super) enum TagTone {
    Accent,
    Neutral,
    NeutralFilled,
    Quiet,
}
```

Das `match` in `build` wird in eine eigene, anzeigefreie Funktion gezogen, damit
die Zuordnung ohne GTK prüfbar ist und die Tests denselben Namen lesen wie das
CSS:

```rust
pub(super) const fn tone_class(tone: TagTone) -> &'static str
```

`build` ruft `label.add_css_class(tone_class(tag.tone))` statt des Inline-Match.
Am Ende der Datei entsteht ein `#[cfg(test)] mod tests` mit
`every_tag_tone_maps_to_its_own_css_class` (vier Varianten, vier verschiedene
Klassennamen, jeder mit dem Präfix `updates-tag-`).

**Änderung 2 — `updates/css.rs`.** Nach dem bestehenden Block
`.updates-tag.updates-tag-neutral` (der **unverändert** bleibt) kommen zwei
Blöcke, zeichengleich zu den Deklarationen von
`.reprise-concert-ticket-tag.off-sale` und `.reprise-concert-ticket-tag.unknown`
in `crates/reprise-gnome/src/ui/concerts/css.rs`:

```
.updates-tag.updates-tag-neutral-filled {
    border: 1px solid alpha(@window_fg_color, 0.20);
    color: @reprise_secondary_fg_color;
    background-color: alpha(@window_fg_color, 0.08);
}
.updates-tag.updates-tag-quiet {
    border: 1px solid alpha(@window_fg_color, 0.12);
    color: @reprise_hint_fg_color;
    background-color: transparent;
}
```

(In der Datei in der dort üblichen Fortsetzungsschreibweise mit `\` am
Zeilenende.) Im Test `css_covers_every_new_release_class` werden die beiden
neuen Selektoren in die Liste aufgenommen. In
`contrast_1_text_classes_consume_roles_without_local_dimming` wird **nur**
`(".updates-tag.updates-tag-quiet", "@reprise_hint_fg_color")` ergänzt — siehe
Falle R-3 in Abschnitt 6; ein knapper englischer Kommentar hält fest, warum
`.updates-tag-neutral-filled` dort nicht steht.

**Änderung 3 — `concerts_section.rs`.** In `delta_presentations` ersetzt eine
`match`-Zuordnung den heutigen `then(...)`-Ausdruck:

```rust
tag: match row.availability {
    TicketAvailability::OnSale => None,
    TicketAvailability::OffSale => Some(feed_row::Tag {
        text: strings::text(strings::CONCERTS_OFF_SALE),
        tone: feed_row::TagTone::NeutralFilled,
    }),
    TicketAvailability::Unknown => Some(feed_row::Tag {
        text: strings::text(strings::CONCERTS_UNKNOWN),
        tone: feed_row::TagTone::Quiet,
    }),
},
```

Ein `match` statt einer Bedingung, damit eine spätere vierte Ausprägung von
`TicketAvailability` hier vom Compiler aufgehalten wird und nicht still ohne
Tag durchläuft. Darüber ein englischer Kommentar mit einem Satz zur
Begründung (der Feed markiert die Abweichung, die Tabelle vergleicht).

Im Inline-Testmodul derselben Datei entsteht:

```rust
#[test]
fn nr_39_the_feed_tags_only_the_exception() { … }
```

Er baut drei Events desselben Tages mit `OnSale`, `OffSale`, `Unknown`, ruft
`delta_presentations` einmal für alle drei und prüft:
`presentations[0].tag == None`;
`presentations[1].tag == Some(Tag { text: "Off sale", tone: NeutralFilled })`;
`presentations[2].tag == Some(Tag { text: "Unknown", tone: Quiet })`.
`Tag` und `TagTone` leiten `PartialEq`/`Eq` bereits ab, der Vergleich braucht
also keinen Zusatzcode. Kein `#[ignore]`: `delta_presentations` ist eine reine
Funktion und braucht kein Display (der bestehende Nachbartest
`concert_fields_and_target_follow_the_shared_row_contract` läuft heute schon
anzeigefrei).

Der bestehende Test `concert_fields_and_target_follow_the_shared_row_contract`
bleibt inhaltlich gültig (sein Event ist `OffSale`) und wird nicht angefasst.

**Änderung 4 — `docs/ux-rules.md`.** Am Ende von Abschnitt R, nach dem
NR-21a-Eintrag und vor `## S. Surfaces & Geometry`:

```
- **NR-39** [active] [gtk] — In the popover a status tag marks the exception,
  not the state. A concert row carries a ticket tag only when its availability
  is `Off sale` or `Unknown`; `On sale` is the expectation for a freshly
  announced show and carries none. The Concerts table is the comparison
  surface and keeps naming all three values in its Tickets cell (CONC-13), so
  no column reads as a missing value. Where both surfaces show a tag they show
  the same word and the same pill: the popover's ticket tones are declared
  exactly as the table's `on-sale`, `off-sale` and `unknown` classes are. The
  Releases chip keeps its own outlined tone and is untouched by this. CONC-12
  remains the only source of the values, and the tag carries no tooltip of its
  own, so the row keeps naming its source per NR-38.
  Test: `nr_39_the_feed_tags_only_the_exception`
  (`ui/updates/concerts_section.rs`, `#[cfg(test)]`).
```

Das Format ist bindend, nicht kosmetisch: `check-ux-traceability.sh` liest
Status und Ebene mit
`^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(active|planned)\] \[(core|gtk|e2e|manual)\]`
— Zeilenanfang, exakt zwei Klammerausdrücke, dann der Gedankenstrich.

**Abnahme**
- `cargo test -p reprise-gnome nr_39` grün; der Test wurde vorher rot gesehen
  (TDD: erst der Test, dann die `match`-Zuordnung).
- `cargo test -p reprise-gnome updates::` grün.
- `scripts/check-ux-traceability.sh` meldet `UX traceability ok: <n> active
  rules covered` mit `<n>` = bisher + 1.
- `git diff docs/ux-rules.md` enthält genau einen Hunk (den neuen Eintrag).
- `git diff crates/reprise-gnome/src/ui/updates/css.rs` zeigt am Block
  `.updates-tag.updates-tag-neutral` **keine** Änderung.
- `cargo fmt --check`, `clippy -D warnings` grün.

### Aufgabe 2 — `Prove the popover pill and the table pill are one pill`

**Dateien**
- `crates/reprise-gnome/src/ui/updates/css.rs` (nur das Testmodul)

**Änderung 1 — Deklarationsvergleich (anzeigefrei).** Neuer Test
`the_popover_ticket_tones_declare_what_the_concerts_table_declares`. Er liest
`super::css()` und `crate::ui::concerts::css::css()` — beide sind
`pub(in crate::ui)` bzw. der Modulpfad ist `pub(super) mod css` in
`ui/concerts/mod.rs`, aus `ui::updates::css` also erreichbar; die Nachbarschaft
ist auch sonst üblich (`concerts_section.rs` liest
`crate::ui::concerts::concerts_presentation::format_event_date`).

Der Test zieht mit dem in der Datei bereits vorhandenen `rules_for`-Muster für
jedes Paar den Regelblock heraus, normalisiert Leerraum (die beiden Dateien
rücken unterschiedlich ein) zu einer sortierten Menge von
`eigenschaft: wert`-Paaren und vergleicht:

| Popover | Tabelle |
|---|---|
| `.updates-tag` | `.reprise-concert-ticket-tag` |
| `.updates-tag.updates-tag-accent` | `.reprise-concert-ticket-tag.on-sale` |
| `.updates-tag.updates-tag-neutral-filled` | `.reprise-concert-ticket-tag.off-sale` |
| `.updates-tag.updates-tag-quiet` | `.reprise-concert-ticket-tag.unknown` |

Die Fehlermeldung nennt Selektorpaar und Differenzmenge, nicht nur „ungleich".

**Änderung 2 — Pixelvergleich (Anzeigetest).** Neuer Test
`the_popover_ticket_pills_render_exactly_as_the_table_pills`, markiert mit
`#[ignore = "requires a display; run via xvfb-run"]` (der Marker, den
`check-ux-traceability.sh` ausdrücklich nicht als abschaltendes `ignore`
wertet und den `scripts/check-display-tests.sh` ausführt).

Vorbild ist `btn_1_hover_active_focus_distinct` in
`crates/reprise-gnome/src/ui/style/buttons.rs`: dort wird ein Widget über
`gtk4::WidgetPaintable` + `gtk4::Snapshot` in einen `render_texture` gegeben und
mit `save_to_png_bytes()` zu Bytes gemacht. Hier:

1. `crate::ui::test_main_context::lock_main_context()`, `gtk4::init()`,
   `crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test())`
   (dieselbe Zeile wie in `player_bar_layout_tests.rs`) — der Test misst die
   **komponierte** App-CSS, nicht zwei isolierte Schnipsel.
2. Für jedes der beiden Paare zwei `gtk4::Label` mit demselben Text
   (`strings::text(strings::CONCERTS_OFF_SALE)` bzw. `…CONCERTS_UNKNOWN`),
   das eine mit `updates-tag` + Popover-Tonklasse, das andere mit
   `reprise-concert-ticket-tag` + Tabellenklasse.
3. **Beide in dieselbe präsentierte `gtk4::Window`** (eine `Box`), damit ein
   Renderer, ein Fensterhintergrund und eine Schriftkonfiguration gelten.
4. **Das Kriterium steht fest, es wird nicht zur Laufzeit gewählt** (Grill,
   15.08.2026): erst Geometrie (`width()`/`height()` beider Beschriftungen
   gleich, eigene Fehlermeldung), dann drei Farbstichproben je Paar —
   Rahmen links, Füllung Mitte, Textkörper — mit Toleranz ±1 pro Kanal. Kein
   Byte-Vergleich der PNGs: er hinge an Subpixel-Ausrichtung und natürlicher
   Textbreite und brächte gegenüber den Stichproben nichts hinzu, was diese
   Runde belegen will. Bewiesen wird, dass GTK füllt und rahmt wie deklariert.
   Die Stichprobenpunkte werden aus der gemessenen Widget-Geometrie errechnet,
   nicht als feste Pixelkoordinaten hingeschrieben.

**Änderung 3.** Der bestehende Anzeigetest
`new_releases_css_parses_without_errors` deckt die neuen Deklarationen ohne
Änderung mit ab; ebenso `the_composed_stylesheet_parses_without_errors` in
`ui/style/mod.rs`. Beide bleiben unangetastet — im Abnahmeprotokoll aber
namentlich nennen, dass sie gelaufen sind.

**Abnahme**
- `cargo test -p reprise-gnome updates::css` grün (anzeigefreier Teil).
- Der Deklarationsvergleich wurde einmal künstlich rot gesehen (probeweise
  `0.08` in `0.09` ändern, Meldung lesen, zurücknehmen) — ein Vergleichstest,
  den nie jemand hat scheitern sehen, beweist nichts.
- Beide Anzeigetests laufen unter
  `dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d)
  XDG_CACHE_HOME=$(mktemp -d) GDK_BACKEND=x11 WAYLAND_DISPLAY=
  REPRISE_AUDIO_SINK=fakesink GSK_RENDERER=cairo cargo test -p reprise-gnome
  <name> -- --ignored --exact` grün.
- `crates/reprise-gnome/src/ui/updates/css.rs` bleibt unter 800 Zeilen
  (heute 278).

### Aufgabe 3 — `Record R4 as closed by construction`

**Datei**
- `docs/plans/updates-concerts-releases-rework.md`

**Änderung.** Genau ein Hunk: ein neuer Unterabschnitt am **Ende** von
Abschnitt 9, also nach „### Was offen bleibt". Kein Zeichen oberhalb wird
angefasst — insbesondere nicht der R4-Text in der Post-Merge-Querprüfung 5 und
nicht die drei Aufzählungspunkte unter „Was offen bleibt".

```markdown
### Nachtrag (15.08.2026) — R4 ist geschlossen

Dieser Nachtrag ändert oben keinen Satz. Der Anforderungstext R4
(Post-Merge-Querprüfung 5) bleibt wörtlich stehen: nachträglich umgeschrieben
wäre er kein Nachweis mehr, sondern eine Behauptung über die Vergangenheit.

R4 verlangte drei Paritäten zwischen Popover-Zeile und Tabellenzeile:
44×44-Kachel, 2px-Akzentmarke, Tag-Typografie.

1. **Die Kachel-Hälfte war per Konstruktion gegenstandslos.** Strang 1 hat die
   Concerts-Tabelle bewusst einzeilig gebaut (CONC-14), und in
   `crates/reprise-gnome/src/ui/concerts/` existiert kein Bild-Widget:
   `git grep -nE 'Picture|gtk::Image|Avatar|Texture|Pixbuf'` über dieses
   Verzeichnis liefert null Treffer, und `ConcertColumn` kennt keine
   Cover-Spalte. Es gibt kein Gegenstück, das eine Kachel zeigen könnte, also
   auch keine Parität, die ein Screenshot herstellen oder widerlegen könnte.
   Prüfung 5 hat das gemessen und „überholt, nicht verletzt" genannt; dieser
   Nachtrag zieht daraus den Schluss.
2. **Die Akzentmarken-Hälfte ist erfüllt** — wortgleiche Deklaration in
   `updates/css.rs` und `concerts/css.rs`, in Prüfung 5 gemessen.
3. **Die Tag-Hälfte ist erfüllt und wird verschärft.** Prüfung 5 fand als
   einzige echte Abweichung die Füllung der neutralen Variante. Die Runde
   `feed-tags-mark-the-exception` (15.08.2026) hebt sie auf: der Popover-Tag
   trägt für `Off sale` und `Unknown` genau die Deklarationen der
   Tabellenklassen `.off-sale` und `.unknown`, festgehalten von einem
   Deklarations- und einem Pixelvergleich beider Pillen.

Damit ist R4 geschlossen. Eine normative Regel in `docs/ux-rules.md` entsteht
daraus ausdrücklich **nicht**: die Kachel-Hälfte hat keinen Gegenstand, und die
Tag-Hälfte ist ab jetzt von NR-39 abgedeckt.

Der zweite Punkt aus „Was offen bleibt" — der fehlende Status-Tag bei `OnSale`
und `Unknown` — ist mit NR-39 entschieden: `Off sale` und `Unknown` tragen im
Popover einen Tag, `On sale` nicht. Beide Aufzählungspunkte bleiben oben als
Protokoll ihres Zeitpunkts unverändert stehen.
```

**Abnahme**
- `git diff docs/plans/updates-concerts-releases-rework.md` zeigt genau einen
  Hunk, ausschließlich Hinzufügungen, am Dateiende von Abschnitt 9.
- Der Nachtrag steht **nach** Aufgabe 1 im Verlauf: Punkt 3 behauptet einen
  Zustand, den erst Aufgabe 1 herstellt. Vorher gemergt wäre er unwahr.

### Aufgabe 4 — `Capture the counter-proof to 41-footer-loaded`

**Dateien (getrackt, gehen ins Repo)**
- `artifacts/feed-tags-mark-the-exception/probe-feed-tags.sh`
- `artifacts/feed-tags-mark-the-exception/manifest.txt`

**Dateien (flüchtig, gehen NICHT ins Repo)** — Ablageort ist das
Scratchpad-Verzeichnis des Laufs, der Pfad steht im Manifest:
- `01-popover-tags.png`, `02-concerts-table-tags.png`,
  `control-01-popover-pinned-base.png`

**Warum das Skript bleibt und die Bilder gehen** (Grill, 15.08.2026). Der
Prüfstand der letzten Runde lag in einem flüchtigen Verzeichnis und ist weg —
genau das darf sich nicht wiederholen, deshalb wird **das Skript** getrackt
(Präzedenz: `docs/evidence/bounded-daemon-stop/probe-stop-daemon.sh`). Die
Aufnahmen selbst sind Beleg für Menschen und jederzeit neu erzeugbar, solange
das Skript existiert; dauerhaft im Baum wären sie Ballast. Was von ihnen
bleiben muss, sind die **Zahlen**: `manifest.txt` trägt die gemessenen
RGB-Tripel mit ihren Koordinaten, beide Commit-SHAs, Zeitstempel, Auflösung
und die Werkzeugversionen — wer später fragt, was gemessen wurde, liest das
Manifest und muss kein Bild öffnen. Der dauerhafte, wiederholbare Beweis ist
ohnehin der Anzeigetest aus Aufgabe 2.

**Was das Skript tut** (kein Verweis auf einen fremden Prüfstand; die
Bausteine kommen aus dem, was das Repo dauerhaft mitbringt):

1. Baut die App im Worktree: `cargo build -p reprise-gnome`. **Nicht** unter
   `/tmp` bauen (`AGENTS.md`: 16G-tmpfs).
2. Legt ein Wegwerf-Profil unter `~/.cache/reprise-scratch/feed-tags.XXXXXX`
   an (`mktemp -d`), mit `data/`, `cache/`, `config/`.
3. Startet Display und Barrierefreiheitsbus über die dauerhaften Bausteine des
   Repos: `scripts/cua-common/session.sh` wird gesourct und liefert
   `cua_common_start_display` (Xvfb + openbox), `cua_common_start_driver`
   (`at-spi-bus-launcher`, `at-spi2-registryd`, cua-driver-Daemon) und
   `cua_common_exec_private` (dbus-run-session mit isolierten XDG-Wurzeln,
   `GDK_BACKEND=x11`, leerem `WAYLAND_DISPLAY`, `REPRISE_AUDIO_SINK=fakesink`).
   Die Snapshot- und Klick-Hilfen (`cua_snapshot`, `element_index_for_label`,
   `element_center_for_index`) kommen aus `scripts/cua-e2e/lib.sh`.
4. **Erster Start** nur, damit die App ihr Schema anlegt; danach beenden.
5. **Fixture seeden** mit `sqlite3` in `$XDG_DATA_HOME/reprise/reprise.db`:
   - `INSERT OR REPLACE INTO settings(key,value) VALUES
     ('module.concerts.enabled','1');` — `modules::enabled_key` bildet
     `module.<id>.enabled`, `settings::set_bool_in` schreibt `'1'`/`'0'`.
   - `INSERT OR REPLACE INTO settings(key,value) VALUES
     ('concerts.bandsintown_app_id','probe-fixture');` — **notwendig**:
     `feed_snapshot::concerts` fragt die Zeilen nur bei
     `enabled && credentials` ab. Ein Wert genügt; es wird nichts abgerufen.
   - Drei Zeilen in `concert_events` mit verschiedenen `dedupe_key`,
     `seen_at = NULL` (sonst ist der Delta leer), `is_similar = 0`, einem
     `date_key` in der Zukunft und
     `ticket_availability` in `('on_sale','off_sale','unknown')` —
     die persistierte Schreibweise stammt aus
     `TicketAvailability::as_str`. Der Standardfilter (`ConcertFilter::default`:
     kein Radius, kein Land, `AllUpcoming`, ohne Similar) lässt genau solche
     Zeilen durch; drei Zeilen sind zugleich die Obergrenze des Abschnitts
     (`CONCERTS_DELTA_CAP = 3`).
6. **Zweiter Start**, Fenster abwarten, den ✦-Auslöser über den
   Barrierefreiheitsbaum finden und klicken, `01-popover-tags.png` ablegen.
7. In die Concerts-Ansicht navigieren, `02-concerts-table-tags.png` ablegen.
8. **Pixelsonde** nach dem Vorbild von
   `scripts/cua-e2e/selection_anchor.sh::assert_selected_rows`:
   `magick "$png" -crop "1x1+$x+$y" -depth 8 txt:-` liefert das RGB-Tripel.
   Gemessen werden je ein Punkt in der Füllfläche der `Off sale`-Pille im
   Popover und in der Tabelle; das Skript scheitert, wenn die Kanäle um mehr
   als 2 auseinanderliegen. Die Koordinaten werden aus dem AT-SPI-Rahmen der
   jeweiligen Beschriftung errechnet, nicht geraten, und im Manifest
   protokolliert.
9. **Kontrollarm:** derselbe Ablauf mit einem aus `origin/dev` @ `b6be7cdc61`
   gebauten Binary in einem zweiten Scratch-Verzeichnis →
   `control-01-popover-pinned-base.png`. Erwartung dort: `Off sale` trägt eine
   **ungefüllte** Pille, `Unknown` trägt **gar keine**. Eine grüne Messung ohne
   Kontrollarm misst nichts (so schon §5.2 des Mutterplans).
10. Schreibt `manifest.txt`: Commit-SHA beider Arme, Zeitstempel,
    Bildschirmauflösung, die gemessenen RGB-Tripel mit ihren Koordinaten, die
    Versionen von `cua-driver` und `magick`.

**Was die Bilder zeigen müssen** (sonst gilt der Punkt als nicht gezeigt):
- `01-popover-tags`: drei Concert-Zeilen; die `Off sale`-Zeile trägt eine
  gefüllte Pille, die `Unknown`-Zeile eine leise ungefüllte, die
  `On sale`-Zeile **keine**.
- `02-concerts-table-tags`: dieselben drei Ereignisse in der Tabelle, alle drei
  mit Wort in der Tickets-Spalte.
- Die `Off sale`-Pille sieht in beiden Bildern gleich aus; das belegt die
  Pixelsonde numerisch, nicht das Auge.

**Wenn der Prüfstand fehlt.** Ist `cua-driver`, `magick`, `openbox` oder
`at-spi2-registryd` auf der Maschine nicht da, bricht das Skript mit klarer
Meldung ab; der Lauf notiert die genau fehlende Prüfung und macht weiter
(`AGENTS.md`, Definition of Done, Punkt 1). Der Merge hängt nicht an den
Bildern: die belastbare, wiederholbare Aussage steht im Pixel-Anzeigetest aus
Aufgabe 2. Die Bilder sind menschlicher Beleg, nicht die Behauptung — genau die
Rolle, die `scripts/cua-e2e/README.md` den Screenshots zuweist.

**Abnahme**
- Getrackt sind genau zwei Dateien: `probe-feed-tags.sh` und `manifest.txt`.
  `git status` zeigt kein PNG unter `artifacts/` — die Bilder liegen im
  Scratchpad, ihr Pfad steht im Manifest. Ein `*.png` im Diff ist ein Fehler,
  kein Zusatznutzen.
- `manifest.txt` nennt für jede der beiden Pillen die gemessenen RGB-Tripel
  samt Koordinate, beide Commit-SHAs (Arm und Kontrollarm), Zeitstempel,
  Auflösung und die Versionen von `cua-driver` und `magick`.
- `scripts/check-shell.sh` grün — es prüft **jede** getrackte `*.sh` über
  `git ls-files`, also auch diese; beide Läufe (`-S warning` und `-S style`)
  müssen durch, `shellcheck disable=`-Direktiven brauchen eine Begründung in
  der Zeile darüber.
- Niemals die echte Datenbank `~/.local/share/reprise/reprise.db` anfassen; das
  Skript setzt eigene XDG-Wurzeln und weigert sich zu laufen, wenn
  `XDG_DATA_HOME` nicht unter seinem Scratch-Verzeichnis liegt.

### Aufgabe 5 — `Run the merge gate`

**Dateien:** keine (bzw. nur `.superpowers/sdd/progress.md`).

**Der lokale Umfang steht fest** (Grill, 15.08.2026): **nicht** die volle
Kette von `check-merge-readiness.sh` — sie ist lang, die Anzeigesuite gilt im
Rudel als flaky, und sie ist lokal in früheren Runden nie ganz durchgelaufen.
Gefahren wird die berührte Teilmenge, abgeleitet aus dem Skript (nicht aus dem
Gedächtnis), plus die Brandstellen der letzten Runde:

1. `cargo fmt --check`
2. `cargo clippy --locked --all-targets --workspace -- -D warnings`
3. `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`
4. `cargo test --locked --workspace --exclude reprise-platform-linux`
5. `scripts/check-ux-traceability.sh` — NR-39 braucht seinen Test
6. `scripts/check-shell.sh` — das neue Sondenskript ist getrackt
7. `scripts/check-ai-hygiene.sh`
8. `scripts/check-architecture.sh`
9. `scripts/check-frontend-thinness.sh` — **obwohl rechnerisch unberührt**:
   genau sie war in der letzten Runde dreimal rot, und sie läuft sonst nur in
   CI
10. `scripts/check-display-tests.sh` — der neue Anzeigetest lebt dort

Alles Übrige aus Abschnitt 5 ist CIs Arbeit. Vor dem eigenen Lauf einmal
gegen `origin/dev` messen, was ohne diese Änderung schon rot ist (Falle R-6),
sonst wird fremdes Rot als eigene Schuld verbucht.

**Abnahme:** die zehn Punkte oben grün, jeder mit seiner Ausgabezeile
protokolliert; was nicht lief, wird **namentlich** genannt statt
stillschweigend übergangen. Danach PR gegen `dev`.

---

## 3. Was diese Runde nicht anfasst

- Der Releases-Chip (`chip_presentation` in `release_row.rs`) — weder Text
  noch Ton noch CSS.
- Die Concerts-Tabelle: `ticket_presentation`, ihre Klassen, ihr
  `Off sale`-Tooltip und `concerts/css.rs` bleiben, wie sie sind. Die Tabelle
  ist in dieser Runde die **Referenz**, nicht der Gegenstand.
- Ein Tooltip am Popover-Tag (Beschluss 3).
- Ein Cover in der Concerts-Tabelle. R4s Kachel-Hälfte wird geschlossen, nicht
  nachgebaut.
- Eine neue normative Regel für R4 (ausdrücklich nicht gewünscht).
- Übersetzungen: `Off sale` und `Unknown` sind bestehende `N_!`-Strings in
  `ui/strings_concerts.rs`; es entsteht keine neue msgid.
- Die Übergabedatei `docs/plans/updates-concerts-releases-rework.HANDOFF.md`.
  Sie ist **ungetrackt** und lebt allein im geteilten Hauptcheckout — ein
  Worktree sieht sie gar nicht. Sie nachzuführen ist Sache der Sitzung, nicht
  dieses Plans.
- Der zu lange Standort-Chip der Concerts-Ansicht
  (`Zürich, Bezirk Zürich, Zürich, Schweiz/Suisse/Svizzera/Svizra · 500 km`).
  Am 15.08.2026 gemeldet, eigener Befund, eigener Plan: `AppLocation.name`
  trägt Nominatims rohen `display_name`, obwohl CONC-2 vom `{city}` spricht.
  Fasst andere Dateien an und gehört nicht in diese Runde.

---

## 4. Tests — die vollständige Liste

| Test | Datei | Art | Was er festnagelt |
|---|---|---|---|
| `nr_39_the_feed_tags_only_the_exception` | `ui/updates/concerts_section.rs` | anzeigefrei | `Off sale` → Tag, `Unknown` → Tag, `On sale` → kein Tag, samt Ton |
| `every_tag_tone_maps_to_its_own_css_class` | `ui/updates/feed_row.rs` | anzeigefrei | vier Töne, vier verschiedene Klassen |
| `the_popover_ticket_tones_declare_what_the_concerts_table_declares` | `ui/updates/css.rs` | anzeigefrei | Deklarationsgleichheit beider Pillen-Familien |
| `the_popover_ticket_pills_render_exactly_as_the_table_pills` | `ui/updates/css.rs` | Anzeige (xvfb) | Pixelgleichheit im komponierten App-Stylesheet |
| `css_covers_every_new_release_class` (erweitert) | `ui/updates/css.rs` | anzeigefrei | kein Ton ohne Selektor |
| `contrast_1_…_without_local_dimming` (erweitert) | `ui/updates/css.rs` | anzeigefrei | der leise Ton konsumiert die Hint-Rolle statt lokal zu dimmen |
| `new_releases_css_parses_without_errors` (unverändert) | `ui/updates/css.rs` | Anzeige (xvfb) | GTK schluckt die neuen Deklarationen wirklich |

Der Testverweis der Regel NR-39 zeigt auf den ersten Eintrag. Die
Hausordnung verlangt „genau eine primäre Regel-ID im Namen" — alle übrigen
Tests tragen deshalb bewusst **keine** Regel-ID.

---

## 5. Gates — abgeleitet aus `scripts/check-merge-readiness.sh`, nicht aus dem Gedächtnis

Das Skript verlangt zuerst einen **sauberen Worktree einschließlich
unversionierter Dateien** und dass die Basis (`origin/main`, überschreibbar per
`MERGE_READINESS_BASE_REF`) Vorfahr von `HEAD` ist. Danach in dieser
Reihenfolge — mit Vermerk, was diese Runde jeweils berührt:

| Gate | berührt? |
|---|---|
| `scripts/check-shell.sh` | **ja** — Aufgabe 4 fügt eine getrackte `*.sh` hinzu |
| `scripts/tests/worktree-gc.sh`, `…-schedule.sh` | nein |
| `scripts/check-architecture.sh` | **ja** — 800-Zeilen-Grenze über alle `crates/**/*.rs`; `css.rs` 278 → ca. 390, `concerts_section.rs` 313 → ca. 345, `feed_row.rs` 115 → ca. 140. Kernreinheit unberührt (kein `reprise-core`-Diff) |
| `scripts/check-device-sync-gstreamer.sh` | nein |
| `scripts/check-accessibility-semantics.sh` | nein — kein `set_focusable(true)`, kein Slider, kein Tab |
| `scripts/check-input-parity.sh` | nein |
| `scripts/check-runtime-service-install.sh` | nein |
| `scripts/check-frontend-thinness.sh` | **ja, aufmerksam lesen** — siehe unten |
| `scripts/check-ux-traceability.sh` | **ja** — NR-39 braucht seinen Test |
| `check-appstream.sh`, `check-flatpak-manifest.sh`, `check-gnome-idioms.sh` | nein |
| `scripts/check-ai-hygiene.sh` | **ja** — GP-19: keine anleitungsartigen Kommentare, keine Bannerzeilen `// ------`, keine Emoji in Kommentaren |
| `scripts/check-motion-tokens.sh` | nein — die neuen Blöcke enthalten kein `transition` |
| `cargo fmt --check` | ja |
| `cargo clippy --locked --all-targets --workspace -- -D warnings` | ja |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | ja — der Doc-Kommentar am Enum muss gültig sein |
| `cargo test --locked --workspace --exclude reprise-platform-linux` | ja |
| `cargo test --locked -p reprise-platform-linux -- --test-threads=1` | nein, aber Teil der Kette |
| `scripts/check-display-tests.sh` | **ja** — der neue Anzeigetest läuft hier mit (die Kette ist lang; Laufzeit einplanen) |
| Runtime-Service-Bus-Tests unter `dbus-run-session` | nein |
| `cargo audit` | nein (keine Abhängigkeitsänderung) |

### Die Falle aus der letzten Runde, konkret geprüft

Die Frontend-Schlankheits-Prüfung hat in der letzten Runde dreimal das
Quality-Gate rot gemacht. Sie steht in `check-merge-readiness.sh` unter
`== Frontend thinness ==` — sie ist also lokal fahrbar und war nur nicht in
der Gate-Liste aus dem Gedächtnis. Ihre Zähler sind **Gleichheiten**, nicht
Obergrenzen: zu wenig ist genauso rot wie zu viel. Für diese Runde geprüft:

- **`rusqlite`-Budget (113).** Gezählt wird
  `rusqlite::|use rusqlite|params!|\.prepare\(|\.query_row\(|Connection` über
  Produktionszeilen des Frontends. Unser Diff fügt keinen dieser Ausdrücke
  hinzu und entfernt keinen. **Unverändert.** Warnung an die Umsetzung: kein
  Bezeichner mit `Connection` im Namen, auch nicht im Testfixture-Umfeld
  außerhalb eines `#[cfg(test)]`-Blocks auf Spalte 0.
- **`filesystem` (13), `threads` (15).** Kein `std::fs`, kein
  `thread::spawn`. Unverändert.
- **`workers` (7).** Zählt Dateien mit `worker` im Namen. Wir legen keine an.
- **Verbote `db_handle_access`, `gstreamer`, `zbus`.** Unberührt.
- **`view_floor` (2116).** Zählt Produktionszeilen in
  `crates/reprise-view/src`. Diese Runde fasst `reprise-view` **nicht** an —
  wichtig, weil dieser Zähler exakt stimmen muss und in der letzten Runde
  einmal rot war.
- **Dead-Code-Freigabeliste.** Wir fügen kein `#[allow(dead_code)]` hinzu und
  entfernen keins; die Liste bleibt unverändert. Zu beachten: `TagTone` erhält
  zwei neue Varianten — **beide werden konstruiert** (Aufgabe 1, Änderung 3),
  also entsteht kein toter Code und keine Versuchung, eine Freigabe
  einzutragen. Genau deshalb liegen Enum, CSS und Aufrufer in einem Commit.
- **Neue Datei?** Aufgabe 4 legt eine Datei unter `artifacts/` an — außerhalb
  von `crates/`, also für alle Zähler dieses Skripts unsichtbar. Kein neues
  UI-Modul, keine neue `.rs`-Datei.
- **`cargo machete`.** Keine Abhängigkeitsänderung.

Fazit: diese Runde **berührt kein Budget**. Sie muss trotzdem laufen, und das
Ergebnis („at budget" für alle vier, „unchanged" für die Freigabeliste) gehört
ins Abnahmeprotokoll.

---

## 6. Risiken und Fallen

**R-1 — Der Upcoming-Chip.** Die eine Fläche, die nicht wackeln darf. Schutz:
der CSS-Block `.updates-tag.updates-tag-neutral` wird nicht angefasst und
`release_row.rs` gar nicht editiert. Prüfung: `git diff --stat` darf
`release_row.rs` nicht nennen, und `git diff` von `updates/css.rs` darf im
Neutral-Block keine Zeile zeigen.

**R-2 — Vier statt drei Töne.** Der Beschluss verlangte „einen dritten Ton";
die Rechnung ergibt vier (Beschluss 3). Wer das für falsch hält, muss eine der
drei Forderungen fallen lassen — dann ist die Umbenennungsvariante der
nächstbeste Weg, und sie kostet die exakte `Unknown`-Spiegelung. Der Plan legt
sich fest, damit Codex nicht mitten im Lauf entscheiden muss.

**R-3 — Der CONTRAST-1-Test schlägt aus dem Hinterhalt zu.**
`contrast_1_text_classes_consume_roles_without_local_dimming` prüft für jeden
gelisteten Selektor `!rules.contains("color: alpha(@window_fg_color")`. Der
String `background-color: alpha(@window_fg_color, 0.08)` **enthält** diese
Teilzeichenkette. Wer `.updates-tag-neutral-filled` in die Liste aufnimmt,
macht den Test rot, obwohl die Deklaration korrekt ist (eine Fläche, kein
Vordergrund). Deshalb kommt dort nur `.updates-tag-quiet` hinein, mit einem
Kommentar. Die sauberere Alternative — den Test vor dem Vergleich
`background-color`-Deklarationen entfernen zu lassen — ändert einen
regelbenannten Test einer fremden Regel und bleibt bewusst außen vor.

**R-4 — Der Anzeigetest darf nicht flattern.** Ein Byte-Vergleich zweier
gerenderter Beschriftungen wäre der strengste Beweis und zugleich der
zerbrechlichste: Subpixel-Ausrichtung und natürliche Textbreite entscheiden
mit. Deshalb steht das Kriterium **vorab** fest (Aufgabe 2, Änderung 2):
Geometrie plus drei Farbstichproben mit Toleranz ±1. Der Entwurf hatte das als
Rückfallposition formuliert — das wäre eine Entscheidung mitten im Lauf
gewesen, genau das, was Beschluss 3 an anderer Stelle vermeidet. Der
Deklarationsvergleich bleibt der harte Beweis, dass in beiden Dateien dasselbe
steht; der Anzeigetest ist der Beweis, dass GTK es auch anwendet — die Lücke,
die der Kommentar an `btn_1_hover_active_focus_distinct` beschreibt.

**R-5 — Das Fixture zeigt keine Zeilen.** `feed_snapshot::concerts` fragt nur
bei `enabled && credentials` ab; ohne hinterlegten Credential-Schlüssel bleibt
das Popover leer und der Bildbeweis misst nichts. Zweite Stolperstelle:
`seen_at` muss `NULL` sein und `date_key` in der Zukunft liegen. Die Sonde
prüft deshalb **zuerst** die Concerts-Tabelle (dort müssen drei Zeilen stehen)
und erst danach das Popover.

**R-6 — Der Anzeigetest-Gate ist lang und im Rudel bekannt flaky.** Vor dem
eigenen Lauf gegen `origin/dev` messen, was ohne diese Änderung schon rot ist;
sonst wird fremdes Rot als eigene Schuld verbucht (dieselbe Warnung steht in
§5 des Mutterplans).

**R-7 — `check-merge-readiness.sh` verlangt einen sauberen Baum inklusive
unversionierter Dateien.** Die Bilder aus Aufgabe 4 müssen also committet
sein, bevor das Gate läuft — nicht danebengelegt.

**R-8 — Sprache.** `AGENTS.md` erklärt ein neues deutsches Dokument zum Defekt.
Dieser Plan und der Nachtrag im (ebenfalls deutschen) Mutterplan sind die
ausdrückliche Ausnahme; **Regeltext, Code, Kommentare, Testnamen, Commits,
Branch und PR sind englisch**. Codex darf aus der Sprache dieses Plans nicht
schließen, dass deutsche Kommentare erlaubt wären.

**R-9 — Der Beweis überlebt seine Bilder nicht von allein.** Die Aufnahmen
liegen bewusst im flüchtigen Scratchpad. Damit steht und fällt die
Wiederholbarkeit mit `probe-feed-tags.sh` — das Skript ist deshalb kein
Wegwerf-Harness, sondern getrackter Code mit `check-shell.sh` als Gate. Wer es
später nicht mehr zum Laufen bringt, hat den Beweis verloren; die Zahlen im
`manifest.txt` bleiben dann das Einzige, was von der Messung übrig ist. Genau
darum tragen sie Koordinaten, SHAs und Werkzeugversionen und nicht nur ein
„grün".

---

## 7. Git-Artefakte (englisch)

- Branch: `feature/feed-tags-mark-the-exception` (von `dev`, per
  `docs/agents/branching.md`).
- Commits:
  1. `Tag the exception, not the state, in the Updates feed`
  2. `Prove the popover pill and the table pill are one pill`
  3. `Record R4 as closed by construction`
  4. `Capture the counter-proof to 41-footer-loaded`
- PR gegen `dev`: `The Updates feed tags the exception, in the table's tones`

---

## Parallelität

**Der Versuch, ernsthaft durchgerechnet.** Ein dateidisjunkter Schnitt in drei
Stränge ist konstruierbar:

**Strang A — `feed-tags`** (Zweck: Verhalten, Töne, Regel, Beweise)
- Besitz: `crates/reprise-gnome/src/ui/updates/**`, `docs/ux-rules.md`
- Aufgaben: 1, 2

**Strang B — `r4-addendum`** (Zweck: das Protokoll schließen)
- Besitz: `docs/plans/updates-concerts-releases-rework.md`
- Aufgabe: 3

**Strang C — `visual-proof`** (Zweck: Bildbeweis samt Kontrollarm)
- Besitz: `artifacts/feed-tags-mark-the-exception/**`
- Aufgabe: 4

**Merge-Reihenfolge, wenn man ihn führe:** A → B → C. B ist von A nicht
technisch, aber inhaltlich abhängig: sein Punkt 3 behauptet einen Zustand, den
erst A herstellt; vor A gemergt wäre der Nachtrag unwahr. C braucht A als
Binary, sonst fotografiert es den alten Zustand — und zwar genau den, den es als
*Kontrollarm* fotografieren soll, was die Verwechslungsgefahr eher erhöht als
senkt.

**Post-Merge-Querprüfungen** (jede liest Dateien, die der prüfende Strang nicht
besäße; sie gehören ausdrücklich nicht in eine Strang-Aufgabe):

1. `scripts/check-ux-traceability.sh` über die ganze `docs/ux-rules.md` samt
   aller Tests: NR-39 ist `[active]` und hat seinen Test; kein Test zeigt auf
   eine ersetzte ID.
2. Der Deklarationsvergleich liest `ui/concerts/css.rs`, das kein Strang
   besitzt — nach dem Merge einmal gegen den Hauptzweig fahren.
3. Ein Wort, zwei Flächen: dasselbe Ereignis zeigt in Tabelle und Popover
   dasselbe Wort aus derselben Konstante (`CONCERTS_OFF_SALE`,
   `CONCERTS_UNKNOWN`).
4. Der Bildbeweis aus C gegen den Code aus A: die `Off sale`-Pille misst in
   beiden Aufnahmen dasselbe RGB.
5. Die vollständige Gate-Kette aus Abschnitt 5, einschließlich der
   Frontend-Schlankheit und der Anzeigesuite.

**Ergebnis: der Schnitt trägt nicht — ein Strang** (im Grill am 15.08.2026
bestätigt). Aufgaben 1 und 2 fassen
beide `crates/reprise-gnome/src/ui/updates/**` und wären derselbe Besitzer;
Aufgaben 3 und 4 sind reiner Nachlauf von 1 und können vor 1 nicht ehrlich
grün werden. Der gesamte Produktionsdiff sind zwei Enum-Varianten, zwei
CSS-Blöcke und ein `match` — rund 200 Zeilen samt Tests. Drei Worktrees, drei
Branches und fünf Querprüfungen für diese Menge zu bezahlen, kostet mehr
Koordination als es an Durchsatz bringt. Die Aufgaben laufen deshalb der Reihe
nach in **einem** Worktree auf `feature/feed-tags-mark-the-exception`; die
Querprüfungen 1–5 bleiben als Schlussliste vor dem PR bestehen, weil die Gates
ohnehin den ganzen Baum lesen.
