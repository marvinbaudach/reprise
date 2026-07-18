# Now-Playing-Panel — Beschlussdokument (Grilling 2026-07-18)

Normativer Kontext für den Umbau der rechten Spalte zum Now-Playing-Panel nach
Design **21** (verbindliche Fassung, konsolidiert aus 10b + Screenshot-Review),
Frames **21a** (Lyrics-Tab) und **21b** (Visual-Tab). Referenz für den
Artist-News-Folge-Task: Frame **22a**. Design-Quelle ist das claude.ai/design-
Projekt „Audio-Player für große Bibliotheken" (Share-Link, kanonisch; PDFs in
`docs/design/` sind nicht maßgeblich).

> **Regelwerk-Vermerk:** Die NPP-Regeln leben vorerst **in diesem Dokument**
> (`[geplant]` → `[aktiv]` im jeweiligen Implementierungs-Commit). Sie werden
> **nicht** in diesem Task nach `docs/ux-rules.md` eingetragen —
> Ownership-Beschluss, um Konflikte mit parallel laufenden Regelwerk-Branches
> zu vermeiden. Die Überführung als **Sektion P** ins Regelwerk (mit
> regelbenannten Tests, `scripts/check-ux-traceability.sh`-konform) ist ein
> eigener Folge-Commit nach Merge-Lage.

## NPP-Regeln (verbindliche Fassung aus Design 21)

### Struktur

- **NPP-1 · Geometrie** `[aktiv]` — Panel fix **300 px** (die linke Sidebar
  fix **240 px** — bewusst ungleich), einklappbar mit derselben
  Slide-Transition wie die linke Sidebar (MOT-3, Standard-Token 250 ms
  ease-out-cubic; die bestehende `OverlaySplitView`-Transition liefert sie).
- **NPP-2 · Aufbau vertikal** `[aktiv]` — Cover **168 px** (Radius 12,
  Schatten + 1 px Inset-Hairline) → Titel 15 px bold / „Artist · Album" 12 px
  weiß 55 % → **Pill-Toggle** Up Next | Lyrics | Visual (Segmente, kein
  Tab-Bar-Widget) → Tab-Inhalt füllt den Rest → Fußzeile 10.5 px weiß 35 %
  (Inhalt pro Tab, siehe Beschlüsse 3/9). **Kein Volume-Regler im Panel** —
  Lautstärke lebt ausschließlich in der Playerleiste (P-1).
- **NPP-3 · Glow statt Volltint** `[aktiv]` — Radialer Verlauf aus der
  Cover-Akzentfarbe (bestehende Extraktions-Pipeline) hinter dem Cover — nur
  oberes Drittel (~300 px Ellipse, weich auslaufend, Opacity ~0.4), läuft nach
  unten auf neutrales Panel-Dunkel aus. Basis-Hintergrund bleibt neutral,
  damit der Lyrics-Kontrast konstant ist. Fallback Petrol (= Theme-Akzent,
  Beschluss 8). Als Verlauf/Textur einmal gerendert, kein Live-Blur.
- **NPP-4 · Tab-Gedächtnis** `[aktiv]` — Gewählter Tab bleibt innerhalb der
  Session erhalten (NAV-5), Neustart = Up Next. Die bisherige Persistenz des
  Panel-Tabs über Neustarts (`info_panel_tab`-Setting) entfällt **bewusst**;
  die Persistenz der Panel-*Sichtbarkeit* bleibt.

### Synced Lyrics (Lyrics-Tab)

- **NPP-5 · Zeilen-Styling** `[aktiv]` — Aktive Zeile 15 px bold weiß +
  Akzent-Unterstrich (26×2.5 px, zentriert, Farbe = Cover-Akzent).
  Nachbarzeilen gestuft: ±1 → weiß 45 %, ±2 → 32 %, weiter → 28 %. Alle Zeilen
  zentriert, 13 px, großzügiger Abstand (~13 px gap). Ganze LRC-Zeilen, kein
  Karaoke-Wort-Highlight.
