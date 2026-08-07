# Suchen und Filtern, ohne auf die Uhr zu warten

Die Suche in der Bibliothek fühlt sich träge an. Sie ist es auch, aber nicht
aus dem Grund, den man zuerst vermutet: die Datenbank hat damit nichts zu tun.
Der `LIKE`-Filter über `title`/`artist`/`album`/`genre`
(`queries/clauses.rs:144`) beantwortet eine Bibliothek von 1.915 Titeln warm im
Sub-Millisekundenbereich. Die Wartezeit ist eingebaut, und sie ist doppelt.

**Erstens** ist `GtkSearchEntry::search-changed` GTK-intern gedrosselt — die
Property `search-delay` steht per Default auf 150 ms, und Reprise setzt sie
nirgends. **Zweitens** sitzt darauf ein eigener Timer von 200 ms
(`view_session.rs:22`), der bei jedem Zeichen neu startet. Zwischen dem letzten
Anschlag und dem Ergebnis liegen damit rund 350 ms, von denen nur eine Hälfte im
Code sichtbar ist.

Dasselbe trifft das Zurücksetzen. `Esc`, das `×` am Such-Chip und
„Show all N tracks" laufen alle über `set_text("")` und damit durch dieselbe
Kette — obwohl ein Clear kein Tippen ist und auf nichts warten muss.

Dazu kommt eine zweite, unabhängige Ursache. `filter_change_viewport`
(`track_list_reload.rs:70`) stellt bei **jeder** Filteränderung auf
`CenterPlayingTrack`. Das ist kein Versehen, sondern die aktive Regel **FIL-9**
(`ux-rules.md:1440`): ist der geladene Titel Teil der neuen Ergebnismenge, wird
seine Zeile zentriert statt an der Tabellenoberkante verankert. FIL-9 deckt
dabei Suche **und** Facetten ab. Diese Spec nimmt ihr die Suche und lässt ihr
die Facetten — die Regel hat einen realen Preis, den sie damals nicht beziffert
hat, und er fällt beim Tippen ungleich häufiger an als bei einem Facettenklick.

Läuft ein Titel und ist er im Ergebnis — also praktisch immer, wenn man beim
Hören sucht —, kostet jedes Feuern des Debounce:

- eine sortierte Full-Table-ID-Query (`current_view_ids()`), auch wenn es nichts
  wiederherzustellen gibt, weil `reveal_playing_track` den `is_noop`-Early-Return
  in `restore_reload_anchor` aushebelt,
- einen Sprung auf den laufenden Titel,
- acht Nachbesserungsrunden à 16 ms (`SCROLL_RESTORE_MAX_ATTEMPTS`), die jede für
  sich die Scrollposition schreibt — rund 128 ms sichtbares Nachzappeln.

Die Liste springt also beim Tippen weg und zittert danach nach. Beim „Clear all"
auf die volle Bibliothek ist das der teuerste Fall von allen.

Diese Spec macht aus zwei Wartezeiten eine, nimmt dem Filtern den Zentriersprung
und legt vorher fest, woran der Erfolg gemessen wird.

---

## Geltungsbereich

| Bereich | betroffen |
| --- | --- |
| Header-Suche über Track-Quellen (`view_session::wire_search`) | ja |
| Viewport beim Filterwechsel (`filter_change_viewport`) | ja |
| Facetten-Chips (`reload_centering_playing_track`) | **nein** — zentrieren weiter wie heute |
| Browse-Chooser, Device-Sync-Picker, Section-Search | **nein** — eigene Entries, eigener Takt |
| Inkrementelles Delta-Modell statt Modelltausch | **nein** — siehe „Verworfen" |

Der Facetten-Pfad bleibt bewusst außen vor. Er wurde nicht als störend gemeldet,
und ihn mitzuziehen würde die Änderung über den gemessenen Befund hinaus
ausweiten. Sollte er nach der Messung dasselbe Bild zeigen, ist er eine eigene,
kleine Folgeänderung.

---

## Teil 1 — Eine Uhr statt zwei

`window.rs` setzt auf dem `SearchEntry` explizit `set_search_delay(0)`. Damit
verschwindet GTKs unsichtbare Drosselung und unser eigener Timer ist der einzige
Taktgeber — die Wartezeit steht an genau einer Stelle im Code und ist dort auch
justierbar. `SEARCH_DEBOUNCE_MS` sinkt von 200 auf **150**. Die 150 sind der
Startwert, mit dem gemessen wird; die Vorher-/Nachher-Messung aus Teil 3 zurrt
die endgültige Zahl fest.

