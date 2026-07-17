# UX-Regelwerk + Akzeptanztest-Fundament — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Status: **gegrilled, ready to implement**
Branch: `feat/ux-rules-acceptance-tests` (Worktree `.worktrees/ux-rules-acceptance-tests`)
Date: 2026-07-17

**Goal:** `docs/ux-rules.md` als verbindliches, gehärtetes UX-Regelwerk etablieren (deutsch, nummerierte Regeln mit Status- und Ebenen-Tags), per Traceability-Lint mit den Tests verdrahten und das Muster mit einem Pilot (Bereich C, Playback/Queue) einmal komplett vorführen.

**Architecture:** Das Regelwerk ist die einzige UX-Wahrheitsquelle; Tests referenzieren Regel-IDs im Testnamen; ein Bash-Lint in `check-merge-readiness.sh` erzwingt Dokument↔Test-Konsistenz in beide Richtungen. Verhaltensänderungen passieren NICHT in diesem Branch — Regeln dafür stehen als `[geplant]` im Dokument.

**Tech Stack:** Markdown, Bash (Lint), Rust `cargo test` (reprise-core), bestehender cua-e2e-Harness (AT-SPI/Xvfb).

## Global Constraints

- Dokumentsprache **Deutsch**; Code, Commits, AGENTS.md-Edit Englisch (Repo-Konvention).
- Regel-IDs sind **append-only**: nie umnummerieren, nie wiederverwenden; Ersetzungen als `[ersetzt durch <ID>]`, Ziel-ID immer benannt.
- Status **binär**: `[aktiv]` (einklagbar: Code konform + regelbenannter Test grün + Merge-Blocker) / `[geplant]` (Zielbild). Wechsel auf `[aktiv]` **nur im selben Commit**, der Verhalten/Tests nachweist. Halb umgesetzt → a/b-Split.
- **Genau eine primäre Regel-ID pro Test**: Rust `fn play_1a_…` (snake_case), cua-e2e-Szenarien `play-2-…` (kebab-case). Sammel-Tests über mehrere Regeln sind verboten.
- `#[ignore = "UX <ID> [geplant] — …"]` ist nur auf `[geplant]`-Regeln erlaubt (Lint prüft das).
- **Keine Verhaltensänderungen in diesem Branch.** Insbesondere: Queue-View, Toasts, History-Stack nicht anfassen.
- **Koordination:** Ein paralleler Agent implementiert QUE-1–5 + NAV-9 im selben Branch. Task 1 (Dokument) wird deshalb **sofort und als erstes committet** — der Queue-Agent flippt „seine" Regeln dann in seinen Implementierungs-Commits. Dateien des Queue-Agenten (Queue-View, Player-Leiste) sind tabu.
- Commit-Format `<type>: <description>`, **kein Attribution-Footer** (Repo-Regel in AGENTS.md).
- Nach jedem Task: `.superpowers/sdd/progress.md` (append-only Ledger) ergänzen.
- Jeder Commit lässt `scripts/check-merge-readiness.sh` grün (mindestens: fmt, clippy, Workspace-Tests, ab Task 3 auch den neuen Lint).
- 800-Zeilen-Grenze pro Datei (Repo-Lint); `docs/ux-rules.md` ist als Markdown davon ausgenommen.

## Entscheidungstabelle (Grilling 2026-07-17, alle vom User bestätigt)

