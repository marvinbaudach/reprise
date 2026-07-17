# Tag-Editor-Rework — Beschlussdokument (Grilling 2026-07-17)

Normativer Kontext für den Umbau des Tag-Editors nach den Designs 3a (Multi-Track)
und 4a (Autocomplete). Alle Beschlüsse wurden am 2026-07-17 gegrillt und bestätigt.

> **Regelwerk-Vermerk (aktualisiert 2026-07-17):** Der Rules-Branch ist inzwischen in
> `main` gelandet — `docs/ux-rules.md` ist damit die verbindliche UX-Wahrheitsquelle
> und dieser Branch trägt seine Regeln selbst ein. **TAG-1–8 leben ab sofort in
> Abschnitt K von `docs/ux-rules.md`** (dort `[geplant]`), ebenso die früher hier
> rekonstruierten Regeln SET-3, FB-1, FB-3, P-2, P-4 — deren Wortlaut stand bereits
> im Regelwerk und deckt sich mit dem Diktat. Statuswechsel `[geplant] → [aktiv]`
> passieren im Implementierungs-Commit der Regel **im Regelwerk**, und nur mit einem
> regelbenannten Test (`fn tag_1_…` / cua-e2e `tag-1-…`); erzwungen von
> `scripts/check-ux-traceability.sh`. Dieses Dokument bleibt das Beschluss-Ledger:
> es hält das *Warum* und die Detailentscheidungen, die unterhalb der Regel-Ebene
> liegen.

## TAG-Regeln (Zusammenfassung — normativ ist Abschnitt K in `docs/ux-rules.md`)

- **TAG-1 · Save ist navigationsneutral** `[geplant]` — Speichern ändert weder
  Scroll noch Ansicht der Library (NAV-5 gilt durch den Dialog hindurch); das alte
  „Springen zum nächsten Song" entfällt ersatzlos. Mechanik an der Wurzel: das nackte
  `reload()` sichert vor dem Model-Swap Selektion (Track-IDs) und Scroll-Anker
  (Track-ID + Offset, nicht Pixel) und stellt beide danach wieder her — für **alle**
  Aufrufer (Save, Watcher-Reconcile, Sortier-Klick, Rating, DnD, …). Gelöschte IDs
  fallen still aus der Selektion; ein gewollter Reset ist künftig explizit
  (`clear_selection()` vor `reload()`), nie Nebeneffekt. Nach dem Schließen liegt der
  Fokus auf der Library; Selektion = die **geschriebenen** Tracks (bei Teilfehlern die
  gelungenen; Cancel/Discard: unverändert) — Feedback über die eigene Handlung ist
  erlaubt, der Sprung zu unbeteiligten Tracks nicht.
