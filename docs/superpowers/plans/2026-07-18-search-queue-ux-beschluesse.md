# Search-Streifen + Queue-Vereinheitlichung — Beschlussdokument (Grilling 2026-07-18, abends)

UX-Feedback auf den gebauten Zustand. Zwei Teile: die Suchleiste bekommt ihre
gestalterische Form, und die Queue wird auf **ein Modell mit zwei Flächen**
gebracht.

> **Regelwerk:** SEARCH-2/3/4 werden in Sektion Q **korrigiert** (nicht neu
> angelegt); QUE-1..6 gehen nach Sektion J, wobei QUE-1/5 gegenüber dem
> `[geplant]`-Bestand umformuliert werden. Status-Flip im
> Implementierungs-Commit, jede aktive Regel mit regelbenanntem Test.

## Audit-Befunde, die die Vorgabe verändert haben

1. **Die SearchBar ist bereits eine zweite Top-Bar der `ToolbarView`** — sie
   schiebt den Content schon, überlappt nicht, kein `Overlay` beteiligt. Der
   „schwebende" Eindruck hat drei rein visuelle Ursachen: `ToolbarStyle::Flat`
   unterdrückt den Hintergrund der Leiste (dieselbe Falle wie beim
   Headerbar-Titel heute Nachmittag), `GtkSearchBar` zentriert sein Kind
   konstruktionsbedingt in einer `CenterBox`, und die frühere 300-px-
   Breitenvorgabe entfiel beim Umbau. **Es braucht Styling und einen Clamp,
   kein Re-Parenting.**
2. **Es gibt zwei Queue-Renderer, nicht einen.** Der Up-Next-Tab (schlichte
   Buttons in einer Box) und die Queue-`ColumnView`, erreichbar über
   Sidebar-Zeile **und** Playerleisten-Icon. Die ColumnView erfüllt bereits
   QUE-1/3/4 vollständig (Sektionsheader, DnD, Rechtsklick-Entfernen, Clear,
   StatusPage); der Panel-Tab kann nichts davon — **bewusst**, siehe
   NPP-Beschluss 3.
3. **Das Playerleisten-Icon öffnet kein Panel**, es navigiert die Hauptansicht
   zur Queue-ColumnView. Die Doc-Kommentare behaupten das Gegenteil und sind
   veraltet.
4. **Der Sprung in die manuelle Sektion ist destruktiv**: `take_through`
   löscht alle davorliegenden Einträge. Ein Klick auf den 4. Eintrag
   vernichtet still die Einträge 1–3.
5. **Die Up-Next-Fußzeile ist die einzige Stelle ohne Tausendertrenner** —
   alle anderen Zähler nutzen `format_thousands`.
6. **N+1-Abfrage ohne Sichtbarkeits-Guard**: eine DB-Query pro Queue-Zeile,
   bei jeder Queue-Änderung, unabhängig davon, ob das Panel offen ist.
7. **Eine „Queue-Historie" existiert nicht** — der NPP-Beschluss von heute
   früh hat sie behauptet, im Code gibt es sie nicht (Beschluss 3 unten macht
   sie überflüssig).

## Gegrillte Beschlüsse

1. **QUE-1 korrigiert: ein Modell, zwei Flächen mit unterschiedlicher Tiefe.**
   Nicht „das Panel ist kanonisch", sondern:
   - **Sidebar „Queue" → ColumnView = Verwaltungsfläche.** Sektionen, DnD-
     Reorder, Rechtsklick, Clear, StatusPage. Hier wird die Queue *bearbeitet*.
   - **Panel „Up Next" → Sichtfläche.** Dieselbe Datenquelle, dieselben zwei
     Sektionen, Klick = Sprung, Remove ja, **kein Reorder**. Hier wird die
     Queue *überflogen*.
   - **Das Playerleisten-Icon öffnet das Panel** (Glance), nicht die große
     Ansicht.
   Damit löst sich die Redundanz-Sorge auf: keine zweite Liste, sondern zwei
   Tiefen auf derselben Liste. QUE-3/4 bleiben unangetastet, der
   NPP-Beschluss „das Panel verwaltet nichts" bleibt gültig — Authoring lebt
   in der ColumnView.
2. **QUE-2: zwei Sektionen mit bedingten Headern.** „Next in Queue" (manuell
   einsortiert) und „Continuing from ‚<Album/Playlist>'" (automatischer
   Kontext). Ein Sektionsheader erscheint nur, wenn seine Sektion Einträge
   hat. Ist die manuelle Sektion leer, bleibt nur „Continuing…" — **nie eine
   komplett leere Queue**, solange etwas spielt.
