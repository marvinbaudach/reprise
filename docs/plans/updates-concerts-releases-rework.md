---
slug: updates-concerts-releases-rework
worktree:
branch:
phase: shipped
codex_session:
created: 2026-08-14
strands: 1,2,3
merge_order: 1,2,3
---
# Updates, Concerts und Releases — eine Zeile, ein Zeitstempel, ein Status

> **Mutterplan.** Er trägt den gemeinsamen Kontext: Ausgangslage, alle
> Beschlüsse, die englischen Quellstrings, die UX-Regeln, die Abnahme und die
> Parallelität. Er wird mit dem Ende der Planungsphase **eingefroren**. Die
> Arbeit steht in drei Strangdateien:
>
> | Strang | Datei | Zweck |
> |---|---|---|
> | 1 | `docs/plans/updates-concerts-releases-rework-1.md` | `core-concerts` — Kern, Concerts-Tabelle, Migrationen, `feed_footer.rs` |
> | 2 | `docs/plans/updates-concerts-releases-rework-2.md` | `updates-popover` — Popover, Releases-Fußzeile |
> | 3 | `docs/plans/updates-concerts-releases-rework-3.md` | `update-notifications` — Benachrichtigung, Einstellung |
>
> Jeder Codex-Lauf liest **seine Strangdatei plus diesen Mutterplan**. Die
> Strangdatei sagt, was zu tun ist; der Mutterplan sagt, warum.
>
> `worktree:` und `branch:` bleiben hier **leer**. Der Mutterplan wird nie
> selbst implementiert; leere Felder verhindern, dass das Landeskript zu einem
> Branch zwei Pläne findet.

