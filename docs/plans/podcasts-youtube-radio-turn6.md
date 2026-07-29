# Implementationskonzept — Turn 6 und 7 (Podcasts, YouTube, Radio, MTP-Sync)

Stand: 2026-07-28 · Branch `feature/podcast-channel-redesign` · Basis
`77b3f54545dadcf44850736d110792fae89ae428`

## 1. Zweck und Quelle

Turn 6 **und Turn 7** des Design-Dokuments `Tourdaten Varianten.dc.html`
(Claude-Design-Projekt `8fb24732-431c-447f-9a74-08d3229a0c33`) sind die
verbindliche Vorlage für den Umbau der drei Online-Quellen.

| Teil | Gegenstand | Issues |
| --- | --- | --- |
| 6a | RSS und YouTube getrennt, Gruppierung nach Show/Kanal | #96, #98 |
| 6b | Kanal-Detail: Fenster, Shorts, Download-Kontrolle, Sync | #106 |
| 6c | Drei getrennte Add-Dialoge, ein Button-Muster | #101, #102, #103 |
| 6d | Sync-Ziele in der MTP-Geräteansicht | Sync |
| 6e | Offline-Konzept | #107 |
| 6f | Leere Ansichten | #98 |
| 7a | Geräteansicht: Speicher nach Kategorie, drei Inhalts-Quellen, Diff | Sync |
| 7b | Settings „Online sources": drei Blöcke auf einer Ebene | #96 |
| 7c | Geräte-Karte in der Sidebar sagt, was sie meint | Sync |
| 7d | Zielordner frei wählbar per Geräte-Browser | Sync |

Turn 7 ist jünger als Turn 6 und **präzisiert 6d**: 7a/7d ersetzen die
Ordner-Skizze aus 6d, 7b ersetzt die Preferences-Skizze aus #96.

Andere Turns des Dokuments (Tourdaten/Concerts, Releases) sind ausdrücklich
nicht Teil dieses Konzepts.

