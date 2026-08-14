---
slug: location-is-not-a-concerts-setting
worktree: /home/marvin/Projects/reprise-location-is-not-a-concerts-setting
branch: feature/location-is-not-a-concerts-setting
phase: planned
codex_session:
created: 2026-08-14
---
# Der Standort ist keine Concerts-Einstellung

> Alle Zeilennummern wurden gegen `origin/dev` @ `5721ade95e` erhoben
> (14.08.2026, nach dem Merge von #471). Der lokale Hauptcheckout stand auf
> `be5f014d3b` und taugt nicht als Basis — insbesondere `ui/radio/` weicht dort
> ab. Wo unten eine Zeilennummer nicht mehr passt, gilt der genannte
> Funktions-/Symbolname; Nummern nachziehen, nicht raten.

Entwurf: Claude-Design-Projekt `c947ce4e`, Datei `Plugins Preferences.dc.html`,
Panel **3a** (Zeilen 25–215) und `agent-prompt-location.md`. Die beiden anderen
Panels derselben Datei (**2a** Preferences-Suche, **1a** Plugins-Hauptschalter)
sind eigene Themen und **nicht** Teil dieses Auftrags.

---

## 0. Ausgangslage — was schon steht und was nicht

Die Aufgabenstellung nimmt an, der Standort liege in der Plugin-Konfiguration
von Concerts. Das gilt nur noch für die **Oberfläche**, nicht für die
Speicherung:

- **Die Speicherung ist bereits app-weit.** `crates/reprise-core/src/location.rs`
  hält seit `O-4` (29.07.2026) die Keys `location.lat` / `location.lon` /
  `location.name` / `location.country_code` (`:17-26`) in der allgemeinen
  `settings`-Tabelle. `concerts::config::location()` (`concerts/config.rs:86-90`)
  leitet nur noch dorthin weiter. Der Wert überlebt heute schon ein
  abgeschaltetes Concerts-Modul und einen abgeschalteten Online-Gate: der
  Gate (`online_sources::network_allowed()`) sperrt Netzanfragen, nicht das
  Lesen.
- **Der Radius liegt noch bei Concerts.** `concerts.default_radius_km`
  (`concerts/config.rs:10`) ist der app-weite Vorgabewert,
  `concerts.filter.radius_km` (`:13`) der aktuell aktive Ansichtsfilter.
  Presets `[100, 250, 500, 1000]` (`:18`), Default `1000.0` (`:17`).
- **Die Oberfläche gehört ganz Concerts.** `preference_concerts.rs`:
  City-`EntryRow` + beide Buttons + Status-Zeile in `location_rows()`
  (`:394-413`), Geocoding beim Bestätigen (`:420-443`), XDG-Portal für „Use
  current location" (`:450-471`), „Clear location" (`:479-483`), der einzige
  Schreibpfad `apply_location()` (`:534-540`), Radius-`ComboRow`
  (`:595-599`) mit Schreibzugriff auf `DEFAULT_RADIUS_KEY` (`:602-614`).
- **Der Deep-Link existiert schon**, zeigt aber auf die Plugins-Seite:
  `preferences.rs:440-444` (`present_location_settings()` →
  `present_plugins(["concerts"])`), Aufrufer `window_runtime_wiring.rs:708`.
  Der Kommentar dort sagt selbst, `O-4` habe „nur die Speicherung gehoben,
  nicht diese Seite".
- **Das Änderungssignal gehört Concerts.** `ConcertsRuntime::notify_settings_changed()`
  (`concerts/concerts_worker.rs:243`) mit `settings_subscribers`. Jeder
  Standort-Schreibvorgang ruft es (`preference_concerts.rs:542`, `:557`).
  Radio kann daran nicht hängen, ohne genau die Abhängigkeit zu erben, die
  dieser Auftrag auflöst. **Das ist der Kern der Arbeit, nicht ein Detail.**

### Die Prämisse aus §5 stimmt — hier ist die Wurzel

`active_facets()` (`concerts/concerts_filter_bar.rs:62-77`) zählt den
Radius-Facet als aktiv, sobald `filter.radius_km.is_some()`. Das ist er
**immer**, weil `config::persisted_filter()` (`concerts/config.rs:104-115`)
ohne gespeicherten Filterwert auf `DEFAULT_RADIUS_KEY` und weiter auf
`DEFAULT_RADIUS_KM` zurückfällt. Ob ein Standort existiert, fragt an dieser
Stelle niemand. Daraus folgt alles drei:

- `rebuild()` (`:349-399`) rendert einen Radius-Chip, den `:372-376` nur
  insensitiv schaltet (Tooltip `CONCERTS_SET_LOCATION_TOOLTIP`) — er
  verschwindet nicht;
- `active` (`:357`) wird wahr → Zählung schaltet auf
  `concert_count_line_markup(shown, total)` = `"{shown} of {total} concerts"`
  (`strings_concerts.rs:78`);
- `query::filtered_events()` (`concerts/query.rs:196-200`, `:247-252`) berechnet
  ohne Standort gar keine Distanz und filtert folglich nichts → `shown == total`
  → **„415 of 415"**, jede Distance-Zelle ein „—"
  (`concerts_presentation.rs:32-37`).

Der Fix hängt an einem Satz: **die Aktivität des Radius-Facets muss vom
Standort abhängen, nicht nur vom gespeicherten Wert.**

---

## 1. Beschlüsse (zwei davon vom Auftraggeber am 14.08.2026 entschieden)

1. **Podcasts (§6) wird nicht wörtlich umgesetzt.** Einen Filter „in deiner
   Nähe" gibt es bei Podcasts nicht — `PodcastFilter`
   (`podcasts/podcasts_presentation.rs`) kennt nur Unplayed / Source /
   Downloaded / Query. Der einzige Standort-Leser dort ist der Länder-Chip im
   Apple-Podcasts-Dialog (`podcasts/add_dialog.rs:195`), der laut **SRC-19**
   bewusst auf die System-Locale zurückfällt. Entscheidung: Die dritte
   „Used by"-Zeile heißt ehrlich
   `Podcasts · Popular in DE` / `Apple's country chart in Add Podcast`,
   **kein** leerer Zustand, SRC-19 bleibt unangetastet. Badge bleibt `3` — der
   Chip liest den Standort tatsächlich, er kommt nur ohne ihn zurecht.
2. **Standort ohne Ländercode bekommt einen eigenen Text.** „Use current
   location" (XDG-Portal) speichert nie ein `country_code`
   (`location.rs:20-26`), also kann Radios „Near you" auch mit gesetztem
   Standort nicht suchen (`radio_chips.rs:36-44`). „No location set" wäre dort
   gelogen. Gleiche Hülle, eigener Text — Wortlaut siehe Paket E.
3. **Keine Migration** (Abweichung von §2, folgt `AGENTS.md:269`): Reprise ist
   nicht ausgeliefert, es gibt keine Installationen. Kompatibilitätsfallbacks
   und Alt-Key-Leser sind laut Repo-Regel ausdrücklich **kein**
   Entwurfskriterium. Der Radius-Key wird sauber umbenannt, ohne Lesen des
   alten Namens. `location.rs:11-12` hat für die Standort-Keys damals genau so
   entschieden.
4. **Nur der Vorgabewert wandert, der Ansichtsfilter bleibt.**
   `concerts.default_radius_km` → `location.default_radius_km` (app-weit, auf
   der Location-Seite bearbeitbar). `concerts.filter.radius_km` bleibt, wo er
   ist: das ist der Zustand *einer Ansicht*, nicht der Standort.
5. **Die Presets bleiben unverändert** `[100, 250, 500, 1000]`, Vorgabe
   `1000 km` — §1 verlangt „Werte wie heute".

---

## 2. Pakete

Reihenfolge: **A → (B ∥ C ∥ D ∥ E) → F**. A legt Key und Broadcast an, davon
hängen alle anderen ab. B–E sind danach unabhängig und dürfen parallel laufen;
sie berühren disjunkte Dateien. F (Regeln/Tests) schließt ab, weil es die
tatsächlich gebaute Fassung beschreiben muss.

### Paket A — Fundament: app-weiter Radius + app-weites Standort-Signal

**A1 — Radius-Vorgabe nach `location`.**
`crates/reprise-core/src/location.rs`: neu `LOCATION_DEFAULT_RADIUS_KEY =
"location.default_radius_km"`, dazu `RADIUS_PRESETS_KM` und `DEFAULT_RADIUS_KM`
(aus `concerts/config.rs:17-18` hierher verschoben, nicht kopiert) sowie
`default_radius_km(&Db)` / `set_default_radius_km(&Db, f64)`.
`concerts/config.rs`: `DEFAULT_RADIUS_KEY` (`:10`) **löschen**;
`persisted_filter()` (`:104-115`) liest den Vorgabewert aus
`location::default_radius_km()`. Kein Fallback auf den alten Key (Beschluss 3).
Aufrufer von `RADIUS_PRESETS_KM` mitziehen (`concerts_filter_bar.rs:435-451`,
`preference_concerts.rs:595-599`).

**A2 — Standort-Broadcast aus Concerts herauslösen.**
Neu: eine app-weite Bekanntmachung „Standort hat sich geändert", nach dem
Vorbild von `ConcertsRuntime::settings_subscribers`
(`concerts_worker.rs:239-245`) — dieselbe `subscribe(is_alive, callback)` /
`notify()`-Form, damit es kein zweites Muster im Haus gibt. Sie hängt an einem
Ort, den auch Radio erreicht, ohne die Concerts-Runtime anzufassen
(Vorschlag: neben dem übrigen Fensterzustand in `ui/`, nicht in `ui/concerts/`).
Wer notifiziert: ausschließlich der neue Schreibpfad aus B1.
Wer abonniert: die Concerts-Ansicht (heute über die Concerts-Runtime, siehe
`concerts_view.rs:41`), die Verweiszeile aus Paket C, der Add-Station-Dialog
aus Paket E.
Concerts' eigenes `notify_settings_changed()` bleibt bestehen — es trägt
weiterhin `app_id`, Zeitfenster und Similar-Einstellungen
(`preference_concerts.rs:222`, `:238`, `:309`, `:322`, `:613`, `:631`).
**Achtung:** `ConcertsRuntime::request()` (`:247-250`) gibt bei abgeschaltetem
Modul `false` zurück — der neue Broadcast darf keine solche Sperre erben, sonst
verliert Radio das Signal genau dann, wenn Concerts aus ist. Das ist der Punkt,
den dieser Auftrag beweisen soll.

### Paket B — Die neue Seite `Preferences › Location`

**B1 — Neues Modul `preferences/preference_location.rs`.**
Die gesamte Standort-Logik zieht aus `preference_concerts.rs` hierher um
(verschieben, nicht duplizieren): `location_rows()` (`:394-413`), der
Bestätigen-Handler mit Geocoding (`:420-443`), der Portal-Pfad (`:450-471`),
`clear_location()` (`:479-483`, `:554-558`), `apply_location()` (`:534-540`),
`radius_row()` (`:595-599`, `:602-614`). Nach dem Umzug ruft der Schreibpfad
den Broadcast aus A2 statt `runtime.notify_settings_changed()`.

Seitenaufbau, Texte **wörtlich** aus §1 (Englisch, über die bestehenden
`N_!`-Konstanten in einem neuen `strings_location.rs`; die drei bereits
existierenden Strings `CONCERTS_USE_CURRENT_LOCATION`,
`CONCERTS_CLEAR_LOCATION`, `CONCERTS_DEFAULT_RADIUS` — `strings_concerts.rs:54`,
`:55`, `:58` — ziehen mit um und werden umbenannt; die `po/`-Dateien werden
**nicht** von Hand angefasst):

1. Einleitung: `One place, used by everything that asks "near you". Set once — no plugin owns it.`
2. Karte (eine `AdwPreferencesGroup`, zwei Zeilen):
   - `City` mit dem aktuellen Wert als Untertitel (`Berlin, DE`), Stift zum
     Bearbeiten, danach `Use current location` und `Clear location`
     — unverändertes Verhalten aus den heutigen Concerts-Einstellungen.
   - `Default radius` als `AdwComboRow`, Presets wie heute, Vorgabe `1000 km`.
3. Gruppe `Used by` mit Badge `3` und drei aktivierbaren Zeilen, die das
   jeweilige Feature öffnen:
   - `Concerts` — `Upcoming shows within the radius, for artists in your library`
   - `Radio · Near you` — `Stations from your country and city in Add Station`
   - `Podcasts · Popular in DE` — `Apple's country chart in Add Podcast`
     (Beschluss 1; das `DE` ist der real aufgelöste Code bzw. der
     Locale-Fallback aus SRC-19, kein fester Text)
4. Fußnote: `Clearing the location only stops these three. Switching a plugin off never removes it.`

**B2 — Seite registrieren.** `PageId::Location` in
`preferences_window.rs:7-21` **zwischen `Library` und `Plugins`** in `PageId`
*und* `PAGE_ORDER`; Titel/Icon in `:42-61`; Fabrik-Arm in
`preferences.rs:283-289`. Die Suche indiziert die Seite dadurch automatisch
(`preferences_search_index.rs:110` läuft über den Widget-Baum, SET-13) — das ist
zu **prüfen**, nicht anzunehmen: eine Suche nach „radius" muss die neue Seite
treffen.

**B3 — Icon.** Map-Pin, symbolisch. Der Name ist gegen das *ausgelieferte*
Icon-Theme zu prüfen, nicht aus dem Gedächtnis zu setzen (im Haus gab es
bereits einen Fall eines nicht existierenden Adwaita-Icons):
`find-location-symbolic` prüfen, sonst `mark-location-symbolic`, sonst ein
mitgeliefertes eigenes. Ein fehlendes Icon fällt still auf „kaputtes Bild"
zurück — im Screenshot der Abnahme muss der Pin zu sehen sein.

### Paket C — Concerts-Plugin: Felder raus, Verweis rein

**C1 — Entfernen.** Aus `preference_concerts.rs` `build()` (`:191-275`) fallen
die City-Zeile samt beider Buttons und die Radius-Zeile ersatzlos weg (die
Logik ist in B1 umgezogen). Es bleiben: `Bandsintown app_id` (`:288-292`),
`Consider artists played in the last N days` (`:620-621`),
`Include similar artists` (`:209-212`), `Similar artists per top artist`
(`:226-228`).

**C2 — Verweiszeile.** Als **erste** Zeile unter dem Concerts-Schalter, optisch
abgesetzt (dezenterer Hintergrund, gedämpfter Text, nicht editierbar):
Map-Pin-Präfix, Text `Location · Berlin, DE, within 1000 km`, rechts
`Change in Location →`. Ohne gesetzten Wert: `Location · not set` und
`Set location →`. Beides führt auf den Deep-Link aus D. Die Zeile abonniert den
Broadcast aus A2 und aktualisiert sich sofort — **CONC-4b** verlangt für
Standort- und Radiusänderungen ohnehin sofortige Neubewertung.

### Paket D — Deep-Link umhängen

`preferences.rs:440-444`: `present_location_settings()` öffnet die neue Seite
(`open(Some("location"))`, Namensauflösung `preferences_window.rs:108-113`)
statt `present_plugins(["concerts"])`, setzt den Fokus in das City-Feld und hebt
es kurz hervor, solange kein Wert gesetzt ist.
`SettingsDeepLink::ConcertLocation` heißt danach `Location`; der Arm in
`plugin_targets_for_deep_link` entfällt, weil es kein Plugin-Ziel mehr gibt.
Bestandsaufrufer: `window_runtime_wiring.rs:708`. Der separate
Concerts-Preferences-Button (`present_plugins(["concerts"])`) bleibt, wie er
ist — er meint das Plugin, nicht den Standort.

### Paket E — Concerts-Ansicht und Radio ohne Standort

**E1 — Radius-Facet nur mit Standort aktiv (der Kern).**
`active_facets()` (`concerts_filter_bar.rs:62-77`) bekommt den Standortzustand
gereicht und nimmt `FilterFacet::Radius` **nicht** auf, solange keiner gesetzt
ist. Damit fällt `active` (`:357`) zurück, die Zählung zeigt automatisch
`415 concerts` (`strings::concert_total_line`, `:394`) statt `415 of 415`, und
die Bruchzahl verschwindet, weil sie nichts mehr behauptet. `has_location` ist
bereits vorhanden (`:372`, `:413`).

**E2 — Der Chip sagt „off" und führt zum Standort.** Statt des heute nur
insensitiven Chips (`:372-376`) ein eigener, gestrichelter, gedämpfter Chip
`500 km · off`, der **nicht** in die aktive Filterliste zählt und dessen Klick
den Deep-Link auslöst — nicht wie sonst den Filter entfernt (`:378-382`). Der
Facet-Eintrag im Chooser (`:413-416`) bleibt insensitiv wie heute.

**E3 — Leiste über der Tabelle.** Dezent im Akzent getönt, Titel
`No location set — showing all 415 concerts worldwide` (Zahl aus dem realen
Gesamtbestand), Unterzeile
`Distance and the radius filter stay switched off until a city is known.`,
rechts `Set location →`. Hausform: die Karten-/Bannerform aus
`source_error_banner.rs:73-100` (`Revealer` + `card` + `heading` + `dim-label`
+ Aktionsbox) trägt Titel *und* Unterzeile *und* Knopf; `adw::Banner`
(`online_discovery_banner.rs:69-80`) kann nur eine Zeile und ist hier
**nicht** geeignet. Platz: über der Tabelle, neben dem bestehenden
`SourceErrorBanner` (`concerts_view.rs:143-147`). Beide dürfen gleichzeitig
sichtbar sein — Reihenfolge festlegen und im Screenshot zeigen.

**E4 — Distance-Spalte ausblenden, Breite an Venue.**
`concerts_columns.rs:344-355` definiert die Spalte, `concerts_view.rs:102`
hängt sie ein. Ohne Standort ist sie unsichtbar; die frei werdende Breite geht
an Venue (`:332-343`). **Nicht** mit „—" füllen (`concerts_presentation.rs:32-37`
bleibt für den Einzelfall bestehen). Fallstrick: die Sichtbarkeit ist
persistiert (`concerts_column_layout.rs`, Header-Popover
`concerts_view.rs:105-108`) — das automatische Ausblenden darf die vom Nutzer
gespeicherte Spaltenwahl **nicht** überschreiben, und beim Setzen eines
Standorts muss genau der vorherige Zustand zurückkommen. Im Header-Popover
darf die Spalte ohne Standort nicht als „einschaltbar" angeboten werden.

