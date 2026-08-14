---
slug: updates-concerts-releases-rework-2
worktree: /home/marvin/Projects/reprise-updates-concerts-releases-rework-2
branch: feature/updates-concerts-releases-rework-2
phase: planned
codex_session:
created: 2026-08-14
---
# Strang 2 — `updates-popover`

> **Lies zuerst den Mutterplan:**
> `docs/plans/updates-concerts-releases-rework.md`. Er trägt die Ausgangslage
> (§0), alle 18 Beschlüsse (§1), die englischen Quellstrings (§3), den
> vollständigen UX-Regeltext (§4), die Abnahme (§5), die Abgrenzung (§6), die
> Parallelität (§7) und die Nachträge der Schlussprüfung (§8). Diese Datei sagt, **was**
> zu tun ist; der Mutterplan sagt, **warum**. Wo beide sich zu widersprechen
> scheinen, gewinnt der Mutterplan.

> Zeilennummern gegen `origin/dev` @ `5721ade95e`. Von den Dateien dieses
> Strangs hat sich seither keine geändert; `docs/ux-rules.md` liegt ab `:1062`
> um **+1** verschoben (Abschnitt R also `:2130`, NR-10a `:2211`, NR-5b
> `:2235`, NR-21 `:2324`, NR-22 `:2335`, NR-23 `:2341`, CONC-7 `:4989`).
> **Achtung:** dieser Strang rebast ohnehin auf den gemergten Strang 1, der in
> Abschnitt AE geschrieben hat — die AE-Nummern verschieben sich dadurch
> weiter nach unten. Für CONC-7 gilt: **am Regelnamen suchen, nicht an der
> Nummer.** Der Hauptcheckout ist geteilt — nicht umschalten, per
> `git show origin/dev:<pfad>` lesen.

## Zweck

Dieser Strang gibt beiden Feeds im Updates-Popover **eine** Zeilenform, macht
die Abschnittsköpfe `Releases` und `Concerts` zur einzigen Brücke in die
Vollansicht (die zwei Sprungzeilen fallen), und stellt die beiden
verbleibenden Fußzeilen — Releases-Vollansicht und Popover — auf den
gemeinsamen Live-Footer um, den Strang 1 gebaut hat. Danach zeigt keine der
drei Flächen mehr ein Aktualisierungsalter, und jede zeigt ihren Zeitstempel
**genau einmal**. Er enthält Paket D und den Rest von Paket C.

## Dateibesitz

```
crates/reprise-gnome/src/ui/updates/**
crates/reprise-gnome/src/ui/releases/releases_view.rs
crates/reprise-gnome/src/ui/releases/releases_presentation.rs
crates/reprise-gnome/src/ui/strings_news.rs
crates/reprise-gnome/src/ui/strings_releases.rs
docs/ux-rules.md    (NUR Abschnitt R: NR-34…NR-38, NR-21a und die Marker auf
                     NR-5b/10a/21/22/23
                     + GENAU EINE Zeile in Abschnitt AE: der Statusmarker auf CONC-7)
```

**Ausdrücklich NICHT — diese Dateien gehören anderen und werden nicht
angefasst, auch nicht „nur eine Zeile":**

```
crates/reprise-gnome/src/ui/feed_footer.rs                → Strang 1 (nur konsumieren!)
crates/reprise-gnome/src/ui/style/mod.rs                  → Strang 1
crates/reprise-gnome/src/ui/concerts/**                   → Strang 1
crates/reprise-gnome/src/ui/strings_concerts.rs           → Strang 1
crates/reprise-core/src/**                                → Strang 1 / Strang 3
crates/reprise-gnome/src/ui/notifications*.rs             → Strang 3
crates/reprise-gnome/src/ui/preferences/**                → Strang 3
crates/reprise-gnome/src/ui/strings.rs, po/POTFILES.in    → Strang 3
```

Zwei Sperren innerhalb des eigenen Umfelds:

