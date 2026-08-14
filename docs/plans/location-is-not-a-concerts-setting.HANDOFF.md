# Übergabe — Der Standort ist keine Concerts-Einstellung

Stand: **2026-08-14, 16:25**. Plan: `docs/plans/location-is-not-a-concerts-setting.md`
(`phase: refactored`). Die Arbeit ist fertig, geprüft und **nicht gelandet**.
Nichts wurde gepusht.

| | |
|---|---|
| Branch | `feature/location-is-not-a-concerts-setting` |
| Worktree | `/home/marvin/Projects/reprise-location-is-not-a-concerts-setting` |
| HEAD | `ee99dc4260` |
| Branch-Punkt | `origin/dev` @ `5721ade95e` (#471) |
| Umfang | 13 Commits, 59 Dateien, +3031/−751 |
| Arbeitsbaum | sauber |

## Was der Auftrag war

Der Standort (City + Radius) lag in den Einstellungen des Concerts-Plugins, wurde
aber von mehreren Features gelesen. Wer Concerts nie öffnete, fand das Feld nicht;
wer Concerts abschaltete, verlor Funktionen, die mit Konzerten nichts zu tun haben.
Der Standort wird deshalb eine app-weite Einstellung mit eigener Seite.

Vorlage: Claude-Design-Projekt `c947ce4e`, `Plugins Preferences.dc.html`, **Panel 3a**.
Die anderen beiden Panels derselben Datei (**2a** Preferences-Suche, **1a**
Plugins-Hauptschalter) sind eigene Themen und wurden **nicht** angefasst.

## Beschlüsse, die von der Vorlage abweichen

Wer hier weiterarbeitet, muss diese vier kennen, sonst wirkt der Code falsch:

1. **Keine Migration.** Die Vorlage forderte eine Einmal-Migration alter Werte.
   Entfällt: Die Standort-Keys waren seit `O-4` (29.07.2026) ohnehin app-weit
   (`location.lat/lon/name/country_code`), und `AGENTS.md:269` verbietet
   Kompatibilitätsfallbacks, solange Reprise nicht ausgeliefert ist. Nur der
   Radius-Vorgabewert zog um — `concerts.default_radius_km` →
   `location.default_radius_km`, sauber umbenannt, ohne Alt-Key-Leser. Ein
   Regressionstest setzt den alten Key und prüft, dass er ignoriert wird.
2. **Nur der Vorgabewert wanderte.** `concerts.filter.radius_km` (der aktive
   Ansichtsfilter) blieb bei Concerts — das ist Zustand einer Ansicht, nicht der
   Standort.
3. **Podcasts bekam keinen leeren Zustand** (Nutzerentscheidung 14.08.2026). Einen
   Filter „in deiner Nähe" gibt es dort nicht; der einzige Standort-Leser ist der
   Länder-Chip im Apple-Dialog, der laut **SRC-19** bewusst auf die System-Locale
   zurückfällt. Geändert wurde nur die „Used by"-Zeile:
   `Podcasts · Popular in {country}` / `Apple's country chart in Add Podcast`.
   SRC-19 blieb inhaltlich unangetastet.
4. **Standort ohne Ländercode hat einen eigenen Text** (Nutzerentscheidung
   14.08.2026). „Use current location" (XDG-Portal) speichert nie ein
   `country_code`, also kann Radios „Near you" auch mit gesetztem Standort nicht
   suchen. Dort steht `Location has no country`, nicht `No location set` — sonst
   widerspräche die App ihrer eigenen Location-Seite.

## Was gebaut wurde

- **A** `1f19104ce5` — Radius-Vorgabe app-weit; neuer `LocationBroadcast`
  (`ui/location_broadcast.rs`), angelegt in `window_runtime_setup.rs` unabhängig
  von `ConcertsRuntime`. Das war der Kern: das Signal „Standort geändert" gehörte
  vorher der Concerts-Runtime, und Radio konnte nicht daran hängen, ohne genau die
  Abhängigkeit zu erben, die der Auftrag auflöst.
- **B** `383f09fb15` — `Preferences › Location` (`preference_location.rs`, ~600
  Zeilen), Seitenleisten-Eintrag zwischen Library und Plugins, Icon
  `find-location-symbolic`. Die gesamte Standort-Logik zog aus
  `preference_concerts.rs` hierher um.
- **C** `5085e66324` — Concerts-Plugin: City- und Radius-Zeile raus, dafür eine
  nicht editierbare Verweiszeile mit Deep-Link. Sie steht **außerhalb** von
  `module_rows`, bleibt also lesbar, wenn das Plugin aus ist.
- **D** `f1b4b1654a` — `present_location_settings()` öffnet die neue Seite statt
  der Plugins-Seite; `SettingsDeepLink::ConcertLocation` → `Location`.
- **E** `1718642661` — ehrliche Zustände ohne Standort. Wurzel war eine Zeile:
  `active_facets()` zählte den Radius als aktiven Filter, sobald ein Wert
  gespeichert war — und das war er immer, weil `persisted_filter()` auf den Default
  zurückfiel. Niemand fragte nach dem Standort. Daraus folgten gleichzeitig der
  wirkungslose Chip, die Zählung „415 of 415" und die Strich-Spalte.
- **F** `b8bec36db0` — UX-Regeln und Tests. **CONC-2** und **RAD-5** neu gefasst
  (RAD-5 schrieb die alte Zwangsnavigation samt Begründung fest), **SET-10** mit
  ausdrücklicher Ausnahme für die neue Seite, **SRC-19** nur um einen Verweis
  ergänzt, **SET-15** neu.
- `ca658b9b90` — die vier Review-Befunde (siehe unten).
- `9a5999e842` — Nachtrag **E2b** aus einer parallelen Grill-Sitzung: der aktive
  Chip nennt die Stadt (`{city} · {radius} km`). Geprüft und als legitime
  Übernahme einer Fremdentscheidung eingestuft, nicht als Umfangserweiterung.

## Review und Nachweis

Vier Prüfer (Rust, Security, Nachweis-Audit, Regeln/Texte). Vier Befunde, alle vom
Nutzer angenommen, alle behoben und **unabhängig nachgefahren**:

1. **HIGH** — Die Location-Seite umging den Online-Hauptschalter (SET-11: „no
   request of any kind"). Die fehlende `network_allowed`-Prüfung war alt; neu war
   die Reichweite, weil die Seite absichtlich nicht mehr am Modulschalter hängt.
   Behoben: `network_allowed` wurde additiv auf `NetworkScope::{AppWide, Module}`
   erweitert — bestehende Aufrufer kompilieren unverändert. `Clear location` bleibt
   ungated, Löschen ist keine Anfrage.
2. **HIGH** — Ein neuer Test verdrahtete `"Popular in US"` fest und fiel auf jeder
   Nicht-US-Maschine deterministisch um. Behoben, unter `en_GB` **und** `en_US`
   grün gefahren.
3. **MEDIUM** — Die Behauptung „per CUA verifiziert" für den Rückweg des
   Add-Station-Dialogs war unbelegt. Behoben durch einen echten Test über
   `surface::build(...)`, der den **echten** Preferences-Dialog öffnet, über
   `apply_location` schreibt und den weiterhin sichtbaren Dialog ohne zweiten Klick
   auf `Searching` gehen sieht. Die unbelegte Behauptung wurde aus dem Bericht
   entfernt.
4. **LOW** — Kontroll-Screenshot neu aufgenommen.

**Suiten (selbst nachgefahren, nicht aus dem Bericht übernommen):**
`reprise-core` 2439 grün / 0 rot · `reprise-gnome --bin reprise` 1830 grün / 0 rot ·
`fmt --check`, `clippy -D warnings`, Architektur-Lint, UX-Traceability (371 Regeln)
alle Exit 0.

**Sichtprüfung:** neun Screenshots unter
`artifacts/location-is-not-a-concerts-setting/` — sieben Feature-Zustände plus zwei
Kontrollen vom eingefrorenen Basis-Stand `5721ade95e`. Die Kontrolle ist echt: sie
zeigt den aktiven `500 km`-Chip, `415 of 415` und die Strich-Spalte, und es wurde
gegengeprüft, dass dort wirklich keine `PageId::Location` existiert.

## Offene Punkte

1. **Der Kill-Switch-Test fährt die Hilfsfunktion direkt an**, nicht die beiden
   GTK-Klick-Closures. Dass wirklich *beide* Einstiegspunkte durch die Sperre
   gehen, ist am Diff gelesen und nicht vom Test bewiesen. Wenn das zu dünn ist:
   ein zweiter `/refactor`-Punkt, kein Blocker.
2. **`origin/dev` ist weitergezogen** — 7 Commits seit dem Branch-Punkt, zuletzt
   `8b87ae8ada` (#482). Vor dem Landen rebasen. `git merge-tree` meldet **zwei**
   Konflikte, beide harmlos:
   - `crates/reprise-gnome/src/ui/mod.rs` — nebeneinanderliegende `mod`-Zeilen,
     beide Seiten behalten;
   - `.pipeline-codex.md` — Berichtsdatei einer fremden Pipeline, diese Seite
     nehmen.
   **`docs/ux-rules.md` merged sauber durch** — das war das eigentliche Risiko, weil
   dieser Branch vier Regeln umschreibt.
3. **Nicht gelandet.** `scripts/land.sh` findet den Plan über die `branch:`-Zeile
   und setzt `phase: shipped` selbst vor dem Merge. Nicht auf CI warten — rebasen,
   pushen, mergen, dann den dev-Lauf beobachten.

## Ausdrücklich nicht Teil dieser Arbeit

- Panel **2a** (Preferences-Suche verschiebt den Dialog, ESC-Stufen) und Panel
  **1a** (Plugins-Hauptschalter) aus derselben Entwurfsdatei.
- Ein echter Umkreisfilter für Podcasts.
- Reverse-Geocoding, um dem Portal-Standort ein Land zu verschaffen — `O-4` und
  RAD-5 verbieten den zusätzlichen Netzaufruf ausdrücklich.