**E5 — Sortierung.** Ohne Standort ist „nach Distanz" nicht wählbar;
Vorgabesortierung bleibt Datum (`concerts_view.rs:283`). `apply_sort()`
(`:689-701`) darf einen gespeicherten Distanz-Sortierzustand nicht
wiederherstellen, solange kein Standort da ist — sonst sortiert die Ansicht
nach einer Spalte, die sie gerade versteckt.

**E6 — Radio „Near you": leerer Zustand statt Zwangsnavigation.**
Heute öffnet der Klick auf den Chip ohne Standort direkt die Einstellungen
(`radio_chips.rs:36-44` → `NearYouAction::OpenLocationSettings`,
`add_dialog.rs:492-500`, Callback gesetzt `:367`). Neu: Der Chip bleibt
anwählbar, der **Ergebnisbereich** zeigt den leeren Zustand
(`add_dialog.rs:158/202` ist heute nur `ListBox` + Status-`Label` + Spinner —
es braucht eine echte Leerzustandsfläche in der Hausform von
`source_empty_state.rs:62-100`):

- Map-Pin-Icon, Titel `No location set`, Text
  `Near you needs a city to look up stations. It is one setting, shared with Concerts and local podcasts.`,
  Knopf `Open Preferences › Location` mit Pfeil → Deep-Link.