3. **QUE-5 korrigiert: der Sprung verwirft nichts.** Klick auf einen
   Queue-Eintrag setzt nur die Abspielposition und konsumiert **ausschließlich
   den geklickten** Eintrag; davorliegende manuelle Einträge bleiben erhalten
   und spielen danach. Kein Verwerfen, keine Rückfrage, kein Dialog — ein
   Modal für einen Listenklick wäre zu schwer, und die FB-Regeln bevorzugen
   ohnehin Undo-Toast über Dialoge. **Damit entfällt die „Queue-Historie"
   ersatzlos**: Es gibt keinen Verlust, den man rückgängig machen müsste.
   „Remove" nimmt aus der Queue, nie aus der Library.
4. **QUE-3: abgespielte manuelle Einträge verschwinden still.** Beim
   Trackwechsel fallen sie aus Sektion 1 — kein Durchstreichen, kein
   Verharren. Sektion 1 ist reine Zukunft. (Ist bereits so implementiert.)
5. **QUE-4: eine gemeinsame Zahlenformatierung.** Die Queue-Fußzeile nutzt
   denselben locale-Tausendertrenner wie die Library — **eine** Funktion, kein
   zweiter Pfad.
6. **QUE-6 (neu): ein Modell, eine Abfrage, gerendert nur wenn sichtbar.**
   Beides sind Fehler, keine Optimierungen:
   - Ein **gemeinsames Queue-Model** speist beide Flächen — die
     Sammelabfrage kommt damit beiden zugute, kein zweiter Datenpfad.
   - Metadaten in **einer** Query über die Queue-IDs, nicht pro Zeile.
   - **Row-Recycling** und Fetch nur des sichtbaren Fensters: 1.652 Einträge
     dürfen nie 1.652 Queries oder Widgets erzeugen.
   - **Guard**: Trackwechsel und Reorder bei geschlossenem Panel (oder
     anderem aktiven Tab) aktualisieren nur das Model und rendern nichts.
7. **SEARCH-2 korrigiert: vollbreiter Streifen, Clamp, GTK-Default-Dauer.**
   Die Bar bleibt zweite Top-Bar (sie schiebt bereits korrekt), bekommt aber
   **eigene Hintergrundfläche und untere Trennlinie** — explizit gesetzt, weil
   `ToolbarStyle::Flat` sie sonst schluckt. Das Entry wird per `Adw.Clamp`
   (max ~450 px) zentriert statt frei zu schweben.
   **Zur Dauer:** `GtkSearchBar` kapselt seinen Revealer ohne öffentlichen
   Zugriff. Sein Default liegt ohnehin bei 250 ms, identisch mit dem
   Standard-Token. Die Regel lautet deshalb: „slidet mit der Standard-Dauer;
   bei GTK-eigenen Revealern gilt deren Default, sofern er dem Token
   entspricht." Der Test prüft die **Existenz** des Reveals, nicht die
   Millisekunden — dieselbe Denkfigur wie bei TIP-1a/2a: was das Framework
   garantiert, testet man auf Existenz, nicht auf einen nachgebauten Wert.
   MOT-7 greift weiter, weil GTK `gtk-enable-animations` selbst beachtet.
8. **SEARCH-3/4 unverändert in der Sache**, nur präzisiert: Die Lupe ist ein
   `ToggleButton` und trägt bei offener Bar **oder** aktiver Query den
   `:checked`-Akzentstil; kein Badge-Punkt (der bleibt der Bitte-Rolle).
   Esc bleibt zweistufig; eine Bar mit Inhalt klappt nie zu, ohne dass die
   Query als Chip übernommen wird.

## Selbstentscheidungen (Implementierungsebene)

- **`search_1` bricht** (`library_chrome.rs`): Der Test prüft, dass das Entry
  direktes Kind der Bar ist — mit dem Clamp dazwischen stimmt das nicht mehr.
  Neu fassen, nicht löschen.
- **Doppelte Trennlinie vermeiden:** Der Such-Streifen stapelt direkt über der
  `.toolbar`-Filterzeile. Hintergrund und Hairline beider zusammen gestalten.
- **`show_up_next()` fehlt** — das Gegenstück zu `show_lyrics()`
  (`now_playing.rs:336`) ist die ganze Playerleisten-Änderung: Panel öffnen und
  auf Tab 0 schalten, statt zur ColumnView zu navigieren. NPP-4 beachten:
  Sichtbarkeit persistiert, Tab nicht.
- **Veraltete Doc-Kommentare** in `player_bar.rs` (Feld und
  `connect_queue_clicked`) behaupten, der Button öffne ein Panel — mitziehen.
- Die Sidebar-Zeile „Queue" **bleibt** samt QUE-5-Zähler; sie ist der Einstieg
  in die Verwaltungsfläche.
