# Reprise — UX-Regelwerk (verbindlich)

Dieses Dokument ist die einzige UX-Wahrheitsquelle des Projekts. Bei Konflikt
schlägt es den Bestandscode — für `[aktiv]`-Regeln sofort (Abweichung = Bug),
für `[geplant]`-Regeln bindet es jede künftige Arbeit an diesem Bereich.
Akzeptanztests referenzieren Regeln über ihre IDs.

## Prozessregeln

**Status.** Jede Regel trägt genau einen Status:
`[aktiv]` = einklagbar — der Code ist konform, ein regelbenannter Test ist
grün und Merge-Blocker. `[geplant]` = beschlossenes Zielbild, noch nicht
einklagbar. Eine Regel wechselt auf `[aktiv]` **im selben Commit**, der das
Verhalten implementiert bzw. den Test nachweist — nie nachträglich. Halb
umgesetzte Regeln werden sofort in Teilregeln gesplittet (a/b), damit kein
Test je gegen eine halbe Regel geschrieben wird.

**IDs.** IDs sind append-only: nie umnummerieren, nie wiederverwenden. Ändert
sich die Bedeutung, ersetzt eine neue (Teil-)Regel die alte; die alte bleibt
als `[ersetzt durch <ID>]` stehen — als Wegweiser, nie als löschbarer
Ballast. Tests gegen ersetzte Regeln werden im selben Commit umgehängt.

**Test-Ebenen.** Jede Regel trägt ein Ebenen-Tag: `[core]` (reprise-core,
Workspace-Suite), `[gtk]` (Widget-/Logik-Tests in reprise-gnome), `[e2e]`
(cua-e2e-Harness gegen die echte App), `[manuell]` (RELEASING.md-Checkliste,
die dieselben Regel-IDs referenziert). Getestet wird auf der **niedrigsten
Ebene, die die Regel widerlegen kann**. Timing-Zahlen (100 ms, 150 ms, …)
sind Design-Intent, keine Assertions: das *Was* (Feedback existiert) wird
automatisiert, das *Wie-schnell* manuell geprüft. Wird eine `[manuell]`-Regel
später automatisierbar, wechselt nur ihr Tag, nie ihre ID.

**Traceability.** Ein Test trägt **genau eine primäre Regel-ID im Namen**
(Rust: `fn play_1a_…`, cua-e2e-Szenario: `play-1a-…`). Deckt ein Szenario
nebenbei weitere Regeln ab, zählt das nicht — die Zweitregel braucht ihren
eigenen Test. `#[ignore = "UX <ID> [geplant] — …"]` ist nur auf
`[geplant]`-Regeln erlaubt. `scripts/check-ux-traceability.sh` (Teil des
Merge-Gates) erzwingt: jede `[aktiv]`-Regel hat ≥ 1 Test · kein Test
referenziert eine unbekannte oder ersetzte ID · kein Ignore auf `[aktiv]` ·
jedes Ignore auf einem regelbenannten Test hält das Format oben ein. Als
Abdeckung zählen nur echte `#[test]`-Funktionen bzw. ausgeführte
cua-e2e-Zeilen — eine gleichnamige Helper-fn oder ein Kommentar greent das
Gate nicht.

**Sprache.** Dieses Dokument und die Design-Docs sind Deutsch — die
Arbeitssprache des Projekts. Tests und Skripte sind Code und damit Englisch
(AGENTS.md); Regel-IDs und Status-Token werden dort wörtlich zitiert.

**Änderungen.** Begegnet dir beim Implementieren oder Testen ein Fall, den
keine Regel deckt: **Regel ergänzen, nicht lokal entscheiden.** Agenten legen
dazu einen `[geplant]`-Entwurf mit der nächsten freien ID im betroffenen
Abschnitt an, markiert mit `<!-- REVIEW: Regelvorschlag -->` — der Beschluss
fällt beim Menschen. Begründungen für Änderungen leben in der Git-Historie.

## A. Grundprinzipien

