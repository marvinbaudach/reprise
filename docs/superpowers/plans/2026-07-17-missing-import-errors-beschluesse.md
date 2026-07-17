# Missing files / Import errors — Beschlussdokument (Grilling 2026-07-17)

Normativer Kontext für den Umbau von „Missing files" und „Import errors" von statischen
Fehler-Logs (Rhythmbox-Erbe) zu **selbstheilenden Zustandslisten**, Design 18a. Alle
Beschlüsse wurden am 2026-07-17 gegrillt und vom Maintainer bestätigt.

> **Verhältnis zu `docs/ux-rules.md`:** Das Regelwerk ist die einzige UX-Wahrheitsquelle
> und schlägt dieses Dokument bei Konflikt. Die hier gegrillten Entscheidungen sind dort
> bereits als Regeln kodifiziert (FB-4, FB-5, FB-6, FB-7, SET-4, PLAY-4a/4b, PLAY-5a/5b,
> P-6). Dieses Dokument liefert das **Warum** und die Mechanik unterhalb der Regelebene —
> es begründet, es normiert nicht neu. Der Rules-Branch ist vollständig in `main`; Flips
> `[geplant] → [aktiv]` passieren direkt in `docs/ux-rules.md`, im implementierenden Commit
> (Präzedenz: `80c7f4e`).

## Das Grundproblem

Rhythmbox' Fehlerlisten wachsen und werden nie geleert: Duplikate bei jedem Scan, keine
Unterscheidung zwischen „Platte weg" und „Datei gelöscht", kein Weg zurück für einen
verschobenen Track. Der Umbau dreht das um: **die App heilt sich, wo sie Beweise hat, und
schweigt, wo sie keine hat.** Jeder destruktive Schritt ist an Evidenz und Widerrufbarkeit
gebunden.

## Die 13 Beschlüsse

### 1. Datenmodell: eine Wahrheit
`missing_since TIMESTAMP NULL` (NULL = vorhanden) **ersetzt** das alte `missing`-Flag; dazu
`missing_reason` (`unmounted`/`deleted`/`unknown`). Das Prädikat lebt zentral in
`queries/clauses.rs` (`PRESENT`/`MISSING`) — nie eine handgeschriebene Kopie.

**Warum:** Ein Flag plus ein Datum sind zwei Wahrheiten für einen Zustand und laufen
auseinander. Der Auto-Clean löscht Library-Zeilen anhand dieses Datums — eine Zeile mit
unklarem Fristbeginn ist dort inakzeptabel. Bestandszeilen der Migration bekommen
`unknown`, **nie** `deleted`: der Mount-Check existierte in v1 nicht, es gibt also keinen
Beweis. Sie dürfen nie ungeprüft als löschbar gelten.

*Status: implementiert (Schema v10/v11).*

### 2. Erkennung: `st_dev`-Vergleich, kein Trait
Datei weg? Vom Pfad zum nächsten **existierenden** Vorfahren hochlaufen (`lstat`, Symlinks
nicht folgen, Deckel bei `/`) und dessen `st_dev` mit dem gespeicherten `tracks.device`
vergleichen. Mismatch → `unmounted`. Gleich → `deleted`. `device IS NULL` → `unknown`.

**Warum kein Trait, kein `/proc/mounts`:** `reprise-core` darf nie von gtk/gstreamer/zbus
abhängen (hartes Gate), GVolumeMonitor lebt also nur in der GTK-Schicht. `/proc/mounts`
wäre Linux-only und bräuchte einen Plattform-Contract. Der `st_dev`-Vergleich braucht
beides nicht — zwei `stat`-Aufrufe und eine Spalte, die seit v2 existiert. Entscheidend:
**er ist ohne root testbar** (Test fälscht `tracks.device`, kein Mount nötig). `dev_t` ist
über Remounts nicht stabil — akzeptiert: `unmounted` heißt „wahrscheinlich nicht da", ist
nie Lösch-Grundlage.

*Status: implementiert (`library/mounts.rs`).*