- **TAG-2 · Multi-Semantik der Felder** `[geplant]` — Felder mit identischem Wert
  zeigen ihn normal. Felder mit abweichenden Werten zeigen einen kursiven
  Mixed-Platzhalter (weiß 45 %, gestrichelte Border): bei ≤ 2 distinct Werten die
  Werte selbst („Mixed — Ambient, Post-Rock", Ellipsize; leer zählt als eigener Wert:
  „Mixed — Deathcore, empty"), ab 3 die Anzahl („Mixed — 8 different values");
  über dem Feld rechts der Zähler („2 values"). Kein Wert wird vorausgefüllt, kein
  Click-to-unlock: das Feld ist normal fokussierbar, das **erste getippte Zeichen**
  macht es scharf (Akzent-Border, ↺-Revert im Feld, Annotation „will be applied to
  all N", Summary zählt). Backspace/Entf im Platzhalter macht ebenfalls scharf — als
  „leeren für alle N" mit voller Review-Behandlung; nichts wird still verschluckt.
  ↺ setzt zurück auf Platzhalter bzw. Ursprungswert.
- **TAG-3 · Per-Track-Felder sind im Multi read-only** `[geplant]` — Title und
  Track number zeigen „—" mit Tooltip „Per-track field — edit tracks individually".
  Ein Massen-Titel ist immer ein Unfall.
- **TAG-4 · ‹›-Navigation im Einzeltrack-Modus** `[geplant]` — Öffnet der Editor mit
  N=1, blättern ‹ › (+ Ctrl+Page Up/Down) durch einen **Snapshot der sichtbaren Liste
  zum Öffnungszeitpunkt** (Track-IDs, nie Indizes — „Track 3 of 12" bleibt stabil,
  auch wenn Watcher/Reconcile darunter re-sorten). N>1 öffnet den Multi-Dialog ohne
  ‹›. Blättern verwirft nichts: pending Änderungen werden pro Track gehalten; der
  Save-Button schreibt alle pending Tracks. Cancel verwirft alle (Bestätigung ab
  1 pending). Invalides Zahlenfeld (Year/Track) blockt Blättern und Save mit
  Fehlerlabel am Feld.
- **TAG-5 · Änderungs-Review vor dem Save** `[geplant]` — Jedes effektiv geänderte
  Feld zeigt inline den Altwert: unter dem Entry „was: Suicide Silence" (11 px, weiß
  40 %, durchgestrichen), Feld-Border Akzent; die Zeile erscheint nur wenn alt ≠ neu,
  ihr Platz ist immer reserviert (P-4). Summary-Zeile über dem Save-Bereich:
  „2 fields · 30 tracks affected". Review-Expander („Review changes") listet pro Feld
  `Artist: Suicide → Suicide Silence · 30 tracks` — er existiert im Multi-Modus und im
  Einzeltrack-Modus, sobald pending über den aktuellen Track hinausgeht. **Zahlen
  sprechen eine Währung: Tracks = echte Datei-Writes.** No-op-Writes werden
  übersprungen (effektiver Diff pro Track, exakter Vergleich ohne Trim/Case; Rating-
  only zählt mit, schreibt nur DB). Save-Button trägt die Track-Zahl („Save 30",
  Einzeltrack ohne Pending-Nachbarn „Save", verstreut „Save · 2 tracks"); Progress
  („Saving… 12/30") und Toast („Tags updated · 30 tracks") zählen dieselbe Menge.
  Alles scharf, aber 0 effektive Änderungen → Button disabled, Summary „No effective
  changes". Disabled-Buttons tragen ihren Grund als Tooltip (P-2).
- **TAG-6 · Autocomplete-Quelle** `[geplant]` — Für Artist, Album, Album Artist,
  Genre: distinct-Werte aus der eigenen Library mit Track-Zahl, case-insensitive;
  Ranking **Präfix-Treffer vor Substring-Treffern**, innerhalb nach Track-Zahl
  absteigend; max. 6 Zeilen; Dropdown erst ab 2 Zeichen. Sektionstitel „FROM YOUR
  LIBRARY". Letzte Zeile immer „Use ‚X' as new artist…" — neuer Wert ist nie
  blockiert. Erste Zeile vormarkiert. Ein Wert pro Feld (Multi-Genre-Chips sind v2).
- **TAG-7 · Inline-Ghost** `[geplant]` — Bester Präfix-Treffer (Tiebreak Track-Zahl —
  identisch mit Dropdown-Zeile 1) erscheint als Ghost-Text hinter dem Cursor (weiß
  35 %), auch unter 2 Zeichen; Tab übernimmt ihn. Ohne sichtbaren Ghost ist Tab
  reiner Fokuswechsel — eine stille Übernahme der ersten Dropdown-Zeile gibt es
  nicht. Das Tab-Badge im Entry rendert nur bei sichtbarem Ghost. Ghost ist reine
  Anzeige und landet nie im Pending-State. Fallback: per Konstante deaktivierbar
  (dann kein Ghost, kein Badge, Tab = Fokuswechsel, Dropdown voll funktional) — kein
  halb kaputtes Ghost im Release. Popover als GtkPopover am Entry verankert (nicht an
  der Grid-Zelle), kein Fokus-Klau.
