# Handover: Der Sprung zum laufenden Titel vergisst den Filter

**Stand:** 14.08.2026, 15:05 · **Plan:** `docs/plans/jump-to-playing-track-drops-the-filter.md` (1024 Zeilen, `phase: planned`)
**Gebaut ist nichts.** Kein Branch, kein Worktree. Der nächste Schritt ist die Code-Phase.

---

## 1. Der gemeldete Bug

Der Nutzer sucht in der Musik-Bibliothek nach „electr" (15 von 2129 Treffern), spielt einen
Treffer ab („The Sound of Silence II: Electric Boogaloo" von Sail's End) und drückt dann den
Knopf **„Clear all ×"** rechts in der Filterleiste.

- Erwartet: volle Bibliothek, Ansicht springt zum laufenden Titel.
- Tatsächlich: Ansicht landet oben.

Daraufhin klickt er unten links in der Player-Leiste auf den **Titel** (soll zum laufenden
Titel springen).

- Erwartet: der Filter wird vergessen, es wird einfach zum laufenden Titel gescrollt.
- Tatsächlich: die gefilterte Ansicht mit dem Chip „electr" ist zurück.

Wörtlich: *„Ich erwarte, dass der Filter vergessen wird und ich einfach zum aktuellen Titel scrolle."*

---

## 2. Drei Wurzeln, nicht eine

Die vollständige Herleitung mit Datei:Zeile steht im Plan unter „Ausgangslage". Kurzfassung:

**A1 — „Clear all" fährt zwei Reloads statt einem.**
`SectionSearch::clear_all()` ruft erst `clear_facets()` (→ `clear_all_restrictions()` →
`reload_centering_playing_track()`), dann zusätzlich `apply_to_active("")` → `set_filter("")`.
Weil `shared.filter` da schon leer ist, liefert `filter_change_viewport("", "")` ein
`PreserveAnchor` — ein zweiter Modell-Reload. Der zählt `model.generation()` hoch, wodurch die
verzögerten Zentrierungen aus dem ersten Reload wortlos aussteigen, und schreibt stattdessen
die unzentrierte Position zurück.

**A2 — die Zentrierung leitet Erfolg aus veralteter Geometrie ab.** *(erst im Planentwurf gefunden, nicht in der ersten Diagnose)*
`centered_scroll_restore::apply` prüft `upper <= page`, **bevor** es die Zeilenhöhe liest.
Direkt nach dem Modelltausch ist `upper` noch die Allokation der alten, kurzen Trefferliste.
Passte die in den Viewport — bei 15 Treffern der Normalfall —, meldet `apply` `true`
(„nichts zu scrollen"), und `schedule` registriert gar keine Nachbesserung.
**Ohne A2 trifft der Fix für A1 den gemeldeten Fall nicht.** Der Plan verlangt dafür eine
eigene Mutationsprobe, die die A2-These widerlegen darf.

**B — der Sprung reist in die Ansicht zurück, aus der gespielt wurde.**
`window_playing_source_wiring.rs` baut den Reveal mit `origin = player.current_play_origin().place`.
Wurde aus der Trefferliste gestartet, trägt diese Place `state.search = "electr"` samt Facetten.
`route_to_place` → `restore_browser_place` → `prepare_track_view` setzt Query und Facetten
aktiv wieder; `browse_bar.restore_filter` schreibt den Chip neu. Der Filter blieb also nicht
stehen — er wird wiederhergestellt.

**Nebenbefund A3:** derselbe zweite Reload aus A1 wartet auch auf dem Rückweg von Aufgabe 5.
Deshalb sitzt der A1-Fix an einer Stelle, die beides abdeckt.

**Korrektur gegenüber der ersten Diagnose:** der `AdjustmentHold` ist im gemeldeten Fall
vermutlich unbeteiligt — er entsteht nur bei `value() > 0.0`. Der Schaden des zweiten Reloads
ist allein die Generationserhöhung.

---

## 3. Die fünf Grill-Entscheidungen des Nutzers (verbindlich)

Alle fünf sind im Plan bereits eingearbeitet; sie stehen hier, damit eine neue Session sie
nicht erneut aufwirft.

| # | Entscheidung | Begründung |
|---|---|---|
| **G1** | **Nur der Titel-Klick** wird repariert. Cover (`OpenAlbum`) und Interpretenzeile (`OpenArtist`) behalten ihre Query. | Dieselben Intents verschicken auch die Zeilen-Drills der Tabelle, und dort trägt die Query bewusst mit (SEARCH-8a). Eine Änderung bräuchte ein Feld `carry_query` und eine SEARCH-8a-Revision. Als **Folgeplan zugesagt**, siehe „Nachfolgeaufgabe" im Plan. |
| **G2** | **Ein Strang, kein Schnitt.** | Der zweite Strang wären zwei kleine Aufgaben, die ohnehin hinter dem ersten landen müssten; `docs/ux-rules.md` hätte geteilt werden müssen. Der Abschnitt `## Parallelität` trägt den begründeten Nicht-Schnitt. |
| **G3** | Die Zentrierung darf **nur auf gemessener Zeilenhöhe** „fertig" melden; eine angenommene CSS-Höhe führt zu „später nochmal". | Der Fehler ist asymmetrisch: ein falsches „passt in den Viewport" bringt genau diesen Bug zurück, ein falsches „passt nicht" kostet eine endliche Nachbesserungsrunde. Schärfer als der Entwurf. |
| **G4** | Die neue Leer-Regel greift **nur, wenn ein Vor-Such-Anker existiert**. | „Clear all" bei reinen Facetten ohne Suchtext hat keinen Ort zum Zurückkehren — dort bleibt FIL-9s bedingungsloses Zentrieren unverändert. |
| **G5** | Wirft der Reveal etwas weg, geht die verlassene gefilterte Place über `go_new` auf den **Back-Stack**. | Der Filter ist nicht verloren, sondern einen „Zurück"-Klick entfernt. Wirft der Reveal nichts weg, kein Historieneintrag — sonst müllt jedes `Ctrl+L` die Historie zu. |

Die neue Verhaltensregel in einem Satz: **Das Leeren der Suche zentriert auf den laufenden
Titel genau dann, wenn während dieser Suche ein Titel per Nutzerhandlung gestartet wurde**
(`PlaybackStarted` oder `ExplicitTransport`, nie `AutomaticAdvance`); sonst gilt weiter der
Vor-Such-Anker. Das gilt für alle Wege, die die Suche leeren — Chip-×, Escape, Feld leeren,
„Clear all". Heute zentriert nur „Clear all", und das bedingungslos.

---

## 4. Fallen, die diesen Plan schon zweimal eingeholt haben

**`origin/dev` bewegt sich schneller als die Planung.** Der Entwurf wurde gegen `604677322e`
geschrieben, der finale Plan gegen `57ff0bfc74`, und beim Abschluss stand dev bereits auf
`e33cfeb1a0`. **Vor dem Bauen erneut fetchen und die Basis prüfen.** Alle Zeilenangaben im
Plan sind dev-Nummern und können verrutscht sein.

Konkret schon aufgelaufen:
- **#479** („ListLayout stops representing a state it can never be in") hat `list_geometry.rs`,
  `list_geometry_layout.rs`, das neue `list_geometry_content.rs`, `reload_anchor_scroll.rs`,
  `reload_restore.rs` und `track_list_geometry.rs` umgebaut — **genau das Fundament von
  Aufgabe 2**. Steht als erster Punkt im Risikoabschnitt, samt Verweis auf
  `docs/plans/list-geometry-invariants.md`.
- **#481** hat `docs/ux-rules.md` angefasst — dieselbe Datei, die Aufgabe 8 ändert. Kein
  inhaltlicher Konflikt, aber eine wahrscheinliche Merge-Kollision.

**`RowHeightSource` hat keine `Cached`-Variante.** Nur `Assumed` und `Measured`; „gecacht"
fällt im Code mit `Measured` zusammen. Wer G3 umsetzt, darf keine dritte Variante suchen.

**Der Entwurf hat sich bei D4 verzählt:** `pre_search_anchor` hat **acht** Codestellen plus
zwei Kommentare, nicht fünf. Die im Entwurf vergessene ist die Initialisierung in
`track_list_builder.rs`. Der finale Plan führt sie als Tabelle.

**Benannter Widerspruch im Plan:** `had_query` und „ein Vor-Such-Anker existiert" sind nicht
deckungsgleich (eine Query aus Session-Restore oder Back hat keinen Anker). Aufgelöst über die
Rückfallkette aus D5 — nicht still auf `anchor.is_some()` umbauen.

**Nachbarpläne:** `search-popover-commit-chip` (`phase: planned`) hat weder Branch noch
Worktree, also keine akute Kollision in `section_search.rs`. `navback-scroll-jump-to-top`
(`phase: reviewed`) hat noch Branch `fix/navback-anchor` **und** einen ausgecheckten Worktree
`~/Projects/reprise-navback`, beschreibt aber Funktionen, die es auf dev nicht mehr gibt.

**Der bestehende Test deckt den Bug nicht ab, obwohl er danach aussieht:**
`clear_all_restrictions_resets_search_and_browse_in_one_pass` ruft `clear_all_restrictions()`
direkt auf, fährt den zweiten Reload also nie, und misst nur Filterzustand und Trefferzahl,
nie die Scrollposition. Neue Tests müssen über `SectionSearch::clear_all()` gehen — den Knopf.

---

## 5. Wie es weitergeht

```
/code docs/plans/jump-to-playing-track-drops-the-filter.md
```

Ein Strang, ein Worktree, ein Codex-Lauf. Die Pipeline-Skripte liegen unter
`~/.claude/skills/pipeline/scripts/`, **nicht** im Repo.

Danach `/check`, dann `/refactor`, dann landen. Der Plan enthält den Kontrollarm als
ausführbare Schrittfolge: jeder neue Test muss gegen den zurückgerollten Fix nachweislich
**rot** sein, bevor er zählt — inklusive der Mutationsprobe für Aufgabe 2, die die A2-These
widerlegen darf. Vorher einmal die Basislinie auf `origin/dev` fahren, sonst wird fremdes
Rot diesem Branch zugeschrieben.

**Die acht Aufgaben:** 1 ein Filter-Reload nur bei echter Änderung · 2 die Zentrierung glaubt
keiner veralteten und keiner angenommenen Geometrie · 3 der Merker „während dieser Suche
gestartet" · 4 eine Regel für alle Wege, die die Suche leeren · 5 `RevealTrack` lässt Query
und Facetten fallen (Core) · 6 der Sprung durch den echten Router, sichtbar gemessen ·
7 instrumentierte Abnahme statt Behauptung · 8 das Regelwerk zieht nach
(`docs/ux-rules.md`: neu SEARCH-16 und BROWSE-14, revidiert SEARCH-9, FIL-9 und SEARCH-8a).