- `ui/releases/releases_columns.rs` (798 Z.) wird **nicht** angefasst —
  NR-30 und NR-33 bleiben, der Spaltensatz der Releases-Vollansicht bleibt,
  das externe-Link-Icon **bleibt dort** (es fällt nur in der Popover-Zeile).
- `ui/releases/releases_filter_bar.rs` wird nicht angefasst.

`ui/feed_footer.rs` wird **benutzt, nicht bearbeitet.** Fehlt dort etwas,
ist das ein Befund für den Mutterplan, keine Einladung zum Nachbessern in
fremdem Besitz.

## Vorbedingungen

Dieser Strang braucht **drei Dinge aus Strang 1**:

| Was | Wofür |
|---|---|
| `TicketAvailability` (`concerts/availability.rs`) | das `Off sale`-Tag auf Popover-Concert-Zeilen |
| `concerts::query::mark_event_seen(db, id)` | der `Dismiss`-Knopf auf Concert-Zeilen |
| `crates/reprise-gnome/src/ui/feed_footer.rs` | beide Fußzeilen dieses Strangs |

**Fehlt eines davon auf der Basis, ist dieser Strang zu früh dran: erst auf
den gemergten Strang 1 rebasen, dann beginnen.** Die Popover-Zeile in zwei
Stränge zu schneiden (Geometrie hier, Tag dort) wäre der teurere Fehler —
sie bleibt in einer Hand.

Vor dem Rebase `origin/dev` frisch fetchen. Der Rebase ist auch aus einem
zweiten Grund Pflicht: die eine erlaubte Ausnahme beim Dateibesitz (der
CONC-7-Marker, Aufgabe 12) fasst eine Zeile in Strang 1s Abschnitt an, und
das geht konfliktfrei nur nach dessen Merge.

---

## Aufgaben

### Paket D — Das Updates-Popover (Aufgaben 1–7)

#### 1. Die drei Vorab-Extraktionen

`popover.rs` steht bei 786 Zeilen — **kein Spielraum** unter der
800er-Grenze. **Vorab festgelegt** (Beschluss 16), damit nicht improvisiert
wird:

| aus | nach | Inhalt |
|---|---|---|
| `ui/updates/popover.rs` (786) | `ui/updates/footer_state.rs` (neu) | die Live-Zustands-Abbildung des Popovers, **rein** |
| `ui/updates/popover.rs` | `ui/updates/popover_fetch.rs` (neu) | `start_fetch()` (`:560`), `start_news_fetch()` (`:606`), `start_concerts_fetch()` (`:634`), `finish_feed()` (`:668`) |
| `ui/updates/release_row.rs` (501) | `ui/updates/feed_row.rs` (neu) | die gemeinsame Zeilenform |

`release_row.rs` behält danach nur noch die release-spezifische
Feldabbildung.

**Ziel:** Vier Dateien statt zwei, jede deutlich unter 800 Zeilen, gleiches
Verhalten.
**Nachweis:** `cargo test --workspace` grün **vor** der ersten inhaltlichen
Änderung — eine Extraktion, die Verhalten ändert, ist keine Extraktion.

#### 2. `feed_row.rs` — eine Zeilenform für beide Feeds

Die gemeinsame Zeile besteht aus **zwei Geschwistern**:

- einem **echten flachen `gtk4::Button`** als Aktivierungsfläche, der Cover
  (44×44), Titel (15px), Meta (13px) und den optionalen Tag umschließt;
- dem Ignorieren-Knopf **daneben**, nicht darin.

Umgesetzt wird das **nicht** über `GestureClick` (Beschluss 8). Der Grund ist
konstruktiv, nicht stilistisch: aus einem Knopf blubbert kein Event in einen
anderen, also kann der Ignorieren-Knopf den Link **prinzipiell** nicht
mitauslösen. Tastaturaktivierung, Fokusring und Barrierefreiheits-Rolle
kommen dabei von GTK, ohne Zusatzarbeit.

