# Updates-Popover: Delta-Melder statt Übersicht

**Datum:** 2026-08-03
**Status:** Design, freigegeben
**Betrifft:** `reprise-core/src/updates.rs`, `reprise-gnome/src/ui/updates/`
**Regelwerk:** ändert NR-9a, NR-10, NR-13; neue Regel NR-23

## Problem

Das Badge am ✦-Auslöser verspricht „hier ist etwas Neues". Wer daraufhin den
Popover öffnet, findet eine Halde aus alten und neuen Einträgen ohne jedes
Unterscheidungsmerkmal. Die Frage, wegen der man klickt — *was ist neu seit
meinem letzten Nachschauen?* — beantwortet der Popover nicht.

Drei Ursachen im Code, alle auf `origin/dev` (5fff82d1):

### U1 — Die Releases-Sektion kennt kein „neu"

`popover.rs::render` holt `artist_news::query_releases(conn, true, today)` und
filtert einzig `hidden`. Gesehen oder ungesehen spielt für die Anzeige keine
Rolle. Die Liste ist eine Übersicht, kein Delta.

### U2 — Das Neu-Signal löscht sich selbst, bevor es gezeichnet wird

`shell.rs` baut ein `new_tag`-Label („3 new") in die Sektionskopfzeile. In
`render(mark_seen = true)` läuft aber zuerst der Stempelblock (`opening_effect`
liefert *alle* gelisteten IDs an `mark_releases_seen`) und erst danach die
Abfrage `unseen_release_count`. Beim Öffnen ist die Zahl damit strukturell
immer 0 und der Tag unsichtbar. Das vorgesehene Neu-Signal ist toter Code.

### U3 — Die beiden Sektionen folgen gegensätzlichen Modellen

Concerts macht das Gegenteil von Releases: `feed_snapshot::concerts` ruft
`concerts::query_unseen(..., 3)` und zeigt ausschließlich Ungesehenes. Nach dem
ersten Öffnen ist die Sektion leer — ihr Kopf samt Untertitel „new near you"
bleibt aber stehen. Das ist der Zustand aus dem Bericht: eine Überschrift ohne
Inhalt.

## Beschlüsse

Vier Entscheidungen, im Gespräch am 2026-08-03 getroffen:

**B1 — Der Popover zeigt genau eine Charge.** Sichtbar sind alle ungesehenen
Einträge; gibt es keine, die Charge des letzten Besuchs. Das Badge verschwindet
beim ersten Öffnen, die Liste bleibt beim zweiten Öffnen unverändert stehen und
wird erst abgelöst, wenn ein Fetch echte Neuigkeiten bringt. Nachschauen ist
damit möglich, ohne dass der Popover leerläuft.

**B2 — Gedeckelt, mit voller Zahl im Sektionskopf.** Höchstens 5 Releases und 3
Concerts, kein Scroller. Der Zähler-Chip im Kopf nennt die volle Chargengröße,
der Sprung-Link führt zum Rest. Der Popover hat damit eine konstante Höhe.

**B3 — Leere Sektionen fallen weg.** Eine Sektion ohne Neuigkeiten verschwindet
samt Kopf. Sind beide leer, steht genau eine ruhige Zeile über den Sprüngen. Ein
Kopf ohne Inhalt ist strukturell nicht mehr möglich.

**B4 — Redesign freigegeben.** Der bestehende Popover ist ein früher Entwurf;
Layout-Eingriffe sind ausdrücklich erwünscht (siehe „Redesign").

## Die Charge

Definition, angewandt je Feed auf die Kandidatenmenge des Feeds:

```
kandidaten = einträge des feeds, nicht hidden, nicht LibraryPresence::Complete
ungesehen  = kandidaten mit seen_at IS NULL

charge = if ungesehen nicht leer  → ungesehen
         else                     → kandidaten mit seen_at == MAX(seen_at)
```

Der vorhandene `seen_at`-Stempel trägt den Besuchszeitpunkt, damit ist „die
Charge des letzten Besuchs" ohne neue Spalte und ohne Migration adressierbar.

**Liste und Badge sind deckungsgleich.** `unseen_release_count` schließt
`LibraryPresence::Complete` aus (NR-9a); die Delta-Liste tut dasselbe. Ein Badge,
das 3 sagt, und eine Liste, die 5 zeigt, wäre genau die Inkohärenz, die dieser
Umbau beseitigt. Vollständig vorhandene Releases bleiben in der
Releases-Übersicht sichtbar — dort gehören sie hin.

**Öffnen stempelt die gesamte Charge**, auch die Einträge unterhalb der
Deckelung. Sonst käme das Badge nie auf 0. Die Kopfzahl nennt die volle Größe
und der Sprung-Link führt zum Rest, also geht nichts verloren.

**Reihenfolge ist bindend:** Charge bestimmen → rendern (inkl. Kopfzahl) →
stempeln → Badge neu berechnen. Anzeige und Kopfzahl beruhen auf dem Zustand
*vor* dem Stempel, das Badge auf dem *danach*. Die heutige Reihenfolge ist U2.

## Redesign

Sechs Eingriffe gegenüber dem Ist-Zustand:

1. **Abruf und Alter wandern in die Kopfzeile.** „Fetch now" wird zum
   Icon-Button neben „UPDATES", das Alter steht rechts daneben. Der
   Fußbereich verliert seine Verwaltungszeile, der Popover endet mit Navigation
   statt mit Wartung. Ein Fehlerhinweis (NR-21) bekommt im Fehlerfall eine
   eigene Zeile direkt unter dem Kopf.
2. **„NEW RELEASES" heißt „RELEASES".** Das „New" steckt ab jetzt im
   Zähler-Chip; heute steht es doppelt da und beschreibt trotzdem nichts Neues.
3. **Der Zähler-Chip wird gefüllt, die Status-Chips bleiben umrandet.** Heute
   konkurrieren beide um dasselbe Teal. Füllung schlägt Umrandung — das
   Neu-Signal gewinnt die Hierarchie, ohne eine zweite Farbe einzuführen.
4. **Concerts wird symmetrisch zu Releases.** Der Untertitel „new near you"
   entfällt (er war der Ersatz für einen fehlenden Neu-Begriff) und weicht
   demselben Zähler-Chip. Die Ortsangabe steht ohnehin in jeder Zeile.
5. **380 statt 336 px Breite.** Die Hover-Aktionen liegen künftig neben dem
   Status-Chip statt ihn zu verdrängen. Heute (NR-10) verliert eine gehoverte
   Zeile ihre Statusangabe — genau die Information, wegen der man hinsieht.
6. **Kein Scroller.** Die Deckelung aus B2 ersetzt ihn; die Popover-Höhe fällt
   von ~800 px auf ~470 px.

### Zielbild

```
┌─────────────────────────────────────────────┐  380px
│  UPDATES                        ⟳  vor 1 d  │
│                                             │
│  RELEASES                      ⬤ 3 neu      │
│   ▣  TANZNEID                     in 4 d    │
│      Electric Callboy · Album · 7. Aug      │
│   ▣  Where The Light Begins…     in 11 d    │
│      If Not for Me · Album · 14. Aug        │
│   ▣  DEATHRACE                   in 18 d    │
│      Rising Insane · Album · 21. Aug        │
│                                             │
│  CONCERTS                      ⬤ 2 neu      │
│      Miss May I                   Eventim   │
│      Sa, 12 Sep · Köln · Palladium · 38 km  │
│                                             │
│  ───────────────────────────────────────    │
│   Alle Releases (652)                   →   │
│   Alle Concerts (26)                    →   │
└─────────────────────────────────────────────┘
```

Sind beide Sektionen leer, steht zwischen Kopf und Trennlinie eine einzelne
ruhige Zeile („Nichts Neues seit deinem letzten Blick"); die Sprünge und die
Kopfzeile bleiben unverändert stehen.

## Architektur

**Die Chargen-Logik gehört nach `reprise-core`.** `updates.rs` begründet
bereits, warum Badge-Arithmetik dort lebt und nicht im Widget: sie ist weder
toolkit-spezifisch noch braucht sie Gerät, Netz oder Datenbank. Für die Charge
gilt dasselbe, und ein zweites Frontend (CLI, MCP) beantwortet dieselbe Frage.

Eine generische Funktion bedient beide Feeds, weil beide Zeilentypen ein
`seen_at: Option<i64>` tragen:

```rust
pub struct DeltaBatch<T> {
    pub shown: Vec<T>,   // gedeckelt
    pub total: usize,    // volle Chargengröße, Quelle der Kopfzahl
}

pub fn delta_batch<T>(
    items: Vec<T>,
    seen_at: impl Fn(&T) -> Option<i64>,
    cap: usize,
) -> DeltaBatch<T>
```

Damit ist die Kernentscheidung ohne GTK, ohne DB und ohne Display testbar — die
Testsuite dieses Projekts ist im Rudel unzuverlässig (Display-Gate), reine
Core-Tests sind es nicht.

Die GTK-Seite behält ihre Aufgabenteilung: `feed_snapshot.rs` liest, `shell.rs`
baut Widgets, `popover.rs` orchestriert, `concerts_section.rs` und
`release_row.rs` rendern Zeilen. `release_row.rs` ist mit 28 KB die größte
Datei im Modul; der Umbau von Punkt 5 ist eine gute Gelegenheit, den
Chip/Aktions-Block als eigene Einheit herauszulösen.

## Fehlerfälle

- **Feed abgeschaltet oder ohne Credentials:** Sektion fällt weg wie bei einer
  leeren Charge (B3). Der zugehörige Sprung-Link bleibt sichtbar, solange sein
  Modul aktiv ist — sonst wäre die Übersicht aus dem Popover nicht erreichbar.
- **Fetch schlägt fehl:** NR-21 bleibt unverändert gültig. Die Charge bleibt
  stehen, der Fehlerhinweis erscheint als eigene Zeile unter der Kopfzeile.
- **Erster Lauf, noch nie gefetcht:** unverändert NR-8 — Erstlaufzustand statt
  Delta, kein Badge.
- **DB-Fehler beim Lesen:** wie heute `tracing::warn!` und leere Menge; eine
  leere Menge führt über B3 zur ruhigen Zeile, nicht zu einem toten Kopf.

## Tests

- **Core (ohne Display):** `delta_batch` — ungesehene gewinnen; ohne ungesehene
  die jüngste gestempelte Charge; leere Eingabe; Deckelung schneidet `shown`,
  nicht `total`; gemischte Stempel wählen nur `MAX(seen_at)`.
- **Core:** `unseen`-Zählung und Chargen-Menge schließen beide
  `LibraryPresence::Complete` aus (Deckungsgleichheit Badge/Liste).
- **GTK (Display):** Reihenfolge rendern→stempeln — nach dem Öffnen ist die
  Kopfzahl > 0 und das Badge weg (der U2-Regressionstest).
- **GTK:** leere Charge blendet Kopf *und* Liste aus (U3-Regressionstest).
- **GTK/STYLE-1:** Die Aktionen verdrängen den Chip nicht mehr — geprüft wird
  das Ergebnis (Chip bleibt sichtbar und behält seine Allokation, während die
  Aktionen sichtbar sind), nicht das Setzen einer Eigenschaft.

## Regeländerungen

- **NR-9a** → ersetzt. Neue Fassung: Der Popover zeigt die Charge nach B1, nicht
  die volle Liste. Öffnen stempelt die gesamte ungesehene Charge im Scope, auch
  unterhalb der Deckelung. Anzeige und Kopfzahl beruhen auf dem Zustand vor dem
  Stempel.
- **NR-10** → ersetzt. Hover oder Fokus blenden die Aktionen ein, ohne den
  Status-Chip zu verdrängen.
- **NR-13** → präzisiert. Die Markierung „In library" samt „Show in library"
  gilt für die Releases-Übersicht; der Delta-Popover führt vollständig
  vorhandene Releases nicht.
- **NR-23** (neu) — Die Deckelung: 5 Releases, 3 Concerts, kein Scroller; der
  Zähler-Chip nennt die volle Chargengröße; leere Sektionen fallen samt Kopf
  weg, bei zwei leeren Sektionen erscheint genau eine ruhige Zeile.
- **NR-3a, NR-5b, NR-8, NR-21, NR-22** bleiben unverändert gültig.