Ein Debounce bleibt also ausdrücklich bestehen: flüssiges Tippen wird weiterhin
zu einer Suche zusammengefasst, es sucht nicht jeder Anschlag. Was entfällt, ist
das zweite Warten.

`gtk4 0.11.4` mit Feature `v4_22` deckt `set_search_delay` (GTK 4.8) ab.

**Leerer Text umgeht den Timer.** Im Handler in `wire_search` gilt: ist der neue
Text leer, wird ein ausstehender Timer abgebrochen und sofort neu geladen. Mit
`search_delay(0)` erreicht ein `set_text("")` den Handler unverzögert, womit
`Esc`, das Chip-`×`, „Show all N tracks" und ein von Hand geleertes Feld alle
ohne Wartezeit zurückkehren. Der `restoring`-Guard bleibt unangetastet: er
unterscheidet weiterhin programmatische Wiederherstellungen von Nutzereingaben.

**Ein Kommentar muss mit.** `library_chrome.rs:163` begründet die Wahl von
`connect_changed` gegenüber `connect_search_changed` ausdrücklich mit jenen
150 ms Nachlauf. Der Code bleibt richtig — die Lupe soll dem Tippen ohne jede
Verzögerung folgen —, die Begründung wird durch diese Änderung falsch. Sie wird
mitgezogen, sonst steht dort künftig eine Unwahrheit über das eigene System.

## Teil 2 — Der Viewport hört beim Filtern auf zu springen

`ReloadViewport` bekommt zwei weitere Varianten, `filter_change_viewport` drei
statt zwei Ausgänge:

| Übergang | heute | neu |
| --- | --- | --- |
| Filter unverändert | `PreserveAnchor` | `PreserveAnchor` |
| Filter wechselt auf einen nicht-leeren Wert | `CenterPlayingTrack` | **`Top`** |
| Filter wechselt auf leer | `CenterPlayingTrack` | **`RestorePreSearch`** |

Maßgeblich ist allein, ob der **neue** Wert leer ist. Ein Zeichen anhängen, eines
löschen oder den Text ersetzen sind derselbe Fall: `Top`.

`Top` setzt schlicht `adjustment.set_value(0.0)`. Kein Zentriersprung, kein
`AdjustmentHold`, keine Refinement-Schleife. Wer tippt, will seine Treffer von
oben sehen.

`RestorePreSearch` kehrt an die Stelle zurück, an der der Nutzer vor dem Suchen
stand. Dafür legt `set_filter_and_reload` beim Übergang „Filter war leer → wird
nicht leer" einmalig einen Anker in `Shared` ab: Track-ID plus Offset, nie ein
roher Pixelwert — dieselbe Konvention, die BROWSE-2 für Moduswechsel etabliert
hat, und aus demselben Grund (nach einer Neusortierung zeigt ein Pixelwert auf
die falsche Zeile). Beim Leeren wird der Anker verbraucht und das Feld
zurückgesetzt. Ist die Ankerzeile inzwischen fort — gelöscht, umgetaggt, aus der
Ansicht gefallen —, landet der Blick oben; das ist der bestehende
`prepaint_position`-Pfad, der bei unauflösbarem Anker ohnehin aussteigt.

Verbraucht wird der Anker auch bei einem Quellenwechsel: `set_source_and_reload`
leert den Filter bereits und setzt die Scrollposition auf null, ein Anker aus der
vorigen Quelle wäre dort bedeutungslos.

**Der stille Nebengewinn ist der größere.** `reveal_playing_track` in
`restore_reload_anchor` ist nur für `CenterPlayingTrack` wahr. Für beide neuen
Varianten greift damit der `is_noop`-Early-Return wieder, und die sortierte
Full-Table-ID-Query verschwindet beim Tippen **vollständig**, solange nichts
selektiert ist — der Normalfall beim Suchen. Ist etwas selektiert, wird sie für
`select_captured_ids` weiterhin gebraucht und bleibt.

## Teil 3 — Messprotokoll

Zwei Beschwerden, zwei Größen, zwei Instrumente. „Tippen → Ergebnis" ist eine
Latenz; „schwerfällig" ist eine Bewegung.

### (a) Latenz, von außen, ohne Sonde