| Frage | Entscheidung |
|-------|-------------|
| OS-3 vs PLAY-5 | PLAY-5 auf **Hintergrundereignisse** eingeschränkt; Nutzeraktionen wechseln Wiedergabe natürlich |
| FB-1 vs FB-7 | **Zwei-Klassen-Toasts:** aktionslose ersetzen einander (max. 1 wartend); Aktions-Toasts (Undo) unverdrängbar, 10 s |
| ALB-1 vs PLAY-1 | **PLAY-1a Container-Play:** Queue = Container in kanonischer Reihenfolge; Grid-Filter bestimmt nur Erreichbarkeit |
| NAV-3 vs NAV-2 | **Globaler History-Stack** über Ortsgrenzen; Sidebar-Klick ersetzt; Markierung folgt oberstem Eintrag; NAV-2a: Stack nicht sessionpersistent, Back ohne Einträge disabled |
| Beschluss-Referenzen | FB-4/FB-7/SET-4 **inline** als Volltext (siehe Dokument in Task 1) |
| P-1 | **Rollen-Formulierung:** Ankündigung=Toast · View-Zustand=StatusPage · Prozess=Karte · Bitte=Badge |
| NAV-5 vs START-1 | Modus-Gedächtnis nur in der Session; Neustart restauriert nur letzte Ansicht |
| Status-Modell | Binär `[aktiv]`/`[geplant]`; Same-Commit-Aktivierung; a/b-Split statt „teilweise" |
| Sprache/Ort | **Deutsch**, `docs/ux-rules.md` |
| Änderungsprozess | Append-only-IDs; git-Historie statt Inline-Changelog; AGENTS.md-Protokoll für Regelvorschläge |
| Test-Ebenen | Tag `[core]`/`[gtk]`/`[e2e]`/`[manuell]`; niedrigste falsifizierende Ebene; das *Was* automatisiert, das *Wie-schnell* manuell (RELEASING.md-Checkliste spricht dieselben IDs) |
| Branch-Scope | Fundament + Pilot Bereich C; Verhaltensänderungen in eigene Branches |
| Initial-Status | Konservativ: alles `[geplant]`; Audit **sektionsweise** beim Anfassen (Pilot auditiert C komplett) |
| Gates | `[core]`/`[gtk]` → Workspace-Suite → pre-push; `[e2e]` → cua-e2e → Release; Lint in `check-merge-readiness.sh` |
| Traceability | ID im Testnamen; Lint dreifach: jede `[aktiv]`-Regel ≥ 1 Test · keine unbekannte/ersetzte ID · kein `#[ignore]` auf `[aktiv]` |
| QUE-1–5, NAV-9 | Aus dem Queue-Fix-Prompt **wörtlich** als `[geplant]` ins Dokument (Implementierung im parallelen Queue-Branch/Agenten) |

---

### Task 1: `docs/ux-rules.md` anlegen und sofort committen

**Files:**
- Create: `docs/ux-rules.md`
- Create (bereits geschehen beim Planen): `docs/plans/ux-rules-acceptance-tests.md`
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Produces: Regelzeilen-Format `- **<ID>** [<status>] [<ebene>] — <Text>`, auf das sich der Lint (Task 3) und alle Tests (Task 4/5) verlassen. `<ID>` = `[A-Z]+-[0-9]+[a-z]?`, `<status>` ∈ {aktiv, geplant, ersetzt durch <ID>}, `<ebene>` ∈ {core, gtk, e2e, manuell}.

- [x] **Step 1: Dokument schreiben** — exakt dieser Inhalt nach `docs/ux-rules.md`:

````markdown
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
referenziert eine unbekannte oder ersetzte ID · kein Ignore auf `[aktiv]`.

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
  gestarteten Prozesses (Sync-Removals kollabieren). Einblendungen
  (Gerätekarte, ISSUES) faden ohne Reflow benachbarter Inhalte.
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
- **PLAY-2** [geplant] [core] — Doppelklick spielt die Row und hängt den Rest
  der sichtbaren Liste ab dieser Position in die Queue.