- **TAG-8 · Tastatur-Semantik** `[geplant]` —
  **Enter:** Dropdown offen → übernimmt den markierten Vorschlag, Dropdown zu, Fokus
  bleibt im Feld. Dropdown zu → springt zum nächsten **editierbaren** Feld (read-only
  übersprungen, Rating übersprungen). Im letzten Feld → fokussiert den Save-Button
  (sichtbar; der nächste ↵ speichert bewusst). Enter speichert **nie** direkt aus
  einem Textfeld (`activates_default` entfällt). **Ctrl+Enter** = Save von überall
  (dokumentiert, Shortcuts-Overlay); Ctrl+S bleibt stiller Alias — beide feuern
  dieselbe Action (gemeinsamer Disabled-/Saving-Zustand).
  **Esc-Kaskade:** (1) Popover offen → schließt nur das Popover, Text bleibt.
  (2) Feld scharf → Feld-Revert. (3) Dialog-Ebene → Discard-Frage ab 1 pending,
  sonst schließen. Jede Stufe vernichtet höchstens, was die nächste wiederbringen
  kann. Discard-Frage: „Discard changes to N tracks?" (zählt Tracks) ·
  Keep editing (Default) / Discard (destruktiv) — kein Save im Prompt; Speichern ist
  nie der Ausweg aus einer Schließen-Geste.

## Referenzierte Regeln

SET-3 (Modal-Ebenen), FB-1 (Zwei-Klassen-Toasts), FB-3 (Fehler sammeln + Details),
P-2 (Sofort-Feedback, kein toter Button ohne Grund), P-4 (nichts verschiebt sich
ungefragt) und NAV-5 (Modus-Gedächtnis) stehen im Wortlaut in `docs/ux-rules.md` und
decken sich mit dem Diktat vom 2026-07-17 — hier keine Zweitfassung, sonst
divergieren sie. Zwei Präzisierungen, die dieses Vorhaben aus ihnen ableitet:

- P-2 · „kein toter Button ohne benannten Grund" heißt hier: disabled ⇒ Tooltip
  (MB-Button, Save-Button).
- P-4 · „für dynamische Elemente ist Platz reserviert" heißt hier: die Altwert-Zeile
  unter jedem Feld belegt ihren Platz immer, auch leer.

## Weitere Beschlüsse (ohne eigene Regel-ID)

1. **Bestand weiterentwickeln, kein Neubau.** Logik (Autocomplete, Mixed-Felder,
   Dirty-Tracking, Batch-Write, MB-Lookup) bleibt; die Hülle wandert aufs
   3a-Layout: GtkEntry-Felder mit Label darüber, Cover links neben
   Title/Artist/Album, 2-Spalten-Grid (Album Artist/Genre · Year/Track/Rating).
   Stift-Icons und „Change cover…" entfallen (v1: kein Cover-Schreiben; Multi zeigt
   Cover-Stapel + „N covers"-Badge).
2. **Header:** Cancel links · Titel + Subzeile zentriert · Save rechts (Akzent).
   Subzeile Einzeltrack: „Track 3 of 12 · FLAC · 987 kbit/s" (Position im
   Listen-Snapshot; Format aus Dateiendung, Bitrate aus `bitrate_kbps`, fehlende
   Teile entfallen). Subzeile Multi: Erklärtext „Only changed fields will be written
   to all selected tracks" (die Track-Zahl steht in Titel und Save-Button).
3. **MusicBrainz im Multi:** genau **ein** Release-Lookup, wenn Artist+Album über die
   Selektion **effektiv** uniform sind (Original + pending); füllt leere aggregierte
   Felder (Year, Album Artist, Genre) als ganz normale scharfe Felder — Review,
   Revert, Save wie von Hand. Sonst disabled + Tooltip „Requires same artist & album
   across selection". Off-thread mit Spinner im Button; Ergebnisse sind immer
   pending, nie Direkt-Writes. Per-Track-Füllung ist v2. Hint-Text: „fills only
   empty fields".