Die Filterzeile trägt laut FIL-2 den Trefferzähler („15 of 1.664 tracks") und ist
damit der Beobachtungspunkt: Anschlag per `xdotool key` (t₀), danach im 50-Hz-Takt
`Atspi.Accessible.get_name` auf das Zähler-Label, bis es den neuen Wert trägt (t₁).
`t₁ − t₀` ist die Zeit, die der Nutzer tatsächlich wartet — end-to-end, ohne dass
ein Instrument im Code mitmisst und ohne Rebuild.

### (b) Nachlauf, als Videospur

`ffmpeg -f x11grab -framerate 30` über den Moment, dann pro Frame der vertikale
Versatz gegen Frame 1 per minimalem SSD (`a[y] ≈ b[y+k]`). Das macht die
Refinement-Schleife als pixelgenaue Bewegungsspur sichtbar. Ausgewertet wird ein
Band **ohne** die animierte Now-Playing-Zeile, sonst wird ein Animationsartefakt
als Scroll gezählt. Grundrauschen: ±2 px.

### (c) Zerlegung

Die gemessene Gesamtlatenz muss in drei Zahlen aufgehen: Wartezeit,
Reload-Arbeit, Nachlauf. Der Reload-Pfad hat dafür heute keine Instrumentierung —
drei temporäre `tracing::info!` (Timer feuert, `run_query` fertig,
`restore_reload_anchor` fertig) liefern die mittlere Zahl über die Abstände der
Logzeilen. Die Sonden fliegen nach der Messung wieder heraus.

**Bleibt ein unerklärter Rest, ist das ein Befund, kein Rundungsfehler.** „Das
ist halt das Framework" ist unfalsifiziert und beendet die Suche vorzeitig; in
diesem Projekt hat genau diese Erklärung schon einmal einen zweiten, identischen
Kostenpunkt verdeckt.

### Umgebung

- Isoliertes Profil: echte DB kopiert **mit** `-wal`/`-shm`, dann
  `pragma wal_checkpoint(TRUNCATE)`. Ein blankes `cp` zeigt einen veralteten Stand.
- Start unter `Xvfb` + `openbox`, in `dbus-run-session`, mit `GDK_BACKEND=x11`,
  leerem `WAYLAND_DISPLAY` und `REPRISE_AUDIO_SINK=fakesink`. Ohne
  `dbus-run-session` aktiviert GApplication die laufende Instanz des Nutzers und
  die Messung landet in dessen Fenster.
- Tasten per `xdotool key` **ohne** `--window` — mit `--window` wird XSendEvent
  benutzt, und GTK4 ignoriert das.
- `ui.session.v1` wird so präpariert, dass ein Titel geladen ist. Ohne ihn greift
  `CenterPlayingTrack` nicht und man misst den harmlosen Fall.

### Szenarien und Abnahme

Vier Szenarien, je fünf Läufe, Median: Tippen (fünf Zeichen im ~120-ms-Takt),
Clear per `Esc`, Clear per „Show all N tracks", und als Kontrolle dasselbe ohne
geladenen Titel — die Differenz isoliert den Zentrier-Anteil.

| Messgröße | heute (Erwartung) | Ziel |
| --- | --- | --- |
| Tippen → Zähler steht | ~400 ms | ≤ 200 ms |
| Clear → Zähler steht | ~400 ms | ≤ 60 ms |
| Bewegung nach Modelltausch | ~128 ms Nachlauf | keine zweite Bewegung nach ≥ 50 ms |

Die Erwartungswerte sind Herleitungen aus dem Code, keine Messwerte — die
Vorher-Messung ersetzt sie durch echte Zahlen, und erst danach wird die
Debounce-Dauer endgültig festgezurrt. Die Gegenprobe mit zurückgedrehter Änderung
ist Pflicht: kommen die alten Werte nicht reproduzierbar heraus, misst der
Messstand etwas anderes als die Änderung.

## Teil 4 — SEARCH-9

Das Verhalten wird als neue Regel in `docs/ux-rules.md` festgeschrieben —
`SEARCH-9` ist frei (`SEARCH-1` bis `SEARCH-8` sind vergeben).

**`FIL-9` wird nicht ersetzt, sondern eingeschränkt.** Sie bleibt `[active]` und
regelt weiterhin das Zentrieren bei **Facetten**-Filtern; ihr Text wird um die
Textsuche gekürzt und verweist auf `SEARCH-9`. Das hält den in „Geltungsbereich"
gezogenen Schnitt auch im Regelwerk durch, statt den Facettenpfad ungeregelt
zurückzulassen.

Zum Suchanteil von FIL-9 gehört ein Nebensatz, der mit abgelöst wird: „ohne
geladenen Titel im Ziel bleibt der bestehende ID-plus-Offset-Anker erhalten".
Auch dieser Fall geht bei der Suche künftig auf `Top`. Eine frisch gefilterte
Trefferliste an der Position der ungefilterten Liste zu verankern hat keinen
Adressaten — die Zeile, auf die der Anker zeigte, ist im Ergebnis meist gar
nicht mehr enthalten.

Die vier vorhandenen `fil_9_…`-Tests werden dabei einzeln entschieden:

| Test | Ort | Los |
| --- | --- | --- |
| `fil_9_any_search_change_requests_playing_track_centering` | `track_list_reload.rs:764` | wird zum `search_9_…`-Test der neuen Dreiteilung |
| `fil_9_filter_changes_center_the_visible_playing_track` | `current_track_selection_tests.rs:119` | Display-Test, filtert über Suchtext — wird auf einen Facettenfilter umgestellt und bleibt FIL-9 |
| `fil_9_filter_change_centers_playing_track_in_new_results` | `reload_restore.rs:232` | prüft `centered_track_scroll_target`, das für Facetten weiter gilt — bleibt unverändert |
| `fil_9_reveal_drops_when_the_track_left_the_view` | `track_reveal.rs:217` | betrifft den Reveal-Pfad, nicht das Filtern — bleibt unverändert |

`SEARCH-9` deckt vier Aussagen ab:

1. Zwischen Eingabe und Ergebnis liegt **genau eine** Wartezeit, und zwar die der
   Anwendung; die Drosselung des Eingabefelds ist abgeschaltet.
2. Das Leeren der Suche wartet nicht — `Esc`, Chip-`×`, „Show all N tracks" und
   ein von Hand geleertes Feld kehren unverzögert zurück.
3. Eine gesetzte oder verfeinerte Suche stellt den Blick an den Anfang der
   Treffer. Sie zentriert nichts und bewegt den Viewport nach dem Modelltausch
   nicht weiter.
4. Das Leeren der Suche kehrt an die Stelle zurück, an der die Ansicht vor der
   Suche stand; ist diese Zeile fort, an den Anfang.

Tests sind regelbenannt (`search_9_…`) und **display-frei**, weil das Merge-Gate
ohne Xvfb läuft. Das ist hier kein Zugeständnis: `filter_change_viewport` ist eine
reine Funktion, und der Vor-Such-Anker ist reine Zustandslogik über `Shared` —
beides ohne Fenster prüfbar. Sichtbare Belege (der ausbleibende Sprung, das
ausbleibende Nachzappeln) kommen als ignorierte Display-Tests dazu und werden über
den Messstand aus Teil 3 gefahren.

`SEARCH-9` wird zusammen mit der Implementierung angelegt und trägt im selben
Commit bereits `[active] [gtk]` — das Traceability-Gate verlangt für `[active]`
einen regelbenannten Test, und der entsteht hier gleichzeitig. Ein Zwischenstand
als `[planned]` wäre nur dann nötig, wenn Regeltext und Implementierung
auseinanderfallen; das ist hier nicht der Fall.

---

## Verworfen

**Inkrementelle Filterung ohne Modelltausch.** Statt `items_changed(0, old, new)`
bei jedem Query-Wechsel ein Delta-Update. Der Gewinn läge bei sehr großen
Bibliotheken; bei 1.915 Titeln rechtfertigt er den Umbau nicht, und Anker,
Selektion und Sections hängen sämtlich am Swap. Sollte die Messung zeigen, dass
der Modelltausch selbst — nicht der Nachlauf — den Löwenanteil trägt, ist das
neu zu bewerten.

**Debounce nach Reload-Kosten gestaffelt.** Reizvoll, aber mehr Mechanik und
schwerer vorhersagbar als eine feste, an einer Stelle stehende Zahl. Erst wenn
eine Bibliothek auftaucht, bei der 150 ms nicht reichen.

**Mindestlänge vor der ersten Suche.** Würde den teuersten Fall (ein Zeichen
trifft fast alles) sparen, macht das Verhalten aber erklärungsbedürftig. Fällt
weg, weil Teil 2 genau diesen Fall ohnehin entschärft.
