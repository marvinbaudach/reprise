# Tag-Editor — manuelle Testanleitung

Was automatisierte Tests nicht abdecken (Tastaturfluss, Rendering, echtes
Blättern) und deshalb dein Auge braucht. Gebaut auf `main` nach dem Merge des
Tag-Editor-Reworks.

Start:
```sh
cargo run
```
Bibliothek scannen lassen (falls leer), dann Tracks in der Track-Liste
auswählen und im Rechtsklick-Menü **„Edit tags…"**.

---

## 1. Der ursprüngliche Bug — Save springt nicht mehr (TAG-1)

Das ist der wichtigste Test. Früher sprang die Auswahl nach dem Speichern zum
nächsten Song.

1. In einer langen Track-Liste **nach unten scrollen** (nicht ganz oben bleiben).
2. **Einen** Track auswählen, Edit Tags, irgendein Feld ändern (z. B. Genre),
   **Save**.
3. **Erwartet:** Der Dialog schließt, derselbe Track ist noch ausgewählt, die
   Liste steht an **derselben Scroll-Position** — kein Sprung nach oben, kein
   Sprung zum nächsten Song.
4. Gegenprobe mit **Cancel** statt Save: Auswahl und Scroll ebenfalls
   unverändert.

Nebenbei: auch nach einem **Sortier-Klick** auf eine Spaltenüberschrift und
nach einem **Rating-Klick** in der Liste bleibt die Auswahl erhalten (derselbe
Mechanismus).

## 2. Mehrere Tracks — Mixed-Felder und Zähler (TAG-2, TAG-3)

1. **Mehrere Tracks aus verschiedenen Alben** auswählen (damit Artist/Album/
   Genre sich unterscheiden), Edit Tags.
2. **Erwartet im Kopf:** Titel „Edit N Tracks", darunter „Only changed fields
   will be written to all selected tracks".
3. **Felder mit gleichem Wert** zeigen den Wert normal + rechts „same on all".
4. **Felder mit unterschiedlichen Werten** zeigen im Feld einen kursiven,
   gedimmten Platzhalter — bei ≤ 2 verschiedenen Werten die Werte selbst
   („Mixed — Ambient, Post-Rock"), ab 3 „Mixed — N different values" — und
   rechts daneben den **Zähler „N values"**.
   → *Prüfblick:* Steht der Wert-Text im **Feld** und die Zahl in der
   **Annotation** rechts (nicht vertauscht)?
5. **Title und Track number** zeigen „—" und sind nicht editierbar (Hover:
   „Per-track field — edit tracks individually"). Ein Massen-Titel wäre immer
   ein Unfall.
6. In ein Mixed-Feld **tippen**: Border wird zum Akzent, rechts steht jetzt
   „will be applied to all N", im Feld erscheint ein **↺**. ↺ klicken → zurück
   auf den Zähler-Zustand.
7. **Backspace/Entf** in einem Mixed-Feld (ohne vorher zu tippen): macht es
   ebenfalls scharf — als „für alle N leeren". Soll sichtbar wie jede andere
   Änderung aussehen (Review zählt hoch), nichts wird still geschluckt.

## 3. Änderungs-Review (TAG-5)

1. In der Mehrfachauswahl ein, zwei Felder ändern.
2. **Unter dem geänderten Feld** erscheint „was: <alter Wert>" (durchgestrichen,
   gedimmt). → *Prüfblick:* Springt beim ersten Edit **nichts** im Layout? Der
   Platz für diese Zeile ist reserviert und soll schon vorher da sein (P-4).
3. Über dem Save-Bereich: **Summary-Zeile** „N fields · M tracks affected", und
   ein ausklappbarer **„Review changes"** mit einer Zeile je Feld
   (`Artist: alt → neu · M tracks`).
4. Der **Save-Button trägt die Zahl** — „Save N" (N = Tracks, die tatsächlich
   geschrieben werden). Tippst du einen Wert, der schon überall gleich ist, ist
   Save **disabled** mit Tooltip-Begründung.

## 4. Autocomplete (TAG-6) — Artist/Album/Album artist/Genre

1. Einen Einzeltrack editieren, ins **Artist**-Feld ab dem 2. Zeichen tippen.
2. **Erwartet:** Dropdown mit Überschrift „FROM YOUR LIBRARY", Vorschläge mit
   **Track-Zahl** rechts, der getippte Teil hervorgehoben. Präfix-Treffer stehen
   vor Substring-Treffern. Ganz unten immer „Use ‚<dein Text>' as new artist…".
3. Mit der **Maus** einen Vorschlag anklicken → wird übernommen.
4. Unter 2 Zeichen: kein Dropdown.

