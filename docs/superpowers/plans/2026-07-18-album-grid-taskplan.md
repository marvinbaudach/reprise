# Album-Grid Playing, Tastatur und Reveal — Taskplan (2026-07-18)

Setzt `2026-07-18-album-grid-beschluesse.md` strikt in der Reihenfolge T1 bis
T7 um. Zielbranch ist `feat/album-view-improvements`, verifizierte Basis
`main@e0493d0` (zugleich `origin/main` am 2026-07-18). Die Stufe endet nach T7;
`ALB-2` wird nicht begonnen.

## Arbeitsprotokoll

1. Vor dem ersten Schreibzugriff `AGENTS.md`, `TESTING.md`, den Tail von
   `.superpowers/sdd/progress.md`, dieses Dokument, das Beschlussdokument,
   die betroffenen Regeln und `git log --oneline -20` lesen.
2. Arbeitsbaum auf unerwartete Aenderungen pruefen und erhalten. Nicht pushen.
3. Den Repository-Lock nach dokumentiertem Muster beanspruchen. Existiert
   `.superpowers/sdd/LOCK.md` und stammt er von einem aktiven/rezenten Agenten,
   ist das ein erlaubter Stop-Grund; andernfalls Branch, Agent, Basis und Zeit
   eintragen. Der Lock wird nach der Abschlusspruefung entfernt.
4. Pro Verhaltenstest echtes TDD: Test schreiben, den **erwarteten** Fehler
   beobachten, minimal implementieren, gezielten Test gruen sehen.
5. Vor jedem Task-Commit: komplette Gates, adversarial Diff-Review gegen
   Beschluesse/Regeltext, gefundene Fehler beheben, betroffene Gates erneut.
6. Genau die unten angegebene Commit-Message verwenden; keine Footer, kein
   Push. Der finale Fortschritts-Commit T7 ist die etablierte Ausnahme, die
   alle bereits feststehenden Task-Hashes atomar ins append-only Ledger
   schreibt (ein Hash kann nicht korrekt in denselben Commit zeigen).

## Verifizierter Ist-Zustand

- `AlbumView`: `ListStore → SortListModel → FilterListModel → NoSelection →
  GtkGridView`; native Pfeilnavigation und `activate` existieren bereits.
  `single_click_activate(true)` ist aktiv.
- `album_card.rs`: Hover-Layer enthaelt derzeit EQ und Button gemeinsam;
  Playing setzt `album-now-playing`, aber der Layer bleibt ohne Hover
  unsichtbar. Der Karten-Tooltip traegt Titel/Artist/Metadaten und wird von
  der Kontextmenue-Aufloesung missbraucht.
- `album_card_css.rs`: Fokusregel zielt auf `.album-card:focus-visible`, obwohl
  der `GtkGridView`-Child den Fokus traegt. Der Button nutzt `@accent_bg_color`;
  persistenter Playing-Ring und echter Bottom-Gradient fehlen.
- `album_context_menu.rs`: Play, Shuffle, Queue, Playlist-Untermenue, Edit
  Tags (nur Log) und Go to Folder. Keyboard Menu/`Shift+F10` existiert schon,
  loest die Kachel aber ueber deren Tooltip auf.
- Album-Play/Queue laufen ueber `album_card_actions::album_track_ids`, aktuell
  nur nach `track_no`. `PlayerController::play_next`,
  `ArtistView::select_artist_callback` und der Batch-Tag-Editor existieren.
  `tag_edit_flow::begin_for_ids` ist derzeit privat und nur Retry-intern.
- Core-Schema ist v11; `TrackMeta`, Scanner-Insert/Upsert und Move-Reconcile
  tragen `track_no`, aber kein `disc_no`. `scanner.rs` liegt bei 754 Zeilen.
- Playerleisten-Cover/Titel und `Ctrl+L` teilen derzeit denselben NAV-9-Track-
  Sprung in `window_runtime_wiring.rs`. Das NPP hat Cover/Titel, aber keine
  Reveal-Aktivierung. `NavHistory` kann Library-Tab-Orte deduplizieren.