- **P-1** [geplant] [manuell] — Jede Feedback-Rolle hat genau einen
  Mechanismus: Ankündigung eines Ereignisses = Toast · Zustand einer View =
  StatusPage/Inline · laufender Prozess = Fortschrittskarte · offene Bitte =
  Badge. Ein Ereignis darf mehrere Rollen gleichzeitig bedienen (disconnect →
  Toast + StatusPage), aber nie zwei Mechanismen in derselben Rolle (nie zwei
  Toasts, nie Toast + Dialog als doppelte Ankündigung).
- **P-2** [geplant] [gtk] — Klick reagiert sofort: Jeder Klick erzeugt
  sichtbares Feedback (Zustandswechsel, Spinner im Button, Selektion), Ziel
  < 100 ms. Nie ein Klick ins Leere. Automatisiert wird das *Was* (Zustand
  nach Klick ≠ Ausgangszustand); die 100 ms sind manuelle Checkliste.
- **P-3** [geplant] [gtk] — Hover navigiert nie: Hover zeigt (Tooltip, Pills,
  +3 % Fläche), Klick handelt. Kein Hover-to-open.
- **P-4** [geplant] [manuell] — Nichts verschiebt sich ungefragt:
  Layout-Shifts nur als direkte Folge einer Nutzeraktion oder eines von ihm
  gestarteten Prozesses (Sync-Removals kollabieren, Force-show der
  Filter-Zeile bei eigener Sucheingabe, FIL-2). Einblendungen (Gerätekarte,
  ISSUES, Chip-Inhalte der Filter-Zeile) faden ohne Reflow benachbarter
  Inhalte; für dynamisch erscheinende Elemente (Altwert-Zeile im Tag-Editor)
  ist Platz reserviert. Hintergrundereignisse (Scan, Watcher, Mount)
  verschieben nie sichtbare Inhalte unter dem Cursor.
- **P-5** [geplant] [core] — Die App löscht nie Dateien. „Remove" heißt
  immer: Library-Eintrag. Dialoge benennen Kaskaden (Ratings, Hörhistorie)
  beim Namen.
- **P-6** [geplant] [core] — Evidenz-Regel: Was beweisbar da ist, wird
  angezeigt/geheilt (Mount-Event, Resurrect); was beweisbar weg ist, wird
  sofort ehrlich markiert (Eject). Vermutungen (unmounted) sind nie
  Lösch-Grundlage.

## B. Navigationsmodell

- **NAV-1** [geplant] [gtk] — Sidebar = Orte, Content = Modus. Sidebar wählt
  den Ort (Music, Queue, Playlists, My Stats, Devices, Issues). Innerhalb von
  „Music" schaltet der Switcher den Modus: Tracks | Albums | Artists.
- **NAV-2** [geplant] [core] — Ein globaler History-Stack über den gesamten
  Content-Bereich, auch über Ortsgrenzen (Queue → Artist-Detail → Back →
  Queue). Content-Klicks (NAV-3) pushen immer; Alt+← / Maus-Back / Header-‹
  poppen. Sidebar-Klick ersetzt den Stack (Orte sind Neustarts, keine
  Stack-Einträge). Die Sidebar-Markierung folgt dem obersten Stack-Eintrag —
  zeigt der Stack Artist-Detail nach Queue-Klick, ist „Music" markiert.
- **NAV-2a** [geplant] [core] — Der Stack überlebt die Session nicht
  (Session-Restore stellt nur die oberste Ansicht wieder her, START-1
  unverändert); Back ohne Stack-Einträge ist disabled, nie ein No-op.
- **NAV-3** [geplant] [e2e] — Klickbare Metadaten überall gleich: In jeder
  Trackliste (Library, Playlist, Queue, Album-Detail, Top Tracks) gilt: Klick
  auf Artist-Namen → Artist-Detail; Klick auf Album-Namen/Cover →
  Album-Detail; beide pushen den globalen Stack (NAV-2). Hover zeigt
  Unterstreichung als Affordance. Gilt auch in der Player-Leiste (dort:
  Artist-/Album-Klick gemäß dieser Regel, Cover/Titel gemäß NAV-9).