Dieses Dokument ist ein Plan, der seine eigene Ausführung überdauert — spätere
Arbeit muss ihm folgen. Es gehört deshalb nach `docs/plans/` und nicht in den
Sitzungskontext (`AGENTS.md`, Abschnitt „Where we are RIGHT NOW").

`docs/ux-rules.md` bleibt die einzige verbindliche UX-Quelle und steht über
diesem Plan. Wo Turn 6 von einer `[aktiv]`-Regel abweicht, ändert sich die Regel
in demselben Commit, der das Verhalten umsetzt und seinen regelbenannten Test
mitbringt.

## 1b. Rahmenbedingung: keine Rückwärtskompatibilität

Vom Eigentümer am 2026-07-28 bestätigt: Reprise ist **nicht veröffentlicht**,
es gibt **keine bestehenden Installationen**. Migrationen, Kompatibilitäts-
Fallbacks und doppelte Schreibpfade sind damit in dieser gesamten Arbeit kein
Kriterium.

Praktische Folge: wo ein sauberes Datenmodell und ein rückwärtskompatibles
kollidieren, gewinnt das saubere. Ein zurückgelassener zweiter Ort der Wahrheit
ist schlimmer als jede der beiden Varianten für sich. Betrifft unmittelbar G1
(YouTube bekommt einen eigenen `ModuleDescriptor` statt eines verschachtelten
`podcasts.youtube_enabled`) und E1 (benannte Sync-Ziele statt einer Migration
des einzelnen verwalteten Geräteordners).

## 2. Ist-Stand — verifiziert, nicht angenommen

Vor jeder Aufwandsschätzung: ein erheblicher Teil von Turn 6 steht bereits. Die
folgende Bestandsaufnahme wurde am Code geprüft, nicht aus dem Ledger
übernommen.

**Steht bereits:**

- `SRC-5` [aktiv] — RSS und YouTube sind getrennte Library-Orte mit
  show-/kanalgruppierten Quellzeilen, die ihre Episoden aufklappen. Damit ist
  der Kern von **6a** umgesetzt (`podcasts/podcasts_groups.rs`, 454 Zeilen:
  `build_group`, `group_header`, `episode_row`).
- Bereits abonnierte Quellen fallen aus der Suche
  (`podcasts/search_results.rs::filter_unsubscribed`, `source_is_subscribed`
  inklusive stabiler YouTube-Identität über Handle/Channel-Paare).
- **6b** ist überwiegend umgesetzt (`podcasts/youtube_channel_detail.rs`, 599
  Zeilen): `INITIAL_WINDOW = 10`, `EXTENDED_WINDOW = 40`, `set_hide_shorts`,
  `is_short` über `SHORT_MAX_SECONDS = 180`, Mehrfachauswahl (`set_selected`,
  `selected_ids`) und `update_batch_controls`.
- Quellbilder haben eine eigene Fläche (`podcasts/source_image.rs`, 189 Zeilen).
- Geräteauswahl pro Abo existiert (`podcasts/podcasts_device_sync.rs`,
  `selected_for_groups`, Regel `POD-8`).
- Leerzustände sind pro Quelle **klassifiziert**
  (`podcasts_empty_state.rs::podcasts_empty_state_for` mit
  `List/Empty/NoEpisodes/NoResults`, `radio_empty_state.rs` mit
  `List/NoResults/Empty`).

**Fehlt:**

- **6c** vollständig: der Add-Dialog sucht heute **beide** Provider in einem
  Dialog (`add_dialog.rs:253`, `preferred_provider_order` liefert
  `[PodcastKind; 2]`). Es gibt keinen eigenen „Add channel"-Dialog. Der
  Zeilen-Button heißt `PODCAST_SUBSCRIBE` („Subscribe"), Radio hat ein eigenes
  Muster.
- **6f** als Gestalt: die Klassifikation existiert, aber es gibt keine
  gemeinsame Leerzustands-Geometrie, keine der beschriebenen Sätze, keinen
  Zustand „Modul aus", „offline und leer", „nur Shorts" und keine
  Radio-Shortcut-Chips.
- **6e** vollständig: kein einziger `POD-`, `YT-` oder `RAD-`-Regelsatz nennt
  Offline. Siehe #107.
- Aus **6b**: Dateigrößen in der Download-Spalte, Gesamtsumme, „Keep N
  downloaded", Spalte „On phone".
- **6d**: drei getrennte Sync-Ordner, Größen-Cap, „Remove from phone when
  deleted here", Queue-Anzeige in der Geräteansicht.
- Echte Remote-Bilder in den Dialogen (Kanal-Thumbnails, iTunes
  `artworkUrl600`, radio-browser `favicon`).
- Die Settings-Seite „Online sources" und der globale Riegel
  `online-sources-enabled`.

**Teilweise vorhanden — der Sync ist weiter, als es das Design nahelegt:**

- Ein **Podcast-Sync existiert vollständig**, nicht nur als Skizze:
  `device_sync/podcasts.rs` liefert `PodcastSyncCandidate`, `PodcastSyncPlan`
  mit `to_copy`/`to_remove`/`bytes`, `query_candidates_for_device` und
  `build_plan`; verdrahtet ist das über `device_sync_runtime.rs`,
  `device_sync_planned.rs` und `device_sync_compact.rs`. `PodcastSyncSource`
  kennt bereits `Rss` **und** `Youtube`.
- In Betrieb ist davon aber nur RSS: PCR-1 hat den Phone-Sync-Opt-in bewusst
  RSS-only gemacht und ihn für YouTube-Quellen defensiv geleert. Der
  YouTube-Zweig ist also vorbereitet, aber abgeschaltet.
- Es fehlt darum **nicht** der Podcast-Transfer, sondern: benannte Ziele statt
  eines einzigen verwalteten Geräteordners (`78e379fd`), das Freischalten des
  YouTube-Zweigs, Größen-Caps, der Diff nach Kategorie, der Geräte-Browser und
  der Zustand „Device contents never verified".

Das verschiebt den Zuschnitt von Block E spürbar: E2 und E4 bauen auf einer
vorhandenen Planungs- und Transferschicht auf, statt sie neu zu erfinden.

## 3. Was Turn 6 entscheidet

Zwei Entscheidungen kippen frühere Festlegungen und müssen sichtbar bleiben:

1. **Bilder sind v1.** Turn 6c hält ausdrücklich fest: „Entscheidung F6 damit
   gekippt: Bilder sind jetzt v1". Das zieht ein Remote-Bild-Modul nach sich.
2. **Kanal-Listing ist keyless.** `videos.xml?playlist_id=UULF…` (Kanal-ID mit
   `UC` → `UULF`) liefert Long-Form ohne Shorts, 15 Einträge, ohne API-Key.
   yt-dlp bleibt nur für die Tonspur (`-x --audio-format opus`) und für
   „Load more" (`--flat-playlist -I 1:40`).

Weitere tragende Sätze aus Turn 6:

- **Harte Trennung** der Dialoge: „kein gemischtes Ergebnis, keine geteilte
  Suche". Drei Dialoge mit identischer Struktur — Titel, ein Feld,
  Ergebnisliste, Quellen-Fußnote.
- **Ein Button-Muster** überall: derselbe kleine `+ Add`. Bereits abonniert =
  inaktives „✓ Added", und die Quelle fällt aus *späteren* Suchen heraus.
- **Sync-Verantwortung geteilt:** *wo und wie* synchronisiert wird, ist
  Geräte-Konfiguration; *was* mitkommt, entscheidet der Kanal-Toggle.
- **Offline ist ein Zustand, kein Fehler.** Online-Aktionen werden vorgemerkt
  und beim nächsten Netz automatisch ausgeführt. Radio ist die Ausnahme.
- **Leerzustände:** Glyphe, ein Satz *was* hier landet, ein Satz *woher*,
  Primär-Button, darunter der URL-Weg. Keine Filterzeile, kein Zähler, kein
  „0 of 0". Nie eine generische Platzhaltergrafik, nie ein Spinner ohne
  Auftrag, und nie dieselbe Erklärung nach dem ersten Abo weiterzeigen.

### 3b. Was Turn 7 entscheidet

Turn 7 macht den MTP-Sync zur eigenständigen Aufgabe. Er wird **redesignt**,
und der Sync für YouTube-Tonspuren und Podcast-Episoden wird mitgebaut.

**Ordner (löst die frühere offene Frage O-6):**

| Quelle | Zielordner | Cap | Begründung im Design |
| --- | --- | --- | --- |
| Playlists | `/Music/Reprise` | kein Cap | bestehendes Verhalten |
| YouTube-Tonspuren | `/Music/Reprise-YouTube` | 8 GiB, ältestes zuerst | Ordnername ist Androids einzige Sortierhilfe — Tonspuren gehören **nicht** unter `/Music/Reprise` |
| Podcast-Episoden | `/Podcasts/Reprise` | 4 GiB | Androids eigener `/Podcasts`-Ordner; der Media-Scanner erkennt ihn und hält ihn aus der Musikbibliothek heraus |

Die Defaults sind Vorschläge: 7d gibt jeder Quelle einen Geräte-Browser mit
Speicherauswahl, „New folder", Zielvorschau und Warnung, wenn der gewählte
Ordner im Playlist-Ziel liegt. Bereits synchronisierte Dateien bleiben liegen
und werden beim nächsten Sync **verschoben, nicht doppelt kopiert**.

**MTP-Realität, die den Entwurf bindet:**

- MTP kennt keine Pfade. Ordner sind Object-Handles unter einer `StorageID`.
  Persistiert werden `StorageID` + Pfad-String; Handles werden bei jedem
  Reconnect neu aufgelöst, weil sie nicht stabil sind. Deshalb Browser statt
  Texteingabe.
- Größen und Änderungen kommen aus `GetObjectPropList` — ein Roundtrip pro
  Ordner statt Datei-für-Datei-Abfragen.
- Löschen und Kopieren laufen seriell, ein Transfer gleichzeitig; Fortschritt
  aus dem Send-Callback.
- Anlegen via `SendObjectInfo` (Association). Manche Geräte verbieten das im
  Wurzelverzeichnis — dann Fehler zeigen und Unterordner vorschlagen.
- Ein Ordner kann nicht über Speichergrenzen verschoben werden. Wechsel
  bedeutet neu kopieren und das alte Ziel aufräumen.

**Transfer-Profil:** bleibt für Musik („Music · Opus 160 kbit/s", verlustfreies
wird umkodiert, verlustbehaftetes bleibt unangetastet). Podcast- und
YouTube-Audio wird **immer 1:1 kopiert** — es ist bereits Opus oder AAC.

**Zustände, die heute fehlen:** „Device contents never verified" als prüfbarer
Zustand mit Aktion „Scan device"; Speicherbalken nach Kategorie segmentiert
inklusive schraffiertem „Incoming this sync"; Diff nach Kategorie
(`0 new · 3 removed`, `source off`, „Unavailable, kept on phone");
„Sync automatically when this phone connects".

**7b — Settings „Online sources":** eine Seite, drei Blöcke auf einer Ebene, je
Block ein Master-Schalter und höchstens drei Zeilen. Ganz oben ein globaler
Riegel „Use online sources"; aus heißt lokaler Player, keine Requests, keine
Downloads, und die drei Sidebar-Einträge verschwinden. Abos und Favoriten
bleiben erhalten, sie werden nie gelöscht.

Stand nach der Design-Aktualisierung vom 2026-07-28 — **vier** Blöcke in dieser
Reihenfolge. Beachte: „Phone sync" trägt **fünf** Zeilen und bricht damit die
frühere „höchstens drei Zeilen"-Beschreibung; maßgeblich ist diese Tabelle.

| Block | Untertitel | Zeilen |
| --- | --- | --- |
| **Phone sync** | Same rules for every device — folders stay per device | Sync playlists `Selected playlists` · Sync YouTube audio `Marked channels · cap 8 GiB` · Sync podcast episodes `Off` · Music transfer profile `Opus 160 kbit/s` · Target folders `Per device →` |
| YouTube | Channel feeds, audio via yt-dlp | Episodes per channel `Latest 10` · Hide Shorts `On` · `yt-dlp 2026.07.04` mit `Update` |
| Podcasts | RSS feeds, search via Apple Podcasts | Episodes per show `Latest 25` · Download new episodes `Off` · Delete played episodes `After 7 days` |
| Radio | Directory: radio-browser.info | Search order `Most voted` · Report plays to the directory `On` |

Der Untertitel des Phone-sync-Blocks ist der ganze Vertrag in einem Satz: die
**Regeln** gelten für jedes Gerät, die **Ordner** bleiben pro Gerät. Die letzte
Zeile „Target folders · Per device →" ist der Absprung in die Geräteansicht.

Technisch: drei Booleans plus ein globales `online-sources-enabled` als
**UND-Bedingung vor jedem Request** — ausdrücklich auch für Cover, Portraits
und Lyrics, damit „aus" wirklich aus heißt.

**Nachtrag vom 2026-07-28 — Sync-Regeln sind global.** Diese Änderung kam nach
der ersten Fassung von 7a/7b und hat Vorrang vor den Absätzen darüber:

- 7b bekommt einen eigenen Block **„Phone sync"**: Playlisten,
  YouTube-Tonspuren und Podcast-Episoden synchronisieren, Caps und das
  Musik-Transfer-Profil. Diese Regeln gelten **für alle Geräte**; in den
  Settings gibt es **keine** Geräteauswahl. Letzte Zeile des Blocks:
  „Target folders · Per device →".
- 7a zeigt dieselben Werte nur noch **lesend** („rules from Preferences",
  „Same on all devices"). Pro Gerät bleibt allein der Zielordner-Picker.
- **7e** hält das fest: global sind die Regeln, pro Gerät nur Zielordner und
  Sync-Stand — Ordnerstrukturen unterscheiden sich zwischen Handy und DAP.
  Defaults werden automatisch gesetzt, damit der Picker meist unangetastet
  bleibt.

Folge für den Aufgabenschnitt: **E1 wird kleiner** (die Ziele tragen nur noch
Ordner und Sync-Stand pro Gerät, die Regeln liegen global), **G1 wird größer**
(der „Phone sync"-Block gehört in die Settings-Seite). 7b und 7e sind vor
Beginn von Block E vollständig zu lesen; die Absätze oben beschreiben den
überholten geräte-lokalen Entwurf.

**7f — Sync in zwei sichtbaren Phasen.** Löst das Problem, dass ein Sync
Dateien braucht, die noch nicht heruntergeladen sind:

1. **Preparation** — der Sync-Überblick listet „2 files to download · 312 MiB"
   mit Titeln und trägt einen Schalter „Download missing files before syncing"
   (online standardmäßig an). Der Primär-Button heißt dann **Download & sync**,
   sonst **Sync now**.
2. **Transfer** — „Step 1 of 2 · Downloading 1 of 2 · 62%" mit Balken, danach
   die eigentliche Übertragung. Abbrechen behält fertige Downloads.

Der entscheidende Kniff gegen Denkarbeit: „Sync to phone" auf einer Episode
**ohne** Datei setzt `wanted_on_device`, und der Download folgt automatisch.
Niemand muss „erst laden, dann auswählen" durchdenken. Preparation benutzt
denselben Download-Manager mit Vorrang, keinen zweiten Pfad.

Randfälle, die den Vertrag tragen:

- **Offline** läuft der Sync trotzdem: vorhandene Dateien gehen rüber, fehlende
  werden mit Notiz übersprungen („2 episodes skipped · not downloaded") und
  bleiben vorgemerkt. Das ist die Sync-Ausprägung von `NET-3` (Block F).
- Bei **getakteter Verbindung** wird Preparation angeboten, nicht gestartet.
- Mit **abgeschalteten Online-Quellen** (`online-sources-enabled` aus, G1b)
  gibt es die Phase gar nicht.

`wanted_on_device` ist neuer persistenter Zustand und gehört damit in E1, nicht
in die Darstellung.

**7c — Geräte-Karte:** nennt die Richtung statt einer nichtssagenden Bilanz.
Vier Zustände: „14 to copy · 2.6 GiB · 3 to remove", „3 to remove · frees
148 MiB" (hier ist 0 B korrekt und darf nicht nach „nichts zu tun" aussehen),
„Up to date · synced 12 min ago", „Tap to scan device contents". Die volle
Bilanz trägt der Tooltip; während des Syncs ersetzt eine dünne Fortschrittslinie
am Kartenboden den Text.

## 4. Auswirkungen auf `docs/ux-rules.md`

Regel-IDs sind append-only; ersetzte Regeln bleiben als
`[ersetzt durch <ID>]` stehen und ihre Tests werden im selben Commit
umgehängt.

| Regel | Änderung |
| --- | --- |
| `SRC-2` | Bleibt für die Toolbar. Der neue kompakte `+ Add` in Ergebniszeilen ist eine **andere** Fläche und braucht eine eigene Regel — `SRC-2` gilt weiter für den Toolbar-Button. |
| `SRC-3` | „Jede Quelle besitzt genau einen Add-Dialog" bleibt wahr und wird durch die harte Trennung sogar strenger. Der Satz „Suche liefert gruppierte Ergebnisse" muss auf einen Provider eingeschränkt werden → `[ersetzt durch SRC-3a]`. |
| `SRC-5` | Der Teil „gruppieren YouTube-Treffer kanalweise und blenden bereits abonnierte … aus" bleibt. Der Code-Test `src_5_search_orders_the_calling_library_source_first` prüft die alte Zwei-Provider-Reihenfolge und muss ersetzt werden. |
| **neu `SRC-6`** | Harte Provider-Trennung der Add-Dialoge. |
| **neu `SRC-7`** | Einheitliches `+ Add` / „✓ Added"-Zeilenmuster inklusive barrierefreiem Namen. |
| **neu `SRC-8`** | Gemeinsame Leerzustands-Grammatik für alle drei Quellen (6f). |
| **neu `NET-3`** | App-weiter Offline-Präsentationsvertrag (6e, #107) — konsolidiert `NR-6`, `NR-8`, `CONC-4b`, `LYR-3`, `INST-12`. |
| `POD-5` | Cleanup-Policy trifft auf „Keep N downloaded" aus 6b — Verhältnis muss geklärt werden, siehe offene Frage O-5. |
| `NET-1` | Wird durch 7b **erweitert**: über die vier heutigen Netzmodule hinaus kommt ein globaler Riegel `online-sources-enabled` als UND-Bedingung vor jedem Request, auch für Cover, Portraits und Lyrics → `[ersetzt durch NET-1a]`. Remote-Bilder (Block C) fallen ebenfalls darunter. |
| **neu `SET-*`** | Settings-Seite „Online sources": drei gleichrangige Blöcke, je Master-Schalter und höchstens drei Zeilen; ein ausgeschalteter Block versteckt seinen Sidebar-Eintrag, stoppt Requests und löscht nichts (7b). |
| **neu `MTP-*`** | Drei benannte Sync-Ziele mit `StorageID` + Pfad, frei wählbar per Geräte-Browser, Cap je Ziel, Diff nach Kategorie, „Device contents never verified" als prüfbarer Zustand (7a, 7c, 7d). Bestehende `MTP-`-Regeln sind vorher auf Kollisionen zu prüfen. |

## 5. Aufgabenschnitt

Jede Aufgabe ist ein Commit, test-first, mit vollständiger Gate-Batterie
(`cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`, Core-Purity, Dateigröße < 800 Zeilen)
und einer Zeile im Ledger `.superpowers/sdd/progress.md`.

### Block A — Dialoge (6c) · Issues #101, #102, #103, #99

- **A1 · Harte Provider-Trennung.** `preferred_provider_order` →
  `dialog_provider(kind) -> PodcastKind`. `add_dialog::search` sucht nur den
  eigenen Provider. Dialogtitel und Platzhalter je Quelle: „Add podcast" /
  „Search by name or paste a feed URL" gegen „Add channel" / „Search or paste a
  channel URL". Neue Regel `SRC-6`, Test
  `src_6_each_dialog_searches_only_its_own_provider`. Ersetzt
  `src_5_search_orders_the_calling_library_source_first`.
  *Offen:* was eine quellfremde URL im Dialog tut — siehe O-1.
- **A2 · Einheitliches Zeilen-Muster.** `+ Add` in allen drei Dialogen,
  „✓ Added" als inaktiver Zustand, Fußnote „Subscribed channels drop out of
  later searches." mit „Show". Übersetzter barrierefreier Name je Aktion
  (`Subscribe to {channel}` / `Add {station}`), Tastaturreihenfolge,
  Enter/Space, Fokusübergabe nach Erfolg, wiederholbare Fehlermeldung. Neue
  Regel `SRC-7`. Schlägt #102.
- **A3 · Dialog-Layout entklemmen.** Nur vertikales Scrollen, fixe Kopf- und
  Fußzeile, Artwork und Aktion behalten ihre Größe, Titel/Untertitel
  ellipsieren, Endabstand zur Overlay-Scrollbar, letzte Zeile scrollt
  vollständig über die Fußzeile. Test bei normaler, schmaler und niedriger
  Zuteilung. Schlägt #101.
- **A4 · Radio-Ergebnisse in einen begrenzten Scroller.** Radio hängt seine
  `GtkListBox` heute direkt in den vertikalen Dialog; Suche und
  Ergebniszusammenfassung bleiben fest, nur die Ergebnisse scrollen. Schlägt
  #99.
- **A5 · Abonnentenzahlen.** `1.2M subscribers · audio only` in
  Kanal-Suchergebnissen und URL-Vorschau. Null/verborgen/fehlerhaft wird
  weggelassen — nie eine erfundene Null, nie „unknown". Ein begrenzter
  yt-dlp-Prozess, kein N+1. Schlägt #103.

### Block B — Leerzustände (6f) · Issue #98

- **B1 · Gemeinsame Leerzustands-Geometrie.** Ein Modul, drei Quellen erben:
  Glyphe des Sidebar-Eintrags, Titel, ein Satz *was*, ein Satz *woher*, genau
  ein Primär-Button, darunter der Zweitpfad. Keine Toolbar, keine Filterzeile,
  kein Zähler im echten Leerzustand. Neue Regel `SRC-8`. Die bestehenden
  Klassifikationen aus `podcasts_empty_state.rs` und `radio_empty_state.rs`
  werden auf die gemeinsame Fläche gehoben, nicht ersetzt.
- **B2 · Weitere Leerzustände.** „Nothing matches these filters" mit
  „Clear filters" und sichtbarer Filterzeile, „Only Shorts here — show them
  anyway?", „Nothing downloaded yet …", „YouTube is turned off" mit
  „Enable in Preferences" statt Add-Button.
- **B3 · Radio-Shortcut-Chips.** „Metal in DE", „Top voted", „Near you" als
  Ein-Klick-Suchen. *Offen:* Herkunft von „Near you" — siehe O-4.

### Block C — Remote-Bilder

- **C1 · Bild-Modul.** Kanal-`thumbnails`, iTunes `artworkUrl600`,
  radio-browser `favicon`; Cache, begrenzte Größe, Fallback auf die
  Quellglyphe, kein Abruf ohne die von `NET-1` verlangte Zustimmung.
  Blockiert die Bild-Anteile von A2 und B1, weshalb diese zunächst mit der
  bestehenden `source_image`-Fläche und Glyphen-Fallback ausgeliefert werden.

### Block D — Kanal-Detail vervollständigen (6b) · Issue #106

- **D1 · Download-Spalte mit Dateigrößen** und Kopfzeilen-Summe
  („10 of 487 · 3 downloaded · 1.2 GB").
- **D2 · „Keep N downloaded"** als Kanal-Eigenschaft, abgestimmt mit der
  bestehenden Cleanup-Policy aus `POD-5` (O-5).
- **D3 · Spalte „On phone"** — Spiegel des Sync-Zustands, schreibend nur über
  den Kanal-Toggle.
- **D4 · Download-Fehler klassifiziert und sanitisiert.** Sichtbarer,
  ohne Zeiger erreichbarer Grund; sauberer Retry durch
  queued/downloading/downloaded; frische Provider-Ausgabe beim Retry; Aufräumen
  von `.part` und Postprozessor-Resten; keine signierten URLs, Query-Strings,
  Zugangsdaten oder lokalen Pfade in UI oder Normal-Logs. Schlägt #106.

### Block E — MTP-Redesign und Sync für YouTube und Podcasts (6d, 7a, 7c, 7d)

Der größte Block. Der bestehende MTP-Sync deckt heute nur Playlists ab; hier
kommen zwei Inhaltsarten und eine neue Geräteansicht dazu.

- **E1 · Sync-Ziel-Modell im Core.** Aus einem einzigen verwalteten Geräteordner
  werden drei benannte Ziele mit je `StorageID`, Pfad-String, Aktivierung und
  optionalem Größen-Cap. Reine Datenschicht. **Keine Migration** — siehe
  Abschnitt 1b; das alte Einzelziel wird ersetzt, nicht überführt.
  Einheitentests für Auflösung und Cap-Berechnung; kein UI.
- **E2 · Inhaltsauswahl je Quelle.** Playlists („2 of 4 selected"), YouTube
  („2 of 6 channels · latest 5 each") und Podcasts („Unplayed downloads only")
  liefern jeweils die Sollmenge an Dateien. Der Kanal-Toggle aus 6b und die
  Show-Auswahl speisen genau hier ein.
- **E3 · Diff nach Kategorie.** Der Sync-Plan wird pro Quelle aufgeschlüsselt
  (`0 new · 3 removed`, `source off`, „Unavailable, kept on phone") und liefert
  die Bilanz „To copy / To remove / Playlists rewritten". Ausdrücklich
  test-getrieben, weil 7c genau hier heute lügt.
- **E4 · Transfer.** Musik folgt weiter dem Transfer-Profil; Podcast- und
  YouTube-Audio wird 1:1 kopiert. Cap-Durchsetzung „ältestes zuerst" beim
  Überschreiten. Serieller Transfer, Fortschritt aus dem Send-Callback.
- **E5 · Geräteansicht (7a).** Segmentierter Speicherbalken mit
  „Incoming this sync", „Device contents never verified" als prüfbarer Zustand
  mit „Scan device", Sektion „Content" mit Zielordner, Auswahl und Cap je
  Quelle, „Next synchronization" mit Diff und Bilanz, „Remove from phone when
  deleted or unsubscribed here", „Sync automatically when this phone connects",
  „Eject". **Nachtrag:** der Schalter wurde hier zunächst nur spezifiziert und
  gerendert, aber von keinem Code gelesen — ein toter Schalter. Sein Verhalten
  (automatischer Sync-Start nach verifiziertem Scan und geplanter Arbeit,
  still bei Ablehnung/Fehler) ist jetzt implementiert (`MTP-30`).
- **E6 · Zielordner-Browser (7d).** Speicherauswahl intern/SD, Baum aus
  `GetObjectPropList`, „New folder" via `SendObjectInfo`, Zielvorschau, Warnung
  bei einem Ordner innerhalb des Playlist-Ziels, „Reset to default". Bereits
  synchronisierte Dateien werden beim nächsten Sync verschoben statt doppelt
  kopiert. Fehlerpfad für Geräte, die das Anlegen im Wurzelverzeichnis
  verbieten.
- **E7 · Sidebar-Geräte-Karte (7c).** Vier Zustände mit Richtungsangabe, volle
  Bilanz nur im Tooltip, Fortschrittslinie während des Syncs.

### Block F — Offline (6e) · Issue #107

- **F1 · Vertrag `NET-3`** für cached, empty, queued, interrupted,
  authentication, rate-limit, provider-failure; Migration von `NR-6`, `NR-8`,
  `CONC-4b`, `LYR-3`, `INST-12` auf den gemeinsamen Vertrag.
- **F2 · Vormerken statt Ausgrauen.** Download- und Sync-Aktionen werden
  angenommen, als „Queued offline" geführt und bei Netz automatisch der Reihe
  nach ausgeführt.
- **F3 · Radio-Ausnahme.** Sender bleiben gelistet, Play meldet
  „No connection · Retry" statt vorzumerken.
- **F4 · Add-Dialoge offline.** Suchfeld deaktiviert mit einzeiliger
  Begründung; das Einfügen einer URL funktioniert weiter und das Abo entsteht
  beim nächsten Abruf.

### Block G — Hierarchie und Gruppierung (6a) · Issue #96

- **G1 · Settings-Seite „Online sources" (7b).** Eine Seite, drei Blöcke auf
  einer Ebene, je Master-Schalter und höchstens drei Zeilen, mit dem exakten
  Zeilensatz aus Abschnitt 3b. Ein ausgeschalteter Block versteckt seinen
  Sidebar-Eintrag und stoppt seine Requests, löscht aber weder Abos noch
  Favoriten.
- **G1b · Globaler Riegel `online-sources-enabled`.** UND-Bedingung vor jedem
  Request, ausdrücklich auch für Cover, Portraits und Lyrics. Das erweitert
  `NET-1` über die vier heutigen Netzmodule hinaus und braucht einen eigenen
  Test je Aufrufpfad, sonst ist „aus" nicht beweisbar.
- **G2 · Restabgleich 6a.** Spaltensatz `Show · Latest · Episodes`, „Show all N
  episodes", Kopfzeile „4 shows · 41 episodes · 7 new" gegen den bestehenden
  Gruppen-Renderer prüfen und nur die Lücke schließen.

### Block H — MCP-Parität (Querschnitt)

Anforderung des Eigentümers: **alles, was die GUI kann, muss auch über MCP
verfügbar sein.** Der heutige Stand deckt das nicht ab — `source_tools.rs`
kennt nur `music_manage_podcasts` und `music_manage_radio` (add/edit/remove/
refresh) plus die gecachten Resources `reprise://podcasts` und
`reprise://radio`. Es gibt **keine** Discovery-, Download-, Sync- oder
Settings-Oberfläche.

Jeder Block bekommt deshalb seine MCP-Aufgabe; sie folgt der jeweiligen
GUI-Aufgabe, damit beide dieselbe Kernfunktion benutzen statt zwei Pfade zu
bauen:

- **H-A** · Discovery: ein quellgetrenntes Such-Tool (`SRC-6`) mit den
  Kandidatenfeldern inklusive optionaler Abonnentenzahl (`SRC-9`). Netz- und
  Subprozessarbeit ist capability-gated wie die Mutationen; „bereits abonniert"
  wird genauso gefiltert wie in der GUI.
- **H-B** · Leerzustände sind reine Darstellung und brauchen kein Tool; der
  Zustand ist aus den vorhandenen Resources ableitbar.
- **H-D** · Kanal-Detail: Fenster, Shorts-Filter, Download-Zustände mit
  Größen und die Batch-Aktionen.
- **H-E** · Sync: Zielordner, Caps, Diff nach Kategorie, `wanted_on_device`
  und das Auslösen eines Syncs.
- **H-F** · Offline: der Zustand aus `NET-3` muss über MCP ablesbar sein,
  damit ein Agent nicht blind in eine vorgemerkte Aktion läuft.
- **H-G** · Settings: die drei Master-Schalter, der globale Riegel und der
  „Phone sync"-Block.

Grundregeln für alle: keine signierten URLs, Zugangsdaten oder lokalen Pfade
in Antworten. **Query-Strings sind ausgenommen** (Eigentümer-Entscheidung vom
2026-07-28): `SRC-5` beweist mit eigenem Test, dass der Query-String Teil der
Identität eines Feeds sein kann — ihn pauschal zu strippen ließe ein
nachfolgendes `add` auf den falschen Feed zeigen. Statt dessen werden Userinfo
und Fragment entfernt, Nicht-HTTP(S)-Schemata abgelehnt und Artwork-URLs ganz
weggelassen. Weiter gilt: Mutationen und Netzzugriff hinter Capabilities;
dieselbe Kernfassade wie die GUI, damit Verhalten nicht auseinanderläuft.

## 6. Reihenfolge

```
A1 → A2 → A3 → A4 → A5        Dialoge zuerst: abgeschlossen, kein Datenmodell
        ↘                     Bild-Anteile warten auf C1
G1 → G1b                      Settings + globaler Riegel: Voraussetzung für
                              den Zustand „Modul aus" in B2
B1 → B2 → B3                  Leerzustände; B2 braucht G1 und F1
C1                            entblockt die Bild-Anteile von A2 und B1
F1 → F2 → F3 → F4             Offline-Vertrag, bevor D und E ihn erben
D1 → D2 → D3 → D4             Kanal-Detail vervollständigen
E1 → E2 → E3 → E4             MTP-Kern: Ziele, Auswahl, Diff, Transfer
        ↘ E5 → E6 → E7        Geräteansicht, Ordner-Browser, Sidebar-Karte
G2                            Restabgleich 6a
```

Begründung der Spitze: Block A ist in sich abgeschlossen, hängt an keinem
Datenmodell-Umbau und schlägt #101, #102, #103 und #99 auf einmal. Block F steht
bewusst **vor** D und E, damit diese den Offline-Vertrag erben statt ihn später
nachgerüstet zu bekommen — genau der Fehler, den die Stage-Review-Notiz PCR
benennt.

Zwei Abhängigkeiten sind hart:

- B2 („Offline & leer") setzt F1 voraus.
- Die Bild-Anteile von A2 und B1 setzen C1 voraus. Beide werden deshalb
  zunächst mit Glyphen-Fallback ausgeliefert und in C1 nachgezogen; das ist
  kein Nacharbeiten, sondern der von Turn 6f geforderte Fallback.

## 7. Verifikation

Drei Ebenen, keine ersetzt eine andere:

1. **Reine Einheitstests** für jede Projektion — Provider-Auswahl,
   Leerzustands-Klassifikation, Offline-Zustandsableitung, Größenformatierung.
   Das ist die Ebene, auf der `search_results.rs` und `podcasts_empty_state.rs`
   heute schon liegen.
2. **Isolierte Display-Tests** für Widget-Aufbau und Zuteilung, unter Xvfb, mit
   privatem `XDG_DATA_HOME`/`XDG_CACHE_HOME`, privatem D-Bus und
   `REPRISE_AUDIO_SINK=fakesink`.
3. **CUA-Szenarien** unter `scripts/cua-e2e/` für die sichtbaren Zustände. Die
   Harness ist arbeitsfähig — ein vollständiger `responsive-window`-Lauf
   erzeugte 213 Evidence-Dateien mit AT-SPI-Bäumen und Screenshots. Neue
   Szenarien folgen der Invariante: frischer `get_window_state` vor jeder
   Aktion und ein weiterer danach.

Für #104 und #106 gilt weiter die Grenze aus der Issue-Ablage: Ursachen erst
nach einem roten, deterministischen Fake-Provider-Lauf benennen. Live-YouTube
ist kein Regressionsgatter.

Eine Vorbedingung: das Szenario `responsive-window` schlägt derzeit fehl
(#108). Die Evidenz deutet auf eine Wettlaufsituation in der Harness — zwei
Aktionen ohne Beruhigungs-Assertion zwischen `responsive_window.sh:189-190` —
nicht auf einen Produktfehler. Das sollte vor dem ersten neuen CUA-Szenario
repariert sein, sonst erbt jedes neue Szenario dasselbe Muster.

## 7b. Was nur ein Mensch abnehmen kann

Die automatischen Gatter beweisen Projektionen, Widget-Aufbau und
Zustandsübergänge. Sie beweisen **nicht**, wie etwas aussieht, sich anfühlt oder
sich an echter Fremdhardware verhält. `AGENTS.md` reserviert diese Kategorie
ausdrücklich für Rendering, Zeigergesten, Medientasten, Wayland-Verhalten und
den Sperrbildschirm. Was aus dieser Arbeit dazugehört:

- **Add-Dialoge (Block A).** Wirkt der kompakte `+ Add` als Zeilenaktion
  richtig gewichtet neben der Fußleiste? Ist das quittierte „Added" erkennbar
  erledigt, ohne wie ein Fehler auszusehen? Liest sich der einzeilige Hinweis
  bei quellfremder URL als Erklärung statt als Zurückweisung?
- **Leerzustände (B1).** Sehen die drei Flächen „ungenutzt" aus statt „kaputt"
  — das ist die eigentliche Zusage von 6f und kein Test kann sie prüfen.
- **Kanal-Detail (D1).** Sind Download-Spalte und Kopfsumme bei langen
  Titeln und schmalem Fenster noch lesbar?
- **Online sources (G1).** Der wichtigste Punkt: Schalter aus, dann durch
  Podcasts, YouTube, Radio, Cover, Porträts und Lyrics gehen — passiert
  wirklich nichts im Netz? Die Zusage lautet „no requests, no downloads,
  nothing hidden".
- **MTP-Sync (Block E).** Ein echter Gerätelauf mit angestecktem Telefon. Das
  Testdoppel beweist die Reihenfolge und die Buchführung, aber weder
  Handle-Auflösung nach Reconnect noch das Verhalten des Android-Media-Scanners
  mit den drei Zielordnern.
- **Offline (Block F).** Netz trennen und prüfen, ob heruntergeladene Episoden
  unverändert spielen, nicht heruntergeladene gedimmt „Needs network" lesen und
  Radio „No connection · Retry" statt einer Warteschlange anbietet.

## 8a. Entschieden

Vom Eigentümer am 2026-07-28 entschieden; diese Punkte sind keine offenen
Fragen mehr und gehen so in die Regeln `SRC-6` und `SRC-7` ein.

- **E-1 (vormals O-1) · Quellfremde URL wird abgelehnt.** Eine YouTube-URL in
  „Add podcast" — und umgekehrt eine Feed-URL in „Add channel" — wird nicht
  ausgewertet. Der Dialog meldet einzeilig, dass die Quelle zum anderen Ort
  gehört, und legt nichts an. Kein stiller Dialogwechsel. Damit gilt die harte
  Trennung aus 6c für Suche **und** URL-Weg.
- **E-2 (vormals O-2) · `+ Add` trägt Icon und Text.** Turn 6c ist die jüngere
  Quelle und gewinnt gegen die Icon-only-Formulierung in #102; #102 wird
  entsprechend nachgezogen. Der barrierefreie Name bleibt trotzdem
  verpflichtend, weil das Label allein die Quelle nicht benennt
  (`Subscribe to {channel}` / `Add {station}`).
- **E-4 (vormals O-6) · Ordner sind entschieden und frei wählbar.** Turn 7
  ersetzt die Skizze aus 6d: `/Music/Reprise` für Playlists,
  `/Music/Reprise-YouTube` für Tonspuren und `/Podcasts/Reprise` für Episoden,
  jeweils als *Vorschlag*, den der Geräte-Browser aus 7d überschreiben kann.
  Der Podcast-Ordner liegt bewusst nicht unter `/Music`, weil Androids
  Media-Scanner `/Podcasts` erkennt und aus der Musikbibliothek heraushält;
  Tonspuren liegen aus demselben Grund nicht unter `/Music/Reprise`. Der
  bestehende verwaltete Geräteordner (`78e379fd`) wird auf das Ziel
  „Playlists" migriert (Aufgabe E1).
- **E-3 (vormals O-3) · „Später" heißt: ab der nächsten abgeschickten Suche.**
  Eine gerade hinzugefügte Quelle bleibt in der aktuellen Trefferliste als
  inaktives „✓ Added" stehen, damit der Erfolg sichtbar ist. Erst die nächste
  abgeschickte Suchanfrage filtert sie heraus. Das heutige sofortige Entfernen
  der Zeile (`remove_candidate_result`) entfällt und sein Test
  `src_5_successful_subscribe_removes_the_result_row` wird auf den neuen
  Zustand umgehängt.

- **E-5 · Genau ein MTP-Gerät.** Reprise unterstützt ein verbundenes Gerät, nicht
  mehrere. Turn 7e (mehrere Geräte) entfällt ersatzlos. Begründung des
  Eigentümers am 2026-07-29: zu viel Komplexität für einen zu seltenen Fall.
  Mehrgeräte-Betrieb kostet kein einzelnes Feature, sondern zieht die Frage
  „für welches Gerät gilt das?" in jede Regel, jede Einstellungszeile und jede
  Statusanzeige.

  **Das Datenmodell bleibt trotzdem gerätebezogen**: `device_settings` nach
  Seriennummer und drei `SyncTarget` je Gerät sind gebaut, getestet und
  kostenlos, und der Grund aus `E-4` gilt weiter — Ordnerstrukturen
  unterscheiden sich zwischen Handy und DAP. Gespeichert wird pro Gerät;
  *verwaltet* werden nicht mehrere. Kein Rückbau am Modell.

- **E-6 · Sync-Regeln leben auf der Geräteseite.** Ersetzt den Nachtrag vom
  2026-07-28, dessen Global-vs-pro-Gerät-Zuschnitt ausschließlich mit mehreren
  Geräten begründet war („gilt für alle Geräte, keine Geräte-Auswahl in den
  Settings"). Mit `E-5` entfällt diese Begründung, und mit ihr der Querverweis:
  die Geräteansicht zeigt heute „rules from Preferences" und „Same on all
  devices" und verweist damit auf einen Block, den es nicht gibt — bei einem
  Gerät ist „Same on all devices" eine Aussage über nichts.

  Also: Regeln (Playlists/YouTube/Podcasts, Caps, Transfer-Profil) wandern
  dorthin, wo das Gerät ist. Die Einstellungen behalten allein den
  Datenschutz-Riegel „Online sources" (`NET-1a`, `SET-8`). Der geplante
  „Phone sync"-Block in 7b entfällt als eigene Fläche. `MTP-28` hält die
  aufgehobene Trennung als `[aktiv]` bindend fest und wird daher regulär über
  `[ersetzt durch …]` abgelöst, nicht stillschweigend umgedeutet.

## 8. Offene Fragen — nicht lokal entscheiden

Nach `AGENTS.md` werden diese als `[geplant]`-Entwurf mit
`<!-- REVIEW: Regelvorschlag -->` eingetragen, nicht im Code beantwortet.

- **O-4** „Near you" als Radio-Shortcut braucht eine Standortquelle. Das ist
  eine Datenschutzentscheidung und fällt unter `NET-1`.
- **O-5** „Keep N downloaded" gegen die bestehende Cleanup-Policy aus `POD-5`:
  ersetzt die Kanal-Eigenschaft die globale Policy, überschreibt sie sie, oder
  gilt das Minimum?
- **O-7** OPML-Import. Turn 6f nennt „or import an OPML file" als Zweitpfad im
  Podcast-Leerzustand. Ein Import-Pfad dafür ist im Code nicht belegt — der
  Leerzustand darf nichts versprechen, was es nicht gibt.

## 9. Ausdrücklich nicht enthalten

- Alle anderen Turns des Design-Dokuments.
- #100 (Auswahl gegen Now-Playing-Hervorhebung) — eigenständig, quellenunabhängig.
- #104 und #105 (Streaming-Stalls, gepufferter Bereich in der Wellenform) —
  Playback, nicht Quellenverwaltung.
- #97 und #108 (responsive Flanken, CUA-Szenario) — nur als Vorbedingung für
  die CUA-Abdeckung in Abschnitt 7 relevant.