- **NPP-6 · Zeilenwechsel-Motion** `[geplant]` — Beim Timestamp-Wechsel
  blendet die neue Zeile 45 % → weiß+bold, die alte zurück (Micro-Token
  150 ms ease-out); gleichzeitig scrollt die Liste die aktive Zeile mittig
  (Standard-Token, ease-out-cubic — kein Spring). Der Unterstrich wandert
  nicht — er gehört zur aktiven Zeile und faded mit ihr.
- **NPP-7 · Manuelles Scrollen gewinnt** `[geplant]` — User-Scroll pausiert
  den Auto-Scroll 4 s (Timer resettet bei jedem User-Scroll-Event); danach
  gleitet die Liste zurück zur aktiven Zeile. Während der Pause bekommt die
  aktive Zeile ihr Highlight weiter (nur kein Scroll). User-Scroll bricht auch
  einen laufenden Rück-Glide ab und startet die 4 s neu; programmatische
  Scrolls resetten den Timer nie.
- **NPP-8 · Klick = Seek** `[geplant]` — Klick auf eine Zeile seekt zum
  Timestamp (nur synced). Hover: weiß 65 % + Pointer. Einzige
  Klick-Interaktion im Lyrics-Tab; Lyrics-Text ist nicht selektierbar.
- **NPP-9 · Fallbacks** `[aktiv]` — Unsynced → statischer scrollbarer Text
  (13 px, weiß 65 %), kein Highlight, kein Auto-Scroll, Fuß „lyrics · tags".
  Keine Lyrics → dezenter Leerzustand („No lyrics found", keine Suche-CTA in
  v1) mit **Inline-Retry** bei Fehlern (Beschluss 9). Instrumental-Gap
  (>10 s ohne Zeile) → aktive Zeile bleibt, dimmt auf 60 %.
- **NPP-10 · Trackwechsel** `[geplant]` — Cover, Titelblock, Glow und
  Tab-Inhalt crossfaden gemeinsam (Standard-Token, MOT-5); Lyrics starten auf
  Zeile 0 zentriert. Kein Slide — Trackwechsel ist kein Ortswechsel.

### Verhalten & Kanten

- Seek (Waveform oder Lyrics-Klick) springt den Auto-Scroll sofort auf die
  neue aktive Zeile (kein 4-s-Timer).
- Pause friert das Highlight ein; Play nimmt es wieder auf. Ein pausierter
  Track zählt als geladen — das Panel zeigt ihn weiter. Das Panel folgt
  **immer** dem spielenden/geladenen Track, nie der Library-Selektion.
- `gtk-enable-animations=false`: alle NPP-Motions werden Hard-Switch (MOT-7,
  zentral über `ui/motion.rs`).
- Der gemeinsame Kopf (Cover/Glow/Titel/Toggle) ist ein Widget, die
  Tab-Inhalte wechseln darunter.

## Gegrillte Beschlüsse (2026-07-18, alle bestätigt)

1. **Artist News zieht um** — Der Information-Tab entfällt. Artist News gehört
   zum Künstler, nicht zum spielenden Track: Wiederanschluss als Sektion in
   der Artist-Detail-View nach Frame **22a** (eigener Folge-Task; Sektion nur
   bei Einträgen, genau eine Akzent-Release-Karte, ⟳ TIP-konform, Cache-Alter
   statt Fehlerbanner). Dieser Task **entkoppelt nur**: Worker, Cache und
   Settings bleiben unangetastet erhalten. Kein Informationsverlust bei den
   Track-Metadaten — Codec/Bitrate/Pfad leben im Tag-Editor bzw. den Spalten.
2. **Scope dieser Iteration** — Gemeinsamer Kopf + Lyrics-Tab (NPP-5..10) +
   Up-Next-Tab. Das **Visual-Segment erscheint erst mit dem
   Visualizer-Folge-Task** (21b): Die Labs-Regel „Plugin deaktiviert →
   Segment verschwindet aus dem Panel" macht die Zwei-Segment-Pill
   designkonform, solange das Plugin nicht existiert.
