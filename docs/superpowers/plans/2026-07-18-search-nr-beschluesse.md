# Suchleiste + New Releases — Beschlussdokument (Grilling 2026-07-18)

Normativer Kontext für zwei Features nach Design **23c** (Popover + Headerbar)
und **23a** (Digest-View). Design-Quelle ist das claude.ai/design-Projekt
„Audio-Player für große Bibliotheken" (Share-Link kanonisch).

> **Regelwerk:** SEARCH-1–5 und NR-1–7 gehen als **Sektion Q** und **Sektion R**
> nach `docs/ux-rules.md` — Status `[geplant]` beim Anlegen, Flip auf `[aktiv]`
> im jeweiligen Implementierungs-Commit, jede aktive Regel mit regelbenanntem
> Test (`scripts/check-ux-traceability.sh`). Dieses Dokument ist das
> Beschluss-Ledger: es hält das *Warum* und die Detailentscheidungen unterhalb
> der Regel-Ebene.

## Audit-Befunde, die den Zuschnitt bestimmt haben

Beide Features wurden gegen den Code auditiert, bevor gegrillt wurde. Vier
Befunde haben die Vorgabe verändert:

1. **Es gibt bereits eine vollständige MusicBrainz-Schicht.**
   `reprise-core/src/musicbrainz.rs` erzwingt prozessweit 1 req/s über einen
   `static Mutex<Option<Instant>>`, setzt den User-Agent und hat einen
   Fixture-Seam für Tests. `artist_news.rs` bringt Release-Group-Parsing,
   Datumsfenster, Sekundärtyp-Ausschluss, Sortierung und Cache mit. Der Worker
   läuft, ist verdrahtet — und hat **null Konsumenten** (bewusste Entkopplung
   im NPP-Umbau, gedacht für Frame 22a). NR-1 hätte das dupliziert.
2. **Die Datenbank speichert keine Artist-MBID.** NR-1 setzt sie voraus.
   `artist_news` löst pro Artist per Namenssuche auf (Score ≥ 95) — ein
   zusätzlicher Request am 1-req/s-Limit. Stichprobe: nur **20 %** der Dateien
   der Referenz-Library tragen überhaupt MusicBrainz-Tags. 160 Artists × 2
   Requests ≈ 5 Minuten Dauerfetch, bei jedem Lauf erneut.
3. **Es existiert kein einziger `GtkSearchBar` im Code** — kein Vorbild. Ctrl+F
   ist belegt (`shortcuts.rs:186`), Esc ist zweistufig, hängt aber an GTKs
   `stop-search`-Signal, das nur bei **fokussiertem** Entry feuert. Drei
   Testverträge brechen mit: cua-e2e tippt in das AT-SPI-Element
   „Search all fields", `ptr-e2e/geometry.sh` rechnet Header-Pixel vom rechten
   Rand, `help.rs` prüft die exakte Shortcut-Liste.
4. **Cover-Download und Interpreten-Portraits sind ungated.** Kein
   `is_enabled`-Check: die App kontaktiert coverartarchive.org, MusicBrainz und
   Deezer bedingungslos, während Last.fm und ListenBrainz opt-in sind.

## Gegrillte Beschlüsse (2026-07-18, alle bestätigt)

1. **NR schluckt Artist News.** New Releases wird die **eine**
   Release-Pipeline: bibliotheksweiter Fetch in eine DB-Tabelle (mit `seen_at`
   und `hidden`), gespeist aus dem vorhandenen `musicbrainz.rs`. Die geplante
   22a-Artist-Sektion liest später nur noch pro Artist aus derselben Tabelle —
   kein zweiter Fetch, kein zweiter Cache, kein zweites Rate-Limit-Budget.
   `artist_news.rs` wird zur Query-Schicht umgebaut; der JSON-Datei-Cache und
   das eigene Modul-Toggle entfallen. Ein Feature, eine Wahrheit.
2. **Artist-MBIDs: Tags zuerst, Rest per Suche — beides persistiert.** Neue
   Spalte `artist_mbid`: beim Scan aus `ItemKey::MusicBrainzArtistId` lesen
   (kostenlos, deckt ~20 % sofort). Der Rest wird lazy per Namenssuche
   aufgelöst, Top-Artists nach Play-Count zuerst, und **dauerhaft gespeichert**
   — einmalig ~130 Requests über Tage verteilt statt bei jedem Fetch erneut.
   **Auch negative Ergebnisse** (nicht gefunden / mehrdeutig) werden gemerkt,
   sonst sucht die App ewig neu.