- **NAV-4** [geplant] [gtk] — Doppelklick auf Row = abspielen im Kontext der
  sichtbaren Liste (siehe PLAY-2). Einfachklick = selektieren. Enter = wie
  Doppelklick. Ausnahme Queue-View: Doppelklick springt gemäß QUE-3 zum Track
  (Playhead), statt die Queue neu zu bauen.
- **NAV-5** [geplant] [gtk] — Modus-Gedächtnis (Scroll + Selektion je
  Tracks/Albums/Artists) gilt nur innerhalb der Session; auch Sidebar-/
  Ortswechsel erhalten Scroll + Selektion des verlassenen Modus. START-1
  restauriert über Neustarts ausschließlich die zuletzt aktive Ansicht samt
  Scroll-Position; alle anderen Modi starten oben, unselektiert.
- **NAV-6** [geplant] [e2e] — Suche (Ctrl+F) filtert die aktuelle Ansicht
  live; Esc leert und schließt. Suche navigiert nie selbst.
- **NAV-7** [geplant] [e2e] — Hamburger-Menü: „Scan Library" → startet Scan,
  bleibt in der Ansicht (Karte erscheint). „Preferences" →
  Preferences-Fenster. „Keyboard Shortcuts" → Shortcuts-Overlay. „About
  Reprise" → About-Dialog. Kein Menüpunkt wechselt kommentarlos die
  Content-Ansicht.
- **NAV-8** [geplant] [gtk] — My Stats ist ein Sidebar-Ort wie jeder andere:
  volle Content-Fläche, Headerbar mit Suche bleibt stehen (Suche dort
  disabled/ausgeblendet ist erlaubt, aber die Leiste bleibt).
- **NAV-9** [geplant] [gtk] — „Jump to Now Playing": Klick auf Cover oder
  Titel in der Player-Leiste navigiert zur Heimat des spielenden Tracks
  (Library-Modus Tracks bzw. Playlist, aus der er spielt), selektiert die Row
  und zentriert sie (Scroll so, dass die Row im mittleren Drittel liegt —
  kein scrollIntoView-Kantenkleben). Zusätzlich Shortcut Ctrl+L. Das ist die
  explizite „wo bin ich gerade"-Geste; sie pusht auf den History-Stack
  (NAV-2 global, Back kehrt zurück). Artist-/Album-Klick in der Leiste
  behalten ihre NAV-3-Ziele — nur Cover/Titel springen zum Track.

## C. Abspielen, Queue, Shuffle, Filter

- **PLAY-1** [geplant] [gtk] — Queue-Quelle = sichtbare Trackliste. „Was du
  siehst, ist was spielt": Doppelklick/Play all/Shuffle in einer Trackliste
  bauen die Queue aus der aktuell sichtbaren (gefilterten, sortierten)
  Liste. Für Container-Buttons gilt PLAY-1a.
- **PLAY-1a** [geplant] [core] — Container-Play (Play-Button auf Cover, Play
  all/Shuffle in Hero-Bereichen) baut die Queue ausschließlich aus dem
  Container in seiner kanonischen Reihenfolge (Album: Disc/Tracknummer;
  Playlist: Positionsreihenfolge; Artist „Play all": Alben nach Jahr, darin
  Tracknummer). Der sichtbare Grid-Filter bestimmt nur, welche Container
  erreichbar sind, nie den Queue-Inhalt.
- **PLAY-2** [aktiv] [core] — Doppelklick spielt die Row und hängt den Rest
  der sichtbaren Liste ab dieser Position in die Queue.
- **PLAY-3** [ersetzt durch PLAY-3a/PLAY-3b] — Ursprüngliche Sammelregel
  „Filter schränkt Shuffle ein"; nach der Prozessregel für halb getestete
  Regeln in Treffer-Shuffle (3a) und Filter-Nachträglichkeit (3b) gesplittet.