- **PLAY-3** [geplant] [core] — Filter schränkt Shuffle ein — absichtlich.
  Gefilterte Playlist + Shuffle = Shuffle über die Treffer („shuffle my 90s
  tracks"). Filter nachträglich ändern fasst eine bereits gebaute Queue nicht
  an (Queue ist ein Snapshot; sichtbar in „Queue").
- **PLAY-4a** [geplant] [core] — Missing in Listen: Listen-Playback und
  Queue-Advance überspringen Missing still.
- **PLAY-4b** [geplant] [gtk] — Doppelklick auf konkrete Missing-Row: Toast
  „File missing since …" + Button „Show in Missing files". Einreihen (Play
  next/Add to queue) ist für Missing disabled.
- **PLAY-5** [ersetzt durch PLAY-5a/PLAY-5b] — Ursprüngliche
  Queue-Hygiene-Sammelregel; beim Härten in die Teilregeln deleted (5a) und
  unmounted (5b) gesplittet.
- **PLAY-5a** [geplant] [core] — Deleted-Hygiene: Extern gelöschte Tracks
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

---

Wenn beim Testen ein Fall auftaucht, den keine Regel deckt: Regel ergänzen
(Prozessregeln oben), nicht lokal entscheiden.
````

- [x] **Step 2: Format-Sanity prüfen**

Run: `grep -cE '^\- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant|ersetzt)' docs/ux-rules.md`
Expected: `60` (Regelzeilen inkl. der ersetzten PLAY-5; bei Abweichung Zeilenformat fixen)

Run: `grep -c '\[aktiv\]' docs/ux-rules.md`
Expected: `0` (alles startet geplant)

- [x] **Step 3: Ledger ergänzen** — an `.superpowers/sdd/progress.md` anhängen:

```markdown
## 2026-07-17 — UX-Regelwerk Task 1 (docs/ux-rules.md)

- Verbindliches UX-Regelwerk eingecheckt: 60 Regelzeilen (Sektionen A–J,
  alle `[geplant]`, PLAY-5 als Ersetzt-Wegweiser), mit Prozessregeln
  (Status, append-only IDs, Ebenen-Tags,
  Traceability, Änderungsprotokoll). Härtung gemäß Grilling 2026-07-17
  (docs/plans/ux-rules-acceptance-tests.md). QUE-1–5/NAV-9 aus dem
  Queue-Fix-Prompt wörtlich übernommen — Implementierung läuft parallel.
```

- [x] **Step 4: Committen (sofort — der parallele Queue-Agent wartet darauf)**

```bash
git add docs/ux-rules.md docs/plans/ux-rules-acceptance-tests.md .superpowers/sdd/progress.md
git commit -m "docs: add binding UX rulebook (all rules [geplant])"
```

---

### Task 2: AGENTS.md-Anbindung

**Files:**
- Modify: `AGENTS.md` (nach der Sektion „## Shared workflow skills (read these)")
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Consumes: `docs/ux-rules.md` aus Task 1.
- Produces: Verbindlichkeits-Absatz, auf den sich künftige Agenten-Sessions stützen.

- [x] **Step 1: Sektion einfügen** — direkt nach dem „Shared workflow skills"-Block:

```markdown
## UX rules are binding

`docs/ux-rules.md` is the single UX source of truth (German). Before touching
any user-facing behavior, read the sections you work in. The contract:

- `[aktiv]` rules are enforceable: deviation is a bug; every `[aktiv]` rule
  has a rule-named test (`fn play_1a_…` / cua-e2e `play-1a-…`) that gates
  merges via `scripts/check-ux-traceability.sh`.
- A rule flips `[geplant]` → `[aktiv]` in the same commit that implements
  the behavior and adds its test — never retroactively.
- Rule IDs are append-only; replaced rules stay as `[ersetzt durch <ID>]`
  and their tests are re-pointed in the same commit.
- If you hit a case no rule covers: do NOT decide locally. Add a
  `[geplant]` draft with the next free ID in the affected section, marked
  `<!-- REVIEW: Regelvorschlag -->`, and surface it for human review.
```

- [x] **Step 2: Ledger ergänzen** — an `.superpowers/sdd/progress.md` anhängen:

```markdown
- AGENTS.md: binding-UX-rules section added (contract, flip rule, proposal
  protocol) — UX-Regelwerk Task 2.
```

- [x] **Step 3: Committen**

```bash
git add AGENTS.md .superpowers/sdd/progress.md
git commit -m "docs: bind agents to the UX rulebook contract"
```

---

### Task 3: Traceability-Lint + Gate-Verdrahtung

**Files:**
- Create: `scripts/check-ux-traceability.sh`
- Modify: `scripts/check-merge-readiness.sh` (nach dem `check-architecture.sh`-Aufruf, Zeile ~43)
- Modify: `TESTING.md` (Abschnitt „Required merge gates", ein Satz)
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Consumes: Regelzeilen-Format aus Task 1 (`- **<ID>** [<status>] …`).
- Produces: `scripts/check-ux-traceability.sh` (exit 0 = konsistent), von Task 4/5 und jedem künftigen Branch als Gate benutzt. Testnamens-Konvention: Rust `fn <prefix>_<nr><suffix?>_…` mit `<prefix>` ∈ {p, nav, play, alb, art, fx, mtp, set, fb, os, start, que}; cua-e2e-Szenario-Stems `<prefix>-<nr><suffix?>-…`.

- [x] **Step 1: Lint-Skript schreiben** — exakt dieser Inhalt nach `scripts/check-ux-traceability.sh`:

```bash
#!/usr/bin/env bash
# Traceability-Gate: docs/ux-rules.md <-> regelbenannte Tests.
#
# Prüft drei Richtungen:
#   1. Jede [aktiv]-Regel hat >= 1 Test, der ihre ID im Namen trägt
#      (Rust-fn snake_case oder cua-e2e-Szenario kebab-case).
#   2. Kein Test referenziert eine ID, die im Dokument fehlt oder
#      [ersetzt ...] ist.
#   3. Kein #[ignore] auf einem Test, dessen Regel [aktiv] ist.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

doc=docs/ux-rules.md
[[ -f $doc ]] || { echo "check-ux-traceability: $doc fehlt" >&2; exit 1; }

prefixes='p|nav|play|alb|art|fx|mtp|set|fb|os|start|que'
fail=0

# --- Dokument einlesen: ID -> Status (aktiv|geplant|ersetzt) ---
declare -A status_of
while read -r id st; do
  status_of[$id]=$st
done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant|ersetzt)' "$doc" \
  | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(aktiv|geplant|ersetzt)/\1 \2/')

# --- Test-Referenzen einsammeln (snake aus Rust, kebab aus cua-e2e) ---
snake_refs=$(grep -rhoE "fn (${prefixes})_[0-9]+[a-z]?_" crates --include='*.rs' 2>/dev/null \
  | sed -E 's/^fn //; s/_$//' | sort -u || true)
kebab_refs=$(grep -rhoE "(${prefixes})-[0-9]+[a-z]?-[a-z0-9-]+" scripts/cua-e2e 2>/dev/null \
  | grep -oE "^(${prefixes})-[0-9]+[a-z]?" | sort -u || true)

to_id() { # play_1a bzw. play-1a -> PLAY-1a
  local raw=${1//-/_} prefix nr
  prefix=${raw%%_*}; nr=${raw#*_}
  printf '%s-%s' "${prefix^^}" "$nr"
}

declare -A tested
for ref in $snake_refs $kebab_refs; do
  id=$(to_id "$ref")
  tested[$id]=1
  case "${status_of[$id]:-fehlt}" in
    fehlt)   echo "FEHLER: Test referenziert unbekannte Regel $id" >&2; fail=1 ;;
    ersetzt) echo "FEHLER: Test referenziert ersetzte Regel $id — umhängen" >&2; fail=1 ;;
  esac
done

# --- Richtung 1: jede [aktiv]-Regel hat einen Test ---
for id in "${!status_of[@]}"; do
  if [[ ${status_of[$id]} == aktiv && -z ${tested[$id]:-} ]]; then
    echo "FEHLER: [aktiv]-Regel $id hat keinen regelbenannten Test" >&2; fail=1
  fi
done

# --- Richtung 3: kein #[ignore] auf [aktiv]-Regeln ---
while read -r fn_name; do
  id=$(to_id "$fn_name")
  if [[ ${status_of[$id]:-} == aktiv ]]; then
    echo "FEHLER: Test $fn_name ist ignored, aber Regel $id ist [aktiv]" >&2; fail=1
  fi
done < <(grep -rA3 --include='*.rs' '#\[ignore' crates 2>/dev/null \
  | grep -oE "fn (${prefixes})_[0-9]+[a-z]?_" | sed -E 's/^fn //; s/_$//' | sort -u || true)

if (( fail )); then exit 1; fi
active_count=$(grep -cE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[aktiv\]' "$doc" || true)
echo "UX-Traceability ok: $active_count aktive Regeln abgedeckt"
```

Dann: `chmod +x scripts/check-ux-traceability.sh`

- [x] **Step 2: Negativ-Test (rot sehen, dann grün)**

```bash
scripts/check-ux-traceability.sh   # Erwartet: "UX-Traceability ok: 0 aktive Regeln abgedeckt", Exit 0
sed -i 's/- \*\*P-1\*\* \[geplant\]/- **P-1** [aktiv]/' docs/ux-rules.md
scripts/check-ux-traceability.sh   # Erwartet: "FEHLER: [aktiv]-Regel P-1 hat keinen regelbenannten Test", Exit 1
git checkout docs/ux-rules.md      # Fixture zurückrollen
scripts/check-ux-traceability.sh   # Erwartet: wieder ok, Exit 0
```

- [x] **Step 3: In das Merge-Gate hängen** — in `scripts/check-merge-readiness.sh` direkt nach der Zeile `scripts/check-architecture.sh` einfügen:

```bash
echo "== UX traceability =="
scripts/check-ux-traceability.sh
```

- [x] **Step 4: TESTING.md ergänzen** — im Abschnitt „Required merge gates" nach dem ersten Absatz einfügen:

```markdown
Merge readiness also runs `scripts/check-ux-traceability.sh`: every `[aktiv]`
rule in `docs/ux-rules.md` needs a rule-named test, no test may reference an
unknown or replaced rule ID, and no `[aktiv]` rule test may be `#[ignore]`d.
```

- [x] **Step 5: Gate komplett laufen lassen**

Run: `scripts/check-merge-readiness.sh`
Expected: alle Abschnitte grün inkl. `== UX traceability ==`, Abschluss `Merge-readiness checks passed`

- [x] **Step 6: Ledger + Commit**

```markdown
- Traceability-Lint eingeführt (scripts/check-ux-traceability.sh, 3 Richtungen)
  und in check-merge-readiness verdrahtet; TESTING.md dokumentiert das Gate —
  UX-Regelwerk Task 3.
```

```bash
git add scripts/check-ux-traceability.sh scripts/check-merge-readiness.sh TESTING.md .superpowers/sdd/progress.md
git commit -m "test: add UX rulebook traceability gate"
```

---

### Task 4: Audit Bereich C + Pilottests `[core]` + Status-Flips (EIN Commit)

**Files:**
- Modify: `crates/reprise-core/src/queue_tests.rs` (Regeltests anhängen)
- Modify: `docs/ux-rules.md` (Flips PLAY-2/PLAY-3/PLAY-5a; Audit-Ergebnis ggf. weitere)
- Modify: `.superpowers/sdd/progress.md` (append, inkl. Audit-Notizen)

**Interfaces:**
- Consumes: `Queue`-API aus `crates/reprise-core/src/queue.rs`: `new()`, `set_tracks(Vec<i64>, usize)`, `current() -> Option<i64>`, `advance_auto() -> Option<i64>`, `set_shuffle(bool)`, `ids_in_order() -> Vec<i64>`, `remove_ids(&[i64]) -> bool`, `is_empty() -> bool`. Lint aus Task 3.
- Produces: regelbenannte Tests `play_2_*`, `play_3_*`, `play_5a_*`, `que_1_*` (ignored) in der Workspace-Suite.

- [x] **Step 1: Audit Bereich C durchführen und Befunde notieren** (Kommandos + erwartete Anker):

```bash
# PLAY-1/PLAY-2-Verdrahtung (sichtbare Liste -> Queue):
grep -n "queue_ids_for_activation" crates/reprise-gnome/src/ui/track_list/track_list_activation.rs
grep -n "play_from_view" crates/reprise-gnome/src/ui/playback/player_controller.rs
# PLAY-3: keine reaktive Queue-Neubau-Verdrahtung bei Filteränderung:
grep -rn "set_tracks" crates/reprise-gnome/src --include='*.rs' | grep -v test
#   Erwartung: Treffer NUR in player_controller.rs (play_from_view) und
#   up_next_transport.rs — kein Aufruf aus Filter-/Suche-Handlern.
# PLAY-4a (Missing-Skip beim Advance): Implementierung suchen:
grep -rn "missing" crates/reprise-gnome/src/ui/playback crates/reprise-core/src --include='*.rs' | grep -iv test | head
# PLAY-5a (deleted -> still raus): Kern-API + bestehende Tests:
grep -n "remove_ids" crates/reprise-core/src/queue.rs crates/reprise-core/src/queue_remove_tests.rs | head
# PLAY-6 (Repeat-Zyklus off->all->one in der Player-Leiste):
grep -rn "Repeat::" crates/reprise-gnome/src --include='*.rs' | grep -v test | head
```

Audit-Verdikt je Regel in die Ledger-Notiz schreiben (implementiert+getestet / implementiert+ungetestet / nicht implementiert). **Geflippt wird in diesem Task nur, was in Step 2 einen regelbenannten Test bekommt: PLAY-2, PLAY-3, PLAY-5a.** PLAY-1, PLAY-4a/b, PLAY-5b, PLAY-6, PLAY-1a bleiben `[geplant]` (= noch nicht einklagbar), auch wenn Teile implementiert sind — ihre Flips kommen mit ihren Tests in Folgearbeit. Falls das PLAY-4a-Grep zeigt, dass Listen-Skip komplett fehlt: nur Ledger-Notiz, kein Doc-Edit nötig (steht ja schon `[geplant]`).

- [x] **Step 2: Regeltests schreiben** — ans Ende von `crates/reprise-core/src/queue_tests.rs` anhängen:

```rust
// --- UX-Regelwerk-Tests (docs/ux-rules.md) ---------------------------------
// Charakterisierungs-Tests für Bestandsverhalten: sie sind ab dem ersten
// Lauf grün (das Verhalten existiert schon); der TDD-Rot-Schritt wird durch
// den Assertion-Flip in Step 3 ersetzt, der beweist, dass sie beißen.

// UX PLAY-2: Doppelklick spielt die Row und hängt den Rest der sichtbaren
// Liste ab dieser Position in die Queue (Aktivierungs-Snapshot).
#[test]
fn play_2_activation_snapshot_starts_at_clicked_row() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 2);
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.advance_auto(), Some(40));
    assert_eq!(
        q.advance_auto(),
        None,
        "Tracks vor der geklickten Row folgen nicht automatisch (Repeat::Off)"
    );
}

// UX PLAY-3: Queue ist Snapshot der gefilterten Treffer; Shuffle permutiert
// genau die Treffer (Queue = Treffermenge, kein Track von außerhalb).
#[test]
fn play_3_shuffle_stays_inside_filtered_snapshot() {
    let mut q = Queue::new();
    let treffer = vec![11, 22, 33, 44, 55];
    q.set_tracks(treffer.clone(), 0);
    q.set_shuffle(true);
    let mut queue_ids = q.ids_in_order();
    queue_ids.sort_unstable();
    assert_eq!(queue_ids, treffer);
    assert_eq!(q.current(), Some(11), "aktueller Track bleibt beim Shuffle stehen");
}

// UX PLAY-5a: Extern gelöschte Tracks verlassen die Queue still; der
// spielende Track bleibt unangetastet.
#[test]
fn play_5a_deleted_tracks_leave_queue_silently() {
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2, 3, 4], 1);
    assert!(q.remove_ids(&[3]));
    assert_eq!(q.ids_in_order(), vec![1, 2, 4]);
    assert_eq!(q.current(), Some(2), "Hintergrund-Removal stoppt den spielenden Track nie");
}