Im Popover gilt **Einfachklick** — dort gibt es keine Auswahl-Semantik zu
schützen, und ein transientes, menüartiges Popover ist genau die Fläche, auf
der Einfachklick die Hausform ist. (Die Concerts-**Tabelle** behält
Doppelklick; das ist Strang 1 und ausdrücklich abgenommen.)

**Ziel:** Screenshot 1: identisch aufgebaute Zeilen für Releases und
Concerts.
**Nachweis:** `nr_36_dismissing_a_row_never_opens_its_link` grün, plus die
Messung „Der Ausblenden-Knopf öffnet keinen Browser" aus §5.2 des
Mutterplans: Klick auf den Knopf bei laufendem `REPRISE_LOG` darf **keine**
`launch`-Zeile erzeugen.

#### 3. Die Feldbelegung beider Feeds

| Feed | Titel | Meta |
|---|---|---|
| Release | Releasetitel | `{artist} · {type} · {date}` |
| Concert | Künstler (+ gedimmtes `similar to {seed}`) | `{date} · {city} · {venue}` |

Das `similar to {seed}` sitzt **in derselben Zeile** hinter dem Namen, als
gedimmtes Nachsatz-Segment mit `ellipsize = End`, während der Name
`ellipsize = None` behält — bei Platzmangel geht also die Herkunftsangabe
verloren, nie der Name (Beschluss 7). Dieselbe Technik benutzt Strang 1 in
der Artist-Zelle der Tabelle; die beiden Flächen sollen sich hier gleich
verhalten.

**Ziel:** Beide Feeds tragen dieselbe Geometrie, nur andere Felder.
**Nachweis:** Screenshot 1 der Abnahme.

#### 4. Die Tags

| Bedingung | Tag | Klasse |
|---|---|---|
| Datum ≤ heute | `Released` | `.updates-tag.updates-tag-accent` |
| Datum > heute | `In {days} day` / `In {days} days` (Plural über die Haus-Pluralform) | `.updates-tag.updates-tag-neutral` |
| Concert-Zeile mit `TicketAvailability::OffSale` | `Off sale` | `.updates-tag.updates-tag-neutral` |
| Concert-Zeile mit `OnSale` oder `Unknown` | **kein Tag** | — |

„Kein Tag = Tickets verfügbar" ist beabsichtigt (Anforderung R5): das
Popover trägt nur die Ausnahme, nicht den Normalfall.

`Off sale` ist ausdrücklich **nicht** „Sold out" — keine der beiden Quellen
kennt den Unterschied zwischen ausverkauft und noch-nicht-im-Verkauf
(Beschluss 2). Der Wortlaut kommt aus Strang 1s Kern-Typ; hier wird er nur
angezeigt.

**Ziel:** Dasselbe Event trägt im Popover exakt denselben Status wie in der
Tabelle.
**Nachweis:** Screenshot 1; der Gleichlauf beider Flächen ist eine
**Post-Merge**-Prüfung (siehe unten), nicht Sache dieses Strangs.

#### 5. Das Cover der Concert-Zeilen