- **PLAY-3a** [aktiv] [core] — Filter schränkt Shuffle ein — absichtlich.
  Gefilterte Playlist + Shuffle = Shuffle über die Treffer („shuffle my 90s
  tracks"); die Queue ist genau die Treffermenge, kein Track von außerhalb.
- **PLAY-3b** [geplant] [gtk] — Filter nachträglich ändern fasst eine bereits
  gebaute Queue nicht an (Queue ist ein Snapshot; sichtbar in „Queue").
- **PLAY-4a** [geplant] [core] — Missing in Listen: Listen-Playback und
  Queue-Advance überspringen Missing still.
- **PLAY-4b** [geplant] [gtk] — Doppelklick auf konkrete Missing-Row: Toast
  „File missing since …" + Button „Show in Missing files". Einreihen (Play
  next/Add to queue) ist für Missing disabled.
- **PLAY-5** [ersetzt durch PLAY-5a/PLAY-5b] — Ursprüngliche
  Queue-Hygiene-Sammelregel; beim Härten in die Teilregeln deleted (5a) und
  unmounted (5b) gesplittet.
- **PLAY-5a** [aktiv] [core] — Deleted-Hygiene: Extern gelöschte Tracks
  verlassen die Queue still; der spielende Track wird dadurch nie gestoppt
  (faultet der spielende Track selbst, gilt FB-6: Skip + ein Toast).
- **PLAY-5b** [geplant] [core] — Unmounted-Hygiene: Unmountete Tracks bleiben
  grau in der Queue, werden beim Advance übersprungen und heilen beim
  Mount-Event (P-6). Kein Hintergrundereignis (deleted, unmounted,
  Sync-Removal, Watcher) stoppt den spielenden Track — explizite
  Nutzeraktionen (Doppelklick, Play all, OS-Open) wechseln die Wiedergabe
  natürlich.
- **PLAY-6** [geplant] [gtk] — Shuffle/Repeat sind globale Player-Zustände
  (Player-Leiste), keine Ansichts-Zustände. Repeat zyklisch: off → all → one.

## D. Albums- & Artists-Ansicht

- **ALB-1** [geplant] [gtk] — Album-Grid: Hover = Abdunkel-Gradient +
  Play-Button unten rechts (fade 150 ms). Klick Cover/Titel → Album-Detail
  (push). Klick Play → spielt das Album sofort gemäß PLAY-1a, ohne zu
  navigieren. Kontextmenü: Play next / Add to queue / Edit tags / Show files.
- **ALB-2** [geplant] [gtk] — Album-Detail: Hero mit Cover + dominanter
  Farbfläche (Akzent-Pipeline), Play all/Shuffle-Pills (PLAY-1a), Trackliste
  nach Disc/Tracknummer. Spielender Track: Akzent-Row + EQ-Icon + bold —
  identisch in jeder Liste der App (eine Markierungssprache).
- **ART-1** [geplant] [gtk] — Artist-Liste: Klick selektiert und zeigt Detail
  rechts; Selection folgt NIE der Wiedergabe, spielender Artist zeigt nur
  Mini-EQ.
- **ART-2** [geplant] [gtk] — Artist-Detail: Hero-Glow (vorberechnete
  Textur, 250 ms Crossfade beim Wechsel), Alben-Reihe (Hover wie ALB-1), Top
  Tracks (Doppelklick spielt gemäß PLAY-2 im Kontext „Top Tracks"). „Show all
  N tracks ›" → Tracks-Modus mit gesetztem Artist-Filter-Chip (sichtbar, per
  × entfernbar).
- **FX-1** [geplant] [manuell] — Alle Effekte respektieren
  `gtk-enable-animations=false` (harte Schaltung) und laufen nur GPU-billig
  (Opacity/Transform, vorgerenderte Glows). Keine Live-Blurs in Listen.

## E. MTP / Sync

- **MTP-1** [geplant] [gtk] — Einstecken: Toast „Pixel 8 connected",
  Gerätekarte faded in die Sidebar. Keine Auto-Navigation — der User wird nie
  aus seiner Ansicht gerissen.
- **MTP-2** [geplant] [gtk] — Karte: Klick auf Karte → Device-View (push).
  Klick auf „Sync"-Pill → startet Sync sofort, ohne Navigation
  (stopPropagation). Hover-Tooltip zeigt Details.
- **MTP-3** [geplant] [core] — Sync läuft: Karte und (falls offen)
  Device-View zeigen denselben Fortschritt (ein State). Cancel überall =
  gleiche Aktion: aktuelle Datei sauber beenden, Toast „Sync cancelled · 28
  copied".
- **MTP-4** [geplant] [gtk] — Unmount/Eject in der Device-View: Eject-Klick →
  Button wird Spinner → unmount → Toast „Pixel 8 can be unplugged" → View
  poppt selbst zur vorherigen Ansicht (150 ms Crossfade), Karte verschwindet.
  Eject während Sync: disabled + Tooltip „Sync in progress".
- **MTP-5** [geplant] [gtk] — Kabel gezogen (ohne Eject): Toast „Pixel 8
  disconnected" (+ „— sync incomplete (54 of 82)" falls mitten im Sync). Ist
  die Device-View offen, wechselt sie auf eine StatusPage „Device
  disconnected" mit Button „Back to Library" — sie schließt sich nicht selbst
  (der User soll lesen können, was passiert ist). Karte verschwindet.
  Nächster Sync resumt via .part-Regel.
- **MTP-6** [geplant] [gtk] — Sync-Ende: Toast „Sync complete · 82 copied, 14
  removed" (+ „· 3 failed" mit „Details"). Karte morpht zu Idle („synced ✓"),
  Delta-Karte zeigt „Everything in sync ✓". Kein 100 %-Haltezustand.