- Das vorhandene `adw::NavigationView` kapselt nur die Library-Shell und wird
  nicht als Detailseiten-Stack gepusht. Enter nutzt deshalb den etablierten
  globalen `NavHistory`-Push auf `ViewSource::Album`; die Hero-Detailseite ist
  weiterhin der spaetere `ALB-2`-Task.
- gtk4 0.11 stellt `GtkGridView::scroll_to(position, ListScrollFlags,
  Option<ScrollInfo>)` bereit; dies ist der vorgeschriebene Reveal-Pfad.
- Shared EQ-CSS nutzt bereits `@reprise_player_accent`, friert unter
  `.playback-paused` ein und wird bei deaktivierten GTK-Animationen statisch.
- Beide alten Parallel-Lanes sind in dieser Basis gemergt:
  `c785a32` (Theme) und `eddd0f7` (NPP). Der entsprechende Block am Ende von
  `AGENTS.md` hat seine eigene Loeschbedingung erfuellt und ist veraltet.

## Datei- und Architekturgrenzen

- `reprise-core` bleibt frei von GTK/libadwaita/GStreamer/zbus.
- `disc_no` bleibt Core-/DB-/Scanner-Datum; kein GTK-Typ und kein neues
  Tag-Editor-Feld.
- Keine reale DB, kein reales Music-Verzeichnis, kein Live-Wayland und kein
  realer Session-Bus. Fixtures erzeugen Temp-DBs und synthetische Tags.
- Jede neu erstellte oder substanziell geaenderte Codedatei bleibt unter 800
  Zeilen. Insbesondere `scanner.rs` (754), `album_card.rs` (562),
  `now_playing/now_playing.rs` (580) und `window_runtime_wiring.rs` (646)
  frueh in kohaerente Geschwistermodule extrahieren; keine Kommentare kuerzen.
- `@reprise_player_accent` konsumieren, nie Theme-/Cover-Akzentlogik in der
  Albumansicht duplizieren.
- RefCell-Borrows vor Callback-/GTK-Aufrufen in einer eigenen Anweisung
  droppen. Recycelte Karten und Reveal-Timer brauchen Generationstoken.

## T1 · Regelvertrag und abgelaufene Lane-Koordination

Dies ist ein Dokumentations-/Koordinations-Task; kein kuenstlicher Rust-Test.

- Red-equivalent: `scripts/check-ux-traceability.sh` auf der unveraenderten
  Basis laufen lassen und den aktuellen gruenen Ausgangszustand notieren.
- Aus `AGENTS.md` ausschliesslich den Abschnitt „Aktive Parallel-Lanes ..."
  entfernen: beide dort genannten Mergebedingungen sind mit `c785a32` und
  `eddd0f7` erfuellt. Keine anderen Projektanweisungen aendern.
- In `docs/ux-rules.md`:
  - `ALB-1` → `[ersetzt durch GRID-2/GRID-4]`, historischen Text erhalten;
  - `NAV-9` → `[ersetzt durch NAV-9a/GRID-5]`, historischen Text erhalten;
  - `ALB-2` unangetastet lassen;
  - `NAV-9a` und `GRID-1..5` exakt gemaess Beschlussdokument als `[geplant]`
    mit Level `[gtk]` anlegen.
- Die NAV-3-Querverweisung `Cover/Titel gemaess NAV-9` auf `GRID-5`
  nachziehen. Historische Codekommentare duerfen bis T7 noch NAV-9 nennen;
  normative Regelverweise duerfen nicht auf einer ersetzten ID stehen.
- Gate erneut ausfuehren: geplante Regeln verlangen noch keine aktiven Tests,
  ersetzte IDs duerfen danach von keinem Test mehr referenziert werden. Auf
  der verifizierten Basis existiert kein regelbenannter NAV-9-Test; der echte
  `nav_9a_*`-Red-Test entsteht deshalb erst in T7, nicht als Scheintest in T1.