3. **Digest-View ist ein vollwertiger Ort ohne Sidebar-Eintrag.** Echter
   `NavPlace` mit Back/Forward-Historie (NAV-2/9), erreichbar **nur** über
   „See all" im Popover — genau wie die Artist-Detail-View. NR-5 bleibt
   unangetastet: das *Popover* berührt NAV nicht, der Sprung in die View ist
   normale Navigation. Kein Dauer-Eintrag in der nach NPP-1 knappen 240-px-
   Sidebar.
   **Konsequenz:** „See all" ist der einzige Einstieg, also muss es auch dann
   erscheinen, wenn die Liste ins Popover passt, sobald **Hidden-Einträge
   existieren** — sonst gibt es keinen Weg zurück zu Weggeblendetem. Regel:
   „See all" sichtbar, sobald `total > im Popover sichtbar` **oder** es
   Hidden-Einträge gibt; die Digest-View trägt dann die Fußzeile
   „N hidden · Show" (analog zur Dismissed-Fußzeile der Import-Errors).
4. **Filtersatz: Alben/EPs voll, Singles nur zukünftig.** Der Kern ist die
   Trennung von Signal und Rauschen: eine **kommende** Single kündigt ein Album
   an (Vorfreude-Info), eine sechs Wochen alte Single ist Rauschen.
   - `type=album|ep` → `first-release-date ≥ heute − 90 Tage`
   - `type=single` → **nur** `first-release-date > heute`
   - „zukünftig" heißt: das MB-Datum liegt nach heute. Releases mit
     **unvollständigem Datum** (nur Jahr oder Jahr-Monat) werden **konservativ
     als nicht zukünftig** behandelt, damit keine Alt-Single durchrutscht.
   - Sekundärtypen (Live, Remix, Compilation, Soundtrack, Mixtape, DJ-Mix)
     bleiben draußen wie bisher; Kappung 5 pro Artist bleibt.
5. **Die Lupe trägt einen Toggle-Zustand, keinen Punkt.** Die Lupe ist ohnehin
   ein `ToggleButton` und bekommt bei aktiver Query den vorhandenen
   `:checked`-Akzentstil (`.reprise-panel-toggle` existiert). Zustand statt
   Bitte — dieselbe visuelle Sprache wie Sidebar- und Info-Panel-Toggle.
   **Der Badge-Punkt bleibt exklusiv der Bitte-Rolle** (NR, ISSUES), wo er
   tatsächlich etwas erbittet; FB-4 und P-1 bleiben ohne Ausnahme. SEARCH-3
   wird entsprechend formuliert.