## F. Einstellungen & Modale

- **SET-1** [geplant] [gtk] — Preferences = ein Fenster mit vertikaler
  Navigation (Seiten: General, Library, Playback, Audio, Sync, Plugins).
  Klick auf Seite wechselt rechts den Inhalt; kein Tab-Overflow, neue
  Features = neue Seite oder Sektion.
- **SET-2** [geplant] [gtk] — Unterseiten (z. B. Scrobbler-Konfiguration)
  sind Navigation-Pages im selben Fenster mit ‹-Back im Header — keine neuen
  Fenster.
- **SET-3** [geplant] [gtk] — Modal-Ebenen: maximal zwei. Ebene 1 = ein
  Fenster über dem Hauptfenster (Preferences ODER Tag-Editor ODER Shortcuts —
  nie zwei gleichzeitig). Ebene 2 = genau ein Dialog darüber (FileChooser,
  Bestätigung). Ein Dialog öffnet nie einen weiteren Dialog. Esc schließt
  immer die oberste Ebene.
- **SET-4** [geplant] [gtk] — Settings wirken sofort (kein Apply/OK).
  Destruktiver Umschalter konkret: Wird Auto-clean aktiviert, während die
  Deleted-Gruppe bereits Zeilen jenseits der gewählten Frist enthält,
  erscheint einmalig ein Dialog: „This will remove N tracks now (deleted
  more than 30 days ago) — their ratings and listening history go with
  them. Remove now / Start counting from today." Letzteres speichert das
  Aktivierungsdatum als Stichtag (`auto_clean_armed_at`); gelöscht wird nur,
  was Frist UND Stichtag reißt. Beide Lösch-Dialoge der App (dieser und
  „Remove all N") benennen die Kaskade explizit: Ratings + Hörhistorie gehen
  mit (P-5).

## G. Feedback-Vokabular

- **FB-1** [geplant] [core] — Zwei-Klassen-Toasts (Pill unten zentriert, eine
  Zeile, max. 1 Action-Button, 4 s / mit Undo 10 s; nur für abgeschlossene
  Aktionen oder Ereignisse): Aktionslose Ereignis-Toasts ersetzen einander —
  maximal einer wartet, der neueste gewinnt, kein Backlog-Rauschen. Toasts
  MIT Aktion (Undo) sind unverdrängbar und laufen ihre vollen 10 s;
  Ereignis-Toasts warten solange.
- **FB-2** [geplant] [gtk] — Fortschrittskarte (Sidebar-Bottom-Slot,
  stapelbar Scan über Sync): Spinner + Titel + % rechts (tabular) +
  3-px-Balken + ellipsierte Detailzeile. Für alles > ~1 s: Scan, Sync,
  Relink-Suchlauf, Playlist-Import. Klick auf Karte → zugehörige Ansicht;
  Cancel auf Karte bricht ab.
- **FB-3** [geplant] [core] — Fehler: Einzelfehler im Lauf werden gesammelt,
  nie einzeln getoastet. Am Ende EIN Toast mit „N failed · Details" →
  Details öffnet die zuständige View/Dialog. Persistente Probleme leben als
  Badge + ISSUES-Eintrag, nicht als wiederkehrende Toasts.
- **FB-4** [geplant] [core] — Badges zählen nur Einträge, die neuer sind als
  das letzte Öffnen der jeweiligen View (`last_viewed`-Timestamp je View im
  Settings-Store): Missing zählt `missing_since > last_viewed`, Import-Errors
  zählt `first_seen > last_viewed` — ohne dismissed-Zeilen und ohne
  Hinweiszeilen („imported without metadata"), denn gezählt wird nur, was den
  User um etwas bittet. Reaktivierung einer dismissed-Zeile (Datei geändert)
  startet eine neue Episode: `first_seen = now`, `seen_count = 1` — sie badgt
  also wieder. View öffnen = Badge weg, die Gesamtzahl steht in der View.
- **FB-5** [geplant] [gtk] — StatusPages für leere Zustände mit genau einem
  nächsten Schritt („No missing files ✓", „Library folder unavailable —
  Retry").
- **FB-6** [geplant] [core] — Gelöschte Datei (extern, Watcher): kein Toast
  pro Datei (Rauschen) — Row wird grau/verschwindet gemäß Missing-Regeln,
  ISSUES-Badge zählt hoch. Ausnahme: der gerade spielende Track faultet →
  Skip + ein Toast „Track unavailable — skipped".
- **FB-7** [geplant] [core] — „Remove from library" löscht nicht, sondern
  setzt `removed_at` (Tombstone); die Zeile mit Ratings, Play-Counts und
  Playlist-Positionen bleibt 10 s vollständig erhalten, Undo setzt nur
  `removed_at = NULL` zurück — deshalb ist die Wiederherstellung exakt
  (gleiche id, keine Race mit parallel laufenden Scans). Der Remove-Toast
  trägt immer Undo (FB-1, 10 s). Nach Toast-Ablauf wird hart gelöscht
  (Kaskade: Playlist-Einträge, Hörhistorie, Sync-Zustand); App-Ende im
  Fenster → Löschung wird beim nächsten Start committed, nie zurückgerollt
  („7 removed" muss wahr bleiben). Auto-clean (opt-in, default off, nur
  deleted-Tracks) löscht hart ohne Toast und ohne Undo — es feuert
  frühestens 30/90 Tage nach dem Verschwinden (SET-4).

## H. Dateiassoziation & OS-Integration

- **OS-1** [geplant] [e2e] — Eine Datei geöffnet (Doppelklick im
  Dateimanager): Hauptfenster öffnet in der zuletzt benutzten Ansicht
  (Session-Restore), Wiedergabe startet sofort, Player-Leiste zeigt den
  Track. Keine Sonder-Ansicht, kein Mini-Player-Autostart.
- **OS-2** [geplant] [core] — Datei in der Library → normaler Track (mit
  Historie/Rating). Datei außerhalb → transienter Track: spielt, erscheint in
  Queue/Player-Leiste mit dezentem „not in library"-Chip, wird NICHT
  importiert, hinterlässt keine DB-Zeile über die Session hinaus.
  Kontextmenü bietet „Add to library…" an.
- **OS-3** [geplant] [e2e] — Mehrere Dateien (Auswahl → „Öffnen mit
  Reprise"): ersetzt die Queue durch die Auswahl in
  Dateimanager-Reihenfolge, spielt die erste, Toast „12 files queued".
  Zweiter Aufruf während laufender Instanz: gleiche Semantik (Queue
  ersetzen, nicht anhängen — vorhersagbar schlägt schlau). Das Ersetzen bei
  laufender Wiedergabe ist eine explizite Nutzeraktion und verstößt nicht
  gegen PLAY-5.
- **OS-4** [geplant] [e2e] — Single-Instance: ein zweiter Start reicht
  Dateien an die laufende Instanz durch und fokussiert deren Fenster.
- **OS-5** [geplant] [e2e] — MPRIS spiegelt immer den Player-Zustand;
  Abspielen aus Dateiassoziation ist dort identisch sichtbar.

## I. Startzustand

- **START-1** [geplant] [e2e] — Normaler Start: letzte Ansicht +
  Scroll-Position, Wiedergabe pausiert auf letztem Track (Position
  restauriert), Startup-Reconcile läuft still (Karte nur bei echter Arbeit).
- **START-2** [geplant] [gtk] — Start mit unavailable Library-Root:
  StatusPage gemäß Root-Guard, keine Missing-Massenmarkierung; Library-Views
  zeigen den letzten bekannten Bestand normal (Root-Guard hat nichts
  markiert), nur die StatusPage/Karte meldet den Zustand. Kein Blank-Screen.

## J. Queue-Ansicht

- **QUE-1** [geplant] [gtk] — Die Queue ist nie leer, solange etwas spielt.
  Sie zeigt drei Abschnitte, in dieser Reihenfolge: **Now Playing** (1 Row,
  Akzent + EQ, wie überall) · **Play Next** — manuell eingereihte Tracks
  („Play next"/„Add to queue"), nur wenn vorhanden, mit Sektionstitel ·
  **Up Next · aus <Quelle>** — der Rest des Playback-Snapshots (z. B. „Up
  Next · from Late Night" oder „· from Neverbloom"), inklusive
  Shuffle-Reihenfolge, falls Shuffle an.
- **QUE-2** [geplant] [core] — Abspiellogik = Anzeigereihenfolge: erst
  Play-Next-Einträge (FIFO), dann der Snapshot ab aktueller Position. Keine
  versteckte Priorität — was die View zeigt, ist was passiert.
- **QUE-3** [geplant] [gtk] — Interaktion: DnD-Reorder innerhalb „Play
  Next"; Up-Next-Rows per DnD nach „Play Next" ziehbar; Rechtsklick „Remove
  from queue" überall (entfernt aus dem Snapshot, nicht aus der Library);
  Doppelklick auf eine Queue-Row springt dorthin (Playhead, kein Neuaufbau —
  Ausnahme zu NAV-4). „Clear queue"-Button räumt nur „Play Next"; der
  Snapshot bleibt (er verschwindet erst mit Playback-Stop oder neuem
  Kontext).
- **QUE-4** [geplant] [gtk] — Leerzustand gibt es nur ohne Wiedergabe:
  StatusPage „Nothing queued — play something" (FB-5, ein nächster Schritt,
  kein Grid an Vorschlägen).
- **QUE-5** [geplant] [core] — Sidebar-Zähler „Queue · N": N = Play Next +
  verbleibende Up-Next-Tracks (nicht Gesamt-Snapshot). Der Zähler ist eine
  Bestandsanzeige, kein Badge (P-1: keine „Bitte").

## K. Filter- & Such-Sichtbarkeit

- **FIL-1a** [geplant] [gtk] — Eine Wahrheit über Einschränkungen
  (Track-Listen): Alles, was die sichtbare Track-Liste einschränkt, steht als
  Chip in der Filter-Zeile direkt über der Liste — auch die Headerbar-Suche
  (Chip ⌕ „falling“ in any field, eigenes ×-Klickziel ≥ 20 px; das × entfernt
  nur die Suche, Esc gemäß NAV-6). Gilt in jeder Track-Quelle (Library,
  Playlist, Smart, Queue, Missing). Die Suche ist global über Track-Quellen
  und reist beim Ortswechsel mit; ihr Chip erscheint überall dort, wo sie
  tatsächlich einschränkt — in Quellen ohne Suchwirkung (Import-Errors:
  eigene Panel-Rows) erscheint kein Chip. Facetten-Chips und „+ Add filter"
  bleiben Library-only. Ein unsichtbarer aktiver Filter ist ein Bug.
  Per-Ort-Scoping der Suche wäre eine eigene künftige Regel, nicht Teil
  dieser.
- **FIL-1b** [geplant] [gtk] — Albums-/Artists-Modus: Die globale Suche
  wirkt dort bereits (Grid-Filterung); dieselbe Chip-Zeile inkl. Zählung und
  „Clear all" folgt dort nach dem Muster von FIL-1a/FIL-2. Bis dahin ist die
  Lücke hier benannt statt still gebrochen.
- **FIL-2** [geplant] [gtk] — Zählung ist Zustand: Die Filter-Zeile ist
  permanenter Listen-Header jeder Track-Quelle — sie erscheint und
  verschwindet nie (kein Layout-Shift by design, P-4). Idle maximal leise:
  nur die neutrale Gesamtzahl rechts (dim, caption), in der Library
  zusätzlich das „+ Add filter"-Pill; kein „FILTER"-Label. Bei aktiver
  Einschränkung: „FILTER"-Label + Chips + akzentuierte Trefferzahl
  („15 of 1,664 tracks", Trefferzahl Akzentfarbe bold) + „Clear all ×"
  (räumt Suche und alle Filter in einem Klick). Die Ausblende-Preference der
  Leiste regelt nur den Idle-Zustand — bei aktiver Einschränkung erscheint
  die Zeile immer (Force-show; der Shift ist direkte Folge der eigenen
  Eingabe, P-4-konform). Das Status-Overlay unten rechts zeigt immer die
  neutrale Bibliotheks-Statistik; seine „X of Y"-Variante entfällt — die
  Filter-Zeile spricht über die Sicht, das Overlay über die Bibliothek.
- **FIL-3** [geplant] [gtk] — Ende-der-Treffer-Zeile: Unter der letzten Row
  einer eingeschränkten Liste (≥ 1 Treffer) steht zentriert „End of results —
  1,649 tracks hidden by search “falling”" + Pill „Show all 1,664 tracks"
  (= Clear all). Sie gehört visuell zum Listenende: direkt unter der letzten
  Row, wenn die Liste kürzer als der Viewport ist; bei längeren Listen
  erscheint sie erst, wenn das Listenende in den Viewport scrollt; sie
  schwebt nie über Rows (nicht sticky). Umsetzung als positioniertes Overlay
  — die Virtualisierung des ColumnView bleibt unangetastet; Input-durchlässig
  außer der Pill; Position wird bei Scroll-, Model-/Filter- und
  Resize-Änderungen neu berechnet.
- **FIL-4** [geplant] [gtk] — Suchfeld trägt seinen Zustand: Sobald das Feld
  Text enthält, bekommt es Akzent-Border + getönten Hintergrund — auch
  unfokussiert.
- **FIL-5** [geplant] [gtk] — Treffer-Highlighting: Der Suchbegriff wird in
  allen durchsuchten, sichtbaren Textspalten hervorgehoben (Title, Artist,
  Album, Genre; Akzent bold, Pango-escaped). Ist die einzige matchende
  Spalte ausgeblendet, bleibt die Row unmarkiert — akzeptierte Restlücke.
  Chip-Wortlaut bleibt „in any field".
- **FIL-6** [geplant] [gtk] — 0-Treffer-Leerzustand: StatusPage mit genau
  einem Button „Show all 1,664 tracks" (= Clear all) — FB-5-konform; der
  eine Schritt führt garantiert zu Inhalt, nie in einen zweiten Leerzustand.
  „Clear all ×" (Filter-Zeile), „Show all N tracks" (Ende-Zeile,
  Leerzustand) feuern dieselbe Action — zwei kontextgerechte Namen, ein
  Verhalten.

---

Wenn beim Testen ein Fall auftaucht, den keine Regel deckt: Regel ergänzen
(Prozessregeln oben), nicht lokal entscheiden.