### 3. Mount-Label: aufgezeichnet, nicht abgeleitet
`tracks.mount_point`, berechnet zur Scan-Zeit (Walk nach oben, solange `st_dev` gleich
bleibt), memoisiert pro Ordner, **beim Move mit-geupdatet**. N unmounted Platten = N Cards.
NULL → Gruppe „unknown location", actionless, Text „will be verified on next scan" (nicht
„returns automatically" — das Versprechen kann sie nicht halten).

**Warum:** Aus `/media/nas/Rock/x.flac` ist nicht ableitbar, ob der Mount `/media/nas` oder
`/media` ist — und genau wenn man es braucht (Platte weg), hat `/proc/mounts` keinen
Eintrag mehr. Der Mount-Point ist nur bekannt, **solange die Platte da ist**. Bekannte
Grenze (dokumentiert, nicht gefixt): btrfs-Subvolumes/Bind-Mounts können einen zu hohen
Vorfahren liefern — die Gruppierung bleibt korrekt genug, sie gruppiert, was zusammen
verschwindet.

*Status: implementiert.*

### 4. Scan = Reconcile, mit Root-Guard
Vanish-Markierung ist **in `scan_folder` gefaltet** — walk + upsert + markieren +
klassifizieren in **einer** Transaktion. `ScanOutcome::{Completed, RootUnavailable}`.

**Warum gefaltet:** Die Reihenfolge (erst scannen, dann markieren) war load-bearing und
brauchte drei Absätze Doku — eine Regel, die jeder Aufrufer erinnern muss, gehört in die
Struktur. Nach der Faltung gibt es nichts mehr falsch herum aufzurufen. Vorher rief nur der
Watcher sie; manuelle Scans erkannten Missing gar nicht.

**Root-Guard:** Ist der Scan-Root selbst unavailable, bricht der Scan früh ab und markiert
**nichts**. Ein Scan, der nichts sehen kann, hat keine Beweise über 10.000 Einzeldateien —
er weiß nur „mein Root ist weg", und genau das meldet er. Sonst beginnt jeder Morgen mit
schlafender NAS bei leerer Library, während die Wahrheit „Ordner nicht erreichbar" ist. Gilt
**nur für den Root**; Root da, Unterordner-Mount weg → normales Marking (ehrlicher
Teilausfall, 18a zeigt ihn als unavailable-Card).

*Status: implementiert.*

### 5. Fehlerklassifikation an der Quelle
`ImportErrorKind` (`UnreadableTags`, `PermissionDenied`, `UnsupportedFormat`, `Io`,
`Unknown`) wird aus `lofty::error::ErrorKind` gemappt, `Io(e)` heruntergebrochen auf
`e.kind()`. `reason_detail` ist reine Anzeige-Payload, nie ausgewertet. `Unknown` loggt
warn mit Originaltext.

**Warum nicht aus dem Fehlertext parsen:** lofty-Meldungen sind kein API-Vertrag. Ein Bump
formuliert um, alles fällt still in `Unknown`, kein Test wird rot (keiner kennt die
Fremdtext-Konstante). Und `PermissionDenied` ist über Strings gar nicht sauber zu kriegen —
lofty reicht `EACCES` als `ErrorKind::Io` durch. Das Auffangbecken ist nur eins, wenn
jemand sieht, was reinfällt.

Ordner-Traversal-Fehler: **eine** Zeile für den Ordner (Pfadspalte = Ordnerpfad), nicht N
für nie gesehene Dateien darunter.

*Status: implementiert (`library/import_errors.rs`).*

> **Befund gegen die ursprüngliche Aufgabenstellung:** Der behauptete Bug „Directory-Fehler
> zeigen den Fehler-Index statt des Pfads (‚1' in der Pfadspalte)" **existiert im Code
> nicht** — `err.path()` war immer korrekt gebunden. Vermutlich eine Fehldeutung der alten
> UI-Anzeige. Nichts zu fixen.

### 6. Untagged-Import: verdient, nicht blind
Scheitert Pass 1 (`read_from_path`), läuft Pass 2 mit
`ParseOptions::new().read_tags(false).parsing_mode(Relaxed)`. Kommt er durch, ist der
**Container** parsebar → Import mit **echter Dauer/Bitrate**, Titel = Dateistamm, Album =
Ordnername, `tracks.untagged = 1`. Scheitert auch Pass 2 → nur Fehlerzeile, `reason_kind`
aus dem **Pass-2**-Fehler.

**Warum kein Playbar-Test:** Core hat keinen Decoder (nur GStreamer in der Plattform-Crate
hinter `WaveformBackend`); ein Decode-Test hieße Plattform-Contract plus Pipeline-Start pro
kaputter Datei. **Warum nicht blind importieren:** Scheitert Pass 1, gibt es auch keine
`properties` — der Track landete mit **Dauer 0** in der Library und vergiftete
Smart-Playlists, Stats, Queue-Restdauer und ausgerechnet den Fingerprint-Relink
(`ABS(duration_ms - ?) <= 2000`). Eine Zeile „0:00" sieht aus wie ein Bug, weil sie einer
ist.

**Koexistenz-Regel:** Ein Pfad hat dann `tracks`- **und** `import_errors`-Zeile. Die
Fehlerzeile bleibt als **Hinweis** („imported without metadata"), nicht als Fehler. Die
Selbstheilungsregel schärft sich dadurch: gelöscht wird die Zeile, wenn **die Tags wieder
lesbar sind**, nicht wenn der Import gelingt. Hinweise zählen **nie** ins Badge — die App
bittet um Tags, nicht um Hilfe. Hinweis ist ableitbar, **ohne** `is_hint`-Spalte:
`EXISTS(SELECT 1 FROM tracks WHERE path = import_errors.path AND untagged = 1 AND {PRESENT})`.

`CorruptAudio` als eigene Gruppe ist **zurückgestellt**, bis ein echter Decode-Test
existiert. Relink-Grenze: untagged Tracks haben Titel = Dateiname, der Fingerprint-Pfad
(Titel+Artist+Album) greift für sie nach einem Move nicht — nur Stufe 1 (device+inode).

*Status: implementiert.*

### 7. Undo = Tombstone, nicht Snapshot
„Remove" setzt `tracks.removed_at`; die Zeile bleibt, die id bleibt belegt, die Kaskaden
feuern nie. Undo = Spalte auf NULL. Nach Toast-Ablauf (oder beim nächsten Start, falls die
App im Fenster beendet wurde) wird real gelöscht — **committed, nie zurückgerollt**: der
User hat „7 removed" gelesen, das muss wahr bleiben.

**Warum Snapshot disqualifiziert ist (nicht nur unschöner):** `tracks.id` ist ein plain
`INTEGER PRIMARY KEY` **ohne AUTOINCREMENT** — SQLite vergibt `max(id)+1`. Löscht man die
höchste id, ist sie sofort frei; läuft im 10-s-Fenster ein Scan (der Watcher feuert von
selbst) und fügt einen Track ein, bekommt der genau diese id. Das Undo kollidiert oder
hängt die Historie an den falschen Track. **Ein Undo, das gegen den Watcher rennt, ist
keins.** Drei Tabellen kaskadieren auf `tracks`: `playlist_tracks` (Mitgliedschaft **und
Position**), `listen_events` (Hörhistorie), `device_files`.

**Resurrect-bei-Evidenz:** Findet der Scan die Datei am Pfad, ist sie beweisbar da — ein
„Remove", dessen Gegenstand zurückgekehrt ist, ist gegenstandslos: `removed_at = NULL`,
Missing-Zustand geräumt. Dieselbe Evidenz-Regel wie beim Mount-Event, nur aus der anderen
Richtung.

`removed_at IS NULL` gehört auch in die Sync-Delta-Queries.

*Status: implementiert (Kern). GUI-Verdrahtung offen — siehe Taskplan.*

> **Zwei echte Bugs, die die Reviews hier gefangen haben** (beide gefixt, als Warnung
> dokumentiert): `purge_tombstones` **und** `run_auto_clean` holten ihre ids per SELECT und
> löschten dann `WHERE id = ?` **ohne Recheck**. Der Watcher läuft auf eigenem Thread mit
> eigener Connection unter WAL — resurrected er in dem Fenster, wäre ein beweisbar
> lebender Track samt Playlist-Positionen und Hörhistorie gelöscht worden. Also **genau die
> id-Race-Klasse, wegen der der Snapshot verworfen wurde, durch die Hintertür**. Gefixt über
> `RemoveGuard{Any, MissingOnly, TombstonedOnly, AutoCleanEligible}`, der beim DELETE
> nachprüft. **Jeder neue Lösch-Pfad braucht seinen Guard.**

### 8. `import_errors`: Neubau, Episoden, Dismiss-Skip
`path` als PRIMARY KEY, `reason_kind`/`reason_detail`, `first_seen`/`last_seen`/
`seen_count`, `dismissed_mtime`/`dismissed_size`. Bestandszeilen wurden **verworfen**, nicht
migriert.

**Warum verworfen — bewusster Kontrast zu Beschluss 1:** Fehlerzeilen sind *reproduzierbare
Beobachtungen*; der nächste Scan erzeugt jede noch gültige Zeile korrekt typisiert neu.
`tracks`-Zeilen tragen *Nutzerdatum* (Ratings, Playlist-Plätze) und **müssen** migrieren.
Alte Zeilen hätten außerdem nur Freitext — ihre Typisierung ginge nur über das String-Parsing,
das Beschluss 5 verwirft, und würde die `Unknown`-Log-Regel sofort mit Altlasten fluten.

**Dismiss:** `stat` **vor** `read_meta` (dismissed heißt: die Datei kostet beim Scan nichts
mehr); `seen_count` zählt nur echte Fehlversuche, nie Skips. Datei geändert → `dismissed_*`
genullt, **neue Episode** (`first_seen = now`). **Restore** = un-dismiss **+ sofortiger
Einzel-Retry** — der User sagt „doch wieder anschauen", die Antwort kommt direkt, nicht beim
nächsten Scan.

**Sichtbarkeit:** Dismissed-Zeilen leben in einer Fußzeile „N dismissed · Show" mit
Restore-Pill. Ganz ausblenden wäre ein Zustand, den der User gesetzt hat und nirgends
zurücknehmen kann; eine volle Gruppe gäbe Weggeklicktem dauerhaft Fläche.

*Status: implementiert (Kern + Queries). GUI offen.*

### 9. Auto-Clean: Default OFF, mit Scharfstell-Kante
Setting `missing_auto_clean` ∈ `off | 30 | 90`, **Default off**. Nur `deleted` —
**nie** `unmounted`, **nie** `unknown`. Hartes Löschen ohne Tombstone und ohne Undo.

**Warum off:** Wir binden jedes Löschen dreifach an Evidenz und Widerrufbarkeit — und würden
es dann per Verzögerungszünder, ohne Zuschauer, ungefragt wieder einführen. Stattdessen
Teal-Hinweis am Ort des Geschehens, wenn off und die Deleted-Gruppe nicht leer ist.
**Warum kein Tombstone:** Auto-Clean feuert frühestens 30 Tage nach dem Verschwinden, ohne
Toast, den man widerrufen könnte. Ein Tombstone ohne Zuschauer ist nur eine zweite Frist
hinter der ersten — Theater.

**Scharfstell-Kante (SET-4):** Wer bei bestehendem Rückstand aktiviert, löst sonst sofort
eine Massenlöschung aus. Dialog: „Remove now / Start counting from today"; Letzteres
speichert `auto_clean_armed_at`, gelöscht wird nur, was **Frist UND Stichtag** reißt
(`max(missing_since, armed_at) + days*86400 <= now`). Fehlt `armed_at`, ist das Feature
inert — die Fail-Safe-Richtung: „nichts getan" ist heilbar, „400 Tracks gelöscht" nicht.
`missing_since` bleibt unangetastet die historische Wahrheit.

**Beide Lösch-Dialoge benennen die Kaskade** (Ratings + Hörhistorie) — die eine Stelle, wo
die Wahrheit wehtun darf.

*Status: implementiert (Kern). Dialoge offen.*

### 10. View-Architektur: eigene Widgets, geteilter Bausatz
Eigene Widgets nach dem Präzedenzfall `import_errors_view.rs` (plain rows,
tear-down-and-rebuild), gemeinsamer `ui/issues/`-Bausatz (Cards/Rows/Pills/Collapse)
**ohne Import-Error-Spezifika** — die Device-View (17a) ist der dritte Nutzer.

**Warum kein Gruppen-Modus im geteilten ColumnView:** Header mit eigenen Buttons,
Collapse-after-2, Hover-Pills statt Zellen — jede Sonderlocke landete im Code-Pfad, den
**alle sechs** anderen Sources durchlaufen. Der teuerste Platz im Projekt für
Spezialverhalten. Der Skalen-Präzedenz steht im Repo: Issue-Listen bleiben klein (Dutzende),
da ist eine Factory pure Overhead.

**Collapse ist ein struktureller Deckel**, nicht nur visuell: eingeklappte Zeilen werden erst
beim Expand gebaut, Expansion paginiert („Show 50 more"). Dann kann die View auch im
40k-Fall nicht explodieren.

### 11. Missing in Playlisten und Queue
Der `PRESENT`-Filter fällt aus den manuellen Playlist-Window/Count-Queries; Rows bleiben
**grau, durchgestrichen, an fester Position**. M3U-Export exkludiert weiter (eine exportierte
Playlist mit toten Pfaden ist für jeden fremden Player Müll).

**Die Asymmetrie der Höflichkeit:** Doppelklick auf die konkrete Row → Toast + „Show in
Missing files" (der User meinte *diesen* Track — stiller Skip wäre die falsche Höflichkeit).
Play all/Shuffle → **stiller** Skip (der User meinte „spiel die Liste"), **kein Toast im
Queue-Advance**. Einreihen ist deaktiviert; sonst bauen wir Queue-Einträge, deren einziger
Lebenszweck der Fault-Skip ist. DnD/Remove erlaubt — die Row ist vollwertiges Mitglied, nur
nicht abspielbar. Die Playlist zählt sie mit; die grauen Rows erklären die Zahl selbst.

**Queue-Asymmetrie:** `deleted` → still raus (PLAY-5a). `unmounted` → **bleibt** grau in der
Queue, wird beim Advance übersprungen, heilt beim Mount-Event (PLAY-5b). Ein Eject würde
sonst still die Queue-Reihenfolge vernichten — Zustand, der beim Wiedereinstecken nicht
zurückkommt, obwohl die Tracks es tun. **Der spielende Track wird nie proaktiv gestoppt** —
GStreamer faultet von selbst, ein harter Stopp wäre ein zweiter Wahrheitspfad neben dem
Fault-Handler.

Tooltip differenziert nach Reason: „On unavailable drive — returns when mounted" vs. „File
missing since Jul 12".

### 12. Locate: ein Schwellwert, ein Pfad
Warndialog ab **Dauer-Differenz > 2 s** — dieselbe Toleranz wie `find_move_candidate`. Ein
eigener, strengerer Schwellwert wäre eine zweite Wahrheit neben dem Matcher, die
auseinanderläuft. Dialog symmetrisch alt → neu (Dauer, Titel), Button heißt **„Relink
anyway"** — der User soll benennen können, was er überstimmt.

Beim Bestätigen: volle Row-Aktualisierung über `apply_file_identity` — **dieselbe Funktion,
die der Move-Arm ruft**. Die Datei ist die Wahrheit, auch wenn der User den Warndialog
überstimmt hat; Ratings/Positionen bleiben, weil die id bleibt.

**Ordner-Wahl:** matcht nur gegen die Missing-Rows der Gruppe, **importiert nie** (sonst wäre
es ein verstecktes „Add folder to library"), bricht nach dem letzten Match ab, läuft
off-thread mit Fortschritt + Abbruch. Ziel außerhalb des Library-Roots ist **erlaubt** — es
ist die explizite Wahl des Users — aber mit Hinweiszeile „won't be watched or rescanned".
Verbieten wäre bevormundend, verschweigen unehrlich.

### 13. Badges, Toasts, Mount-Events
**Badge = Episoden, nicht Dateischicksale.** `missing_since > last_viewed` bzw.
`first_seen > last_viewed`, ohne dismissed, ohne Hinweise. **Nicht `last_seen >`**: ein
Dauerfehler wird bei jedem Scan neu gesehen und würde ewig neu badgen — nagt also über etwas,
das der User längst gesehen und stehen gelassen hat. Reaktivierung einer dismissed-Zeile =
neue Episode → badgt wieder (die alte Episode endete mit dem Dismiss).

**Scan-Toast** aggregiert, nur bei > 0: „3 moved files relinked · 2 previously failed files
imported".

**Mount-Event:** `mount-added` → `unmounted` **und `unknown`** verifizieren (die
Migrationsaltlasten bekommen so ihre erste Verifikationsgelegenheit geschenkt).
**Eager Unmount-Marking** bei `mount-removed`: das Signal **ist** Evidenz, kein `stat` nötig
— dieselbe Evidenz-Regel wie beim Resurrect, nur in Gegenrichtung. Ein Unmount ist fast
immer bewusste Nutzeraktion (Eject) oder ein Ausfall, den man ohnehin bemerkt; die Views
werden im selben Moment ehrlich und zeigen die unavailable-Card als Erklärung.

## Abnahme (aus der Aufgabenstellung)

- Datei umbenennen/verschieben + Scan → Track bleibt **mit Ratings**, Toast „relinked".
- NAS unmounten + Scan → Gruppe „unavailable", nichts löschbar; mounten → verschwindet **ohne
  Scan**.
- Datei löschen + Scan → Gruppe „deleted", „Remove all" mit Undo.
- Kaputte Tags fixen + Scan → Error-Zeile weg, Track da.
- Dismiss + Datei ändern → Eintrag kommt wieder.
- **Keine Duplikate nach 5 Scans.**
- Playlist zeigt missing Track grau durchgestrichen an fester Position.