> Alle Zeilennummern gegen `origin/dev` @ `5721ade95e` (14.08.2026, nach #471).
> Der geteilte Hauptcheckout steht auf `be5f014d3b` und taugt nicht als Basis.
> Lesen per `git show origin/dev:<pfad>`, den Hauptcheckout **nicht** umschalten.
> Wo eine Nummer nicht mehr passt, gilt der genannte Funktions- oder Symbolname —
> nachziehen, nicht raten.
>
> **Drift seit dem Pin:** `origin/dev` ist inzwischen auf `a7febd7d92` (#476,
> „Device card status, one cancel affordance, and playlist sync"). Von den
> Dateien, die dieser Plan anfasst, hat sich **genau eine** geändert:
> `docs/ux-rules.md` bekam einen einzigen Hunk bei `:1059-1068` (MTP-63),
> netto **+1 Zeile**. Auf dem aktuellen Stand liegt also jede
> `docs/ux-rules.md`-Zeile ab `:1062` um **eins höher** als in diesem Plan
> notiert (Abschnitt H `:1320`, FIL-3a `:1540-1556`, Abschnitt R `:2130`,
> Abschnitt AE `:4941`, …). Kein Code-Pfad dieses Plans ist betroffen.

Entwurf: Claude-Design-Projekt `2224f5ff-00b4-4614-9120-22be5cc2e5be`,
Datei `Updates Popup.dc.html`, Panels **1a** (Popup), **1b** (Benachrichtigung),
**2a/2b** (Concerts-Übersicht). Panel **1c** ist ein überholter Vorentwurf
(vier Spalten, `Artist` zuerst) und gilt **nicht** — es gewinnt 2a mit der
Spaltenfolge `Date · Artist · City · Venue · Distance · Tickets`.

Die App-Quellsprache ist **Englisch**. Der Entwurf ist auf Deutsch gezeichnet;
jede deutsche Beschriftung darin ist die Sprache des Designers. Dieser Plan
nennt für jede davon den englischen Quellstring wörtlich. Deutsch entsteht in
`po/de.po` über den normalen Extraktionslauf, nicht von Hand.

---

## 0. Ausgangslage

### 0.1 Das Updates-Popover

`ui/updates/shell.rs` (212 Z.) baut den Aufbau: Kopf mit Zähl-Tag
(`:46-51`), Release-Liste, `ConcertsSection`, dann **zwei Sprungzeilen**
(`build_jump_row()`, `:120-132`, Klassen `new-release-history-row` /
`new-release-history-label`) und ein Kopf-Bereich `build_header()`
(`:134-170`), in dem der **Fetch-Knopf das Alters-Label enthält** — der
Test `:172-212` friert genau das ein („updated label lives inside
fetch_button"). Icon des Knopfes: `view-refresh-symbolic` (`:145`).

`ui/updates/popover.rs` (786 Z., **kein Spielraum**) hält
`footer_presentation()` (`:47-54`, liefert `updated` als Altersstring plus
`show_cached_failure`), `wire_jump()` (`:276`), `render()` (`:290`),
`REFRESH_TIMER_SECONDS = 3600` (`:124`), `start_fetch()` (`:560`),
`start_news_fetch()` (`:606`), `start_concerts_fetch()` (`:634`),
`finish_feed()` (`:668`).

`ui/updates/release_row.rs` (501 Z.) baut die Zeile: `LazyReleaseCover`,
Titel-Label (`new-release-title`), Meta-Label (`new-release-meta`),
`ReleaseRowActions`. `release_row_actions.rs` (314 Z.) baut den Status-Chip
(`chip()`, `:94-117`) und die Aktionsbox, die im Ruhezustand
**Opazität 0** hat (`:119-152`) — dort sitzt auch der Hide-Knopf mit
`view-conceal-symbolic` und dem Label `HIDE_RELEASE`, sowie ein
Primärknopf mit `go-jump-symbolic` bzw. `external-link-symbolic`.

`ui/updates/css.rs` (295 Z.) hält alle `new-release-*`-Selektoren; die
Registrierung läuft über `ui/style/mod.rs:135` (`super::updates::css()`),
daneben stehen dort `super::concerts::css::css()` und
`super::releases::css::css()` — beide sind also bereits verdrahtet.

### 0.2 Die Concerts-Übersicht

`ui/concerts/concerts_columns.rs` (579 Z.): sechs Spalten in
`append_columns()` (`:302-357`) — Date, Artist, City, Venue, Distance,
Tickets. Die Tickets-Zelle ist ein echter `gtk4::Button` mit `flat`/`link`
(`:220-287`); ihr Label kommt aus `ticket_button_label()`
(`concerts_presentation.rs:87-98`) und ist der **Quellenname**
(`"Ticketmaster"`, `"Eventim"`, …) oder generisch `"Tickets"` oder `None`.
`ticket_target()` liegt **in derselben Datei** (`concerts_columns.rs:23`,
nicht in `concerts_presentation.rs`) und probiert `ticket_url`, dann
`event_url`, beide durch `is_launchable_url` gefiltert. Zeilenaktivierung heute:
`column_view.connect_activate()` (`concerts_view.rs:232`) — also
**Doppelklick/Enter**, kein Einfachklick, kein `GestureClick`.

`concerts_view.rs` (709 Z.): `build_footer()` (`:446-474`) liefert
`updated`-Label (`dim-label caption`) plus `FETCH_NOW`-Knopf plus Spinner;
`request_fetch()` (`:487`), Aufrufstellen `:201`/`:209`; `apply_sort()`
(`:689-701`).

`concerts_presentation.rs` (223 Z.): `format_distance_km()` (`:32`),
`row_distance()` (`:39`), `ticket_button_label()` (`:87`),
`updated_ago()` (`:100`).

`concerts_filter_bar.rs` (762 Z.): `active_facets()` (`:62-77`),
Chip-Aufbau in `rebuild()` (`:349-399`), `has_location` (`:372`, `:413`).

### 0.3 Die Releases-Seite

`releases_view.rs` (687 Z.): `build_footer()` (`:421`) liefert
`fetch_label`, `updated`, `progress`; `apply_footer()` (`:476`) ruft
`releases_footer_presentation()` (`releases_presentation.rs:38-69`) mit den
drei Zuständen `Idle { latest }` / `Starting` / `Running(progress)`.
`releases_columns.rs` (798 Z., **kein Spielraum**) hält
`column_contract()` (`:26-40`) mit `Cover · Date · Title · Artist · Type ·
Status · Link` — **diese Datei fasst der Auftrag nicht an** (NR-30, NR-33).

### 0.4 Der Kern

`concerts.rs:79-97` — `ConcertRow` mit 17 Feldern, darunter `ticket_url`,
`ticket_source`, `event_url`, `is_similar`, `similar_to`, **kein**
Verfügbarkeitsfeld. `provider.rs:36-49` — `ProviderEvent`, ebenfalls ohne.
`db_concerts.rs:20-41` — Tabelle `concert_events`, ebenfalls ohne;
einzige Migration dort `migrate_v31()` (`:49-59`), aufgerufen in
`db.rs:713`. Die Kette endet bei `db_artwork::migrate_v72` (`db.rs:754`,
letzte Zeile vor `Ok(())`), `SUPPORTED_SCHEMA_VERSION = 72` (`db.rs:26`).

Die Tabelle `new_releases` wird im Basis-Schema `db.rs:370` angelegt; ein
Modul `db_new_releases.rs` gibt es **nicht**. Vorhanden sind nur die
themenbenannten `db_new_releases_accent.rs` (`:63`) und
`db_new_releases_history.rs` (`:39`), die die Tabelle je für ihre eigene
Migration neu bauen. Wo `migrate_v74` wohnt, klärt **Nachtrag 1**
(siehe §8).

`ticketmaster.rs:118-157` — `parse_event()` liest `/dates/start/localDate`,
`/dates/start/localTime`, `/_embedded/venues[0]/…`, das oberste `url`; es
setzt `ticket_url` und `event_url` auf **denselben** Wert und
`ticket_source` fest auf `"Ticketmaster"`. `/dates/status/code` wird **nie
gelesen**. `bandsintown.rs:100-140` — sucht das erste Angebot mit
`status == "available"`, nimmt dessen URL und **verwirft die Information**;
ohne verfügbares Angebot bleibt `ticket_url = None`, das Event bleibt.

`pipeline.rs` `reconcile_artist()` — ein **positioneller 18-Spalten-Upsert**
(`?1`…`?18`) mit passendem `ON CONFLICT … DO UPDATE SET`-Block; das SQL
steht in `concerts/pipeline.rs:398-436`.

`artist_news_query.rs:30-44` — `StoredRelease` mit `first_release_date`,
`fetched_at`, `seen_at`, `hidden`. Ob ein Release *upcoming* oder
*released* ist, entscheidet `release_kind()` /`announcement_kind()` in
`artist_news_parsing.rs:294` allein aus dem Datum: `> heute` → `Upcoming`,
`heute-90 d … heute` → `New`, sonst `Catalog`.

### 0.5 Hausform für Nachbarthemen

`ui/notifications.rs` (72 Z.) ist die einzige Benachrichtigungsstelle:
`gio::Notification::new(&title)`, `set_body`, `send_notification(Some("now-playing"), …)`,
Cover asynchron nachgeladen und über einen Generationszähler gegen
veraltete Treffer gesichert (`:42-44`). **Keine `default`-Aktion** wird
heute gesetzt; `gio::SimpleAction` ist die Hausform für Aktionen
(z. B. `compact/compact_player_menu.rs:58ff`).

`ui/external_link.rs:23-44` — `launch(url, context, on_error)` über
`gtk4::UriLauncher`, mit `reprise_core::external_link::is_launchable_url`
als Torwächter. **Jeder** externe Aufruf geht hier durch.

Einstellungen sind Key/Value in der SQLite-Tabelle `settings`; die
Zugriffsschicht liegt in **`crates/reprise-core/src/library/settings.rs`**
und `settings_api.rs` (Zeilennummern hier bewusst weggelassen — die Datei ist
groß und die Nummern des Entwurfs ließen sich nicht bestätigen; am
Funktionsnamen orientieren). Es gibt **kein** GSettings-Schema (GP-6 ist
`[planned]` und bleibt es).

---

## 1. Beschlüsse

Nummeriert. Jeder Beschluss, der einen früheren umkehrt, nennt ihn.
Beschlüsse 1–8 beantworten die acht offenen Punkte des Auftrags; 9–18 nageln
den Rest fest.

**Stand nach dem Grilling (14.08.2026):** alle 18 Beschlüsse sind vom
Auftraggeber abgenommen. Die einzige inhaltliche Änderung gegenüber dem
Entwurf betrifft **§7 Parallelität** — `crates/reprise-core/src/db.rs` gehört
jetzt ausschließlich Strang 1, der **beide** Migrationen anlegt. Beschluss 2
und Beschluss 13 sind entsprechend nachgezogen.

### Beschluss 1 — „Beim Öffnen fetchen" ändert die Anzeige, nicht die Netzpolitik

**Entschieden: Lesart (b).** Der Veraltungs-Gate aus **CONC-5a** bleibt
(24-h-TTL + Jitter, stündliche Fälligkeit); was sich ändert, ist die
Fußzeile: sie zeigt ab jetzt den **Live-Zustand** statt eines
Aktualisierungsalters, und der `Fetch now`-Knopf wird zum
Icon-Knopf `view-refresh-symbolic` mit Tooltip `Reload`.

**Ausdrücklich abgenommen (Grilling, 14.08.2026):** der Auftragswortlaut
„beim Öffnen fetchen" wird als **Anzeigeanforderung** gelesen, nicht als
Netzanforderung. Der Auftraggeber hat diese Lesart bestätigt; die Fetch-
Politik bleibt damit unverändert bei CONC-5a mit 24-h-TTL. Der TTL-Kompromiss
auf 1 h ist ebenfalls ausdrücklich verworfen.

Begründung: CONC-5a ist keine Bequemlichkeit, sondern die Antwort auf
1 Anfrage/s über Bandsintown/Ticketmaster/Nominatim hinweg bei
`MAX_ARTISTS_PER_RUN = 30`. Bedingungsloses Abrufen bei jedem Öffnen
multipliziert die Anfragen mit der Navigationshäufigkeit — bei einer App,
in der man zwischen Ansichten hin- und herspringt, ist das die eine
Änderung, die einen Anbieter-Key sperren lässt. Der Auftragstext beschwert
sich außerdem inhaltlich nicht über zu alte Daten, sondern über den
**Footer** („Kein ‚Updated 2 h ago' + ‚Fetch now'-Button mehr"). Der Satz
„Aktuell — beim Öffnen geladen, HH:MM" ist nur dann wahr, wenn beim Öffnen
wirklich geladen wurde — die Anzeige muss also ohnehin zwischen „geladen"
und „aus dem Cache bedient" unterscheiden können. Genau das leistet (b).

**Verworfene Alternative (a):** bedingungslos bei jedem Öffnen abrufen.
Kehrt CONC-5a um, verletzt das gemeinsame 1-req/s-Limit bei schneller
Navigation, und macht aus einem Anzeigeproblem ein Netzproblem.

**Die vollständige Zustandstabelle der Fußzeile** (gilt für Concerts,
Releases und das Popover; `{unit}` ist pro Fläche fest ausformuliert, nicht
zur Laufzeit zusammengesetzt):

| Zustand | Auslöser | Punkt | Text (englischer Quellstring) | rechts |
|---|---|---|---|---|
| `Loaded { at }` | in dieser Sitzung fertig geladen | Akzent | `Up to date — loaded at {time}` | Icon-Knopf `view-refresh-symbolic`, Tooltip `Reload` |
| `Cached { at }` | innerhalb der TTL, kein Abruf beim Öffnen | Akzent | `Up to date — checked {time}` | derselbe Icon-Knopf |
| `Fetching { checked, total }` | Abruf läuft (Öffnen, Fälligkeit oder Knopf) | gedimmt | `Updating concerts …` / `Updating releases …` / `Updating …` | Fortschrittsleiste, Anteil `checked/total`; `total == 0` → unbestimmt |
| `Failed { latest }` | letzter Abruf scheiterte, Cache vorhanden | gedimmt | `Update failed — showing saved concerts from {time}` (analog `releases` / `updates`) | Icon-Knopf |
| `Offline { latest }` | Fenster meldet „offline" | gedimmt | `Offline — showing saved concerts from {time}` (analog) | Icon-Knopf |
| `NeverFetched` | kein Cache, kein laufender Abruf | gedimmt | `Not loaded yet` | Icon-Knopf |
| `NoCredentials` | Concerts ohne Anbieter-Schlüssel | gedimmt | `Concerts needs provider credentials` (bestehende Konstante `CONCERTS_NEEDS_CONFIGURATION`) | **kein** Knopf |
| `NetworkOff` | Online-Gate aus (`online_sources::network_allowed()` falsch) | gedimmt | `Online sources are off` | **kein** Knopf |
| `ModuleOff` | Modul abgeschaltet | — | Fußzeile ist **nicht sichtbar** | — |

`{time}`: heute → `%H:%M` in der lokalen Zeitzone; älter → das kurze
Locale-Datum. Der Zeitstempel steht **genau einmal je Fläche** — in der
Fußzeile, nie zusätzlich in einer Zeile oder im Knopf.

Die ehrliche Unterscheidung zwischen `Loaded { at }` („beim Öffnen wirklich
geladen") und `Cached { at }` („aus dem Cache bedient") ist der Kern dieses
Beschlusses und ausdrücklich bestätigt: die Fußzeile behauptet nie, geladen
zu haben, wenn sie nur nachgesehen hat.

Fußzeile des Popovers: dieselbe Tabelle, mit `updates` als Einheit.
Der Zustand ist die **Aggregation** beider Feeds: läuft einer, gilt
`Fetching`; sonst zählt der **ältere** der beiden Zeitstempel (die
Fußzeile behauptet damit nie mehr Frische als die schwächere Hälfte). Der
Reload-Knopf löst beide Feeds aus. **Der Widerspruch aus dem
Design-Auszug ist damit aufgelöst:** Mock 1a zeigt das alte Paar
„Updated 18 h ago" + `Fetch now` noch, der Auftragstext streicht es. Der
Auftragstext gewinnt, weil er auch für das Popover „Zeitstempel nur einmal
im Footer" verlangt.

### Beschluss 2 — „Sold out" heißt `Off sale`, und die Daten dafür entstehen wirklich

**Entschieden: Variante (c)** — drittes, neutrales Wort, aber mit echtem
Datenweg statt Vertagung.

Neuer Kern-Typ `TicketAvailability { OnSale, OffSale, Unknown }`
(`crates/reprise-core/src/concerts/availability.rs`), persistiert als TEXT
(`on_sale` / `off_sale` / `unknown`). Anbieter-Abbildung:

| Anbieter | Signal | Ergebnis |
|---|---|---|
| Ticketmaster | `/dates/status/code == "onsale"` | `OnSale` |
| Ticketmaster | `/dates/status/code == "offsale"` | `OffSale` |
| Ticketmaster | `cancelled`, `postponed`, `rescheduled`, fehlend, unbekannt | `Unknown` |
| Bandsintown | ein Angebot mit `status == "available"` | `OnSale` |
| Bandsintown | `offers` vorhanden, keines `available` | `OffSale` |
| Bandsintown | `offers` fehlt oder leer | `Unknown` |

Benutzer-Strings: `On sale`, `Off sale`, `Unknown`. Tooltip auf `Off sale`:
`The ticket source reports no active sale. This can mean sold out, or not on sale yet.`

Begründung: „Sold out" ist eine Behauptung über einen Sachverhalt, den
keine der beiden Quellen kennt. Ticketmasters `offsale` heißt gleichermaßen
„Vorverkauf noch nicht offen", „Verkaufsfenster vorbei" und „ausverkauft";
Bandsintowns fehlendes Angebot heißt oft schlicht „nie Ticketinfo gehabt".
`Off sale` ist wörtlich das, was die Quelle sagt, es steht typografisch
und semantisch neben `On sale`, und der Tooltip nennt beide möglichen
Ursachen. Damit bleibt das dritte Tag des Entwurfs erhalten, ohne dass die
App etwas erfindet.

**Verworfene Alternative (b):** `offsale` bei zukünftigem Termin als
„Sold out" deuten. Das ist eine als Status verkleidete Vermutung; sie ist
in der Mehrheitsrichtung falsch (Vorverkaufsstart ist häufiger als
Ausverkauf) und niemand kann sie im Nachhinein widerlegen, weil die App
das Rohsignal nicht mehr aufhebt.
**Verworfene Alternative (a):** nur `On sale`/`Unknown`. Wirft das
`offsale`-Signal weg, das Ticketmaster tatsächlich liefert, und lässt das
dritte Tag des Entwurfs für immer leer.

**Migration konkret.** Neue Funktion `migrate_v73()` in
`crates/reprise-core/src/db_concerts.rs` nach dem Muster von `migrate_v31`
(`:49-59`): `PRAGMA user_version` lesen, `if version >= 73 { return Ok(()) }`,
`unchecked_transaction()`, `ALTER TABLE concert_events ADD COLUMN
ticket_availability TEXT NOT NULL DEFAULT 'unknown'`,
`pragma_update(None, "user_version", 73)`, `commit()`. Aufruf in `db.rs`
**nach** `db_artwork::migrate_v72` (`:754`).

**`SUPPORTED_SCHEMA_VERSION` (`db.rs:26`) wird in einem Zug auf `74`
gesetzt, nicht auf `73`** — Strang 1 ist alleiniger Besitzer von `db.rs` und
legt auch `migrate_v74` aus Beschluss 13 an. Begründung siehe §7; ein
Zwischenschritt auf `73` gäbe es nur, wenn zwei Stränge dieselbe Konstante
anfassen dürften, und genau das ist ausgeschlossen.

**Stolperstellen, die der Compiler nicht findet:**
1. Der **positionelle 18-Spalten-Upsert** in `pipeline.rs::reconcile_artist`
   wird zu 19 Spalten: Spaltenliste, `VALUES (?1 … ?19)`, `params![…]` und
   der `ON CONFLICT … DO UPDATE SET`-Block (dort
   `ticket_availability = excluded.ticket_availability`) müssen **alle vier**
   mitwachsen. Eine vergessene Stelle ist kein Compilerfehler, sondern ein
   Laufzeit-`rusqlite`-Fehler oder eine still nie aktualisierte Spalte.
2. Jede `SELECT`-Spaltenliste in `concerts/query.rs`, die
   `concert_events` liest, und jedes `row.get(n)` dahinter: heute
   `query_cached_events()` (`:44-82`) und `filtered_events()` (`:179`ff).
   Die Indizes sind positionell — eine neue Spalte in der Mitte verschiebt
   alles danach. **Die neue Spalte immer am Ende anhängen.**
3. `ProviderEvent` (`provider.rs:36-49`), `ConcertRow` (`concerts.rs:79-97`)
   und `CachedConcertEvent` bekommen das Feld; alle feldweise gebauten
   Test-Helfer findet der Compiler.

### Beschluss 3 — Der Standort gehört dem anderen Plan; dieser Plan färbt nur

**Entschieden: (a) Vorbedingung mit scharfem Zuständigkeitsschnitt.**

Der Plan `location-is-not-a-concerts-setting`
(Branch `feature/location-is-not-a-concerts-setting`, `phase: planned`,
Worktree `/home/marvin/Projects/reprise-location-is-not-a-concerts-setting`,
ein Commit `958209c387` = nur das Dokument) besetzt mit **Paket E**
(`:231-302`) `concerts_filter_bar.rs`, `concerts_columns.rs`,
`concerts_view.rs`, `concerts_column_layout.rs` und ist dort **präziser**
als unser Auftragstext: E1 hängt die Aktivität des Radius-Facets an den
Standort (die Wurzel von „415 of 415"), E4 blendet die Distance-Spalte aus,
**ohne die persistierte Spaltenwahl des Nutzers zu überschreiben**, E5
verhindert eine wiederhergestellte Distanz-Sortierung auf eine versteckte
Spalte. Diese drei Feinheiten stehen in unserem Auftragstext nicht und
gingen bei einer Übernahme verloren.

Der Schnitt verläuft entlang **„Standort gesetzt" vs. „Standort fehlt"**:

| Anforderung | Wer |
|---|---|
| Ohne Standort: Radius-Facet inaktiv, Chip `500 km · off`, Leiste über der Tabelle, Distance-Spalte **ausgeblendet statt „—"**, keine Distanz-Sortierung | **jener Plan**, Paket E1–E5 |
| Mit Standort: Distanzen **innerhalb** des Radius in Akzentfarbe, alle anderen gedimmt | **dieser Plan**, Strang 1 |
| Standort-Chip nennt Ort **und** Radius (`Zürich · 500 km`) | **jener Plan**, als Nachtrag **E2b** (siehe unten) |

**Nachtrag E2b, den dieser Plan an jenen Plan übergibt** (dort in Paket E
zu ergänzen, hier nicht umzusetzen): der Chip **mit** gesetztem Standort
liest `{city} · {radius} km`, englischer Quellstring
`N_!("{city} · {radius} km")`, Ort aus `location::app_location().name`,
Radius aus dem aktiven Filter. Grund für die Übergabe: E2 schreibt genau
diesen Chip (`concerts_filter_bar.rs:372-382`) neu; zwei Pläne, die
dieselbe Funktion umbauen, kollidieren garantiert.

**Der Nachtrag E2b wird tatsächlich in
`docs/plans/location-is-not-a-concerts-setting.md` eingetragen und dort
committet** — nicht bloß hier als Notiz vermerkt. Das ist eine Aufgabe
dieses Auftrags und steht in der Strangdatei 1; ein Nachtrag, der nur im
abgebenden Plan steht, erreicht den empfangenden Codex-Lauf nie.

Die **Distanzfärbung** hängt dagegen an nichts aus Paket E — sie braucht nur
`app_location()` und den aktiven Radius, beides existiert heute. Sie bleibt
darum hier und ist **nicht** von der Merge-Reihenfolge abhängig.

**Verworfene Alternative (b) Übernahme:** zieht Radio (`radio_chips.rs`,
`add_dialog.rs`), die neue Preferences-Seite und den app-weiten
Standort-Broadcast in diesen Auftrag — thematisch fremd, und die drei
Feinheiten oben müssten neu erfunden werden.
**Verworfene Alternative (c) Vollständiger Schnitt:** hieße, die
Distanzfärbung ebenfalls fallen zu lassen. Sie ist die einzige
Distance-Anforderung, die jener Plan nicht abdeckt; sie hier zu streichen
ließe sie zwischen zwei Plänen verschwinden.

### Beschluss 4 — Die Abschnittsüberschrift wird die Brücke ins Vollbild

**Entschieden:** Die Abschnittsköpfe `Releases` und `Concerts` im Popover
werden **anklickbare Knöpfe**. Ein Klick (oder Enter/Space bei Fokus)
schließt das Popover und öffnet die zugehörige Vollansicht — genau das, was
`wire_jump()` (`popover.rs:276`) heute für die Sprungzeilen tut. Rechts im
Kopf steht weiter der Zähl-Chip mit NR-23s Semantik (nennt die volle
Stapelgröße, erscheint nur, solange der Stapel wirklich ungesehen ist).

**Der Kopf bleibt sichtbar, solange sein Modul aktiv ist — auch bei leerem
Abschnitt**, und zeigt dann darunter eine ruhige Leerzeile
(`No new releases` / `No new concerts`). Damit bleibt NR-23s ausdrückliche
Zusage („jump rows remain visible while their module is active, even when
its delta section is absent") in neuer Gestalt erhalten, und das Popover
wird keine Sackgasse.

Begründung: Der Kopf steht ohnehin da, kostet keine Zeile, und die
Navigation sitzt an genau dem Wort, das den Zielort benennt. Als echter
`gtk4::Button` bringt er Fokusring, Tastaturaktivierung und
Barrierefreiheits-Rolle mit — ACC-1 (volle Eingabeparität) ist ohne
Zusatzarbeit erfüllt. Die Seitenleiste bleibt der zweite Weg, ist aber
nicht der einzige.

**Verworfene Alternative:** ein `View all`-Element in der Fußzeile. Die
Fußzeile trägt jetzt Punkt, Live-Text und Reload-Knopf; ein vierter Platz
drängt, und die Navigation stünde weit weg von dem Abschnitt, den sie
meint — bei zwei Abschnitten in einer Fußzeile sogar mehrdeutig.
**Ebenfalls verworfen:** die Seitenleiste als einziger Weg. Das ist der
Zustand, den NR-12a/NR-23 damals ausdrücklich behoben haben.

### Beschluss 5 — Die Quelle bekommt eine eigene, abschaltbare Spalte

**Entschieden: (a) hover-freier Zweitort.** Die Concerts-Tabelle bekommt
eine **siebte Spalte `Source`**, in `concerts_column_layout.rs` registriert,
**standardmäßig ausgeblendet**, über das bestehende Kopf-Popover
(`concerts_view.rs:105-108`) einschaltbar, Sichtbarkeit persistiert wie
bei allen anderen Spalten. Inhalt: der reine Anbietername als Label
(`ticket_source`, sonst `provider`) — kein Knopf, kein Link-Icon.

Damit ist **TIP-3** erfüllt: die Information aus dem Zeilentooltip
(`Opens {source}`) ist ohne Hover in einer Ansicht erreichbar, und zwar
für beide Flächen — die Popover-Zeile verweist auf dieselbe Tabelle. Der
Tooltip bleibt, was TIP-3 verlangt: ein Komfort-Duplikat.

Das ist **kein** Rückbau der gestrichenen Spalte: gestrichen wird die
Spalte, die Quellenname und Kaufaktion in einem Knopf vermengte. Neu
getrennt: `Tickets` sagt „kann ich kaufen", `Source` sagt „wer behauptet
das". Im Auslieferungszustand sieht der Nutzer genau das, was der Auftrag
verlangt — die Quellenspalte ist aus.

**Abnahme-Notiz (Grilling, 14.08.2026):** Der Auftraggeber hat ausdrücklich
abgenommen, dass dieser Beschluss dem Wortlaut des Auftrags („Entfernen: die
Quellen-Spalte") **formal widerspricht**, und ihn dennoch so entschieden,
weil TIP-3 sonst verletzt wäre. Diese Abweichung ist bewusst und braucht im
Review keine erneute Diskussion; sie gehört in die Commit-Nachricht des
Strangs 1.

**Verworfene Alternative (b) begründete Ausnahme von TIP-3:** TIP-3 ist
`[manual]` und der Traceability-Gate hält eine Verletzung nicht auf — genau
deshalb wäre eine stille Ausnahme hier billig und falsch. Eine bereits
vorhandene, persistierte Spaltenmechanik zu benutzen kostet weniger als die
Ausnahme zu verteidigen.

### Beschluss 6 — FIL-3a gilt unverändert; die Beschwerde ist ein Bug, keine Regel

**Entschieden: an die bestehende Grammatik angleichen, keine neue Regel.**

FIL-3a (`docs/ux-rules.md:1539-1555`) schreibt bereits wörtlich vor, was
der Auftrag verlangt: *„directly below the last row when the list is
shorter than the viewport"*. Die Beschwerde „nicht zentriert im Leerraum"
beschreibt damit eine **Abweichung vom Ist-Zustand gegenüber FIL-3a**, kein
Regeldefizit. Behoben wird der Code, nicht die Regel.

Die Wortwahl gleicht sich an FIL-3as Grammatik an, mit dem Radius als
benannter Einschränkung:

- `End of results — {hidden} concerts hidden by the {radius} km radius around {city}`
- ohne Ortsnamen: `End of results — {hidden} concerts hidden by the {radius} km radius`
- Pille: `Show all {total} concerts` (bestehende Funktion
  `strings::show_all_concerts`, `strings_concerts.rs:91-96`)

**Das Scroll-Verhalten aus FIL-3a bleibt vollständig erhalten:** bei Listen
länger als das Sichtfenster erscheint die Zeile erst, wenn das Ende in den
Blick scrollt; sie schwebt nie über Zeilen; sie ist nicht sticky; das
Overlay bleibt eingabetransparent außer der Pille.

**Die Zentrierung bleibt, und die Linksbündigkeit aus Mock 2b ist bewusst
verworfen** (bestätigt im Grilling, 14.08.2026): FIL-3a bindet **sechs
Ansichten**; eine Ausnahme für genau eine davon zersplittert eine
funktionierende Grammatik. Der Entwurf verliert hier gegen die Hausregel,
und das ist die Entscheidung, nicht ein Versehen. Wer im Review die
Linksbündigkeit des Mocks vermisst: sie wurde gesehen und abgelehnt.

**Verworfene Alternative: eine Unterregel `FIL-3b`** für den
radiusspezifischen Fall. Sie wäre inhaltsleer — FIL-3a sagt „names the
restriction", und „the 500 km radius around Zürich" *ist* eine benannte
Einschränkung. Eine Regel, die nichts hinzufügt, verwässert nur den Index.

### Beschluss 7 — „similar to {seed}" bleibt, einzeilig, und schrumpft zuerst

**Entschieden:** Die Bildunterschrift wandert **in dieselbe Zeile**, als
gedimmtes Nachsatz-Segment direkt hinter dem Künstlernamen — in beiden
Flächen:

- **Concerts-Tabelle:** die Artist-Zelle wird eine `gtk4::Box` mit zwei
  Labels. Label 1 = Künstlername, `ellipsize = None`, `hexpand = false`.
  Label 2 = `similar to {seed}` (bestehende Funktion
  `strings::concert_similar_caption`, `strings_concerts.rs:87-89`),
  gedimmt, `ellipsize = End`, schrumpft also **zuerst**.
- **Popover-Zeile:** dieselbe Technik in der Titelzeile (bei Concerts ist
  der Titel der Künstler).

Damit ist CONC-6 unangetastet erfüllt (die gedimmte Unterschrift existiert
und verschwindet mit „Library artists only"), die Zeile bleibt einzeilig,
und bei Platzmangel geht die **Herkunftsangabe** verloren, nie der Name.
CONC-10s Forderung, dass die Unterschrift die Zeile *dehnt*, fällt — sie
wird durch CONC-14 ersetzt (siehe §4).

**Verworfene Alternative:** die Unterschrift in eine optionale Spalte
`Similar to` verschieben. Sie wäre standardmäßig aus und die Antwort auf
„warum steht dieser Künstler hier?" damit unsichtbar — genau die Frage, für
die CONC-6 existiert.

### Beschluss 8 — Doppelklick in der Tabelle, Einfachklick im Popover

**Entschieden: zwei Aktivierungsmodelle, je nach Fläche.**

**Concerts-Tabelle: Doppelklick und Enter bleiben** (`connect_activate`,
`concerts_view.rs:232`, unverändert). Ein Einfachklick, der einen Browser
öffnet, macht Auswählen unmöglich: `ColumnView` braucht den Einfachklick
für die Selektion, an der Sortierung, Tastaturnavigation (ACC-4a: Pfeile
navigieren zeilenweise) und jede künftige Mehrfachauswahl hängen. Ein
versehentlicher Streifklick würde außerdem eine externe Anwendung starten —
die teuerste Fehlbedienung, die eine Tabelle anbieten kann. GNOMEs eigene
Konvention trennt genauso: Einfachklick-Aktivierung für Navigationslisten,
Doppelklick für Datentabellen.

**Abnahme-Notiz (Grilling, 14.08.2026):** Der Auftragstext verlangt wörtlich
den **Zeilenklick**. Die Tabelle behält dennoch den Doppelklick, weil GTK4s
`single-click-activate` die Auswahl dem Mauszeiger folgen ließe. Der
Auftraggeber hat diese Abweichung ausdrücklich abgenommen — sie ist bewusst,
nicht übersehen, und gehört in die Commit-Nachricht des Strangs 1.

**Was sich in der Tabelle trotzdem ändert:** die Tickets-Zelle ist kein
Knopf mehr, sondern ein Status-Label. Damit ist die **Zeile die einzige
Aktivierungsfläche** — CONC-3s „Zeile *und* Ticket-Zelle" wird zu
„nur Zeile" (CONC-13, §4).

**Popover-Zeile: Einfachklick.** Dort gibt es keine Auswahl-Semantik zu
schützen; das Popover ist eine menüartige, transiente Fläche, in der ein
Einfachklick die Hausform ist. Umgesetzt **nicht** über `GestureClick`,
sondern als echter flacher `gtk4::Button`, der Cover + Titel + Meta + Tag
umschließt; der Hide-/Dismiss-Knopf ist sein **Geschwister**, nicht sein
Kind. Damit kann der Ignorieren-Knopf den Link prinzipiell nicht
mitauslösen (kein Event-Bubbling aus einem Knopf in einen anderen), und
Tastaturaktivierung, Fokusring und Rolle kommen von GTK.

**Verworfene Alternative: Einfachklick auch in der Tabelle.** Er kostet die
Auswahl-Semantik der Tabelle vollständig: es gäbe keine Möglichkeit mehr,
eine Zeile zu markieren, ohne den Browser zu starten; Tastaturnavigation
(ACC-4a) würde beim bloßen Durchpfeilen keine Auswahl mehr hinterlassen, an
der etwas ansetzen kann; und jede künftige Mehrfachauswahl wäre
ausgeschlossen. **Ebenfalls verworfen: die schmale Öffnen-Spalte** am
Zeilenende als Kompromiss — das wäre genau die Knopfspalte, die dieser
Auftrag gerade streicht.

**Zeile ohne Link:** In der Tabelle ist die Zeile **nicht aktivierbar** —
`connect_activate` prüft `ticket_target()` und tut ohne Ziel nichts (wie
heute); der Zeilentooltip sagt `No ticket or event link available`
(bestehende Konstante `CONCERTS_NO_LINK`), und dieselbe Zeichenkette steht
in der `accessible-description` der Zeile, damit ACC-2 erfüllt bleibt. Im
Popover kann der Fall bei Releases nicht auftreten (NR-11 endet immer beim
MusicBrainz-Fallback); bei Concert-Zeilen ohne Ziel ist der umschließende
Knopf **insensitiv** und trägt denselben Tooltip.

### Beschluss 9 — Die 2px-Akzentmarke ist ein transparenter Rand, kein Schatten

Der Web-Entwurf nutzt `box-shadow: inset 2px 0 0 accent`. In GTK ist der
robuste Weg ein **Rand, der im Ruhezustand transparent ist**:

```
.new-release-row      { border-left: 2px solid transparent; }
.new-release-row:hover,
.new-release-row:focus-within { border-left-color: <Akzent>; }
```

Damit gibt es **keinen Layoutsprung** beim Hover (der Platz ist immer
reserviert), keine Abhängigkeit von GTKs Schattenrendering und kein
Clipping in `ColumnView`-Zeilen. Die Tönung daneben ist
`background-color: alpha(currentColor, 0.06)`.

Wo: Popover-Zeilen in `ui/updates/css.rs`; Tabellenzeilen in
`ui/concerts/css.rs` (heute 5 Zeilen — viel Platz). Beide `css()`-Funktionen
sind in `ui/style/mod.rs:135` bereits registriert.

**Die Akzentfarbe wird nicht neu erfunden:** verwende exakt die Quelle, die
`.new-release-chip` heute schon benutzt (STYLE-8, „effective accent color").
Keine Nocturne-Werte, kein neues Farbtoken, kein hartkodiertes Hex.
`:selected` behält seine normale Adwaita-Behandlung — die Akzentmarke ist
Hover/Fokus, nicht Auswahl.

### Beschluss 10 — Farbrollen auf vorhandene Klassen

Die Nocturne-Tokens des Entwurfs werden **nicht** übernommen; übernommen
werden die Rollen:

| Rolle im Entwurf | Umsetzung | Datei | Strang |
|---|---|---|---|
| Meta-Zeile, kontraststärker als heute | eigene Klasse `.new-release-meta` mit `opacity: 0.78` statt Adwaitas `dim-label` (≈0.55) | `ui/updates/css.rs` | 2 |
| Titel Vollton, 15px | `.new-release-title`, Schriftgröße explizit | `ui/updates/css.rs` | 2 |
| Cover-Kachel 44×44, Radius 4px | `.new-release-cover` mit `min-width`/`min-height: 44px` | `ui/updates/css.rs` | 2 |
| Tag `Released` (Akzent) | `.updates-tag.updates-tag-accent` | `ui/updates/css.rs` | 2 |
| Tag `In {n} days`, `Off sale` (neutral) | `.updates-tag.updates-tag-neutral` | `ui/updates/css.rs` | 2 |
| Ignorieren-Knopf, dezent im Ruhezustand | `.new-release-row-actions { opacity: 0.55 }`, `:hover`/`:focus-within → 1` — **ersetzt die heutige Opazität 0** (`release_row_actions.rs:119-152`) | `ui/updates/css.rs` | 2 |
| `On sale` (Akzent-Umriss) | `.reprise-concert-ticket-tag.on-sale` | `ui/concerts/css.rs` | 1 |
| `Off sale` (neutral gefüllt) | `.reprise-concert-ticket-tag.off-sale` | `ui/concerts/css.rs` | 1 |
| `Unknown` (gedimmt) | `.reprise-concert-ticket-tag.unknown` | `ui/concerts/css.rs` | 1 |
| Distanz **innerhalb** des Radius | `.reprise-concert-distance-near` (Akzent) | `ui/concerts/css.rs` | 1 |
| Distanz außerhalb | `.reprise-concert-distance-far` (gedimmt) | `ui/concerts/css.rs` | 1 |
| Venue „trägt den Scan" (heller) | `.reprise-concert-venue` | `ui/concerts/css.rs` | 1 |
| City gedimmt | `.reprise-concert-city` | `ui/concerts/css.rs` | 1 |
| Live-Punkt der Fußzeile | `.reprise-feed-footer-dot`, `.live` = Akzent | `ui/feed_footer.rs::css()` | 1 |

Die 800-Zeilen-Grenze gilt auch für `css.rs`-Dateien
(`ui/updates/css.rs` steht bei 295 — reicht).

### Beschluss 11 — Concert-Zeilen bekommen Initialen, und ein Porträt nur, wenn es schon da liegt

Für Concerts gibt es heute kein Cover. Der garantierte Zustand ist die
**Initialen-Kachel** aus `ui/updates/release_cover.rs` — dieselbe Geometrie
(44×44), dieselbe Herleitung aus dem Namen, damit NR-2 („missing cover →
equally sized tile … never a hole") unverändert gilt.

**Ein Künstlerporträt wird verwendet, wenn — und nur wenn — es bereits im
lokalen Cache liegt** (`reprise_core::artist_portrait::cache`) **und** das
Artwork-Modul aktiv ist. Es wird für Concerts **nie** nachgeladen: ein
Netzabruf beim Öffnen des Popovers verstößt gegen CONC-5a, und
`docs/plans/deezer-placeholder-portraits-handover.md` hält fest, dass Deezer
Platzhalter-Porträts liefert (MD5 des Leerstrings) — ein „Porträt", das wie
eine kaputte graue Kachel aussieht, ist schlechter als Initialen. Die
Fallkette ist damit: **gecachtes Porträt → Initialen**, mehr nicht.

### Beschluss 12 — Der Ignorieren-Knopf ist die `Hide`-Familie, nicht das X

Der Entwurf zeigt ein X mit Tooltip „Ignorieren". Umgesetzt wird
**`view-conceal-symbolic`**, wie heute in `release_row_actions.rs` —
nicht ein X. Ein X liest sich als „löschen", und im Haus gibt es bereits
eine getrennte `deleted_releases`-Erinnerung mit bekannten Fehlern; die
beiden Begriffe dürfen sich nicht vermengen (NR-13/NR-14, `HIDE_RELEASE`).
Der Knopf behält Größe (28×28) und Platz (ganz rechts, hinter dem Tag) aus
dem Entwurf.

- **Release-Zeile:** Tooltip `Hide` (bestehende Konstante `HIDE_RELEASE`),
  bestehende Semantik, in der Vollansicht umkehrbar.
- **Concert-Zeile:** Tooltip `Dismiss`. Er markiert **genau dieses eine
  Event** als gesehen, wodurch es aus dem Delta des Popovers fällt. Dafür
  bekommt `concerts/query.rs` eine Einzel-Variante von `mark_scope_seen`
  (`:150-173`): `mark_event_seen(db, id)`. Kein neuer Zustand, keine neue
  Spalte, keine neue Begrifflichkeit.

Zwei Wörter für zwei verschiedene Zusagen (`Hide` ist umkehrbar, `Dismiss`
ist ein Gesehen-Stempel) sind ehrlicher als ein Wort für beides.

### Beschluss 13 — Der Vorher-Zustand der Benachrichtigung ist das Datum plus ein Stempel

`seen_at` beantwortet „gesehen", nicht „war vorher upcoming" — richtig
erkannt. Der Statuswechsel Upcoming → Released ist ohnehin kein
Fetch-Ereignis: `announcement_kind()` (`artist_news_parsing.rs:294`)
entscheidet ihn **allein aus dem Datum gegen heute**. Ein Release wird also
released, weil ein Tag vergeht, nicht weil eine Antwort eintrifft.

Daraus folgt die Bedingung, ohne neuen Vorher-Zustand:

Benachrichtige für ein `StoredRelease`, wenn **alle drei** gelten:
1. `release_kind(...) == NewsKind::New` **und** `first_release_date == heute`
   (das Datum ist gerade erreicht, nicht irgendwann in den letzten 90 Tagen);
2. `fetched_at < <Startzeitpunkt dieses Laufs>` — die Zeile stand **vorher
   schon** in der Liste. Beim allerersten Fetch ist jede Zeile in diesem
   Lauf entstanden, also feuert nichts. **Genau das ist die Ausnahme des
   ersten Fetches, und sie braucht keine Sonderbehandlung.**
3. `notified_released_at IS NULL` — sonst meldete die stündliche
   Fälligkeitsprüfung dasselbe Release bis zu 24-mal am selben Tag.

Bedingung 3 braucht Persistenz: **`migrate_v74()`** auf der Tabelle
`new_releases`, `ALTER TABLE new_releases ADD COLUMN notified_released_at
INTEGER`, Muster wie `migrate_v31`, Aufruf in `db.rs` **nach** `migrate_v73`
aus Beschluss 2, `SUPPORTED_SCHEMA_VERSION` auf `74`. Gestempelt wird
unmittelbar nach dem erfolgreichen `send_notification`. In **welchem Modul**
`migrate_v74` steht, klärt **Nachtrag 1** (§8) — ein Modul
`db_new_releases.rs`, wie der Entwurf annahm, existiert nicht.

**Wer die Migration schreibt (geändert im Grilling, 14.08.2026):**
`migrate_v74` gehört zu **Strang 1**, nicht zu Strang 3. Strang 1 ist
alleiniger Besitzer von `db.rs`; er legt beide Migrationen an und setzt
`SUPPORTED_SCHEMA_VERSION` in einem Zug auf `74`, obwohl er die Spalte
`notified_released_at` selbst nie liest. **Strang 3 hat keine Schema-Arbeit
mehr**: er liest und schreibt die Spalte ausschließlich über
`crates/reprise-core/src/artist_news_query.rs`. Begründung und Folgen in §7.

### Beschluss 14 — Eine Benachrichtigung je Release, ab vier eine gesammelte

`send_notification`-IDs:

| Fall | ID | Titel | Body |
|---|---|---|---|
| 1–3 Releases | `updates-release-{release_group_mbid}` | der Releasetitel (Daten, nicht übersetzt) | `{artist} · {type} · out today` |
| ≥4 Releases | `updates-releases` (eine einzige) | `{count} releases are out` (Plural) | die ersten drei Künstlernamen, mit `·` verbunden |
| Concerts (`all`) | `updates-concerts` (eine einzige je Lauf) | `{count} new concerts` (Plural) | `{artist} · {city} · {date}` des ersten Eintrags |

Die stabile, releasebezogene ID sorgt dafür, dass ein wiederholter Versand
die alte Meldung **ersetzt statt stapelt**. Der Deckel bei 4 verhindert,
dass ein Freitag mit acht Veröffentlichungen den Benachrichtigungsschirm
flutet.

**Kicker:** Der Entwurf zeigt „Jetzt erschienen" als eigene Zeile.
`gio::Notification` hat dafür **keinen Platz** (nur Titel, Body, Icon) — der
Kicker-Slot ist bei GNOME der App-Name in der Kopfzeile. Der Sinn („warum
kommt das jetzt?") wandert in den Body: `… · out today`. Das ist eine
bewusste Abweichung vom Entwurf, keine Auslassung. Der Auftraggeber hat
**Titel = Releasetitel** ausdrücklich gegen die Alternative
„Titel = Just released" bestätigt.

**Cover:** wie `notify_now_playing()` (`notifications.rs:33-54`) asynchron
nachgeladen und mit demselben Generationsschutz gegen veraltete Treffer
gesichert; ohne Cover wird die Meldung **ohne** Icon geschickt, nicht
verzögert.

**Klick:** `notification.set_default_action_and_target_value("app.open-updates-link", &url.to_variant())`.
Dazu eine `gio::SimpleAction::new("open-updates-link", Some(glib::VariantTy::STRING))`
auf der `gtk4::Application` (Hausform: `compact/compact_player_menu.rs:58ff`),
deren Handler die URL **erneut** durch `external_link::launch()` schickt —
`is_launchable_url` prüft dort ein zweites Mal, weil der Wert aus
Anbieter-JSON stammt und über den D-Bus-Umweg wiederkommt. Die URL ist
dieselbe, die die Popover-Zeile öffnet (NR-11-Priorität), damit Meldung und
Zeile nie auseinanderlaufen. Die gesammelten Meldungen benutzen stattdessen
`app.open-updates-view` mit Ziel `"releases"` bzw. `"concerts"`.

### Beschluss 15 — Die Einstellung heißt `updates.notifications` und wohnt bei den Plugins

- **Key:** `updates.notifications` in der SQLite-Tabelle `settings`
  (`library/settings.rs`), Werte `off` | `releases` | `all`,
  **Vorgabe `releases`** (wie im Entwurf 1b).
- **Ort:** die Plugin-Zeile **New Releases** in
  `Preferences › Plugins › Online`, konkret als zweite `adw::ComboRow` neben
  dem bestehenden `scope_row()` in
  `ui/preferences/preference_new_releases.rs:50-76`.
  **SET-10** („Plugins is the only settings surface for optional
  capabilities … There are no ‚Online sources', ‚New Releases', or ‚Concerts'
  Preferences main pages") bleibt damit unangetastet — eine eigene Seite
  hätte eine Ausnahme gebraucht.
- **Strings:** Titel `Notify about updates`; Werte `Off` /
  `Releases only` / `All updates`; Untertitel
  `All updates also announces newly found concerts for your artists.`
- **Was „All updates" hinzufügt:** den Concerts-Delta — eine gesammelte
  Meldung je Lauf. Damit ist die dritte Stufe nicht leer und braucht
  **keine** neuen Daten. **Die Zahl kommt aus dem Kern, nicht aus dem
  Popover** (Nachtrag 2, §8): `reprise_core::concerts::count_unseen()` für
  die Zahl, `query_unseen()` für die Beispielzeile im Body. Der
  Popover-Deckel `CONCERTS_DELTA_CAP = 3` ist eine Darstellungsgrenze und
  wird hier ausdrücklich **nicht** benutzt — sonst meldete die App „3 new
  concerts" bei zwölf neuen Terminen. `feed_snapshot.rs` (Strang 2) wird
  dafür nicht angefasst.
- Ist das Concerts-Modul aus, verhält sich `all` wie `releases`.

### Beschluss 16 — Welche Dateien vorab geteilt werden

Die 800-Zeilen-Grenze ist an zwei Stellen faktisch erreicht. **Vorab
festgelegt**, damit Codex nicht improvisiert:

| Datei | heute | Maßnahme | Strang |
|---|---|---|---|
| `ui/updates/popover.rs` | 786 | `footer_state.rs` (Live-Zustands-Abbildung, rein) und `popover_fetch.rs` (`start_fetch`, `start_news_fetch`, `start_concerts_fetch`, `finish_feed`) heraus | 2 |
| `ui/updates/release_row.rs` | 501 | gemeinsame Zeilenform nach **`ui/updates/feed_row.rs`**; `release_row.rs` behält nur die release-spezifische Feldabbildung | 2 |
| `ui/concerts/concerts_columns.rs` | 579 | Zellfabriken für Tickets-Tag, Source und Distanzfärbung nach **`ui/concerts/concerts_status_cells.rs`** | 1 |
| `ui/concerts/concerts_view.rs` | 709 | Fußzeilenaufbau in den gemeinsamen **`ui/feed_footer.rs`** | 1 |
| `ui/releases/releases_view.rs` | 687 | Fußzeile ebenfalls nach `feed_footer.rs` — die Datei **schrumpft** | 2 |
| `ui/releases/releases_columns.rs` | **798** | **wird nicht angefasst** (NR-30, NR-33 bleiben) | — |
| `ui/concerts/concerts_filter_bar.rs` | 762 | **wird nicht angefasst** (Beschluss 3) | — |

`ui/feed_footer.rs` ist neu, enthält den Zustands-Enum aus Beschluss 1, den
Widget-Aufbau (Punkt, Label, Fortschritt, Reload-Knopf) und eine eigene
`css()`, die in `ui/style/mod.rs` neben den bestehenden eingetragen wird.
Er gehört **Strang 1** und wird von Strang 2 nur konsumiert.

### Beschluss 17 — Doppelte msgid statt geteilter Strings-Datei

Die fünf flächenneutralen Fußzeilen-Strings (`Up to date — loaded at {time}`,
`Up to date — checked {time}`, `Not loaded yet`, `Online sources are off`,
`Reload`) werden **wörtlich in `strings_concerts.rs` und in `strings_news.rs`
dupliziert**, statt eine gemeinsame Strings-Datei anzulegen.

Grund: gettext fasst identische `msgid` ohnehin zu **einem** Eintrag in der
`.pot` zusammen — die Duplikation kostet i18n-seitig nichts. Eine geteilte
Datei kostet dagegen genau das, was §7 vermeiden will: zwei Stränge, die in
dieselbe Datei schreiben. Ownership schlägt DRY, wenn DRY nichts einspart.

### Beschluss 18 — Was ersatzlos verschwindet

- Die Sprungzeilen und ihr Aufbau (`shell.rs:60-70`, `:120-132`,
  `popover.rs::wire_jump`), die Strings `updates_show_all_concerts`
  (`strings_news.rs:157`) und `updates_show_all_releases` (`:164`) — in
  dieser Reihenfolge, der Entwurf hatte die beiden vertauscht — und die
  Klassen `new-release-history-row` / `-label` / `-count`.
- Der `Fetch now`-Knopf mit eingebautem Alters-Label (`shell.rs:134-170`,
  `concerts_view.rs:446-474`, `releases_view.rs:421`) samt dem Test in
  `shell.rs:172-212`, der ihn einfriert. Der Test heißt
  `nr_23_shell_is_a_fixed_delta_layout_with_fetch_state_in_its_header`; er
  fällt zusammen mit NR-23 (§4.1).
- `concerts_presentation.rs::updated_ago()` (`:100`) und
  `strings_concerts.rs::concerts_updated_ago()` (`:102-123`) sowie
  `strings_news.rs::new_releases_updated_ago()` (`:109`). Die Prüffrage des
  Entwurfs ist beantwortet: `new_releases_updated_ago` hat genau **zwei**
  Leser, `releases_presentation.rs:45` und `updates/popover.rs:51` — beide
  gehören Strang 2, der die Funktion damit gefahrlos mit entfernen kann.
- Die Ticketmaster-Knopfspalte samt `ticket_button_label()`
  (`concerts_presentation.rs:87-98`) und das externe-Link-Icon in der
  **Popover**-Zeile (`release_row_actions.rs`, `external-link-symbolic`) —
  in der Releases-**Vollansicht** bleibt es (NR-30).
- Keine Rückwärtskompatibilität: alte Settings-Keys, Spalten und
  Zeichenketten werden gelöscht, nicht weitergeschleppt.

---

## 2. Pakete — sie stehen in den Strangdateien

Der Entwurf hatte fünf Pakete A–E. Sie sind auf die drei Stränge verteilt und
stehen dort **vollständig**, mit Zielzustand und Fertig-Kriterium. Diese
Tabelle ist nur die Landkarte:

| Paket | Inhalt | Strang | Strangdatei |
|---|---|---|---|
| **A** | Kern: Ticket-Verfügbarkeit, beide Migrationen, `mark_event_seen` | 1 | `-1.md`, Aufgabe 1–9 |
| **B** | Concerts-Tabelle: Status-Tag, `Source`-Spalte, Distanzfärbung, Listenende | 1 | `-1.md`, Aufgabe 10–17 |
| **C** | Der gemeinsame Live-Footer | 1 (Baustein + Concerts) / 2 (Releases + Popover) | `-1.md` Aufgabe 18–20, `-2.md` Aufgabe 8–11 |
| **D** | Das Updates-Popover | 2 | `-2.md`, Aufgabe 1–7 |
| **E** | Benachrichtigung und Einstellung | 3 | `-3.md`, vollständig |

Reihenfolge insgesamt: **Strang 1 → (Strang 2 ∥ Strang 3)**. Details und
Dateibesitz in §7.

---

## 3. Englische Quellstrings — vollständige Liste

Der Entwurf ist auf Deutsch gezeichnet. Implementiert wird durchweg diese
englische Fassung.

| Entwurf (deutsch) | Quellstring (englisch) | Datei | Strang |
|---|---|---|---|
| `Updates` | `Updates` (vorhanden) | `strings_news.rs` | 2 |
| `5 neu` | `{count} new` (`updates_new_count`, vorhanden) | `strings_news.rs` | 2 |
| `Releases` / `Concerts` (Abschnittsköpfe) | `Releases` / `Concerts` (vorhanden) | `strings_releases.rs` / `strings_concerts.rs` | 2 / 1 |
| — (leerer Abschnitt) | `No new releases` / `No new concerts` | `strings_news.rs` | 2 |
| `Castiel · EP · 24.07.2026` | `{artist} · {type} · {date}` | `strings_news.rs` | 2 |
| `14.08.2026 · Indianapolis · Everwise Amphitheater` | `{date} · {city} · {venue}` | `strings_news.rs` | 2 |
| `Released` | `Released` | `strings_news.rs` | 2 |
| `In 7 Tagen` | `In {days} day` / `In {days} days` (Plural) | `strings_news.rs` | 2 |
| `Sold out` | **`Off sale`** (Beschluss 2) | `strings_concerts.rs` | 1 |
| — | `On sale` | `strings_concerts.rs` | 1 |
| `Unbekannt` | `Unknown` | `strings_concerts.rs` | 1 |
| — (Tooltip auf `Off sale`) | `The ticket source reports no active sale. This can mean sold out, or not on sale yet.` | `strings_concerts.rs` | 1 |
| `Ignorieren` (Release) | `Hide` (`HIDE_RELEASE`, vorhanden) | `strings_news.rs` | 2 |
| `Ignorieren` (Concert) | `Dismiss` | `strings_news.rs` | 2 |
| `Öffnet Ticketmaster` | `Opens {source}` | `strings_news.rs` **und** `strings_concerts.rs` (Beschluss 17) | 2 / 1 |
| — (Zeile ohne Ziel) | `No ticket or event link available` (`CONCERTS_NO_LINK`, vorhanden) | `strings_concerts.rs` | 1 |
| `Aktuell — beim Öffnen geladen, 14:32` | `Up to date — loaded at {time}` | beide (Beschluss 17) | 1 + 2 |
| — (aus dem Cache bedient) | `Up to date — checked {time}` | beide | 1 + 2 |
| `Aktualisiere Konzerte …` | `Updating concerts …` | `strings_concerts.rs` | 1 |
| — | `Updating releases …` / `Updating …` | `strings_news.rs` | 2 |
| — (Fehler) | `Update failed — showing saved concerts from {time}` (analog `releases`, `updates`) | jeweilige Datei | 1 + 2 |
| — (offline) | `Offline — showing saved concerts from {time}` (analog) | jeweilige Datei | 1 + 2 |
| — (nie geladen) | `Not loaded yet` | beide | 1 + 2 |
| — (Gate aus) | `Online sources are off` | beide | 1 + 2 |
| `Neu laden` | `Reload` | beide | 1 + 2 |
| `412 Konzerte liegen ausserhalb von 500 km.` | `End of results — {hidden} concerts hidden by the {radius} km radius around {city}` (Beschluss 6) | `strings_concerts.rs` | 1 |
| `Alle 415 Konzerte zeigen` | `Show all {total} concerts` (vorhanden) | `strings_concerts.rs` | 1 |
| `Zürich · 500 km` | `{city} · {radius} km` — **an den Standort-Plan übergeben** (Beschluss 3) | — | — |
| `Jetzt erschienen` | entfällt als eigene Zeile; wandert in den Body (Beschluss 14) | — | — |
| `Castiel · EP · heute erschienen` | `{artist} · {type} · out today` | `strings_notifications.rs` | 3 |
| — (gesammelt) | `{count} releases are out` / `{count} new concerts` (Plural) | `strings_notifications.rs` | 3 |
| `Benachrichtigen, wenn ein Upcoming-Release live geht` | `Notify about updates` | `strings_notifications.rs` | 3 |
| `Aus` / `Nur Releases` / `Alle Updates` | `Off` / `Releases only` / `All updates` | `strings_notifications.rs` | 3 |
| — (Untertitel) | `All updates also announces newly found concerts for your artists.` | `strings_notifications.rs` | 3 |
| `Source` (Spaltenkopf) | `Source` (`CONCERTS_SOURCE`, vorhanden) | `strings_concerts.rs` | 1 |

`similar to {artist}` (`concert_similar_caption`) und alle Spaltentitel
existieren bereits und werden wiederverwendet.

---

## 4. UX-Regeln

Prozessvertrag: eine Regel wechselt `[planned]` → `[active]` **in demselben
Commit**, der das Verhalten baut und den regelbenannten Test hinzufügt.
Ersetzte Regeln bleiben als `[replaced by <ID>]` stehen, ihre Tests werden
im selben Commit umgehängt. Ein Test trägt **genau eine** primäre Regel-ID
im Namen (`fn conc_12_…`, `fn nr_34_…`, `fn os_6_…`).
`scripts/check-ux-traceability.sh` ist Merge-Gate.

Nächste freie IDs (gegen `origin/dev` erhoben): **`NR-34`** (Abschnitt R,
`:2129`), **`CONC-12`** (Abschnitt AE, `:4940`), **`OS-6`** (Abschnitt H,
`:1319` — dort stehen heute nur `OS-1`…`OS-5`, alle `[planned]`).
Abschnitt K braucht **keine** neue ID (Beschluss 6).

### 4.1 Ersetzte Regeln

| Alt | Zeile | Neu | Warum | Strang |
|---|---|---|---|---|
| `NR-5b` | 2234 | `[replaced by NR-34]` | die Sprungzeilen, die sie namentlich nennt, entfallen | 2 |
| `NR-10a` | 2210 | `[replaced by NR-36]` | Aktionen sind dauerhaft sichtbar statt eingeblendet | 2 |
| `NR-21` | 2323 | `[replaced by NR-21a]` | ihre Kopplung an NR-22 und an das „update age" bricht | 2 |
| `NR-22` | 2334 | `[replaced by NR-37]` | `Fetch now` und Alters-Footer entfallen | 2 |
| `NR-23` | 2340 | `[replaced by NR-34]` | Sprungzeilen entfallen; Deckel und Zähl-Chip wandern in NR-34 | 2 |
| `CONC-3` | 4956 | `[replaced by CONC-13]` | die Ticket-Zelle ist keine Aktivierungsfläche mehr | 1 |
| `CONC-4b` | 4964 | `[replaced by CONC-4c]` | `Fetch now` und `Updated X ago` entfallen; alles Übrige wird wortgleich neu ausgestellt | 1 |
| `CONC-5a` | 4978 | `[replaced by CONC-5b]` | Auslöser bleiben, aber `Fetch now` heißt jetzt Reload-Knopf | 1 |
| `CONC-7` | 4988 | `[replaced by NR-35]` | Sprungzeile entfällt; der Abschnitt gehört inhaltlich zum Popover | **2** (die eine erlaubte Ausnahme, §7) |
| `CONC-10` | 5004 | `[replaced by CONC-14]` | die Unterschrift dehnt die Zeile nicht mehr | 1 |
| `CONC-11` | 5009 | `[replaced by CONC-11a]` | `Updated X ago` entfällt; die Fehlerfläche wird wortgleich neu ausgestellt | 1 |

`CONC-2` wird hier **nicht** angefasst — sie gehört dem Plan
`location-is-not-a-concerts-setting` (Beschluss 3). Ebenfalls unangetastet:
NR-2, NR-3a, NR-7, NR-8, NR-9c, NR-11, NR-24, NR-26…NR-30, NR-32, NR-33,
CONC-1, CONC-6, CONC-8, CONC-9, TIP-3, FIL-2a, FIL-3a, ACC-1…ACC-9.

### 4.2 Neue Regeln

Format wie im Haus: `- **ID** [status] [level] — Text`, Fortsetzungszeilen
zwei Leerzeichen eingerückt.

**Abschnitt AE (Concerts) — Strang 1:**

- **CONC-12** `[active] [core]` — Ticket availability is what the source
  says, never an inference. Ticketmaster's `dates.status.code` maps
  `onsale` → On sale, `offsale` → Off sale, and everything else
  (`cancelled`, `postponed`, `rescheduled`, missing) → Unknown; Bandsintown
  maps an `available` offer → On sale, offers without an available one →
  Off sale, and a missing or empty offers list → Unknown. The app never
  renders "Sold out": no provider distinguishes a sold-out show from a
  pre-sale that has not opened.
  Test: `conc_12_offsale_never_becomes_sold_out`
  (`crates/reprise-core/src/concerts/availability.rs`, `#[cfg(test)]`).

- **CONC-13** `[active] [gtk]` — replaces CONC-3. Double-click, Enter or
  Space on a concert row opens its external target: the offer URL,
  otherwise the event page. The Tickets cell is a status label and is never
  an activation surface. A row without a launchable target does not
  activate, keeps its ordinary appearance, and carries the same sentence in
  its tooltip and its accessible description. There is no play path.
  Test: `conc_13_a_row_without_a_target_does_not_activate`
  (`ui/concerts/concerts_view_tests.rs`).

- **CONC-14** `[active] [gtk]` — replaces CONC-10. Every concert row is a
  single line and never wraps. Every cell ellipsises at its end. The
  optional dimmed "similar to {seed}" sits on the artist's own line,
  directly after the name, and ellipsises before the name does — losing the
  provenance is acceptable, losing the artist is not. Rows keep a common
  vertical center.
  Test: `conc_14_the_similar_caption_shrinks_before_the_artist`
  (`ui/concerts/concerts_view_tests.rs`).

- **CONC-15** `[active] [gtk]` — The feed footer states the live state, not
  an age, and it is the only place any of these views shows a timestamp.
  Its nine states are: loaded in this visit, served from cache, updating
  (with determinate progress), failed, offline, never loaded, no
  credentials, online sources off, module off (footer hidden). A loaded or
  cached state carries the accent dot and a reload button; an updating
  state replaces the button with the progress bar; the two configuration
  states offer no button. "Up to date" never appears while a fetch is
  running or has failed.
  Test: `conc_15_the_footer_never_claims_up_to_date_while_fetching`
  (`ui/feed_footer.rs`, `#[cfg(test)]`).

- **CONC-16** `[active] [gtk]` — The provider name has a hover-free home:
  an optional `Source` column, hidden by default, switchable in the column
  header menu, its visibility persisted like every other column. The row
  tooltip "Opens {source}" is a comfort duplicate of it (TIP-3), never the
  only place the name appears.
  Test: `conc_16_the_source_column_is_available_but_off_by_default`
  (`ui/concerts/concerts_view_tests.rs`).

- **CONC-4c** `[active] [gtk]` — replaces CONC-4b. Wortgleiche
  Neuausstellung von CONC-4b mit **zwei** Änderungen: „Never fetched offers
  exactly ‚Fetch now'" wird zu „Never fetched shows exactly ‚Not loaded
  yet' and offers the reload button", und „Offline or error leaves the
  cache and ‚Updated X ago' visible" wird zu „Offline or error leaves the
  cache visible and states so per CONC-15". Alles Übrige — der neutrale
  Zustand ohne Zugangsdaten, das unsichtbare Popover-Segment, das Fehlen
  eines Deep-Links, die sofortige Neubewertung bei Änderungen an
  Zugangsdaten/Standort/Radius/Zeitraum/Similar — bleibt **wörtlich**
  erhalten.
  Test: der bestehende `conc_4b_…`-Test wird auf `conc_4c_…` umgehängt.

- **CONC-5b** `[active] [core]` — replaces CONC-5a. Wortgleiche
  Neuausstellung mit einer Änderung: der Auslöser „‚Fetch now'" heißt
  „the footer's reload button". Der 24-h-Veraltungs-Gate, der Jitter, die
  stündliche Fälligkeit, die bestätigte Zugangsdatenprüfung, das
  gemeinsame 1-req/s-Limit und der Ausschluss von Track-Wechsel und
  Navigation bleiben **unverändert**.
  Test: der bestehende `conc_5a_…`-Test wird auf `conc_5b_…` umgehängt.

- **CONC-11a** `[active] [gtk]` — replaces CONC-11. Wortgleiche
  Neuausstellung mit einer Änderung: „leaves every cached event and
  ‚Updated X ago' untouched" wird zu „leaves every cached event untouched
  and reports the failure through CONC-15's footer state". Der geteilte
  Banner, die `Details`-Klappe mit Copy, die Offline-Herkunft aus dem
  Fensterzustand, die „Open Preferences"-Regel für fehlende Zugangsdaten —
  alles **wörtlich** erhalten.
  Test: der bestehende `conc_11_…`-Test wird auf `conc_11a_…` umgehängt.

**Abschnitt R (New releases) — Strang 2:**

- **NR-34** `[active] [gtk]` — replaces NR-5b and NR-23. The Updates
  popover shows at most five releases and three concerts without an
  internal scroller, and both feeds use one identical row shape. Each
  section header is the only bridge into its full view: activating it — by
  pointer or by keyboard — closes the popover and navigates, exactly as the
  removed jump rows did. A header stays visible while its module is active
  even when its section is empty, and then shows a quiet empty line. The
  header's count chip names the full batch size and appears only while that
  batch is genuinely unseen. The popover remains transient and has no
  internal subpages.
  Test: `nr_34_an_empty_section_keeps_its_header_and_its_bridge`
  (`ui/updates/popover_tests.rs`).

- **NR-35** `[active] [gtk]` — replaces CONC-7. The popover's Concerts
  section appears only while the Concerts module is active, shows at most
  three unseen entries of the persistent filter scope, and reaches the full
  view through its header per NR-34. Opening still stamps the entire delta
  set of both sections, and the header badge still sums unseen entries
  across all active, fetch-ready feeds.
  Test: `nr_35_the_concerts_section_header_carries_the_unseen_count`
  (`ui/updates/popover_tests.rs`).

- **NR-36** `[active] [gtk]` — replaces NR-10a. The row's trailing slot
  holds the status tag and the dismiss button side by side, permanently:
  the button rests at reduced contrast and reaches full contrast on hover
  or focus, and it never displaces the tag. The button is a sibling of the
  row's activation surface, not a child of it, so dismissing a row can
  never open its link. Both are reachable with Tab and activate with Enter
  or Space.
  Test: `nr_36_dismissing_a_row_never_opens_its_link`
  (`ui/updates/popover_tests.rs`).

- **NR-37** `[active] [gtk]` — replaces NR-22. The Releases view and the
  Updates popover use CONC-15's live-state footer with `releases` and
  `updates` as their unit. There is no "Fetch now" button and no update
  age; the reload icon button carries the manual trigger, and the
  determinate checked/total artist progress appears in the footer's
  progress bar. The popover's footer aggregates both feeds: any running
  fetch makes it "updating", otherwise it reports the older of the two
  timestamps.
  Test: `nr_37_the_popover_footer_reports_the_older_of_both_feeds`
  (`ui/updates/popover_tests.rs`).

- **NR-38** `[active] [gtk]` — A popover row opens its link on a single
  click anywhere on its activation surface — cover, title, meta or tag —
  and on Enter or Space when focused. Releases follow NR-11's URL
  priority, concerts prefer the offer URL over the event page. The
  provider name appears as the row's tooltip and, hover-free, in CONC-16's
  Source column. A concert row without a launchable target is insensitive
  and says why.
  Test: `nr_38_a_row_opens_the_same_url_its_tooltip_names`
  (`ui/updates/popover_tests.rs`).

- **NR-21a** `[active] [gtk]` — replaces NR-21. Wortgleiche
  Neuausstellung mit zwei Änderungen: „leaves every cached release and the
  existing update age untouched" wird zu „leaves every cached release
  untouched and reports the failure through CONC-15's footer state", und
  der Schlusssatz verweist auf **NR-37** statt auf NR-22. Banner,
  `Details`-Klappe, Offline-Herkunft, NR-8s Consent-Schleife: **wörtlich**
  erhalten.
  Test: der bestehende `nr_21_…`-Test wird auf `nr_21a_…` umgehängt.

**Abschnitt H (File association & OS integration) — Strang 3:**

- **OS-6** `[active] [core] [gtk]` — A release that reaches its release
  date announces itself once. The desktop notification fires only for a
  release whose row already existed before the current run began, so a
  first fetch announces nothing, and a stamp on the row prevents the hourly
  due check from repeating it the same day. Up to three releases send one
  notification each, carrying the release title, `{artist} · {type} · out
  today` and the cover when it is available; four or more collapse into a
  single collected notification. Activating a notification opens exactly
  the URL its popover row would open, through the shared external-link
  guard.
  Test: `os_6_the_first_fetch_announces_nothing`
  (`crates/reprise-core/src/artist_news_notify.rs`, `#[cfg(test)]`).

- **OS-7** `[active] [gtk]` — Update notifications are a three-step
  setting on the New Releases plugin row — `Off`, `Releases only`,
  `All updates` — stored as `updates.notifications` and defaulting to
  `Releases only`. `All updates` adds one collected notification per run
  for newly found concerts of library artists, and behaves like
  `Releases only` while the Concerts module is off. Nothing else notifies.
  Test: `os_7_all_updates_adds_the_concerts_delta`
  (`ui/preferences/preference_new_releases.rs`, `#[cfg(test)]`).

**Abschnitt K — keine neue Regel**, aber ein neuer Test unter der
bestehenden ID: `fil_3a_the_concerts_end_of_results_sits_below_the_last_row`
(`ui/concerts/concerts_view_tests.rs`, Strang 1). Beschluss 6 begründet,
warum FIL-3a unverändert bleibt.

---

## 5. Abnahme

Kopfleiste: Headless-Suite und Display-Gate wie üblich. Das Display-Gate ist
im Rudel bekanntermaßen flaky und auf `dev` teils schon rot — **zuerst gegen
`origin/dev` messen, was ohne diese Änderung rot ist**, sonst wird fremdes
Rot als eigene Schuld verbucht.

Gates vor jedem Commit: `cargo fmt --check`,
`cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`. Nach jeder `reprise-core`-Änderung
zusätzlich `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'`
(muss leer sein). Jede angefasste Code-Datei endet **unter 800 Zeilen**.

### 5.1 Bildbeweise (Screenshot-Harness, cage+grim)

Ohne den jeweiligen Screenshot gilt der Punkt als nicht gezeigt.

1. Popover mit beiden Abschnitten: identische Zeilen, Cover-Kacheln 44×44,
   Titel/Meta zweizeilig, Tags rechts, dezenter Ausblenden-Knopf, **ein**
   Zeitstempel unten.
2. Dasselbe Popover mit Mauszeiger auf einer Zeile: Tönung **plus**
   2px-Akzentmarke links, Ausblenden-Knopf in vollem Kontrast.
3. Popover mit leerem Concerts-Abschnitt: Kopf steht noch da, Leerzeile
   darunter (NR-34).
4. Concerts-Übersicht, Standort gesetzt: sechs Spalten ohne
   Ticketmaster-Knopf, Status-Tags rechts, Nahdistanzen in Akzentfarbe,
   Fernedistanzen gedimmt, Venue heller als City.
5. Dieselbe Ansicht mit eingeschalteter `Source`-Spalte (aus dem
   Kopf-Popover heraus) — der Beweis für TIP-3.
6. Concerts gefiltert (3 von 415): das Listenende steht **unmittelbar unter
   der dritten Zeile**, nicht in der Mitte des Leerraums; die Fußzeile
   bleibt unten am Rand.
7. Fußzeile in allen vier interessanten Zuständen: `Updating concerts …`
   mit Fortschritt, `Up to date — loaded at …`, `Up to date — checked …`,
   `Offline — showing saved concerts from …`.
8. Die Desktop-Benachrichtigung mit Cover.

### 5.2 Beweise, die über einen Screenshot hinausgehen

- **„Off sale kommt wirklich aus der Quelle":** eine Ticketmaster-Antwort
  mit `dates.status.code = "offsale"` einspielen, den Wert nach einem
  App-Neustart aus der Datenbank lesen — er muss `off_sale` sein, nicht
  `unknown`. Ein Screenshot des Tags beweist nur die Anzeige.
- **„Beim Öffnen wird nicht bedingungslos gefetcht":** Ansicht zweimal
  hintereinander öffnen; der zweite Öffnen-Vorgang darf **keine**
  Netzanfrage erzeugen (per `REPRISE_LOG` mitschneiden), und die Fußzeile
  muss `checked` statt `loaded` sagen.
- **„Der erste Fetch meldet nichts":** frische Datenbank, Fetch mit einem
  Release, dessen Datum heute ist → keine Benachrichtigung. Danach die Uhr
  vorstellen bzw. ein zweiter Lauf → genau eine.
- **„Der Ausblenden-Knopf öffnet keinen Browser":** Klick auf den Knopf bei
  laufendem `REPRISE_LOG`; es darf keine `launch`-Zeile erscheinen.
- **Kontrollarm:** für Punkt 6 und für die Fußzeilenzustände gilt eine
  Änderung erst als bewiesen, wenn der zurückgerollte Code **jetzt gerade**
  das alte Verhalten zeigt. Eine grüne Messung ohne Kontrollarm misst
  nichts.

---

## 6. Nicht in diesem Auftrag

- **Alles aus Paket E des Plans `location-is-not-a-concerts-setting`**:
  Radius-Facet ohne Standort, Chip `500 km · off`, Leiste über der Tabelle,
  Ausblenden der Distance-Spalte, Sortier-Sperre, Radio „Near you"
  (Beschluss 3). Der Chip-Text `{city} · {radius} km` wird als **E2b**
  dorthin übergeben — der Eintrag in jenen Plan ist allerdings sehr wohl
  eine Aufgabe dieses Auftrags (Strang 1).
- **Die Releases-Vollansicht**: `releases_columns.rs`, die Link-Spalte und
  der Spaltensatz (NR-30, NR-33) bleiben unberührt. Nur die **Fußzeile**
  von `releases_view.rs` ändert sich.
- **Echte Ausverkauft-Daten**. Sollte ein Anbieter einmal ein belastbares
  Signal liefern, ist das eine vierte Variante in `TicketAvailability`,
  kein Umdeuten von `off_sale`.
- **Die `deleted_releases`-Familie** und ihre bekannten Fehler; der
  Ausblenden-Knopf ist ausschließlich `Hide`/`Show again` (Beschluss 12).
- **GSettings**: GP-6 bleibt `[planned]`; der neue Key lebt in der
  `settings`-Tabelle wie alle anderen.
- **`po/`-Dateien von Hand**: neue Strings kommen über den normalen
  Extraktionslauf. Nur `POTFILES.in` bekommt eine Zeile (Strang 3).
- **Panel 1c** des Entwurfs (vier Spalten, `Artist` zuerst) — überholt.
- **Porträt-Beschaffung für Concerts**: keine neuen Netzabrufe, nur der
  vorhandene Cache (Beschluss 11).
- Podcasts, YouTube und Radio: ihre Fußzeilen bleiben, wie sie sind.

---

## 7. Parallelität

Drei Stränge, Deckel eingehalten. Der Schnitt folgt **Flächen**, nicht
Belangen — eine belangorientierte Aufteilung („alle Fußzeilen", „alle
Zeilen") ließe zwei Stränge gleichzeitig in `popover.rs` und
`concerts_view.rs` schreiben und wäre damit kein Schnitt.

### Strang 1 — `core-concerts` (Kern + Concerts-Tabelle + beide Migrationen)

**Zweck:** Der Verfügbarkeitsstatus von der Anbieterantwort bis in die
Zelle, die Concerts-Tabelle, **beide Schema-Migrationen**, und der
**gemeinsame Fußzeilen-Baustein**, den die anderen konsumieren. Enthält
Paket A, Paket B und den Concerts-Teil von Paket C.
Strangdatei: `docs/plans/updates-concerts-releases-rework-1.md`.

**Dateibesitz (Globs)**
```
crates/reprise-core/src/concerts/**
crates/reprise-core/src/concerts.rs
crates/reprise-core/src/db_concerts.rs
crates/reprise-core/src/db.rs                    (ALLEINBESITZ: migrate_v73- UND
                                                  migrate_v74-Aufruf, SUPPORTED_SCHEMA_VERSION = 74)
crates/reprise-core/src/db_new_releases_notify.rs (neu, NUR: migrate_v74 —
                                                  Nachtrag 1, §8)
crates/reprise-gnome/src/ui/concerts/**
crates/reprise-gnome/src/ui/feed_footer.rs       (neu)
crates/reprise-gnome/src/ui/style/mod.rs         (nur: eine Zeile in der css()-Liste)
crates/reprise-gnome/src/ui/strings_concerts.rs
docs/ux-rules.md                                 (nur Abschnitt AE: CONC-12…CONC-16, CONC-4c,
                                                  CONC-5b, CONC-11a und die Marker auf
                                                  CONC-3/4b/5a/10/11 — NICHT CONC-7)
docs/plans/location-is-not-a-concerts-setting.md (nur: Nachtrag E2b, Beschluss 3)
```
**Ausdrücklich NICHT:** `concerts_filter_bar.rs`, `concerts_column_layout.rs`s
Standort-Logik (nur die Registrierung der neuen `Source`-Spalte),
`concerts_view.rs`s Distance-**Sichtbarkeit** — alles Beschluss 3.
Ebenfalls nicht: `artist_news_query.rs` (Strang 3),
`crates/reprise-gnome/src/ui/updates/**` und `…/releases/**` (Strang 2).

**Aufgaben:** Paket A vollständig; Paket B vollständig; aus Paket C der
Baustein `feed_footer.rs` und der Concerts-Teil; die AE-Regeln aus §4.2; der
FIL-3a-Test; der Nachtrag E2b in den Standort-Plan.

**`db.rs` gehört ausschließlich Strang 1.** Er legt **beide** Migrationen an
— `migrate_v73` (`concert_events.ticket_availability`) und `migrate_v74`
(`new_releases.notified_released_at`) — obwohl er die zweite Spalte selbst
nie liest, und setzt `SUPPORTED_SCHEMA_VERSION` in einem Zug auf `74`.

Grund: `db.rs` stand im Entwurf in **zwei** Besitzlisten (Strang 1 für v73,
Strang 3 für v74). Beide fassen dieselbe Konstante und dieselbe Aufrufkette
an; das ist kein disjunkter Besitz, sondern sequenzierter, der allein durch
Rebase-Disziplin hält. Mit Einzelbesitz fällt die einzige garantierte
Konfliktstelle des ganzen Auftrags weg.

**Folge, die im Commit begründet stehen muss:** Strang 1s Diff enthält eine
Spalte, die er selbst nicht liest. Ohne Begründung liest ein Reviewer das als
toten Code. Wortlaut etwa: „v74 legt die Spalte für Strang 3 an, damit
`db.rs` einen einzigen Besitzer hat."

### Strang 2 — `updates-popover` (Popover + Releases-Fußzeile)

**Zweck:** Die gemeinsame Zeilenform, die Abschnittsköpfe als Brücke, und
die zwei restlichen Fußzeilen. Enthält Paket D und den Rest von Paket C.
Strangdatei: `docs/plans/updates-concerts-releases-rework-2.md`.

**Dateibesitz (Globs)**
```
crates/reprise-gnome/src/ui/updates/**
crates/reprise-gnome/src/ui/releases/releases_view.rs
crates/reprise-gnome/src/ui/releases/releases_presentation.rs
crates/reprise-gnome/src/ui/strings_news.rs
crates/reprise-gnome/src/ui/strings_releases.rs
docs/ux-rules.md                                 (nur Abschnitt R: NR-34…NR-38, NR-21a und die
                                                  Marker auf NR-5b/10a/21/22/23
                                                  + GENAU EINE Zeile in Abschnitt AE:
                                                  der Statusmarker auf CONC-7)
```
**Ausdrücklich NICHT:** `releases_columns.rs` (NR-30/NR-33),
`releases_filter_bar.rs`, `ui/feed_footer.rs` (Strang 1 — nur konsumieren),
`ui/style/mod.rs` (Strang 1), alles unter `ui/concerts/**` (Strang 1).

**Aufgaben:** Paket D vollständig; aus Paket C die Releases- und die
Popover-Fußzeile samt Aggregation; die R-Regeln aus §4.2.

**Vorbedingung:** braucht aus Strang 1 den Typ `TicketAvailability` (für das
`Off sale`-Tag in der Popover-Concert-Zeile), `query::mark_event_seen()`
(für `Dismiss`) und `ui/feed_footer.rs`. Deshalb Merge nach Strang 1.

### Strang 3 — `update-notifications` (Benachrichtigung + Einstellung)

**Zweck:** Paket E vollständig, **ohne Schema-Arbeit**.
Strangdatei: `docs/plans/updates-concerts-releases-rework-3.md`.

**Dateibesitz (Globs)**
```
crates/reprise-core/src/artist_news_notify.rs    (neu)
crates/reprise-core/src/artist_news_query.rs     (nur: notified_released_at lesen/schreiben)
crates/reprise-gnome/src/ui/notifications.rs
crates/reprise-gnome/src/ui/notifications_updates.rs   (neu)
crates/reprise-gnome/src/ui/preferences/preference_new_releases.rs
crates/reprise-gnome/src/ui/strings_notifications.rs   (neu)
crates/reprise-gnome/src/ui/strings.rs           (nur: mod-Deklaration + pub use)
po/POTFILES.in                                   (nur: eine Zeile)
docs/ux-rules.md                                 (nur Abschnitt H: OS-6, OS-7)
```
**Ausdrücklich NICHT — und das ist die Grill-Änderung:**
`crates/reprise-core/src/db.rs` und das Modul, das `migrate_v74` trägt
(Nachtrag 1, §8), sind **aus** dem Dateibesitz von
Strang 3 gefallen. Strang 3 legt **keine** Migration an, fasst
`SUPPORTED_SCHEMA_VERSION` **nicht** an und schreibt keine
`CREATE`/`ALTER TABLE`-Anweisung. Er liest und schreibt die Spalte
`notified_released_at` ausschließlich über
`crates/reprise-core/src/artist_news_query.rs`.

**Aufgaben:** Paket E vollständig; die H-Regeln aus §4.2.

**Vorbedingung:** die Spalte `new_releases.notified_released_at` muss auf der
Basis existieren. Findet Strang 3 auf seiner Basis kein `migrate_v74` (oder
`SUPPORTED_SCHEMA_VERSION < 74`), ist er zu früh dran: **erst auf den
gemergten Strang 1 rebasen**, dann beginnen. Ohne die Spalte kompiliert seine
Query nicht — das ist ein harter, sofort sichtbarer Stopp, kein stiller
Fehler.

### Die zwei bekannten Kollisionen — so aufgelöst

**1. `docs/ux-rules.md` ist eine Datei.** Aufgelöst durch **Abschnitts- und
ID-Besitz** statt Dateibesitz, weil die Hausregel jeden Strang zwingt,
seine Regel im selben Commit wie das Verhalten zu schreiben:

| Strang | Abschnitt | erlaubte IDs |
|---|---|---|
| 1 | AE (`:4940`ff) | `CONC-12`…`CONC-16`, `CONC-4c`, `CONC-5b`, `CONC-11a` + Marker auf CONC-3/4b/5a/10/11 |
| 2 | R (`:2129`ff) | `NR-34`…`NR-38`, `NR-21a` + Marker auf NR-5b/10a/21/22/23 |
| 3 | H (`:1319`ff) | `OS-6`, `OS-7` |

Drei disjunkte, weit auseinanderliegende Bereiche derselben Datei — git
merged das ohne Konflikt. Abschnitt H ist bewusst gewählt: dort steht heute
**keine** `[active]`-Regel (OS-1…OS-5 sind alle `[planned]`), und eine
Desktop-Benachrichtigung ist per Definition OS-Integration. Damit muss
Strang 3 **nicht** in R schreiben, wo Strang 2 arbeitet.

**Die einzige erlaubte Ausnahme:** Strang 2 setzt den Statusmarker auf
`CONC-7` (`:4988`) — eine Regel im Abschnitt von Strang 1. Sie ist
inhaltlich Popover und gehört deshalb zu Strang 2. Strang 2 **rebast vor
dieser Änderung auf den gemergten Strang 1** und fasst nur diese eine
Zeile an; Strang 1 appendet sein Neues ans Ende von AE (`≈:5020`), weit
unterhalb.

**2. Das `Off sale`-Tag im Popover braucht `TicketAvailability` aus dem
Kern.** Aufgelöst durch **Merge-Reihenfolge**, nicht durch Aufteilung der
Popover-Zeile: Strang 1 merged zuerst, Strang 2 rebast darauf und findet den
Typ vor. Die Popover-Zeile bleibt damit in **einer** Hand — sie in zwei
Stränge zu schneiden (Geometrie hier, Tag dort) wäre der teurere Fehler.

**Die dritte, ehemalige Kollision ist entfallen:** `db.rs` stand im Entwurf
in zwei Besitzlisten. Seit dem Grilling hat die Datei genau einen Besitzer
(Strang 1). Es bleibt keine Datei übrig, in die zwei Stränge schreiben —
außer `docs/ux-rules.md` mit der einen benannten Zeile oben.

### Merge-Reihenfolge

```
Strang 1 (core-concerts)  ──merge──►  dev
                                       │
                        ┌──────────────┴──────────────┐
                        ▼                             ▼
        Strang 2 (updates-popover)      Strang 3 (update-notifications)
             rebase auf dev                  rebase auf dev
                        │                             │
                        └──────────────┬──────────────┘
                                       ▼
                              Post-Merge-Querprüfungen
```

Strang 1 ist Vorbedingung für **beide**: für Strang 2 wegen
`TicketAvailability`, `mark_event_seen()` und `feed_footer.rs`, für
Strang 3 wegen der Spalte `notified_released_at`. Strang 2 und 3 sind
untereinander unabhängig und dürfen parallel laufen; ihre Dateimengen sind
disjunkt (einzige Berührung: beide schreiben Strings, aber in verschiedene
Dateien — Beschluss 17).

`merge_order: 1,2,3` im Frontmatter meint diese Ordnung: 1 zuerst, danach
2 und 3 in beliebiger Reihenfolge oder gleichzeitig.

### Post-Merge-Querprüfungen

**Diese Prüfungen lesen Dateien, die der prüfende Strang nicht besitzt. Sie
gehören ausdrücklich NICHT in eine Strang-Aufgabe — vor dem Merge können
sie prinzipiell nicht grün werden, und ein Strang, der auf sie wartet,
bleibt mit fertiger, korrekter Arbeit stehen.**

Nach dem letzten Merge, im Hauptzweig:

1. **`scripts/check-ux-traceability.sh`** — liest die ganze
   `docs/ux-rules.md` samt aller Tests aus allen drei Strängen. Jeder
   Strang sieht vor dem Merge nur seinen Ausschnitt; nur hier ist der
   Befund vollständig. Erwartung: jede neue `[active]`-Regel hat ihren
   Test, kein Test zeigt auf `CONC-3`, `CONC-4b`, `CONC-5a`, `CONC-7`,
   `CONC-10`, `CONC-11`, `NR-5b`, `NR-10a`, `NR-21`, `NR-22`, `NR-23`.
2. **Ein Wort, zwei Flächen:** dasselbe Event zeigt in der
   Concerts-Tabelle (Strang 1) und in der Popover-Zeile (Strang 2)
   denselben Status — `Off sale` hier, `Off sale` dort. Liest beide
   Zeilenimplementierungen.
3. **Ein Zeitstempel, drei Fußzeilen:** `git grep -n 'Updated .*ago'`
   über `crates/reprise-gnome/src/ui` liefert für Concerts, Releases und
   Popover **keinen** Treffer mehr. Liest Dateien aus Strang 1 und 2.
4. **Dieselbe URL:** die Benachrichtigung (Strang 3) öffnet für ein
   gegebenes Release exakt die URL, die dessen Popover-Zeile (Strang 2)
   öffnet. Beide gehen durch NR-11s Priorität und
   `external_link::launch()` — die Prüfung ist ein Vergleich zweier
   Ergebniswerte, nicht zweier Implementierungen.
5. **Geometrie-Parität:** Screenshot-Paar Popover-Zeile (Strang 2) gegen
   Tabellenzeile (Strang 1): gleiche 44×44-Kachel, gleiche
   2px-Akzentmarke, gleiche Tag-Typografie. Das ist Anforderung R4, und
   sie ist per Konstruktion nur nach dem Merge messbar.
6. **Migrationskette am Stück:** eine v72-Datenbank durch **beide**
   Migrationen fahren und danach `PRAGMA user_version == 74` sowie beide
   Spalten prüfen. Beide Migrationen stammen jetzt aus **einem** Strang —
   die Prüfung bleibt trotzdem post-merge, weil erst Strang 3 die Spalte
   `notified_released_at` tatsächlich beschreibt und der Beweis beide
   Hälften braucht: Schema von Strang 1, Schreibpfad von Strang 3.
7. **Die üblichen Gesamt-Gates:** `cargo fmt --check`,
   `cargo clippy --all-targets --workspace -- -D warnings`,
   `cargo test --workspace`, `cargo audit`, die Kernreinheit, der
   Icon-Namens-Test `every_icon_name_the_app_asks_for_can_be_drawn`, und
   die 800-Zeilen-Grenze über **alle** angefassten Dateien.
8. **Die Abnahme aus §5** in voller Länge — sie mischt Flächen aus allen
   drei Strängen und ist erst hier durchführbar.

### Wenn ein Strang ausfällt

Strang 1 allein ist lieferbar und sinnvoll (Status in der Tabelle,
Live-Fußzeile in Concerts). Er liefert dann eine ungenutzte Spalte
`notified_released_at` mit — das ist der bewusst gewählte Preis für den
Einzelbesitz an `db.rs` und muss in der Commit-Nachricht stehen. Strang 3
allein ist lieferbar, sobald Strang 1 gemergt ist. Strang 2 ohne Strang 1
ist **nicht** lieferbar — er würde die Sprungzeilen streichen, ohne das
`Off sale`-Tag und ohne `feed_footer.rs`. Das ist der Grund für die
Reihenfolge, nicht Bequemlichkeit.

---

## 8. Die drei Lücken der Schlussprüfung — zwei entschieden, eine ohne Entscheidung

Beim Abgleich der Entwurfszahlen gegen `origin/dev` fielen drei Stellen auf,
die der Entwurf angenommen und die Prüfung widerlegt hat. **Zwei davon sind
am 14.08.2026 nachträglich entschieden worden und stehen unten als Beschluss;
die dritte ist gar keine Entscheidung, sondern eine fehlende Zeilennummer.**
Nichts in diesem Abschnitt darf als Einladung gelesen werden, einen Beschluss
aus §1 neu aufzurollen.

### Nachtrag 1 — In welchem Modul wohnt `migrate_v74`? **(entschieden)**

**Was offen ist.** Der Entwurf schrieb „`db_new_releases.rs` (bzw. das
Modul, das `new_releases` migriert)". Bei der Prüfung gegen `origin/dev`
stellte sich heraus: **ein Modul `db_new_releases.rs` existiert nicht.** Die
Tabelle wird im Basis-Schema `db.rs:370` angelegt; daneben stehen zwei
themenbenannte Migrationsmodule, die sie je für ihren eigenen Zweck neu
bauen — `db_new_releases_accent.rs` (`:63`) und
`db_new_releases_history.rs` (`:39`). Das Haus vergibt pro Migrationsthema
ein eigenes Modul (`db_artwork.rs`, `db_deleted_releases.rs`,
`db_device_sync.rs`, `db_library_doctor.rs` …), nicht pro Tabelle.

**Entschieden (14.08.2026):** ein **neues** Modul
`crates/reprise-core/src/db_new_releases_notify.rs` mit genau
`migrate_v74()`, aufgerufen aus `db.rs` unmittelbar nach `migrate_v73`.
Das folgt der Hausform, und eine brandneue Datei hat null Konfliktfläche —
was für Strang 1s Alleinbesitz an der Migrationskette genau der Punkt ist.

**Die beiden Alternativen und ihr Preis:**
- `migrate_v74` in `db_new_releases_accent.rs` unterbringen: spart eine
  Datei, aber das Modul heißt nach einem fremden Thema (Akzentfarbe), und
  sein Kopfkommentar beschreibt eine andere Migration. Ein späterer Leser
  sucht die Spalte dort nie.
- `migrate_v74` direkt in `db.rs`: bricht die Hausform, nach der `db.rs`
  nur die Kette aufruft und keine Migration selbst enthält, und lässt
  `db.rs` weiter wachsen.

Die Entscheidung ist billig umkehrbar, weil niemand außerhalb von `db.rs`
die Funktion aufruft.

### Nachtrag 2 — Woher `All updates` seinen Concerts-Delta nimmt **(entschieden)**

**Was offen ist.** Beschluss 15 sagt, `All updates` melde „genau den
Concerts-Delta, den das Popover ohnehin berechnet
(`feed_snapshot.rs`, `CONCERTS_DELTA_CAP = 3`, `updates::delta_batch`)".
Die Prüfung gegen `origin/dev` zeigt: `CONCERTS_DELTA_CAP` ist
**`pub(super)`** in `crates/reprise-gnome/src/ui/updates/feed_snapshot.rs:10`
— sichtbar also nur innerhalb von `ui::updates`. Strang 3s neue Datei
`ui/notifications_updates.rs` liegt **nicht** dort und käme ohne eine
Sichtbarkeitsänderung in einer Datei von **Strang 2** nicht heran. Genau das
soll der Schnitt verhindern.

`reprise_core::updates::delta_batch` selbst ist dagegen **`pub`**
(`crates/reprise-core/src/updates.rs:44`) und steht jedem offen.

**Entschieden (14.08.2026): der Deckel wird gar nicht gebraucht.** Strang 3
nimmt die **echte** Zahl ungesehener Konzerte aus dem Kern —
`reprise_core::concerts::count_unseen()`, `pub` in
`concerts/query.rs:137` und aus `concerts.rs:38` re-exportiert — und für den
Body-Text die erste Zeile aus `query_unseen()` (`:106`, ebenso `pub`). Weder
`feed_snapshot.rs` noch eine zweite Deckelkonstante werden angefasst.

**Warum das nicht nur ein Ausweg, sondern die richtigere Antwort ist:**
`CONCERTS_DELTA_CAP = 3` deckelt, wie viele Zeilen ein 470 px breites
Popover **zeigt** — es ist eine Darstellungsgrenze, keine Aussage darüber,
wie viel neu ist. Eine Meldung „3 new concerts" bei zwölf neuen Terminen
wäre schlicht falsch. Der Zähl-Chip im Abschnittskopf nennt nach NR-23
ohnehin die **volle** Stapelgröße und nicht die Zeilenzahl; die
Benachrichtigung stimmt damit mit dem Chip überein, nicht mit der
gedeckelten Liste.

Damit entfällt auch die Prädikat-Drift, die die ursprüngliche Vorgabe in
Kauf genommen hätte (derselbe Deckel an zwei Orten). Beschluss 15 ist
entsprechend zu lesen: `All updates` meldet den Concerts-**Delta**, nicht
den Popover-**Ausschnitt**.

**Verworfene Alternative:** Strang 2 stellt `CONCERTS_DELTA_CAP` und den
Snapshot-Pfad auf `pub(crate)` um. Das macht `feed_snapshot.rs` zu einer
Datei, an der zwei Stränge hängen, und kehrt die Auflösung von §7 um.
**Ebenfalls verworfen:** eine eigene Deckelkonstante im Kern mit demselben
Wert 3 — sie hätte dieselbe Entscheidung an zwei Orten stehen lassen und
obendrein die falsche Zahl gemeldet.

### Nachtrag 3 — Die Zeilennummern der Settings-Zugriffsschicht (keine Entscheidung)

Der Entwurf zitierte `library/settings.rs:44-52` und `:62-84` für die
Key/Value-Zugriffe. Die Datei liegt in **`crates/reprise-core/src/library/`**
(nicht in `reprise-gnome`), und die genannten Zeilenbereiche ließen sich
nicht bestätigen. Das ist keine Entscheidung, nur eine fehlende Nummer:
Strang 3 orientiert sich an den Funktionsnamen der bestehenden
Settings-Leser/Schreiber und legt `updates.notifications` genauso an wie den
nächstliegenden vorhandenen Key. Kein Beschluss hängt daran.

---

## 9. Protokoll der Post-Merge-Querprüfungen (15.08.2026)

Alle drei Stränge sind gelandet — Strang 1 als #493 (`6608af8cca`), Strang 2
als #496 (`334c9adb30`), Strang 3 als #498 (`5bc3fc58a4`). Die Korrekturen aus
diesem Durchgang liegen in #499 (`4f8918e77a`). Gemessen wurde gegen den
gemergten `dev`, mit isolierten XDG-Wurzeln; die Bildbeweise entstanden
headless (Xvfb + openbox, `GDK_BACKEND=x11`, leeres `WAYLAND_DISPLAY`,
`dbus-run-session`, `GSK_RENDERER=cairo`) auf einer **Kopie** der echten
Nutzerdatenbank, nie auf dem Original.

### 1. Traceability über die ganze Regeldatei — **grün**

`UX traceability ok: 379 active rules covered`. Vier Tests trugen noch die
Namen zurückgezogener Regeln und machten das Quality-Gate auf `dev` rot; sie
zeigen jetzt auf die Regel, die sie tatsächlich messen:
`conc_7_filter_changes_refresh_badge_dependents` → NR-35, und die drei
`nr_22_*` in `crates/reprise-core/src/artist_news_progress_tests.rs` → NR-37.
NR-37 hat dabei die `[core]`-Kennzeichnung zurückbekommen, die es von NR-22
geerbt und in der Neufassung verloren hatte — die drei Kerntests sind genau
seine Kernhälfte.

### 2. Ein Wort, zwei Flächen — **grün in der geprüften Form, mit einer Lücke daneben**

`Off sale` heißt in der Concerts-Tabelle und in der Popover-Zeile identisch
`Off sale`, aus derselben Konstante. Daneben steht eine Abweichung, die keine
Regel entscheidet: die Tabelle zeigt für **alle drei** Werte ein Wort
(`On sale` / `Off sale` / `Unknown`, `concerts_status_cells.rs:58-75`), das
Popover setzt seinen Tag nur bei `OffSale` (`updates/concerts_section.rs:65-68`)
— bei `OnSale` und `Unknown` bleibt die Zeile ohne Tag. Im Bildbeweis
`41-footer-loaded` ist das direkt zu sehen: nach einem echten Abruf tragen drei
Tabellenzeilen `On sale`, dieselben Ereignisse im Popover tragen nichts.
NR-36 spricht vom „status tag" im Zeilenende, sagt aber nicht, welche Werte ihn
bekommen; CONC-12 regelt nur die Herkunft der Werte. **Offene Entwurfsfrage,
kein Regelverstoß.**

### 3. Ein Zeitstempel, drei Fußzeilen — **grün**

`git grep -n 'Updated .*ago'` über `crates/reprise-gnome/src/ui` liefert für
Concerts, Releases und Popover keinen Treffer mehr. Der letzte Treffer war ein
Fixture-Literal im Releases-Fehlerbanner-Test; produktiv füllt
`render_current_failure` (`releases_view.rs:593`) den Banner mit
`strings::news_timestamp_date`, also `%Y-%m-%d`. Verbleibender Treffer
repoweit: `strings_podcasts.rs:559` — eine Fläche außerhalb dieser Prüfung.

### 4. Dieselbe URL — **grün, und OS-6 wurde dabei geschärft**

Popover-Zeile (`updates/release_row.rs:66-71`) und Benachrichtigung
(`notifications_updates.rs:74-79`) rufen beide
`reprise_core::artist_news_links::announce_url_or_fallback(release.announce_url.as_deref(), &release.release_group_mbid)`
und geben das Ergebnis derselben `external_link::launch()`-Schranke; nur das
Log-Kontextwort unterscheidet sich. `notification_link_matches_the_release_result_value`
hält das fest.

OS-6 behauptete das pauschal für „eine Benachrichtigung". Ab vier fälligen
Releases fasst `release_notification_specs` sie aber zu **einer** gesammelten
Meldung zusammen, deren Ziel `NotificationTarget::View("releases")` ist
(`notifications_updates.rs:57`) — es gibt dort kein einzelnes Release, auf das
eine URL zeigen könnte. Die Regel sagt das jetzt.

### 5. Geometrie-Parität — **teilweise erfüllt; R4 ist an einer Stelle überholt**

| Merkmal | Popover | Concerts-Tabelle | Befund |
|---|---|---|---|
| 44×44-Kachel | `release_row.rs:19` `COVER_EDGE = 44`, CSS `.new-release-cover` | **existiert nicht** — in `ui/concerts/` kein Bild-Widget, `ConcertColumn` kennt keine Cover-Spalte | nicht vergleichbar |
| 2px-Akzentmarke | `updates/css.rs:68-76` | `concerts/css.rs:36-43` | wortgleiche Deklaration, in zwei Selektoren dupliziert |
| Tag, Akzent-Variante | `.updates-tag-accent` | `.reprise-concert-ticket-tag.on-sale` | in Rahmen, Farbe und Füllung gleich |
| Tag, Neutral-Variante | `.updates-tag-neutral`, Füllung `transparent` | `.off-sale`, Füllung `alpha(@window_fg_color, 0.08)` | **einzige echte Abweichung** |

Grundmaß der Pille (`border-radius: 999px; padding: 2px 8px; font-size: 11px`),
Rahmen und Textfarbe stimmen überein. Die Kachel ist der schwerere Punkt: R4
unterstellt, beide Zeilen zeigten eine 44×44-Kachel, aber Strang 1 hat die
Tabelle bewusst auf **einzeilige** Zeilen gebaut — dort passt konstruktiv keine
Kachel hinein. **R4 ist an dieser Stelle als Anforderung überholt, nicht
verletzt.** Kein Screenshot kann das heilen; es gibt kein Gegenstück zum
Vergleichen.

### 6. Migrationskette am Stück — **grün, auf echten Daten**

`db_concerts::tests::a_v72_database_reaches_v74_with_both_new_columns` ist grün.
Stärker noch: die Laufkopie der echten Nutzerdatenbank (254 MB) stand vor dem
Lauf auf `user_version = 72` und nach einem einzigen Öffnen durch den gemergten
Build auf **74**, mit `concert_events.ticket_availability` und
`new_releases.notified_released_at` vorhanden. Die Verdrahtung steht daneben im
Code: `db.rs:754-756` ruft `migrate_v72`/`v73`/`v74` bei jedem Open unbedingt
und in Reihenfolge auf, jede Funktion selbst versionsgegatet.

### 7. Die üblichen Gesamt-Gates — **grün, nachdem zwei Budgets nachgezogen waren**

`fmt`, `clippy -D warnings`, `cargo doc` mit `RUSTDOCFLAGS=-D warnings`,
`cargo test --workspace` (61 Ergebniszeilen, 0 Fehler), die serialisierten
`reprise-platform-linux`-Tests, die Runtime-Service-Bus-Tests unter
`dbus-run-session`, die Architektur-Lint samt Kernreinheit, die
UX-Traceability, `cargo audit` (eine bekannte erlaubte Warnung), die
Display-Suite (**712 Tests einzeln, 0 Fehler**) und die zwölf weiteren
statischen Skripte der CI-Gate-Kette laufen sauber durch.

Zwei Befunde mussten dafür behoben werden, beide in der Frontend-Schlankheits-
Prüfung — einem Skript, das **nur in CI** läuft:
`ui/strings_notifications.rs` fehlte in der Dead-Code-Freigabeliste, und das
`rusqlite`-Budget musste von 109 auf 113 steigen. Die vier neuen Stellen sind
ausschließlich die `Result<_, rusqlite::Error>`-Rückgabetypen von
`ui/notifications_updates.rs`; `db_handle_access` bleibt `none (banned)`.

**Lehre für die nächste Runde:** dieselbe Datei hat in dieser Runde dreimal das
Quality-Gate auf `dev` rot gemacht (#493 der `view_floor`, #498 die beiden
obigen). Eine lokale Gate-Liste muss aus der CI-Gate-Kette abgeleitet werden,
nicht aus dem Gedächtnis.

### 8. Die Abnahme aus §5 — **weitgehend erbracht, zwei Einschränkungen**

Bildbeweise (alle headless auf der Laufkopie):

| §5.1 | Datei | Ergebnis |
|---|---|---|
| 1 Popover mit beiden Abschnitten | `01-popover` | identische Zeilenform in beiden Abschnitten, Kacheln, Titel/Meta zweizeilig, Tag rechts, dezenter Ausblenden-Knopf, **eine** Fußzeile |
| 2 Popover mit Hover | `11-hover-release` | Tönung **plus** 2px-Akzentmarke links, Ausblenden-Knopf in vollem Kontrast, Tooltip „Opens MusicBrainz" |
| 3 leerer Concerts-Abschnitt | `33-popover-empty-concerts` | Kopf steht, darunter „No new concerts" |
| 4 Concerts-Übersicht | `23-concerts-t36` | einzeilige Zeilen, kein Ticketmaster-Knopf, Status-Tags rechts, Fußzeile mit einem Zeitstempel |
| 5 mit `Source`-Spalte | `30-columns-editor`, `31-concerts-venue-source` | Editor und eingeschaltete Spalten; Venue-Text misst RGB≈(231,233,236) gegen City RGB≈(170,172,175) — Venue ist heller |
| 6 gefiltert | `32-concerts-filtered` | 3 von 412, Endzeile direkt unter der dritten Zeile, Fußzeile bleibt am unteren Rand |
| 7 Fußzeilenzustände | `40-footer-updating`, `41-footer-loaded`, `42-footer-checked`, `43-footer-offline` | alle vier |
| 8 Benachrichtigung mit Cover | `notif/deathrace.png`, `notif/no-place-for-me.png` | **als Bus-Nachweis, nicht als Bild der Blase** — siehe unten |

Aus §5.2:

- **„Off sale kommt wirklich aus der Quelle"** — belegt über die ganze Kette:
  `event_parser_maps_local_date_time_and_venue` parst einen echten
  Ticketmaster-Körper mit `"status":{"code":"offsale"}`,
  `ticket_availability_roundtrips_through_persisted_text`,
  `reconcile_updates_ticket_availability_for_an_existing_event` und
  `query_events_reads_persisted_availability_after_reopening_the_database`
  decken Speichern und Wiederlesen nach Neustart ab. Dazu die
  Live-Beobachtung in `41-footer-loaded`: ein echter Abruf hob die Zahl von
  412 auf 414 und setzte auf genau drei Zeilen `On sale`, während der Rest
  `Unknown` blieb.
- **„Beim Öffnen wird nicht bedingungslos gefetcht"** — im Mitschnitt
  (`REPRISE_LOG=debug`) erzeugte **keine** der beiden Öffnungen eine
  Concerts-Netzanfrage, und die Fußzeile sagte beide Male `checked`, nicht
  `loaded`. **Einschränkung:** da schon die erste Öffnung nicht fällig war,
  belegt der Lauf „in keinem Fall wurde gefetcht", nicht sauber isoliert
  „die zweite Öffnung unterdrückt einen sonst fälligen Fetch".
- **„Der erste Fetch meldet nichts"** — durch
  `os_6_the_first_fetch_announces_nothing` abgedeckt, nicht zusätzlich von
  Hand nachgestellt.

**Zu Bild 8, der Benachrichtigung mit Cover.** Auf dieser Maschine läuft kein
Benachrichtigungs-Dienst (`dunst`, `mako`, `xfce4-notifyd` fehlen; GNOME Shell
läuft headless nicht), also gibt es kein gerendertes Bild der Blase. Statt
dessen wurde die Meldung auf dem privaten Bus abgefangen: ein Stub-Dienst
besaß im selben `dbus-run-session` wie die App den Namen
`org.freedesktop.Notifications`. Die App ruft **genau dieses** Interface auf,
nicht den Portal- und nicht den `org.gtk.Notifications`-Weg.

Mit zwei fällig gemachten Releases kamen **zwei** Meldungen statt einer
gesammelten — wie OS-6 es verlangt, das erst ab vier zusammenfasst. Jede kam
in zwei Zügen: zuerst ohne Cover, dann per `replaces_id` erneut mit dem Hint
`image-path`, sobald der asynchrone Abruf aus dem Cover Art Archive
durchgelaufen war. Das ist genau OS-6s „und das Cover, wenn es verfügbar ist",
und es zeigt, dass die Meldung nicht auf das Bild wartet.

Inhalt der Meldungen: Summary `DEATHRACE` mit Body
`Rising Insane · Album · out today`, und Summary `No Place for Me` mit Body
`Miss May I · Album · out today` — also Titel und `{artist} · {type} · out
today` wie in der Regel beschrieben, dazu die Default-Aktion. Die
herausgeschriebenen Cover sind 250×250 und zeigen die echten Album-Motive,
kein generisches App-Icon.

**Was damit weiterhin nicht gezeigt ist:** wie die Blase aussieht. Der
Nachweis deckt Inhalt, Aktion und Bild ab, nicht die Darstellung.

**Zwei Nebenbefunde aus dem Prüfstand**, für die nächste Abnahme festgehalten:

- Ein leerer Concerts-Abschnitt entsteht **nicht** dadurch, dass man das
  Popover zweimal öffnet: ein gesehener Batch bleibt bewusst stehen
  (`reprise-core/src/updates.rs::delta_batch`, „looking twice must not empty
  the popover"). Leer wird der Abschnitt nur, wenn der aktive Filter
  buchstäblich null Treffer hat.
- Für den Offline-Zustand reicht `unshare -rn` **nicht**: `GNetworkMonitor`
  fragt über den System-D-Bus (AF_UNIX, von Netzwerk-Namespaces nicht
  isoliert) weiter den echten NetworkManager und meldet „online". Zusätzlich
  muss `DBUS_SYSTEM_BUS_ADDRESS` auf einen nicht existierenden Socket zeigen.

### Was offen bleibt

- Von Bild 8 fehlt nur noch die Darstellung der Blase selbst; Inhalt, Aktion
  und Cover sind belegt. Dafür bräuchte es einen installierten
  Benachrichtigungs-Dienst auf der Prüfmaschine.
- Der fehlende Status-Tag im Popover bei `OnSale` und `Unknown` (Prüfung 2)
  ist eine Entwurfsentscheidung, die noch niemand getroffen hat.
- R4s Kachel-Parität ist gegenstandslos geworden und sollte beim nächsten
  Anfassen des Plans umformuliert statt weitergeschleppt werden.