// UX QUE-1 [geplant] — Demo des Aktivierungs-Workflows: Der Queue-Branch
// nimmt das #[ignore] weg und flippt QUE-1 auf [aktiv] im selben Commit.
#[test]
#[ignore = "UX QUE-1 [geplant] — Drei-Sektionen-Queue kommt im Queue-Branch"]
fn que_1_queue_is_never_empty_while_playing() {
    let mut q = Queue::new();
    q.set_tracks(vec![7, 8, 9], 0);
    assert!(!q.is_empty(), "solange etwas spielt, ist die Queue nie leer");
}
```

- [x] **Step 3: Beweisen, dass die Tests beißen (Ersatz für den Rot-Schritt)**

```bash
cargo test -p reprise-core play_2_ play_3_ play_5a_    # Erwartet: 3 passed
# Assertion-Flip: in play_5a temporär `vec![1, 2, 4]` -> `vec![1, 2, 3, 4]` ändern
cargo test -p reprise-core play_5a_                    # Erwartet: 1 FAILED
# Flip zurücknehmen
cargo test -p reprise-core play_5a_                    # Erwartet: 1 passed
```

- [x] **Step 4: Status-Flips im Dokument** — in `docs/ux-rules.md`:

```text
- **PLAY-2** [geplant]  ->  - **PLAY-2** [aktiv]
- **PLAY-3** [geplant]  ->  - **PLAY-3** [aktiv]
- **PLAY-5a** [geplant] ->  - **PLAY-5a** [aktiv]
```

- [x] **Step 5: Lint + Suite laufen lassen**

Run: `scripts/check-ux-traceability.sh`
Expected: `UX-Traceability ok: 3 aktive Regeln abgedeckt`

Run: `cargo test -p reprise-core`
Expected: alle Tests grün, `que_1_…` als ignored gelistet

- [x] **Step 6: Ledger + EIN Commit (Tests + Flips zusammen — Same-Commit-Regel)**

```markdown
- Bereich-C-Audit (Verdikte: PLAY-1 implementiert/ungetestet via
  queue_ids_for_activation; PLAY-2/3/5a implementiert + jetzt regelbenannt
  getestet -> [aktiv]; PLAY-4a/5b/6/1a: <Audit-Ergebnis eintragen>).
  Pilot-Regeltests in queue_tests.rs, QUE-1-Demo als ignored —
  UX-Regelwerk Task 4.