- Adversarial: IDs append-only, keine doppelte ID, kein aktiver Status, keine
  Referenz auf ersetztes `ALB-1`/`NAV-9` in Tests oder `RELEASING.md`.
- Commit: `docs(ux): define GRID rules and split NAV-9`

## T2 · Schema v12 und kanonische Albumreihenfolge

### Red

- Core-Migrationstest `v11_to_v12_adds_disc_number_without_losing_tracks`:
  v11-Fixture migriert auf v12, bestehende Rows bleiben, `disc_no` ist NULL.
- Scanner-Metadaten-Test: synthetische Tag-Datei mit Disc- und Tracknummer
  liefert beide Werte. Die exakte Lofty-Accessor-Methode gegen die lokal
  gelockte Version pruefen, nicht raten.
- Scanner-DB-/Move-Test: Import und Reconcile schreiben/bewahren `disc_no`.
- Query-Test fuer `query_album_track_ids`: absichtlich ungeordnete Rows mit
  Disc 2, Disc 1, NULL-Disc, NULL-Track und gleichen Nummern ergeben
  `COALESCE(disc_no, 1) ASC`, Tracknummer NULL zuletzt, dann Pfad (NOCASE) und
  ID stabil.
- Jeden Test einzeln laufen lassen und den erwarteten Fehler protokollieren.

### Green

- `SCHEMA_VERSION = 12`, nullable `tracks.disc_no INTEGER`; explizite
  v11→v12-Migration und aktuelle Schemaerzeugung anpassen.
- `TrackMeta.disc_no`, Scanner-SQL, Tagparameter und Move-Reconcile erweitern.
  Altbestand und Tag-Edits ohne Disc-Feld duerfen den Wert nicht loeschen.
- Falls `scanner.rs` durch die Aenderung 800 Zeilen erreicht, SQL-/Parameter-
  Binding als kohaerentes Geschwistermodul extrahieren.
- Eine kanonische Album-ID-Query als klar benannte Core-Funktion/Order-Clause
  anbieten. Alle Albumaktionen verwenden diese eine Funktion; generische
  benutzerwaehlbare Tracklisten-Sortierung bleibt unberuehrt.
- Keine reale Bibliothek scannen, keine sichtbare Tag-Editor-Zeile ergaenzen.

### Verifikation

- Gezielte Core-Tests gruen.
- Core-Purity: Ausgabe von
  `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'`
  muss leer sein.
- Dateigroessen pruefen.
- Commit: `feat(core): persist disc numbers for canonical album order`

## T3 · Persistenter Playing-Layer (GRID-1)

### Red

- Regeltest exakt `grid_1_playing_badge_persists_without_hover` `[gtk]`:
  gebundene, geladene Kachel ohne Hover/Fokus besitzt sichtbares EQ-Badge und
  Playing-Innenring; normale Kachel nicht.
- Zusaetzliche Assertions desselben Regeltests oder Hilfstests:
  - Paused behaelt Ring/Badge und setzt den vorhandenen Freeze-Ahnenzustand;
  - `gtk-enable-animations=false` nutzt statische EQ-Praesentation;
  - Recycling/unbind entfernt Playing- und Pulse-State vollstaendig.
- Displaytest rot beobachten; reine Presenter-/Zustandslogik separat ohne
  Display rot beobachten, damit die Zustandsmatrix deterministisch bleibt.

### Green

- Kartenaufbau in benannte Layer trennen: Cover-Bild, persistenter
  Playing-Layer (EQ oben links), Interaktions-Layer, Labelblock. Das EQ darf
  kein Kind des versteckten Hover-Layers mehr sein.
- Cover bekommt einen eigenen inneren Frame fuer den 1.5-px-Ring in
  `@reprise_player_accent`; Labels bleiben ausserhalb.