4. **Save-Ablauf:** Dialog bleibt offen; Save-Button wird Spinner „Saving… 12/30"
   (P-2), Felder + Cancel disabled; kein Abbruch (Batch ist aus User-Sicht atomar).
   Schreiben via Lofty off-thread mit Progress-Streaming (Channel, analog
   `ScanProgress`). Danach Dialog zu + Toast. Watcher-Ignore pro Datei unmittelbar
   vor ihrem Write setzen (nicht als 5-s-TTL für alle upfront); Re-Read über den
   gezielten `scan_folder(datei)`-Pfad + `file_mtime=-1` bleibt, idempotent gegen
   das eigene Watcher-Echo.
5. **Fehlerfall (FB-3):** ein Toast „Tags updated · 33 tracks · 2 failed [Details]"
   (mit Aktion: 10 s, unverdrängbar; ohne Fehler: aktionslos, 4 s, ersetzbar).
   Details-Dialog: pro Fehler Dateiname + klassifizierter Grund (Fehlerklassifikation
   im Kern, z. B. PermissionDenied → „No write permission" — kein Lofty-Rohtext;
   nach Merge des Missing-Umbaus mit dessen Klassifikation zusammenführen) +
   Button „Edit failed tracks…" → öffnet den Editor mit genau diesen Tracks,
   pending-frisch. Gelungene Writes bleiben.
6. **Rating:** DB-only wie bisher; im Multi bei gemischten Werten gedimmte Sterne +
   kursives „mixed", Klick setzt für alle (scharf, Review-Behandlung).
7. **Pending-Session-Modell** als pure Rust in `reprise-core`
   (`library/tag_edit_session.rs`): Snapshot, per-Track-Pending, effektive Diffs,
   Zähler, Review-Zeilen, MB-Uniformitätsprüfung, No-op-Skip. GUI bindet nur
   (Architektur-Regel: keine Fachlogik in der GUI-Schicht).
8. **Branch-Hygiene:** Basis ist `main`; die Queue-Commits (`091c1d5`, `68a87ac`)
   wurden verlustfrei nach `feat/queue-dnd` ausgelagert („ein Branch, ein Anliegen").

## Abnahme

1. 35 Tracks selektieren → Edit Tags: Artist zeigt Wert, Genre zeigt Mixed-Copy
   („Mixed — …", Zähler), Title „—" read-only. „Sui" tippen → Ghost „Suicide
   Silence", Dropdown mit Track-Zahlen, „FROM YOUR LIBRARY", „Use ‚Sui' as new
   artist…"; Tab vervollständigt. Feld zeigt „was: …", Summary „1 field ·
   N tracks affected", Save-Button „Save N" (N = effektive Writes).
2. ↵ bei offenem Dropdown übernimmt die Markierung; ↵ bei geschlossenem springt zum
   nächsten editierbaren Feld; im letzten Feld fokussiert ↵ den Save-Button;
   Ctrl+Enter (und Ctrl+S) speichert von überall.
3. Save: Spinner „Saving… x/N" im offenen Dialog → Dialog zu → Toast in derselben
   Währung → Library: Selektion = geschriebene Tracks, Scroll-Anker unverändert,
   kein Sprung. Fehlerfall: „· 2 failed [Details]" → Details-Dialog mit
   „Edit failed tracks…".
4. Einzeltrack in 12er-Liste öffnen: Subzeile „Track 3 of 12 · FLAC · 987 kbit/s";
   ‹ › blättert durch den Snapshot, Änderungen an Track 3 überleben das Blättern,
   „Save · 2 tracks" schreibt beide; Re-Sort unter dem Dialog ändert „3 of 12" nicht.
5. Esc-Kaskade: Popover zu → Feld-Revert → „Discard changes to N tracks?"
   (Keep editing / Discard) → zu. Test: `tag_esc_cascade_dropdown_then_revert_then_discard`.