Für Concerts gibt es heute kein Cover. Der garantierte Zustand ist die
**Initialen-Kachel** aus `ui/updates/release_cover.rs` (320 Z.) — dieselbe
Geometrie 44×44, dieselbe Herleitung aus dem Namen, damit NR-2 („missing
cover → equally sized tile … never a hole") unverändert gilt.

Ein Künstlerporträt wird verwendet **wenn — und nur wenn — es bereits im
lokalen Cache liegt** (`reprise_core::artist_portrait::cache`) **und** das
Artwork-Modul aktiv ist. Es wird für Concerts **nie nachgeladen**: ein
Netzabruf beim Öffnen des Popovers verstieße gegen CONC-5a, und Deezer
liefert nachweislich Platzhalter-Porträts (MD5 des Leerstrings) — eine
kaputte graue Kachel ist schlechter als Initialen.

Die Fallkette ist damit vollständig: **gecachtes Porträt → Initialen.** Mehr
nicht.

**Ziel:** Keine Concert-Zeile hat ein Loch, und keine löst beim Öffnen einen
Netzabruf aus.
**Nachweis:** Screenshot 1; zusätzlich darf `REPRISE_LOG` beim Öffnen des
Popovers keinen Porträt-Abruf zeigen.

#### 6. Die Abschnittsköpfe werden die Brücke — die Sprungzeilen fallen

Die Abschnittsköpfe `Releases` und `Concerts` werden **echte
`gtk4::Button`**. Ein Klick oder Enter/Space schließt das Popover und öffnet
die zugehörige Vollansicht — genau das, was `wire_jump()` (`popover.rs:276`)
heute für die Sprungzeilen tut. Rechts im Kopf bleibt der Zähl-Chip mit
NR-23s Semantik: er nennt die **volle** Stapelgröße und erscheint nur,
solange der Stapel wirklich ungesehen ist.

**Der Kopf bleibt sichtbar, solange sein Modul aktiv ist — auch bei leerem
Abschnitt**, und zeigt dann darunter eine ruhige Leerzeile (`No new
releases` / `No new concerts`). Sonst würde das Popover zur Sackgasse und
NR-23s ausdrückliche Zusage bräche.

**Ersatzlos verschwinden dabei** (Beschluss 18):
- die Sprungzeilen und ihr Aufbau (`shell.rs:60-70`, `build_jump_row()`
  `:120-132`, `popover.rs::wire_jump` `:276`);
- die Strings `updates_show_all_concerts` (`strings_news.rs:157`) und
  `updates_show_all_releases` (`:164`) — **in dieser Reihenfolge**, der
  Entwurf hatte die beiden vertauscht;
- die Klassen `new-release-history-row` / `-label` / `-count`.

**Ziel:** Screenshot 3: leerer Concerts-Abschnitt, Kopf steht noch da,
Leerzeile darunter.
**Nachweis:** `nr_34_an_empty_section_keeps_its_header_and_its_bridge` und
`nr_35_the_concerts_section_header_carries_the_unseen_count` grün.

#### 7. Aktivierung, Tooltip, `Dismiss`, und was unangetastet bleibt

- Aktivierung öffnet über `external_link::launch()` (`ui/external_link.rs:23-44`,
  der einzige Weg nach außen). Für Releases gilt weiter **NR-11s
  URL-Priorität**, für Concerts das Angebot vor der Eventseite.
- Zeilentooltip `Opens {source}`. Die **hover-freie** Zweitheimat des
  Quellennamens ist die `Source`-Spalte aus Strang 1 — dadurch bleibt TIP-3
  erfüllt und der Tooltip bleibt, was er sein soll: ein Komfort-Duplikat.
- Der Ignorieren-Knopf ist **`view-conceal-symbolic`**, nie ein X
  (Beschluss 12). Ein X liest sich als „löschen", und die getrennte
  `deleted_releases`-Familie darf sich damit nicht vermengen. Größe 28×28,
  Platz ganz rechts hinter dem Tag.
  - **Release-Zeile:** Tooltip `Hide` (bestehende Konstante `HIDE_RELEASE`),
    bestehende Semantik, in der Vollansicht umkehrbar.
  - **Concert-Zeile:** Tooltip `Dismiss`, ruft `mark_event_seen()` aus
    Strang 1 und stempelt genau **dieses eine** Event als gesehen.
  Zwei Wörter für zwei verschiedene Zusagen: `Hide` ist umkehrbar, `Dismiss`
  ist ein Gesehen-Stempel.
- Concert-Zeile **ohne** Ziel: der umschließende Knopf ist **insensitiv** und
  trägt den Tooltip `No ticket or event link available`. Bei Releases kann
  der Fall nicht auftreten (NR-11 endet immer beim MusicBrainz-Fallback).
- **Unangetastet bleiben:** die Reihenfolge *rendern → stempeln → Badge neu
  rechnen* (NR-9c) und NR-29s engeres Ankündigungsfenster (Zukunft + 90
  Tage) — das Fenster wird **nicht** aufgeweitet.
- Das externe-Link-Icon (`external-link-symbolic`) fällt in der
  **Popover**-Zeile; in der Releases-**Vollansicht** bleibt es (NR-30).

**Ziel:** Ein Klick auf die Zeile öffnet genau die URL, die ihr Tooltip
benennt; ein Klick auf den Ignorieren-Knopf öffnet nichts.
**Nachweis:** `nr_38_a_row_opens_the_same_url_its_tooltip_names` und
`nr_36_dismissing_a_row_never_opens_its_link` grün.

---

### Paket C — die zwei restlichen Fußzeilen (Aufgaben 8–11)

#### 8. Die Releases-Fußzeile

`releases_view.rs::build_footer()` (`:421`) liefert heute `fetch_label`,
`updated` und `progress`; `apply_footer()` (`:476`) ruft
`releases_footer_presentation()` (`releases_presentation.rs:38-69`) mit den
drei Zuständen `Idle { latest }` / `Starting` / `Running(progress)`.

Beides weicht dem gemeinsamen `feed_footer.rs` aus Strang 1 mit **neun**
Zuständen; `releases_footer_presentation()` geht darin auf. Einheit im Text
ist `releases`. Die Datei `releases_view.rs` (687 Z.) **schrumpft** dabei.

Der determinierte `checked/total`-Künstlerfortschritt landet in der
Fortschrittsleiste der Fußzeile, nicht in einem eigenen Widget.

**Ziel:** Die Releases-Vollansicht zeigt Live-Zustand statt Alter, mit einem
Reload-Icon-Knopf statt `Fetch now`.
**Nachweis:** Screenshot 7 der Abnahme (die vier interessanten Zustände) —
für diese Fläche analog zu Concerts.

#### 9. Die Popover-Fußzeile aggregiert beide Feeds

Dieselbe Zustandstabelle, Einheit `updates`. Der Zustand ist die
**Aggregation** beider Feeds:

- läuft **einer** von beiden, gilt `Fetching`;
- sonst zählt der **ältere** der beiden Zeitstempel.

Damit behauptet die Fußzeile nie mehr Frische als ihre schwächere Hälfte.
Der Reload-Knopf löst **beide** Feeds aus.

`footer_presentation()` (`popover.rs:47-54`) — heute Altersstring plus
`show_cached_failure` — verschwindet bzw. geht in `footer_state.rs` auf.

Damit ist auch der Widerspruch aus dem Design-Auszug aufgelöst: Mock 1a
zeigt das alte Paar „Updated 18 h ago" + `Fetch now` noch, der Auftragstext
streicht es. Der Auftragstext gewinnt, weil er auch fürs Popover
„Zeitstempel nur einmal im Footer" verlangt.

**Ziel:** Ein Zeitstempel unten im Popover, keiner in einer Zeile, keiner im
Knopf.
**Nachweis:** `nr_37_the_popover_footer_reports_the_older_of_both_feeds`
grün, plus Screenshot 1 der Abnahme.

#### 10. Was an Alters-Anzeige ersatzlos verschwindet

- Der `Fetch now`-Knopf mit eingebautem Alters-Label: `shell.rs:134-170`
  (`build_header()`, Icon `view-refresh-symbolic` bei `:145`) und
  `releases_view.rs:421`.
- **Der Test, der das einfriert:** `shell.rs:172-212`, Name
  `nr_23_shell_is_a_fixed_delta_layout_with_fetch_state_in_its_header`. Er
  fällt zusammen mit NR-23 (Aufgabe 12). Ihn stehen zu lassen bedeutet, den
  alten Zustand einzufrieren, den dieser Strang gerade abschafft.
- `strings_news.rs::new_releases_updated_ago()` (`:109`). Die Prüffrage des
  Entwurfs ist beantwortet: die Funktion hat genau **zwei** Leser,
  `releases_presentation.rs:45` und `updates/popover.rs:51` — **beide
  gehören diesem Strang**, sie kann also gefahrlos mit entfernt werden.
  (`concerts_presentation.rs::updated_ago()` und
  `strings_concerts.rs::concerts_updated_ago()` entfernt Strang 1.)
- **Keine Rückwärtskompatibilität:** alte Zeichenketten werden gelöscht,
  nicht weitergeschleppt.

**Ziel:** Im ganzen Popover und in der Releases-Ansicht wird kein
`Updated … ago` mehr gerendert.
**Nachweis:** `git grep -n 'Updated .*ago'` über
`crates/reprise-gnome/src/ui` findet für diese beiden Flächen nichts mehr.
Der **vollständige** Befund über alle drei Flächen ist post-merge (siehe
unten).

#### 11. Die Strings in `strings_news.rs` und `strings_releases.rs`

Neu bzw. angepasst (englische Quellstrings, Volltabelle in §3 des
Mutterplans):

| Zweck | Quellstring |
|---|---|
| leerer Abschnitt | `No new releases` / `No new concerts` |
| Release-Meta | `{artist} · {type} · {date}` |
| Concert-Meta | `{date} · {city} · {venue}` |
| Tags | `Released`, `In {days} day` / `In {days} days` (Plural) |
| Ignorieren (Concert) | `Dismiss` |
| Zeilentooltip | `Opens {source}` |
| Fußzeile | `Updating releases …`, `Updating …`, `Update failed — showing saved releases from {time}` (analog `updates`), `Offline — showing saved releases from {time}` (analog) |

Dazu die **fünf flächenneutralen** Fußzeilen-Strings, die **wörtlich
dupliziert** werden, weil Strang 1 dieselben msgid in `strings_concerts.rs`
anlegt: `Up to date — loaded at {time}`, `Up to date — checked {time}`,
`Not loaded yet`, `Online sources are off`, `Reload`.

Das ist Absicht (Beschluss 17), kein Versehen und kein DRY-Verstoß zum
Aufräumen: gettext fasst identische msgid ohnehin zu **einem** `.pot`-Eintrag
zusammen, die Duplikation kostet i18n-seitig nichts — eine geteilte
Strings-Datei kostete dagegen genau das, was die Parallelität vermeiden
soll: zwei Stränge, die in dieselbe Datei schreiben. **Ownership schlägt
DRY, wenn DRY nichts einspart.**

`Updates`, `{count} new` (`updates_new_count`), `Hide` (`HIDE_RELEASE`) und
die Abschnittstitel existieren bereits und werden wiederverwendet.

**Ziel:** Jeder neue sichtbare Text hat einen englischen Quellstring an der
richtigen Stelle.
**Nachweis:** `cargo test --workspace` grün; `po/`-Dateien werden **nicht**
von Hand angefasst — Deutsch entsteht über den normalen Extraktionslauf.

---

### 12. Die UX-Regeln in Abschnitt R (plus die eine Zeile in AE)

Prozessvertrag: eine Regel wechselt `[planned]` → `[active]` **in demselben
Commit**, der das Verhalten baut und den regelbenannten Test hinzufügt. Ein
Test trägt **genau eine** primäre Regel-ID im Namen.
`scripts/check-ux-traceability.sh` ist Merge-Gate.

**Neu zu schreiben** (Volltext in §4.2 des Mutterplans — wörtlich übernehmen,
nicht neu formulieren):

| ID | Level | Test (alle in `ui/updates/popover_tests.rs`) |
|---|---|---|
| `NR-34` | `[active] [gtk]` | `nr_34_an_empty_section_keeps_its_header_and_its_bridge` |
| `NR-35` | `[active] [gtk]` | `nr_35_the_concerts_section_header_carries_the_unseen_count` |
| `NR-36` | `[active] [gtk]` | `nr_36_dismissing_a_row_never_opens_its_link` |
| `NR-37` | `[active] [gtk]` | `nr_37_the_popover_footer_reports_the_older_of_both_feeds` |
| `NR-38` | `[active] [gtk]` | `nr_38_a_row_opens_the_same_url_its_tooltip_names` |
| `NR-21a` | `[active] [gtk]` | bestehender `nr_21_…`-Test wird umgehängt |

**Statusmarker zu setzen** (die alten Regeln bleiben stehen, sie bekommen nur
den Marker):

| Alt | Zeile (Pin) | Marker |
|---|---|---|
| `NR-10a` | 2210 | `[replaced by NR-36]` |
| `NR-5b` | 2234 | `[replaced by NR-34]` |
| `NR-21` | 2323 | `[replaced by NR-21a]` |
| `NR-22` | 2334 | `[replaced by NR-37]` |
| `NR-23` | 2340 | `[replaced by NR-34]` |

**Die eine erlaubte Ausnahme beim Dateibesitz:** zusätzlich setzt dieser
Strang **genau eine Zeile** in Abschnitt AE — den Statusmarker auf
`CONC-7` (`:4988` gegen den Pin) auf `[replaced by NR-35]`. Die Regel steht
in Strang 1s Abschnitt, ist inhaltlich aber Popover und gehört deshalb
hierher. Bedingungen, ohne die diese Ausnahme nicht trägt:

1. **Vorher auf den gemergten Strang 1 rebasen.**
2. **Nur diese eine Zeile** anfassen — nichts sonst in Abschnitt AE.
3. Am **Regelnamen** suchen, nicht an der Zeilennummer: Strang 1 hat sein
   Neues ans Ende von AE angehängt und die Nummern verschoben.

**Ziel:** Nach diesem Strang zeigt kein Test mehr auf NR-5b, NR-10a, NR-21,
NR-22, NR-23 oder CONC-7.
**Nachweis:** die Traceability über den **eigenen** Abschnitt ist grün; der
vollständige Lauf ist post-merge (siehe unten).

---

## Was dieser Strang NICHT verifiziert

Die folgenden Prüfungen lesen Dateien, die dieser Strang **nicht besitzt**.
Sie können vor dem Merge prinzipiell nicht grün werden. **Nicht auf sie
warten und nicht versuchen, sie vorzuziehen** — ein Strang, der darauf wartet,
bleibt mit fertiger, korrekter Arbeit stehen. Sie stehen vollständig in
**§7, „Post-Merge-Querprüfungen", des Mutterplans**:

1. `scripts/check-ux-traceability.sh` über die **ganze** `docs/ux-rules.md` —
   dieser Strang sieht nur Abschnitt R und die eine CONC-7-Zeile.
2. „Ein Wort, zwei Flächen": derselbe Status in der Concerts-Tabelle
   (Strang 1) und in der Popover-Zeile (hier) — liest Strang 1s
   Zeilenimplementierung.
3. „Ein Zeitstempel, drei Fußzeilen": `git grep -n 'Updated .*ago'` über
   **alle drei** Flächen — Concerts gehört Strang 1.
4. „Dieselbe URL": die Benachrichtigung (Strang 3) öffnet für ein gegebenes
   Release exakt die URL, die dessen Popover-Zeile (hier) öffnet. Ein
   Vergleich zweier Ergebniswerte, nicht zweier Implementierungen — aber
   Strang 3 existiert vorher nicht.
5. **Geometrie-Parität (Anforderung R4):** Screenshot-Paar Popover-Zeile
   (hier) gegen Tabellenzeile (Strang 1) — gleiche 44×44-Kachel, gleiche
   2px-Akzentmarke, gleiche Tag-Typografie. Per Konstruktion erst nach dem
   Merge messbar.
6. Migrationskette am Stück — betrifft diesen Strang gar nicht.

Was dieser Strang **sehr wohl** vor dem Merge liefert: `cargo fmt --check`,
`cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`, jede angefasste Datei unter 800
Zeilen, und die Screenshots 1, 2 und 3 der Abnahme (§5.1 des Mutterplans)
plus die `REPRISE_LOG`-Messung „Der Ausblenden-Knopf öffnet keinen Browser".

Das Display-Gate ist im Rudel bekanntermaßen flaky und auf `dev` teils schon
rot: **zuerst gegen `origin/dev` messen, was ohne diese Änderung rot ist**,
sonst wird fremdes Rot als eigene Schuld verbucht.