```

```bash
git add crates/reprise-core/src/queue_tests.rs docs/ux-rules.md .superpowers/sdd/progress.md
git commit -m "test: pilot UX rule tests for queue area, flip PLAY-2/3/5a to aktiv"
```

---

### Task 5: cua-e2e-Verdrahtungsszenario `play-2-…`

**Files:**
- Modify: `scripts/cua-e2e/lib.sh` (Helper `cua_double_click_label`, nach `cua_click_label`)
- Modify: `scripts/cua-e2e/run.sh` (Szenario-Block im populated-library-Workflow)
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Consumes: Helper aus `lib.sh` (`cua_snapshot`, `element_index_for_label`, `assert_action_landed`, `assert_snapshot_contains`), Log-Marker `queue set from view` aus `player_controller.rs::play_from_view`, Fixtures `sine_01.flac`/`sine_02.flac` (kopiert in `run.sh` Zeile ~172).
- Produces: Szenario-Stem `play-2-doubleclick-row`, den der Lint als kebab-Referenz auf PLAY-2 zählt.

- [x] **Step 1: Double-Click-Helper in `lib.sh`** — nach `cua_click_label` einfügen (identisch zum Click-Helper, nur Verb `double_click`):

```bash
cua_double_click_label() {
  local pid=$1 window_id=$2 label=$3 stem=$4
  local before_path action_path index payload

  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  index=$(element_index_for_label "$before_path" "$label")
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson element_index "$index" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, element_index: $element_index,
      session: $session}')
  "$CUA_DRIVER_BIN" double_click "$payload" >"$action_path"
  assert_action_landed "$action_path"
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null
}
```

- [x] **Step 2: Szenario in `run.sh`** — im populated-library-Workflow, vor dem Suchfilter (danach ist die Row absichtlich verborgen) und vor `finish_scenario` einfügen (PID-/Window-Variablennamen des umgebenden Blocks übernehmen — im Zweifel die des Such-Workflows wiederverwenden):

```bash
# UX PLAY-2 [e2e]-Verdrahtung: Doppelklick auf eine Row baut die Queue aus
# der sichtbaren Liste (Log-Marker aus play_from_view) und startet Playback.
echo "[cua-e2e] play-2-doubleclick-row: activation builds queue from view"
cua_double_click_label "$APP_PID" "$WINDOW_ID" "sine_01" "play-2-doubleclick-row"
assert_app_log_contains \
  "$APP_LOG" "queue set from view" "play-2-doubleclick-row"