## 5. Tastatur (TAG-8) — bewusst genau so

1. **Enter in einem Textfeld** (Dropdown zu) → springt ins **nächste Feld**,
   speichert **nicht**. (Früher speicherte Enter direkt — das war der Sinn der
   Änderung.) Read-only-Felder (Title/Track im Multi) und Rating werden
   übersprungen.
2. **Enter bei offenem Dropdown** → übernimmt den markierten Vorschlag, Fokus
   bleibt im Feld.
3. Im **letzten Feld** Enter → fokussiert den **Save-Button** (sichtbar); erst
   der nächste Enter speichert.
4. **Ctrl+Enter** (oder Ctrl+S) → speichert von überall.
5. **Esc-Kaskade**, mehrfach drücken: (1) offenes Dropdown schließt sich, Text
   bleibt → (2) ein scharfes Feld wird zurückgesetzt → (3) bei ungespeicherten
   Änderungen kommt „Discard changes to N tracks?" mit **Keep editing** /
   **Discard** (kein „Save"; Enter = Keep editing).

## 6. Durch die Auswahl blättern (TAG-4)

1. In der Track-Liste **einen** Track auswählen, Edit Tags.
2. Kopfzeile: „Track 3 of 12 · FLAC · 987 kbit/s" (Format + Bitrate; die
   „x of N" bezieht sich auf die sichtbare Liste).
3. Mit **‹ ›** (oder **Strg+Bild↑/Bild↓**) blättern.
4. **Erwartet:** Änderungen an Track 3 **überleben** das Blättern zu Track 7 und
   zurück. Der Save-Button zählt alle angefassten Tracks („Save · 2 tracks").
   → *Prüfblick:* Bleibt „3 of 12" stabil, auch wenn im Hintergrund etwas
   nachsortiert? Und: reines Blättern **ohne** zu editieren darf den
   Save-Button nicht scharf machen (Tooltip bleibt „No changes yet").

## 7. Speichern-Ablauf und Fehler (FB-3)

1. Bei vielen Tracks: beim **Save** bleibt der Dialog kurz offen, der Button
   wird zu „Saving… x/N", Felder sind gesperrt (lokal oft zu schnell zum
   Erhaschen — kein Problem).
2. Danach ein **Toast** unten: „Tags updated · N tracks".
3. **Fehlerfall provozieren** (optional): einer Musikdatei vorher
   Schreibrechte entziehen (`chmod 444`), sie mit-editieren. Erwartet: Toast
   „… · 1 failed" mit **„Details"** → Liste mit Dateiname + Grund („No write
   permission"), Button „Edit failed tracks…" öffnet den Editor erneut mit
   genau diesem Track. Danach Rechte zurücksetzen (`chmod 644`).

## 8. ⚠️ Der Ghost — noch abgeschaltet, deine Entscheidung (TAG-7b)

Der Inline-Ghost (grauer Vorschlagstext hinter dem Cursor, Tab übernimmt) ist
**aktuell aus** (`GHOST_ENABLED = false`), weil sich seine pixelgenaue Position
headless nicht prüfen ließ. Zum Ansehen:

1. In `crates/reprise-gnome/src/ui/tag_edit/autocomplete_entry.rs` die Konstante
   `GHOST_ENABLED` auf `true` setzen, `cargo run`.
2. Ins Artist-Feld tippen (z. B. „Sui"). **Erwartet:** hinter dem Cursor
   erscheint der beste Präfix-Treffer gedimmt („cide Silence"), rechts ein
   „Tab"-Badge; **Tab** vervollständigt.
3. **Dein Urteil:** Sitzt der Ghost **bündig** direkt hinter dem getippten Text,
   auf gleicher Höhe? Oder schwebt er versetzt/verschoben?
   - Sitzt bündig → sag Bescheid, dann bleibt `GHOST_ENABLED = true` und
     TAG-7b wird `[aktiv]`.
   - Sitzt daneben → sag mir wie (zu hoch/zu tief/zu weit rechts), dann justiere
     ich das CSS und du prüfst erneut.

---

## Bekannte Rest-Punkte (kein Bug, für später notiert)

- Beim Öffnen des Einzeltrack-Editors lädt der ‹›-Snapshot die Tags der ganzen
  sichtbaren Liste einmalig — bei sehr großen, ungefilterten Bibliotheken ein
  kurzer, einmaliger Moment beim Öffnen.
- Cover-Schreiben ist v1 bewusst nicht dabei („Change cover…" existiert nicht).
- Multi-Genre-Chips und Per-Track-MusicBrainz sind v2.