- **Zweiter Fall** (Beschluss 2), Standort ohne Ländercode: gleiche Hülle,
  Titel `Location has no country`, Text
  `Near you filters by country. The location from "Use current location" carries coordinates only — set a city to get one.`,
  gleicher Knopf.
- `NearYouAction` bekommt dafür zwei unterscheidbare Zustände statt des einen
  `OpenLocationSettings`; `Add station` bleibt inaktiv, solange nichts
  ausgewählt ist (`add_dialog.rs:127-129` — unverändert).
- **Rückkehr ohne zweiten Klick:** Der Dialog abonniert den Broadcast aus A2;
  trifft die Meldung ein, während er im Leerzustand steht, läuft die
  „Near you"-Suche selbst an. Zu klären und im Ergebnis zu zeigen: dass der
  Add-Station-Dialog das Öffnen des Preferences-Dialogs überlebt (beide sind
  `adw::Dialog`) — falls nicht, ist der Weg zurück Teil der Aufgabe und nicht
  wegzulassen.

### Paket F — Regeln und Tests nachziehen

`docs/ux-rules.md` beschreibt heute an vier Stellen genau das Verhalten, das
dieser Auftrag ändert. Wer das nicht mitzieht, hinterlässt verwaiste Regeln:

- **CONC-2** (`:4950-4956`): „Without a location, Radius is disabled and carries
  the tooltip 'Set a location in Preferences'" und „Active, it shows 'X of Y
  concerts'". Beides wird ersetzt: Chip `· off` mit Deep-Link, ohne Standort
  keine Bruchzahl, ausgeblendete Distance-Spalte, Leiste über der Tabelle.