```

Hinweis: Das Row-Label ist der Fixture-Titel (`sine_01`); laut Ledger ist die
exakte Label-Form erst im Host-Lauf verifizierbar. Liefert
`element_index_for_label` keinen Treffer, den Snapshot
(`play-2-doubleclick-row-before.json`) inspizieren und das Label anpassen —
NICHT das Szenario löschen.

- [x] **Step 3: Lint prüfen (kebab-Referenz zählt)**

Run: `scripts/check-ux-traceability.sh`
Expected: weiterhin ok (PLAY-2 ist `[aktiv]` und hat jetzt core- + e2e-Referenz)

- [x] **Step 4: Harness-Lauf versuchen**

Run: `cargo build && scripts/cua-e2e/run.sh`
Expected: alle Szenarien grün inkl. `play-2-doubleclick-row` mit Marker.
Falls die Umgebung keinen Xvfb/AT-SPI-Lauf erlaubt (Sandbox): NICHT grün
behaupten — im Ledger als „deferred host check" eintragen (bestehende
Konvention) und den Lauf dem Host-Release-Gate überlassen.

- [x] **Step 5: Ledger + Commit**

```markdown
- cua-e2e: play-2-doubleclick-row-Szenario + cua_double_click_label-Helper;
  Verdrahtungsbeweis für PLAY-2 (Marker "queue set from view").
  <Lauf-Ergebnis oder deferred host check eintragen> — UX-Regelwerk Task 5.