- Gemeinsame `EqBars`-Glyphe weiterverwenden. Bestehendes
  `.playback-paused`-/Animations-Setting weiterleiten, keine zweite
  Animation implementieren.
- Reine Funktion fuer `normal / playing / playing-paused` und eine kleine
  Presenter-Schicht extrahieren (`album_card_state.rs` oder gleichwertig),
  falls dies `album_card.rs` klein und testbar haelt.
- Tooltip darf nicht mehr Identitaetsspeicher sein. Eine explizite, beim
  bind/unbind generation-sichere Kartenidentitaet/Registry fuer Pointer- und
  Tastaturaktionen einfuehren; keine unsichtbare Accessible-Beschriftung als
  Datenbank missbrauchen.
- `GRID-1` in `docs/ux-rules.md` im selben Commit auf `[aktiv]` flippen.
- Commit: `feat(album-grid): persist playing badge and accent ring (GRID-1)`

## T4 · Tastatur- und Menueaktionen (GRID-2)

### Red

- Regeltest exakt `grid_2_enter_opens_detail_ctrl_enter_plays` `[gtk]`:
  - natives Grid-`activate`/Enter pusht die Albumquelle und fokussiert die
    Trackliste;
  - `Ctrl+Enter` baut kanonische Queue neu und startet Index 0, auch beim
    bereits geladenen Album;
  - plain Enter startet keine Wiedergabe.
- Regeltest exakt `grid_2_space_is_global_playpause_not_album` `[gtk]`:
  Album-Key-Controller gibt Space mit `Propagation::Proceed` weiter; die
  bestehende Fensteraktion toggelt genau einmal, Albumqueue bleibt gleich.
- Tests fuer Menu/`Shift+F10` und Rechtsklick: beide Modelle haben exakt und
  in derselben Reihenfolge `Play`, `Play next`, `Add to queue`,
  `Go to artist`, `Edit tags...`.
- Pure Action-Decision-Tests fuer den pointer-only Primaerbutton:
  other/stopped → rebuild; current+playing → pause; current+paused → resume.

### Green

- Einen Grid-Key-Controller nur fuer `Ctrl+Enter` ergaenzen. Pfeile und
  plain Enter dem `GtkGridView` ueberlassen; Space immer propagieren.
- Kontextmenue auf exakt fuenf Aktionen reduzieren. `AlbumMenuShared` auf
  echte Callbacks fuer Play, Play next, Queue, Artist und Tag-Edit umbauen;
  Shuffle-/Playlist-/Folder-Code und tote Imports entfernen.
- `AlbumView::set_on_play_next` mit `PlayerController::play_next` verdrahten.
  IDs stammen immer aus T2s kanonischer Query.
- Go-to-artist ueber die vorhandene Artist-Tab-Route plus
  `ArtistView::select_artist_callback` implementieren; History-Ausgangsort
  sichern. Album-Artist-Label und Menue nutzen dieselbe Route.
- Den vorhandenen Batch-Editor ueber einen schmalen ID-Einstieg oeffnen
  (`TrackList::edit_tags_for_ids`/`tag_edit_flow::begin_for_ids` oder
  gleichwertig). Nur vorhandene Albumtracks uebergeben; keine neue Editor-
  Implementierung.
- Primaerbutton nutzt die getestete Entscheidung: nur Pause/Resume des
  geladenen Albums ruft den globalen Transport-Toggle auf; Rebuild-Pfade
  nutzen Container-Play. `Ctrl+Enter` und Menue-Play umgehen diese
  Toggle-Entscheidung bewusst.
- Icon-only Tooltip/Accessible-Text nennt `Ctrl+Enter`; Button bleibt
  `focusable(false)`.
- `GRID-2` atomar auf `[aktiv]` flippen.
- Commit: `feat(album-grid): complete keyboard and context actions (GRID-2)`

## T5 · Fokuskomposition (GRID-3)

### Red