6. **Modul auf der Plugins-Seite, default AUS.** `artist_news` wird zu
   `new_releases` umgewidmet: Plugins-Seite, Privacy-Untertitel („contacts
   MusicBrainz"), **default aus** — konsistent mit Last.fm/ListenBrainz und mit
   der Haltung, dass Netzabfragen eine bewusste Entscheidung sind. ✦ erscheint
   erst nach dem Einschalten. Darunter eine ComboRow „nur Top-Artists / alle".
7. **View-Pill wandert nach links** (`pack_start`, neben den Sidebar-Toggle),
   der mittige Headerbar-Titel entfällt. Damit fällt die Mindestbreiten-Fessel
   ganz weg (kein zentriertes Element, das auf beiden Seiten Platz reserviert)
   und rechts bleibt sauber Raum für Lupe + ✦ + Menü. Deckungsgleich mit 23c —
   das war keine Zeichenfreiheit im Mock, sondern die konsequente Folge aus
   Lupe-statt-Feld. `CenteringPolicy::Loose` und seine Notlösungs-Begründung
   entfallen mit.
8. **Cover-Download und Portraits werden ebenfalls Plugins, default AUS —
   aber mit Bestandsschutz.** Beide kontaktieren externe Dienste und waren
   bisher ungated; das war eine Inkonsistenz. Neue Module `cover_download`
   (coverartarchive.org + MusicBrainz) und `artist_portraits` (Deezer).
   **Die Migration schreibt für bestehende Datenbanken explizit
   `enabled = true`** — wer die Funktion heute nutzt, verliert sie nicht
   stillschweigend. Frische Installationen starten aus und holen nichts ohne
   Zustimmung.
9. **Entdeckbarkeit wandert in den Folge-Branch.** Beschluss 6 macht das
   Modul opt-in — damit erscheint ✦ nie und das Feature wäre unsichtbar.
   Die Gegenmaßnahme (kontextueller Einmal-Hinweis) wird **nicht hier**
   gebaut: Sie gehört zu einem querschnittlichen Entdeckungssystem über alle
   vier Netz-Features, samt Evidenz-Triggern und der Regel, dass nie zwei
   „aktivieren"-Zeilen gleichzeitig erscheinen. Gebaut in
   `feat/network-opt-in`; hier in einfacher Form gebaut, müsste sie dort
   sofort umgeschrieben werden.
10. **Ein Branch, sequenziell.** `feat/search-and-new-releases`: erst Teil A
   komplett (Headerbar auf Lupe + Pill-links umgebaut, Tests und e2e
   nachgezogen), dann Teil B in die dann stabile Headerbar. Beide Features
   fassen `library_chrome.rs`, `window_runtime_wiring.rs` und `shortcuts.rs`
   an — parallele Lanes wären Handarbeit an genau den fehleranfälligsten
   Stellen.

## Selbstentscheidungen (Implementierungsebene)

- **FIL-4 wird `[ersetzt durch SEARCH-3]`.** Regel-IDs sind append-only
  (`AGENTS.md`), also wird FIL-4 nicht umgeschrieben, sondern als ersetzt
  markiert. Seine beiden Tests (`fil_4_search_accent_tracks_trimmed_text`,
  `fil_4_css_defines_the_active_search_class`) müssen weichen — die
  Traceability verbietet Tests, die auf eine ersetzte ID zeigen. Die
  CSS-Klasse `.reprise-search-active` entfällt mit dem Dauerfeld.
- **NAV-6 flippt von `[geplant]` auf `[aktiv]`.** Sein Wortlaut („Suche
  (Ctrl+F) filtert die aktuelle Ansicht live; Esc leert und schließt")
  beschreibt exakt, was SEARCH-2 und SEARCH-4 bauen — er setzte schon immer
  etwas Schließbares voraus, das es bisher nicht gab. Braucht einen
  `nav_6_`-benannten Test.
- **Zweiter Suchkonsument bleibt.** Die Albums-Grid-Filterung hängt
  undebounced am selben `search-changed`
  (`window_runtime_wiring.rs:515`); sie zieht mit in die SearchBar um, das
  Verhalten bleibt.
- **CAA braucht einen neuen URL-Builder.** Vorhanden ist
  `/release/{mbid}/front`; NR-2 braucht `/release-group/{mbid}/front-250`.
- **Fallback-Kachel-Akzent wird beim Anlegen des Eintrags berechnet** (Cover
  des meistgespielten Albums des Artists → `accent_from_cover_file`) und als
  Hex-Wert in der NR-Tabelle gespeichert. Kein Live-Extrahieren beim Rendern,
  kein Netz-Spinner in der Row (NR-2).
- **Neue Migration (nächste freie Version).** Tabelle für Releases inkl.
  `seen_at`, `hidden`, `fetched_at`, Fallback-Akzent; Spalte `artist_mbid`
  samt Negativ-Markierung; Bestandsschutz-Writes für die drei neuen Module.
  Eine ausgelieferte Migration wird nie nachträglich editiert (`db.rs:289`).
- **e2e-Verträge ziehen mit:** cua-e2e muss die Lupe aktivieren, bevor es in
  das Suchfeld tippt; `ptr-e2e/geometry.sh` bekommt neue Offsets (das
  300-px-Feld verschwindet aus der Headerbar); `help.rs`' Shortcut-Vertrag
  bleibt in der Liste unverändert, aber Escapes Semantik wird präzisiert.

## Offene Punkte für später (nicht dieser Branch)

| Thema | Referenz | Warum vertagt |
|-------|----------|---------------|
| „Remind me" — Toast am Release-Tag | NR-4 | Braucht einen Scheduler/Weckmechanismus, den es noch nicht gibt; Regel bleibt `[geplant]` |
| Artist News als Sektion in der Artist-Detail-View | Frame 22a | Liest dann aus der NR-Tabelle (Beschluss 1) — reine Leseansicht, eigener Task |
| Audio-Visualizer (Visual-Tab) | Frames 21b/10c | Unberührt von diesen Features |