3. **Up-Next-Tab** — Nur **kommende** Tracks (der spielende hängt groß im
   Kopf — keine Dopplung, P-1). Rows im 21a-Stil (Cover 32 px, Titel 13.5 px,
   Artist dim). **Klick = Sprung** zu diesem Queue-Eintrag: explizite
   Nutzeraktion (PLAY-5-konform), übersprungene Einträge bleiben in der
   Queue-Historie — das Panel verwaltet nichts (kein Reorder, kein
   Entfernen; das kann die Queue-View). Leer: dezentes „Queue is empty".
   Fußzeile: „n tracks · Restdauer".
4. **Idle-Zustand** (kein geladener Track) — Platzhalter-Cover ohne Glow,
   Titelzeile dezent „Nothing playing", Tabs bleiben nutzbar (Up-Next-Klick
   startet die Wiedergabe). Nichts klappt von allein auf oder zu.
5. **Light-Theme** — Das Panel bleibt in beiden Farbschemata die **dunkle
   Bühne** (fixer neutral-dunkler Grund wie ein Player-Canvas): ein
   Alpha-Satz, konstanter Lyrics-Kontrast, Glow wirkt immer.
6. **Geometrie** — Links fix 240 px (ersetzt 220–280 px/22 %-Fraction),
   rechts fix 300 px (ersetzt 340 px). Responsive Collapse (<800 px) bleibt.
7. **14a-Flächenhierarchie** — Eigener schmaler Paralleltask (Branch
   `feat/theme-surface-hierarchy`): linke Sidebar eine Stufe heller als die
   Tabelle, Headerbar eine Stufe darüber, 1-px-Hairlines — in allen drei
   Dark-Themes; die Light-Paletten haben die Hierarchie bereits.
8. **Fallback-Akzent = Theme-Akzent (Petrol)** — `player_accent` wird pro
   Theme auf den Theme-Akzent gesetzt; das statische Orange (#e8703a)
   entfällt. Gilt einheitlich für Play-Button, Waveform, Glow, Unterstrich
   und später Visual. Umsetzung wegen Datei-Ownership (`theme.rs`,
   `cover_accent.rs`) im Paralleltask (Beschluss 7); Status-Flip dort nicht
   möglich — gilt mit dem Merge von `feat/theme-surface-hierarchy`.
9. **Kein Panel-Header** — ⟳/× entfallen ersatzlos (mockgetreu 21a).
   Schließen nur über den App-Header-Toggle (persistiert Sichtbarkeit
   weiterhin), Lyrics-Retry wandert als dezenter Inline-Button in den
   Fehlerzustand des Lyrics-Tabs.

## Selbstentscheidungen (Implementierungsebene)

- `info_panel_tab`-Setting samt Getter/Setter in `reprise-core` entfernen
  (NPP-4); keine Migration nötig — verwaiste Zeile in `settings` ist harmlos.
- Volume-Footer-Entfernung aus dem Diktat ist gegen den Code ein No-op (das
  Panel hatte nie einen); die Regel NPP-2 wird per Struktur-Test absichert.
- Glow als CSS-Radialverlauf auf dem Kopf-Widget (einmal pro Track gesetzt),
  kein Echtzeit-Blur — erfüllt „als Textur einmal rendern".
- Der Visualizer-Folge-Task übernimmt: Spektrum-Pipeline, Presets
  Rings/Flow/Pulse, F11-Fullscreen, Labs-Plugin-Schalter (21b), Ausklingen
  bei Pause, Idle statisch-minimal, MOT-2-Begründung (einzige Dauerbewegung,
  nur in diesem Tab).

## Folge-Tasks (nicht dieser Branch)

| Task | Referenz | Inhalt |
|------|----------|--------|
| Artist News in Artist-Detail-View | Frame 22a | News-Sektion unter Top Tracks, Release-Karte, „Remind me" `[geplant]` |
| Audio-Visualizer (Visual-Tab) | Frames 21b/10b/10c | GPU-Muster, Presets, F11, Labs-Plugin; Segment erscheint erst damit |
| NPP-Regeln → ux-rules.md Sektion P | dieses Dokument | Überführung + Traceability nach Merge-Lage |
| 14a-Flächenhierarchie + Petrol | Beschlüsse 7/8 | läuft parallel auf `feat/theme-surface-hierarchy` |