- Regeltest exakt `grid_3_focus_ring_and_overlay_on_focus` `[gtk]`:
  Tastaturfokus auf dem realen `GtkGridView`-Child zeigt einen 2-px-
  `@accent_color`-Aussenring am Cover und den Interaktions-Layer ohne Hover.
- Zustandsmatrix pruefen: normal, hover, focus, playing sowie
  playing+focus. Im letzten Fall sind innerer Playing- und aeusserer
  Fokusring gleichzeitig vorhanden; EQ bleibt sichtbar.
- Native 2D-Navigation mit mindestens zwei Spalten und zwei Reihen pruefen:
  Pfeile veraendern den fokussierten Grid-Index erwartungsgemaess, ohne einen
  eigenen manuellen Koordinatenalgorithmus.

### Green

- CSS auf den tatsaechlich fokussierten `GtkGridView`-Child ausrichten, nicht
  die nicht fokussierte `.album-card` kuenstlich zum Tab-Stopp machen.
- Cover mit getrenntem aeusseren Fokus- und inneren Playing-Frame aufbauen;
  Ringe nie auf den Labelblock ausdehnen.
- Hover und Fokus schalten ausschliesslich Sichtbarkeit des Interaktions-
  Layers; Playing schaltet ausschliesslich EQ/Innenring. Zustandsklassen
  nicht gegenseitig setzen.
- `GRID-3` atomar auf `[aktiv]` flippen.
- Commit: `feat(album-grid): compose focus hover and playing states (GRID-3)`

## T6 · Bottom-Gradient, Metazeile und Tooltip-Disziplin (GRID-4)

### Red

- Regeltest `grid_4_hover_uses_bottom_gradient_not_tooltip_box` `[gtk]`:
  Hover/Fokus-Layer ist unten verankert, besitzt Meta-Label und Button, aber
  keine mittige Box; ganzer Cover-/Kartencontainer hat keinen Tooltip.
- Metaformat pure testen: `13 tracks · 47 min`, sinnvolle Singular-/fehlende
  Dauer-Grenzen gemaess bestehenden String-Konventionen.
- TIP-1a-Test: nicht abgeschnittenes Titel-/Artist-Label liefert keinen
  Tooltip, ellipsiertes liefert exakt Volltext. Den bestehenden
  `query-tooltip`-Ansatz aus `compact_player_layouts.rs` in einen kleinen
  gemeinsamen Widget-Helper extrahieren statt kopieren.
- CSS-String/Widget-Test prueft `@reprise_player_accent`, Bottom-Gradient,
  Coverabdunklung und bestehendes 150-ms-Micro-Token.

### Green

- Schwebende/mittige Overlay-Box entfernen. Ein full-width, unten
  ausgerichteter linearer Gradient traegt Metazeile und rechts unten den
  runden Play/Pause-Button; Covermitte bleibt ohne Container.
- Album/Artist unter dem Cover unveraendert lassen. Volltext-Tooltips nur
  ueber den gemeinsamen Ellipsis-Helper.
- Buttonfarben und Hover-Glow ausschliesslich
  `@reprise_player_accent`; kein Creme-/`@accent_bg_color`-Fallback.
- Persistent EQ bleibt als separater Layer ueber dem Cover und wird vom
  Gradient nicht ein-/ausgeblendet.
- `GRID-4` atomar auf `[aktiv]` flippen.
- Commit: `feat(album-grid): replace hover box with bottom gradient (GRID-4)`

## T7 · Player-/NPP-Reveal und NAV-9-Aufteilung (GRID-5, NAV-9a)

### Red

- Regeltest exakt `grid_5_reveal_scrolls_to_playing_album` `[gtk]`:
  - von anderem Ort: History-Push → Library/Albums, Suchfeld sichtbar leer,
    Filter leer, kanonischer Filtermodell-Index wird per Grid-Scroll
    fokussiert;
  - bereits im Album-Grid: kein History-Duplikat;
  - Zielkarte erhaelt Pulse-State, verliert ihn generation-sicher nach rund
    1 s, behaelt Fokus-/Playing-State;
  - Animationen aus: statische Klasse gleiche Dauer, kein Motion-Loop;
  - Album nicht auffindbar: exakt einmal NAV-9a-Fallback.
