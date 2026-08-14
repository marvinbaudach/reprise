---
slug: updates-concerts-releases-rework-1
worktree: /home/marvin/Projects/reprise-updates-concerts-releases-rework-1
branch: feature/updates-concerts-releases-rework-1
phase: refactored
codex_session:
created: 2026-08-14
---
# Strang 1 — `core-concerts`

> **Lies zuerst den Mutterplan:**
> `docs/plans/updates-concerts-releases-rework.md`. Er trägt die Ausgangslage
> (§0), alle 18 Beschlüsse (§1), die englischen Quellstrings (§3), den
> vollständigen UX-Regeltext (§4), die Abnahme (§5), die Abgrenzung (§6), die
> Parallelität (§7) und die Nachträge der Schlussprüfung (§8). Diese Datei sagt, **was**
> zu tun ist; der Mutterplan sagt, **warum**. Wo beide sich zu widersprechen
> scheinen, gewinnt der Mutterplan — dann ist diese Datei falsch abgeschrieben
> und die Stelle gehört gemeldet, nicht stillschweigend ausgelegt.

> Zeilennummern gegen `origin/dev` @ `5721ade95e`. `origin/dev` ist inzwischen
> weiter (`a7febd7d92`, #476); von den Dateien dieses Strangs hat sich **keine**
> geändert, nur `docs/ux-rules.md` liegt ab `:1062` um **+1** verschoben
> (Abschnitt AE also `:4941`, CONC-3 `:4957`, CONC-11 `:5010`, Ende von AE
> `:5024`). Der Hauptcheckout ist geteilt — **nicht** umschalten, per
> `git show origin/dev:<pfad>` lesen.

## Zweck

Dieser Strang bringt den Ticket-Verfügbarkeitsstatus von der Anbieterantwort
bis in die Tabellenzelle, baut die Concerts-Übersicht auf einzeilige Zeilen mit
echtem Status, abschaltbarer Quellenspalte und bedeutungstragender
Distanzfärbung um, und legt den **gemeinsamen Fußzeilen-Baustein**
`ui/feed_footer.rs` an, den die beiden anderen Stränge danach nur noch
konsumieren. Er ist außerdem **alleiniger Besitzer der Migrationskette**: er
schreibt `migrate_v73` (für sich) **und** `migrate_v74` (für Strang 3) und
setzt `SUPPORTED_SCHEMA_VERSION` in einem Zug auf `74`. Er enthält Paket A,
Paket B und den Concerts-Teil von Paket C. Er ist Vorbedingung für **beide**
anderen Stränge und wird als erster gemergt.

## Dateibesitz

```
crates/reprise-core/src/concerts/**
crates/reprise-core/src/concerts.rs
crates/reprise-core/src/db_concerts.rs
crates/reprise-core/src/db.rs                     (ALLEINBESITZ)
crates/reprise-core/src/db_new_releases_notify.rs (neu — Nachtrag 1, §8 des Mutterplans)
crates/reprise-gnome/src/ui/concerts/**
crates/reprise-gnome/src/ui/feed_footer.rs        (neu)
crates/reprise-gnome/src/ui/style/mod.rs          (nur: eine Zeile in der css()-Liste)
crates/reprise-gnome/src/ui/strings_concerts.rs
docs/ux-rules.md                                  (NUR Abschnitt AE, und dort nur die
                                                   unten genannten IDs)
docs/plans/location-is-not-a-concerts-setting.md  (nur: Nachtrag E2b)
```

**Ausdrücklich NICHT — diese Dateien gehören anderen und werden nicht
angefasst, auch nicht „nur eine Zeile":**

```
crates/reprise-gnome/src/ui/updates/**                    → Strang 2
crates/reprise-gnome/src/ui/releases/**                   → Strang 2
crates/reprise-gnome/src/ui/strings_news.rs               → Strang 2
crates/reprise-gnome/src/ui/strings_releases.rs           → Strang 2
crates/reprise-core/src/artist_news_query.rs              → Strang 3
crates/reprise-core/src/artist_news_notify.rs             → Strang 3
crates/reprise-gnome/src/ui/notifications*.rs             → Strang 3
crates/reprise-gnome/src/ui/preferences/**                → Strang 3
crates/reprise-gnome/src/ui/strings.rs, po/POTFILES.in    → Strang 3
```

Innerhalb des eigenen Besitzes gelten zwei Sperren aus **Beschluss 3**
(Standort gehört dem Plan `location-is-not-a-concerts-setting`):

- `ui/concerts/concerts_filter_bar.rs` (762 Z.) wird **gar nicht** angefasst.
- In `ui/concerts/concerts_column_layout.rs` wird **nur** die neue
  `Source`-Spalte registriert — keine Standort-Logik, kein Ein-/Ausblenden der
  Distance-Spalte, keine Sortier-Sperre.
- In `ui/concerts/concerts_view.rs` wird die **Sichtbarkeit** der
  Distance-Spalte nicht angerührt; dieser Strang färbt nur.

`ui/concerts/css.rs` hat heute 5 Zeilen — dort ist Platz für alles.
`docs/ux-rules.md`: **CONC-7 nicht anfassen.** Diese eine Zeile gehört
Strang 2 (Mutterplan §7, „die einzige erlaubte Ausnahme"). Neues wird ans
**Ende** von Abschnitt AE angehängt (`≈:5020`, auf dem aktuellen Tip
`≈:5024`), weit unterhalb von CONC-7.

## Vorbedingungen

**Keine.** Dieser Strang startet direkt auf dem aktuellen `origin/dev` und
wartet auf niemanden. Vor dem Branchen `origin/dev` frisch fetchen.

Er ist umgekehrt die Vorbedingung der beiden anderen: Strang 2 braucht
`TicketAvailability`, `query::mark_event_seen()` und `ui/feed_footer.rs`,
Strang 3 braucht die Spalte `new_releases.notified_released_at`. Deshalb ist
Aufgabe 5 (beide Migrationen) **nicht optional und nicht verschiebbar**, auch
wenn dieser Strang die zweite Spalte selbst nie liest.

---

## Aufgaben

### Paket A — Kern: Ticket-Verfügbarkeit (Aufgaben 1–9)

#### 1. Der Typ `TicketAvailability`

Neue Datei `crates/reprise-core/src/concerts/availability.rs` mit
`TicketAvailability { OnSale, OffSale, Unknown }`, `as_str()` /`from_str()`
auf die persistierten Werte `on_sale` / `off_sale` / `unknown`,
`Default = Unknown`.

**Ziel:** Ein Kern-Typ, der die drei Zustände aus Beschluss 2 trägt und
verlustfrei durch TEXT geht.
**Nachweis:** Roundtrip-Test `as_str() → from_str()` für alle drei Varianten;
ein unbekannter String wird zu `Unknown`, nicht zu einem Fehler.

#### 2. Das Feld in den drei Strukturen

`availability: TicketAvailability` in `ProviderEvent` (`provider.rs:36-49`),
`ConcertRow` (`concerts.rs:79-97`) und `CachedConcertEvent`.

**Ziel:** Der Status reist von der Anbieterantwort bis in die UI-Zeile, ohne
unterwegs neu erfunden zu werden.
**Nachweis:** `cargo check -p reprise-core` zeigt alle feldweise gebauten
Test-Helfer an; keiner bleibt übrig.

#### 3. Die beiden Anbieter-Abbildungen

`ticketmaster::parse_event()` (`ticketmaster.rs:118-157`) liest ab jetzt
`/dates/status/code` — heute liest es das Feld **nie**. Abbildung streng nach
der Tabelle in Beschluss 2: `onsale` → `OnSale`, `offsale` → `OffSale`, alles
andere (`cancelled`, `postponed`, `rescheduled`, fehlend, unbekannt) →
`Unknown`.

`bandsintown::parse_event()` (`bandsintown.rs:100-140`) unterscheidet ab
jetzt „Angebote vorhanden, keines `available`" (→ `OffSale`) von „`offers`
fehlt oder ist leer" (→ `Unknown`). Heute wirft der Code diese Information
weg, sobald er die erste verfügbare URL gefunden hat.

**Ziel:** Der Status ist immer das, was die Quelle sagt, nie eine Ableitung.
**Nachweis:** der regelbenannte Test `conc_12_offsale_never_becomes_sold_out`
(siehe UX-Regeln unten) plus je ein Test pro Anbieter, der eine echte
Antwortform durchschickt.

#### 4. `migrate_v73` — `ticket_availability`

Neue Funktion `migrate_v73()` in `db_concerts.rs`, exakt nach dem Muster von
`migrate_v31` (`:49-59`): `PRAGMA user_version` lesen, `if version >= 73 {
return Ok(()) }`, `unchecked_transaction()`, `ALTER TABLE concert_events ADD
COLUMN ticket_availability TEXT NOT NULL DEFAULT 'unknown'`,
`pragma_update(None, "user_version", 73)`, `commit()`. Die Spalte kommt
**ans Ende** der Tabelle.

**Ziel:** Eine v72-Datenbank bekommt die Spalte ohne Datenverlust.
**Nachweis:** ein Migrationstest neben den bestehenden in
`db_concerts_migration_tests.rs`, der eine v72-Datenbank hochzieht und die
Zeilenzahl vorher/nachher vergleicht.

#### 5. `migrate_v74` — `notified_released_at` (für Strang 3)

**Diese Aufgabe schreibt eine Spalte, die dieser Strang nie liest. Das ist
Absicht.** Begründung im Mutterplan §7: `db.rs` stand im Entwurf in zwei
Besitzlisten; mit Einzelbesitz fällt die einzige garantierte Konfliktstelle
des ganzen Auftrags weg.

- `migrate_v74()`: `ALTER TABLE new_releases ADD COLUMN notified_released_at
  INTEGER`, Muster wie `migrate_v31`, `user_version` auf `74`.
- Heimat der Funktion: neues Modul
  `crates/reprise-core/src/db_new_releases_notify.rs` — **entschieden als
  Nachtrag 1** (Mutterplan §8). Ein Modul `db_new_releases.rs`, wie der
  Entwurf annahm, existiert nicht; das Haus vergibt ein Modul je
  Migrationsthema, nicht je Tabelle.
- In `db.rs` beide Aufrufe **nach** `db_artwork::migrate_v72` (`:754`), in
  der Reihenfolge v73, v74.
- `SUPPORTED_SCHEMA_VERSION` (`db.rs:26`) in **einem** Schritt von `72` auf
  **`74`** — kein Zwischenstand `73`.

**Die Commit-Nachricht muss die fremde Spalte begründen**, sonst liest ein
Reviewer sie als toten Code. Wortlaut etwa: „v74 legt die Spalte für Strang 3
an, damit `db.rs` einen einzigen Besitzer hat."

**Ziel:** Nach diesem Strang steht die Datenbank auf `user_version = 74` und
trägt beide Spalten; Strang 3 kann ohne jede Schema-Arbeit starten.
**Nachweis:** ein Test fährt eine v72-Datenbank durch die Kette und prüft
`PRAGMA user_version == 74` sowie die Existenz **beider** Spalten. (Der
vollständige Beweis — dass die zweite Spalte auch beschrieben wird — ist
post-merge, siehe unten.)

#### 6. Der positionelle Upsert in `pipeline.rs`

`reconcile_artist()` hält einen positionellen 18-Spalten-Upsert
(`concerts/pipeline.rs:398-436`). Er wird zu **19** Spalten. **Vier** Stellen
müssen gemeinsam wachsen: die Spaltenliste, `VALUES (?1 … ?19)`, das
`params![…]` und der `ON CONFLICT … DO UPDATE SET`-Block (dort
`ticket_availability = excluded.ticket_availability`).

**Eine vergessene Stelle ist kein Compilerfehler**, sondern ein
Laufzeit-`rusqlite`-Fehler oder — schlimmer — eine Spalte, die still nie
aktualisiert wird.

**Ziel:** Ein zweiter Lauf über dasselbe Event schreibt einen geänderten
Status tatsächlich fort.
**Nachweis:** ein Test, der dasselbe Event zweimal mit unterschiedlichem
Status durch `reconcile_artist()` schickt und danach den **zweiten** Wert aus
der Datenbank liest. Ein Test, der nur einmal einfügt, beweist den
`DO UPDATE`-Zweig nicht.

#### 7. Die Leseseite in `query.rs`

Jede `SELECT`-Spaltenliste, die `concert_events` liest, und jedes
`row.get(n)` dahinter: heute `query_cached_events()` (`:44-82`) und
`filtered_events()` (`:179`ff). Die Indizes sind positionell — **die neue
Spalte immer am Ende anhängen**, nie in der Mitte.

**Ziel:** `query_events()` liefert den persistierten Status zurück.
**Nachweis:** ein Test schreibt `off_sale`, liest nach einem frischen
`Db`-Handle zurück und bekommt `TicketAvailability::OffSale`, nicht
`Unknown`.

#### 8. `mark_event_seen(db, id)`

Einzelvariante von `mark_scope_seen` (`concerts/query.rs:150-173`), für den
`Dismiss`-Knopf aus Beschluss 12. Kein neuer Zustand, keine neue Spalte,
keine neue Begrifflichkeit — dieselbe Stempelmechanik, nur für genau ein
Event.

**Ziel:** Strang 2 kann eine einzelne Concert-Zeile aus dem Popover-Delta
nehmen.
**Nachweis:** ein Test stempelt ein Event von dreien und prüft, dass die
anderen beiden ungesehen bleiben.

#### 9. Kernreinheit

`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` muss
**leer** bleiben.

**Ziel:** Der Verfügbarkeitstyp bleibt portabel und steht der Handy-App offen.
**Nachweis:** der Befehl gibt nichts aus. Nach **jeder** `reprise-core`-
Änderung erneut laufen lassen, nicht nur am Ende.

---

### Paket B — Die Concerts-Tabelle (Aufgaben 10–17)

#### 10. Vorab-Extraktion `concerts_status_cells.rs`

`concerts_columns.rs` steht bei 579 Zeilen; die Arbeit unten würde sie über
die 800er-Grenze treiben. **Vorab festgelegt** (Beschluss 16), damit nicht
improvisiert wird: die Zellfabriken für Tickets-Tag, `Source` und
Distanzfärbung ziehen nach `ui/concerts/concerts_status_cells.rs`.

**Ziel:** Eine neue Datei trägt die neuen Zellen; `concerts_columns.rs`
registriert nur noch.
**Nachweis:** beide Dateien unter 800 Zeilen, `cargo test --workspace` grün.

#### 11. Die Tickets-Spalte wird ein Status-Label

Der echte `gtk4::Button` mit `flat`/`link` (`concerts_columns.rs:220-287`)
weicht einem Status-Label mit Tag-Klasse. Werte: `On sale` / `Off sale` /
`Unknown`, rechtsbündig. `Off sale` trägt den Tooltip
`The ticket source reports no active sale. This can mean sold out, or not on sale yet.`
`ticket_button_label()` (`concerts_presentation.rs:87-98`) entfällt ersatzlos.

Die Zelle ist ab jetzt **keine Aktivierungsfläche mehr** — das ist der
inhaltliche Kern von CONC-13.

**Ziel:** Screenshot 2a ohne Ticketmaster-Knopfspalte, mit Status-Tags rechts.
**Nachweis:** `conc_13_a_row_without_a_target_does_not_activate` grün; kein
`Button` mehr in der Tickets-Zelle.

#### 12. Die neue `Source`-Spalte

Siebte Spalte, in `concerts_column_layout.rs` registriert (nur das —
Beschluss 3), **standardmäßig unsichtbar**, über das bestehende Kopf-Popover
(`concerts_view.rs:105-108`) einschaltbar, Sichtbarkeit persistiert wie bei
jeder anderen Spalte. Inhalt: der reine Anbietername als Label
(`ticket_source`, sonst `provider`) — **kein** Knopf, **kein** Link-Icon.
Spaltentitel: bestehende Konstante `CONCERTS_SOURCE`.

Der Auftraggeber hat ausdrücklich abgenommen, dass diese Spalte dem Wortlaut
des Auftrags („Entfernen: die Quellen-Spalte") formal widerspricht und
dennoch so gewollt ist, weil TIP-3 sonst verletzt wäre (Beschluss 5). **Diese
Begründung gehört in die Commit-Nachricht.**

**Ziel:** Im Auslieferungszustand sieht der Nutzer keine Quellenspalte; wer
sie will, schaltet sie ein, und die Wahl überlebt den Neustart.
**Nachweis:** `conc_16_the_source_column_is_available_but_off_by_default`
grün, plus Screenshot 5 der Abnahme.

#### 13. Die Artist-Zelle wird zweiteilig

Aus dem einen Label wird eine `gtk4::Box` mit zwei Labels:
Label 1 = Künstlername, `ellipsize = None`, `hexpand = false`;
Label 2 = `similar to {seed}` (bestehende Funktion
`strings::concert_similar_caption`, `strings_concerts.rs:87-89`), gedimmt,
`ellipsize = End`. Bei Platzmangel schrumpft damit **zuerst die Herkunft**,
nie der Name.

**Ziel:** CONC-6 bleibt unangetastet erfüllt, die Zeile bleibt einzeilig.
**Nachweis:** `conc_14_the_similar_caption_shrinks_before_the_artist` grün.

#### 14. Einzeiligkeit, Venue, City, Distanzfärbung

- Alle Zellen einzeilig, `ellipsize = End`, kein Umbruch, gemeinsame
  vertikale Mitte.
- Venue heller (`.reprise-concert-venue`), City gedimmt
  (`.reprise-concert-city`) — Venue trägt den Scan.
- Distanzen **innerhalb** des aktiven Radius bekommen
  `.reprise-concert-distance-near` (Akzent), alle anderen
  `.reprise-concert-distance-far` (gedimmt). Radius aus dem aktiven Filter,
  Standort aus `location::app_location()`; beides existiert heute.

**Die Sichtbarkeit der Distance-Spalte ohne Standort ist Sache des anderen
Plans** (Beschluss 3) — hier wird nur gefärbt, nie aus- oder eingeblendet.

**Ziel:** Screenshot 4: Nahdistanzen in Akzentfarbe, Fernedistanzen gedimmt,
Venue heller als City, keine Zeile bricht um.
**Nachweis:** Screenshot 4 der Abnahme plus ein Test, der die Klassenwahl an
der Radiusgrenze prüft (innerhalb → `near`, exakt darüber → `far`).

#### 15. Zeilenaktivierung, Tooltip, Barrierefreiheit

`connect_activate` (`concerts_view.rs:232`) bleibt **unverändert** —
Doppelklick und Enter, kein Einfachklick. Der Auftragstext verlangt wörtlich
den Zeilenklick; die Abweichung ist ausdrücklich abgenommen, weil GTK4s
`single-click-activate` die Auswahl dem Mauszeiger folgen ließe
(Beschluss 8). **Auch diese Begründung gehört in die Commit-Nachricht.**

- Zeilentooltip mit Ziel: `Opens {source}`.
- Zeile ohne Ziel: nicht aktivierbar (`ticket_target()` prüfen,
  `concerts_columns.rs:23`), Tooltip `No ticket or event link available`
  (bestehende Konstante `CONCERTS_NO_LINK`) — und **dieselbe Zeichenkette**
  als `accessible-description` der Zeile, sonst bricht ACC-2.

**Ziel:** Die Zeile ist die einzige Aktivierungsfläche; eine Zeile ohne Ziel
sagt in Tooltip und Screenreader dasselbe.
**Nachweis:** `conc_13_a_row_without_a_target_does_not_activate` grün.

#### 16. Das Listenende nach FIL-3a-Grammatik

FIL-3a (`docs/ux-rules.md:1539-1555`) verlangt bereits wörtlich „directly
below the last row when the list is shorter than the viewport". Die
Beschwerde „zentriert im Leerraum" ist damit ein **Bug gegen FIL-3a**, kein
Regeldefizit — behoben wird der Code, nicht die Regel (Beschluss 6).

- `End of results — {hidden} concerts hidden by the {radius} km radius around {city}`
- ohne Ortsnamen: `End of results — {hidden} concerts hidden by the {radius} km radius`
- Pille: `Show all {total} concerts` (bestehende Funktion
  `strings::show_all_concerts`, `strings_concerts.rs:91-96`)

**Das Scroll-Verhalten aus FIL-3a bleibt vollständig**: bei langen Listen
erscheint die Zeile erst, wenn das Ende in den Blick scrollt; sie schwebt nie
über Zeilen; sie ist nicht sticky; das Overlay bleibt eingabetransparent
außer der Pille. **Die Zentrierung bleibt** — die Linksbündigkeit aus Mock 2b
ist bewusst verworfen, weil FIL-3a sechs Ansichten bindet. Wer sie im Review
vermisst: sie wurde gesehen und abgelehnt.

**Ziel:** Screenshot 6: bei 3 von 415 Treffern steht das Listenende
unmittelbar unter der dritten Zeile, nicht in der Mitte des Leerraums.
**Nachweis:** `fil_3a_the_concerts_end_of_results_sits_below_the_last_row`
grün, plus Screenshot 6 **mit Kontrollarm** — der zurückgerollte Code muss
jetzt gerade die Mittenposition zeigen, sonst misst die grüne Messung nichts.

#### 17. Das CSS der Tabelle

`ui/concerts/css.rs` (heute 5 Zeilen) bekommt:

| Rolle | Klasse |
|---|---|
| `On sale` (Akzent-Umriss) | `.reprise-concert-ticket-tag.on-sale` |
| `Off sale` (neutral gefüllt) | `.reprise-concert-ticket-tag.off-sale` |
| `Unknown` (gedimmt) | `.reprise-concert-ticket-tag.unknown` |
| Distanz innerhalb des Radius | `.reprise-concert-distance-near` |
| Distanz außerhalb | `.reprise-concert-distance-far` |
| Venue heller | `.reprise-concert-venue` |
| City gedimmt | `.reprise-concert-city` |

Dazu die **2px-Akzentmarke** auf Tabellenzeilen als im Ruhezustand
**transparenter `border-left`** (Beschluss 9) — nicht als `box-shadow`:

```
border-left: 2px solid transparent;          /* Ruhezustand */
:hover, :focus-within → border-left-color: <Akzent>;
background-color: alpha(currentColor, 0.06); /* die Tönung daneben */
```

So gibt es keinen Layoutsprung, keine Abhängigkeit von GTKs
Schattenrendering und kein Clipping in `ColumnView`-Zeilen. **Die
Akzentfarbe wird nicht neu erfunden:** exakt die Quelle nehmen, die
`.new-release-chip` heute benutzt (STYLE-8, „effective accent color") — keine
Nocturne-Werte, kein neues Token, kein hartkodiertes Hex. `:selected` behält
seine normale Adwaita-Behandlung; die Marke ist Hover/Fokus, nicht Auswahl.

**Ziel:** Hover zeigt Tönung **plus** Marke, ohne dass die Zeile springt.
**Nachweis:** Screenshot 2 der Abnahme (dort für das Popover; für die Tabelle
derselbe Blick), und `ui/concerts/css.rs` bleibt weit unter 800 Zeilen.

---

### Paket C — Der gemeinsame Live-Footer, Concerts-Teil (Aufgaben 18–20)

#### 18. `ui/feed_footer.rs` — der Baustein

Neue Datei. Sie enthält:

1. `FeedFooterState` mit den **neun** Zuständen aus der Tabelle in
   Beschluss 1: `Loaded { at }`, `Cached { at }`, `Fetching { checked, total }`,
   `Failed { latest }`, `Offline { latest }`, `NeverFetched`, `NoCredentials`,
   `NetworkOff`, `ModuleOff`.
2. Eine **reine** Abbildungsfunktion
   `presentation(state, now) -> FeedFooterPresentation` (Text, Punktzustand,
   Fortschritt, Knopf sichtbar ja/nein). Rein heißt: ohne Widgets, ohne Uhr,
   ohne Datenbank — nur so ist die Zustandstabelle vollständig testbar.
3. Den Widget-Aufbau: Trennlinie, 6px-Punkt (`.reprise-feed-footer-dot`,
   `.live` = Akzent), Label, rechts **entweder** Icon-Knopf
   (`view-refresh-symbolic`, Tooltip `Reload`) **oder** Fortschrittsleiste —
   nie beides.
4. Eine eigene `css()`.

Die Unterscheidung zwischen `Loaded` („beim Öffnen wirklich geladen") und
`Cached` („aus dem Cache bedient") ist der Kern von Beschluss 1 und darf
nicht zusammenfallen. Der Auftragswortlaut „beim Öffnen fetchen" ist
ausdrücklich als **Anzeigeanforderung** gelesen und vom Auftraggeber so
bestätigt; die Fetch-Politik CONC-5a bleibt unverändert.

`{time}`: heute → `%H:%M` lokal; älter → kurzes Locale-Datum.

**Ziel:** Ein Baustein, den Concerts, Releases und das Popover ohne
Anpassung benutzen können.
**Nachweis:** `conc_15_the_footer_never_claims_up_to_date_while_fetching`
grün; **jeder** der neun Zustände hat eine Zeile im Test.

#### 19. Die Concerts-Fußzeile stellt um

- `concerts_view.rs::build_footer()` (`:446-474`) weicht dem gemeinsamen
  Baustein; die Datei **schrumpft** von 709 Zeilen.
- Eine Zeile in `ui/style/mod.rs` neben `super::concerts::css::css()` (`:133`),
  `super::releases::css::css()` (`:134`) und `super::updates::css()` (`:135`)
  registriert `feed_footer::css()`.
- `concerts_presentation.rs::updated_ago()` (`:100`) und
  `strings_concerts.rs::concerts_updated_ago()` (`:102-123`) fallen ersatzlos.
- Die neuen Strings kommen nach `strings_concerts.rs`; die fünf
  flächenneutralen (`Up to date — loaded at {time}`, `Up to date — checked
  {time}`, `Not loaded yet`, `Online sources are off`, `Reload`) werden dort
  **wörtlich dupliziert** — Strang 2 legt dieselben msgid in
  `strings_news.rs` an. Das ist Absicht (Beschluss 17): gettext fasst
  identische msgid ohnehin zusammen, und eine geteilte Datei wäre eine Datei,
  in die zwei Stränge schreiben.

**Ziel:** Die Concerts-Fußzeile sagt `Updating concerts …` mit laufendem
Fortschritt und danach `Up to date — loaded at 14:32`; beim zweiten Öffnen
innerhalb der TTL `Up to date — checked 14:32`, ohne Netzverkehr.
**Nachweis:** Screenshot 7 der Abnahme (vier Zustände) plus die Messung
„Beim Öffnen wird nicht bedingungslos gefetcht" aus §5.2 des Mutterplans:
Ansicht zweimal öffnen, der zweite Vorgang darf per `REPRISE_LOG` **keine**
Netzanfrage zeigen.

#### 20. Den Öffnen-Pfad gegen CONC-5a prüfen

`request_fetch()` (`concerts_view.rs:487`) hat zwei Aufrufstellen, `:201` und
`:209`. Genau **eine** davon ist der Öffnen-Pfad und muss die
Veraltungsprüfung tragen. Steht dort heute bedingungslos `force`, ist **das
der Bug** — nicht der Zielzustand.

**Ziel:** Öffnen löst nur innerhalb der Veraltungsregel einen Abruf aus;
der Reload-Knopf ist der einzige unbedingte Auslöser.
**Nachweis:** dieselbe `REPRISE_LOG`-Messung wie in Aufgabe 19; zusätzlich
wird CONC-5a zu CONC-5b umgeschrieben (unten) — der Auslöser heißt jetzt
„the footer's reload button", alles Übrige bleibt wörtlich.

---

### 21. Der Nachtrag E2b geht an den Standort-Plan

In `docs/plans/location-is-not-a-concerts-setting.md`, Paket E, wird **E2b**
ergänzt und **committet** — nicht bloß hier vermerkt:

> **E2b** — Der Standort-Chip nennt mit gesetztem Standort Ort **und**
> Radius: `{city} · {radius} km`, englischer Quellstring
> `N_!("{city} · {radius} km")`, Ort aus `location::app_location().name`,
> Radius aus dem aktiven Filter.

Grund für die Übergabe: E2 jenes Plans schreibt genau diesen Chip
(`concerts_filter_bar.rs:372-382`) ohnehin neu. Zwei Pläne, die dieselbe
Funktion umbauen, kollidieren garantiert.

**Ziel:** Der Chip-Text ist in dem Plan festgehalten, der ihn umsetzt.
**Nachweis:** der Eintrag steht im Branch dieses Strangs und geht mit ihm ins
`dev` — ein Nachtrag, der nur im abgebenden Plan steht, erreicht den
empfangenden Codex-Lauf nie.

---

### 22. Die UX-Regeln in Abschnitt AE

Prozessvertrag: eine Regel wechselt `[planned]` → `[active]` **in demselben
Commit**, der das Verhalten baut und den regelbenannten Test hinzufügt. Ein
Test trägt **genau eine** primäre Regel-ID im Namen.
`scripts/check-ux-traceability.sh` ist Merge-Gate.

**Neu zu schreiben** (Volltext in §4.2 des Mutterplans — wörtlich übernehmen,
nicht neu formulieren):

| ID | Level | Test |
|---|---|---|
| `CONC-12` | `[active] [core]` | `conc_12_offsale_never_becomes_sold_out` (`concerts/availability.rs`, `#[cfg(test)]`) |
| `CONC-13` | `[active] [gtk]` | `conc_13_a_row_without_a_target_does_not_activate` (`ui/concerts/concerts_view_tests.rs`) |
| `CONC-14` | `[active] [gtk]` | `conc_14_the_similar_caption_shrinks_before_the_artist` (`ui/concerts/concerts_view_tests.rs`) |
| `CONC-15` | `[active] [gtk]` | `conc_15_the_footer_never_claims_up_to_date_while_fetching` (`ui/feed_footer.rs`, `#[cfg(test)]`) |
| `CONC-16` | `[active] [gtk]` | `conc_16_the_source_column_is_available_but_off_by_default` (`ui/concerts/concerts_view_tests.rs`) |
| `CONC-4c` | `[active] [gtk]` | bestehender `conc_4b_…`-Test wird umgehängt |
| `CONC-5b` | `[active] [core]` | bestehender `conc_5a_…`-Test wird umgehängt |
| `CONC-11a` | `[active] [gtk]` | bestehender `conc_11_…`-Test wird umgehängt |

**Statusmarker zu setzen** (die alten Regeln bleiben stehen, sie bekommen nur
den Marker):

| Alt | Zeile (Pin) | Marker |
|---|---|---|
| `CONC-3` | 4956 | `[replaced by CONC-13]` |
| `CONC-4b` | 4964 | `[replaced by CONC-4c]` |
| `CONC-5a` | 4978 | `[replaced by CONC-5b]` |
| `CONC-10` | 5004 | `[replaced by CONC-14]` |
| `CONC-11` | 5009 | `[replaced by CONC-11a]` |

**`CONC-7` (`:4988`) NICHT anfassen** — diese eine Zeile gehört Strang 2.
`CONC-2` ebenfalls nicht: sie gehört dem Standort-Plan (Beschluss 3).

**Zusätzlicher Test ohne neue Regel:** Abschnitt K braucht keine neue ID,
bekommt aber
`fil_3a_the_concerts_end_of_results_sits_below_the_last_row`
(`ui/concerts/concerts_view_tests.rs`) unter der bestehenden FIL-3a.

Neues wird **ans Ende** von Abschnitt AE angehängt (`≈:5020` gegen den Pin,
`≈:5024` auf dem aktuellen Tip), weit unterhalb von CONC-7 — damit git den
Merge mit Strang 2 konfliktfrei fahren kann.

---

## Was dieser Strang NICHT verifiziert

Die folgenden Prüfungen lesen Dateien, die dieser Strang **nicht besitzt**.
Sie können vor dem Merge prinzipiell nicht grün werden. **Nicht auf sie
warten und nicht versuchen, sie vorzuziehen** — ein Strang, der darauf wartet,
bleibt mit fertiger, korrekter Arbeit stehen. Sie stehen vollständig in
**§7, „Post-Merge-Querprüfungen", des Mutterplans**:

1. `scripts/check-ux-traceability.sh` über die **ganze** `docs/ux-rules.md` —
   dieser Strang sieht nur Abschnitt AE, nie die Regeln der Stränge 2 und 3.
2. „Ein Wort, zwei Flächen": derselbe Status in Tabelle (hier) und
   Popover-Zeile (Strang 2).
3. „Ein Zeitstempel, drei Fußzeilen": `git grep -n 'Updated .*ago'` über
   `crates/reprise-gnome/src/ui` — liest auch Strang 2s Dateien.
4. „Dieselbe URL": Benachrichtigung (Strang 3) gegen Popover-Zeile (Strang 2)
   — betrifft diesen Strang gar nicht.
5. Geometrie-Parität Popover-Zeile gegen Tabellenzeile (Anforderung R4) —
   per Konstruktion erst nach dem Merge messbar.
6. **Migrationskette am Stück**: beide Migrationen stammen zwar jetzt aus
   diesem Strang, der vollständige Beweis braucht aber die **Schreibseite**
   der Spalte `notified_released_at`, und die gehört Strang 3. Dieser Strang
   weist nur nach, dass die Kette von v72 auf 74 läuft und beide Spalten
   existieren.

Was dieser Strang **sehr wohl** vor dem Merge liefert: `cargo fmt --check`,
`cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`, die Kernreinheit aus Aufgabe 9,
jede angefasste Datei unter 800 Zeilen, und die Screenshots 4, 5, 6 und 7 der
Abnahme (§5.1 des Mutterplans) — Punkt 6 **mit Kontrollarm**.

Das Display-Gate ist im Rudel bekanntermaßen flaky und auf `dev` teils schon
rot: **zuerst gegen `origin/dev` messen, was ohne diese Änderung rot ist**,
sonst wird fremdes Rot als eigene Schuld verbucht.