```

```bash
git add scripts/cua-e2e/lib.sh scripts/cua-e2e/run.sh .superpowers/sdd/progress.md
git commit -m "test: cua-e2e wiring scenario for PLAY-2 double-click activation"
```

---

### Task 6: Abschluss-Gate

**Files:**
- Modify: `.superpowers/sdd/progress.md` (append)

- [ ] **Step 1: Volles Merge-Gate**

Run: `scripts/check-merge-readiness.sh`
Expected: `Merge-readiness checks passed against origin/main` (inkl. `== UX traceability ==`)

- [ ] **Step 2: Ledger-Abschluss**

```markdown
- UX-Regelwerk-Fundament komplett: Dokument (60 Regelzeilen, 3 [aktiv],
  1 ersetzt),
  AGENTS.md-Bindung, Traceability-Gate, Pilot Bereich C (core + e2e),
  QUE-1-Aktivierungs-Demo. Verhaltensänderungen laufen als [geplant] in
  Folge-Branches (Queue-Branch parallel in Arbeit) — UX-Regelwerk Task 6.
```

```bash
git add .superpowers/sdd/progress.md
git commit -m "chore: close out UX rulebook foundation"
```

- [ ] **Step 3: Branch-Abschluss dem User überlassen** — superpowers:finishing-a-development-branch (Merge in main vs. warten auf den parallelen Queue-Agenten im selben Branch — Koordination liegt beim User).