- Regeltest `nav_9a_ctrl_l_reveals_current_track_origin` `[gtk]`: bestehender
  Ctrl+L-Pfad behaelt Herkunft, Select+Center und Back, ohne Album-Grid-Reveal.
- Aktivierungs-Tests fuer Playerleisten- und NPP-Cover/Titel: focusable,
  AccessibleRole Link (oder aequivalente link-artige GTK-Semantik), Name
  `Reveal playing album`; Klick/Enter aktiviert, Space propagiert.
- Pure Tests fuer Zielauflosung in `FilterListModel`, Suchleerung,
  History-Deduplizierung und Missing-Fallback vor dem Displaytest anlegen.

### Green

- Den bisherigen NAV-9-Closure in `window_runtime_wiring.rs` in zwei
  benannte Koordinatoren extrahieren, vorzugsweise
  `window/album_grid_reveal.rs` plus bestehender Track-Jump-Helfer:
  - `NAV-9a`: nur `Ctrl+L` → Play-Origin/Trackzeile;
  - `GRID-5`: Playerbar/NPP Cover+Titel → Album-Grid.
- `PlayerController` bietet eine kleine, borrow-sichere Abfrage der geladenen
  Albumidentitaet (Album + effektiver Album-Artist). Keine UI liest private
  `RefCell`-Felder direkt.
- PlayerBar und `NowPlayingPanel` erhalten dieselbe wiederverwendete
  Link-Aktivierungsverdrahtung fuer Klick/Enter/Focus-visible; Space wird
  nicht geclaimt. Idle ohne aufloesbares Album ist ein stiller No-op.
- Bei GRID-5 zuerst den Zielort in `NavHistory` recorden (Deduplizierung
  verwenden), Library/Albums anzeigen und `SearchEntry::set_text("")` plus
  Albumfilter-Leerung ausfuehren. Back restauriert den Ort, nicht Suchtext.
- `AlbumView::reveal_album` (oder gleichwertig) sucht nach Album + effektivem
  Artist im **aktuellen, entfilterten** Modell und nutzt ausschliesslich
  `GtkGridView::scroll_to` mit `ListScrollFlags::FOCUS` und vertikalem
  `ScrollInfo`; kein Widget-`scroll_into_view`, keine Pixel-Raterei.
- Virtualisierung robust behandeln: pending Reveal-Identitaet + Generation
  liegt im View-State; der Factory-Bind setzt Pulse auf die materialisierte
  Zielkarte. Timer entfernt nur seine eigene Generation.
- Pulse-CSS: zwei sanfte Akzent-Helligkeitszyklen ~1 s; bei deaktivierten
  Animationen statische Highlightklasse ~1 s. Focus-/Playing-Ringe bleiben
  eigene Layer.
- Kann `reveal_album` kein Ziel finden, den bereits getesteten NAV-9a-Closure
  aufrufen; kein Toast/Dialog.
- `RELEASING.md` um die visuellen/manuellen Punkte aus dem Beschlussdokument
  mit IDs GRID-1/3/4/5 ergaenzen.
- `GRID-5` und `NAV-9a` atomar auf `[aktiv]` flippen.
- Commit: `feat(album-grid): reveal the playing album from player surfaces (GRID-5/NAV-9a)`

## Gates vor jedem Task-Commit