- **RAD-5** (`:5789-5819`): schreibt die Zwangsnavigation ausdrücklich fest,
  samt Begründung („a chip that claims to filter by location but does not is
  worse than sending the user to fix the input"). Die Begründung bleibt gültig
  — der leere Zustand erfüllt sie besser als die Zwangsnavigation. Neu fassen,
  inklusive des countryless-Falls.
- **SET-10** (`:1155-1162`): „Plugins is the only settings surface for optional
  capabilities … There are no 'Online sources', 'New Releases', or 'Concerts'
  Preferences main pages." Die Location-Seite ist keine optionale Fähigkeit,
  sondern ein app-weiter Fakt — der Satz braucht eine ausdrückliche Ausnahme,
  sonst liest die nächste Person einen Widerspruch. (Hinweis: **SET-7**,
  `:1119-1124`, sagte das Gegenteil und ist bereits `[replaced by SET-10]`.)
- **SRC-19** (`:5412-5422`): bleibt **unverändert** (Beschluss 1), bekommt aber
  einen Verweis auf die neue Seite, weil der Chip dort als Leser genannt ist.
- **Neu**: eine Regel für die Location-Seite selbst — ein Wert, ein Besitzer;
  jeder Leser ist dort namentlich aufgeführt; ein abgeschaltetes Plugin nimmt
  keine fremde Einstellung mit.

**Tests, die sich ändern müssen** (nicht „anpassen bis grün", sondern die neue
Zusage prüfen):
- `radio/radio_chips.rs:174-186` — „opens the location setting and starts no
  search" beschreibt die alte Zusage; ersetzen durch die beiden Leerzustände.
- `radio/add_dialog_tests.rs:537`, `:562` — dieselbe Umstellung.
- `concerts/concerts_view_tests.rs:140`, `:154` — hängen am
  `notify_settings_changed`-Pfad, der für den Standort auf A2 wechselt.

**Neue Tests (mindestens):**
1. Kern: `persisted_filter()` + `active_facets()` ohne Standort → Radius ist
   **kein** aktiver Facet; mit Standort → doch. Das ist der eine Satz, an dem
   Zählung, Chip und Leiste hängen.
2. Kern: `filtered_events()` ohne Standort liefert `shown == total` — und die
   Zählung sagt das jetzt auch.
3. Kern: `location::clear()` löscht ausschließlich die vier Standort-Keys;
   `location.default_radius_km`, `concerts.*` und die Modulschalter bleiben
   unberührt (§2, letzter Satz).
4. Kern: Standort und Radius bleiben lesbar, wenn Concerts **und** der
   Online-Gate aus sind.
5. GTK: die Location-Seite steht zwischen Library und Plugins.
6. GTK: die Concerts-Verweiszeile zeigt `Location · not set` / den gesetzten
   Wert und aktualisiert sich auf den Broadcast hin.
7. GTK: Suche nach „radius" trifft die Location-Seite (B2).

---

## 3. Abnahme

Headless-Suite und Display-Gate wie üblich. Der Display-Teil ist im Rudel
bekanntermaßen flaky und auf `dev` teils schon rot — **erst gegen `origin/dev`
messen, was ohne diese Änderung rot ist**, sonst wird fremdes Rot als eigene
Schuld verbucht.

Sichtprüfung (Screenshots, je einer pro Zustand — ohne sie gilt keiner der
Punkte als gezeigt):
1. `Preferences › Location`, kein Standort gesetzt: Einleitung, beide Zeilen,
   `Used by` mit Badge 3, Fußnote, Pin im Seitenleisten-Eintrag zwischen
   Library und Plugins.
2. Dieselbe Seite mit `Berlin, DE` und `1000 km`.
3. `Plugins › Concerts` ausgeklappt: keine City-, keine Radius-Zeile, oben die
   abgesetzte Verweiszeile mit `Change in Location →`.
4. Concerts-Ansicht ohne Standort: Leiste, `500 km · off` gestrichelt,
   **keine** Distance-Spalte, `415 concerts`.
5. Dieselbe Ansicht mit Standort: keine Leiste, Distance-Spalte da, normaler
   Chip, `x of y concerts`.
6. `Add Station › Near you` ohne Standort: leerer Zustand mit Knopf.
7. Derselbe Dialog bei per Portal gesetztem Standort ohne Land: der **zweite**
   Text, nicht der erste.

Beweisführung, die über einen Screenshot hinausgeht:
- **„Der Chip filtert nachweislich nicht"** (§5): Gesamtzahl der Konzerte ohne
  Standort gegen die Zahl mit absichtlich winzigem Radius vergleichen — sie
  muss gleich bleiben. Ein Screenshot allein zeigt das nicht.
- **„Ein abgeschaltetes Plugin nimmt nichts mit"**: Concerts abschalten, dann
  Radios „Near you" ausführen — die Suche läuft. Anschließend den Online-Gate
  abschalten und die Location-Seite öffnen — der Wert steht noch da.
- **Kontrollarm**: für 1 und 4 gilt eine Änderung erst als bewiesen, wenn der
  zurückgerollte Zustand jetzt gerade das alte Verhalten zeigt.

---

## 4. Nicht in diesem Auftrag

- Panel **2a** (Preferences-Suche schiebt den Dialog, ESC-Stufen) und Panel
  **1a** (Plugins-Hauptschalter) aus derselben Entwurfsdatei.
- Ein echter Umkreisfilter für Podcasts (Beschluss 1).
- Reverse-Geocoding, um dem Portal-Standort ein Land zu verschaffen — `O-4` und
  RAD-5 verbieten den zusätzlichen Netzaufruf ausdrücklich.
- `po/`-Dateien von Hand: neue Strings kommen über den normalen Extraktionslauf.