Vom Repo-Root, immer auf dem aktuellen Task-Diff:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --workspace
cargo audit
scripts/check-ux-traceability.sh
scripts/check-architecture.sh
```

Akzeptiert bei `cargo audit` ist ausschliesslich `RUSTSEC-2024-0436` (`paste`
via `lofty`). Jede neue Advisory ist ein ausdruecklicher Stop-Grund.

Nach T2 zusaetzlich der Core-Purity-Befehl. Nach jedem Task alle substanziell
geaenderten Codedateien mit `wc -l` pruefen. Falls das Repository weitere
Gates in `AGENTS.md`/`TESTING.md` nennt, gelten sie zusaetzlich.

### Isolierte Displaytests

Jeder beruehrte `#[ignore = "requires a display; run via xvfb-run"]`-Test wird
mit **allen** Sicherheitsgrenzen gestartet. Beispiel (Testfilter ersetzen):

```bash
REPRISE_TEST_DATA=$(mktemp -d)
REPRISE_TEST_CACHE=$(mktemp -d)
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$REPRISE_TEST_DATA" XDG_CACHE_HOME="$REPRISE_TEST_CACHE" \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  cargo test -p reprise-gnome grid_1_playing_badge_persists_without_hover -- --ignored
```

Wenn Host/Sandbox `dbus-daemon` oder Xvfb mit `Operation not permitted`
blockiert, den privaten AT-SPI-/Display-Harness aus der aktuellen
Repository-Dokumentation verwenden. Nie auf die Live-Session ausweichen;
exakt als `deferred host check` dokumentieren, falls auch der sichere
Fallback nicht verfuegbar ist.

## Adversarial Review pro Task

Vor dem Commit mindestens diese Fragen mit dem realen Diff beantworten:

- Deckt der regelbenannte Test das sichtbare Verhalten oder nur eine Klasse/
  einen Toggle ab?
- Koennen Recycler, Timer oder alte Callbacks State auf eine andere Kachel
  leaken?
- Bleibt `Space` global und plain Enter navigierend?
- Bauen nur die expliziten Container-Play-Pfade die Queue neu?
- Nutzen alle Album-Queueaktionen dieselbe Disc-/Track-Reihenfolge?
- Sind Playing, Fokus, Hover und Reveal-Puls getrennte Layer?
- Ist Suche wirklich sichtbar geleert und History frei von Duplikaten?
- Sind RefCell-Borrows vor potenziell re-entranten Calls geloest?
- Sind alle neuen Texte im String-Katalog und englisch?
- Bleiben Codefiles <800 und Core-Abhaengigkeiten rein?

## Abschluss nach T7

1. Alle Gates ein letztes Mal auf dem gesamten Branch ausfuehren.
2. Gezielte sichere GTK-Displaytests gemeinsam ausfuehren; Ergebnisse und
   echte `deferred host check`-Punkte notieren.
3. `git diff main...HEAD` adversarial gegen alle 23 Beschluesse und die sechs
   aktiven Regeln pruefen. Findings beheben und als eng begrenzte Fix-Commits
   mit Message `fix(album-grid): <konkreter Befund>` committen; danach Gates
   erneut. Keine leeren Review-Commits.
4. `.superpowers/sdd/progress.md` um eine kompakte Stage-Zusammenfassung mit
   T1–T7- und eventuellen Fix-Hashes, Testzahlen, Audit-Ergebnis und
   verbleibenden manuellen Checks ergaenzen.
5. Commit: `docs(progress): album grid improvement stage`
6. Repository-Lock entfernen und sauberen Arbeitsbaum bestaetigen.

## Erwartete Abschlussabnahme

- EQ + Playing-Ring ohne Hover persistent, Pause/reduced motion korrekt.
- Native Pfeile, Fokus-Ring + Overlay, Enter/`Ctrl+Enter`/Space korrekt.
- Kontextmenue per Pointer und Tastatur identisch, alle fuenf Aktionen real.
- Bottom-Gradient statt Tooltip-Box, Meta und petrolfarbener Button unten.
- Playerbar/NPP Reveal leert Suche sichtbar, fokussiert, scrollt und pulst;
  Back und Missing-Fallback funktionieren.
- Ausschliesslich dokumentierte visuelle Host-Checks duerfen offen bleiben.
