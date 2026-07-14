# Reprise „Sexy Adwaita" Redesign — Master-Implementierungsplan

> Kanonische Design-Quelle: der geteilte Design-Link (aktuelle Frames, die der
> Nutzer schickt). Das PDF in `docs/design/` gilt **nicht** (veraltet, bewusst
> gelöscht). PDF-abgeleitete Details vor Umsetzung gegen aktuelle Frames prüfen.

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI/Commit-Text, deutsche interne Doku; keine
realen Musikdateien/DBs; jeder App-Lauf headless (Xvfb, `GDK_BACKEND=x11`,
leeres `WAYLAND_DISPLAY`, privates `XDG_DATA_HOME`/`XDG_CACHE_HOME`, eigene
D-Bus-Session, `fakesink`) — **nie ein Fenster auf dem echten Desktop**. Alle
Gates vor jedem Commit; jede wesentlich geänderte Datei <800 Zeilen. STATUS-Lock
beachten (nur ein Agent auf `main`). Styling ausschließlich über `ui/style`
(`tokens.rs` + eine CSS-Quelle je Feature-`css()`-Sektion) — **keine**
per-Widget-`CssProvider`.

## Umfang

Kompletter UI-Re-Skin jeder Oberfläche + drei neue Subsysteme (echte Waveform,
Accent-aus-Cover, Artists-Ansicht) + neue Views + Copy-Rewrite (= volle
gettext-Neuübersetzung, POTFILES). Kein Fine-Tuning.

---

## Phase B — Bug-Vorlauf (zuerst)

**Befund (2026-07-14):** Die zwei gemeldeten Bugs (Info-Panel liegt über der
Tabelle; Ein-Zeilen-/Render-Gap beim Start) sind im **Quellcode bereits
behoben** — Info-Panel ist eine Sibling-Spalte (`information_column.rs`,
`b3d2592`), der Track-Content-Stack expandiert ab erster Allocation
(`track_list_layout.rs`). Das **installierte Binary** (`~/.local/bin/reprise`,
mtime 09:02) ist jedoch **vor** beiden Fixes (10:01 / 11:58) gebaut → veralteter
Build ist die Ursache.

**Aufgaben:**
1. `meson compile -C _build` + Reinstall des Binaries.
2. Headless-Repro mit Fixture-Library (privates XDG): verifizieren, dass (a) das
   Info-Panel als feste 340px-Spalte neben der Tabelle sitzt (Screenshot bei
   1600×900 mit offenem Panel) und (b) der erste Viewport voll rendert.
3. **Wenn im frischen Build reproduzierbar** → echter Bug → systematic-debugging
   (Verdächtige: `ColumnView`-Virtualisierung/`ScrolledWindow`-Adjustment für den
   Gap; `NavigationSplitView`/`InformationColumn`-Allocation für den Overlap).
   **Wenn nicht** → es war Staleness; Nutzer nutzt das frische Binary.

---

## Scrobbling-Redesign (entschieden)

Kanonischer Frame: Plugins-Seite (vertikale Nav), Scrobbling als **Inline-
`AdwExpanderRow`** je Dienst — kein Modal, keine Navigations-Unterseite.

**Muster je Dienst:** Hauptzeile = eingebauter Enable-Toggle + Connection-Badge
(„Connected as <user>" / „Not connected"). Body (aufklappbar): Credentials
inline, Optionen, **Test connection**, destruktiver **Disconnect**, Statuszeile.
Semantik: **Toggle = an/aus, behält Creds; Disconnect = Creds löschen.** Body
ausgegraut/Hinweis solange nicht verbunden. Toggle-state von `enable-expansion`
via `connect_enable_expansion_notify` entkoppeln (Body bleibt aufklappbar wenn
off). Last.fm-Browser-Autorisierung bleibt notgedrungen modal (System-Browser +
`AlertDialog`).

**Wiederverwendbar (präsentationsunabhängig):** alle Auth-/Keyring-/Persistenz-
Funktionen (`request_lastfm_authorization`, `exchange_lastfm_token`,
`enable_*`/`disconnect_*`/`persist_*_enabled`, `bootstrap`, `status_text`,
`authorization_decision`). **Neu bauen:** Page-Builder → ExpanderRow-Builder;
`push_*_page` → Inline-Wiring; `plugins_page` verzweigt (Scrobbling =
ExpanderRow, Rest = SwitchRow); `add_configure_button`/`bind_visibility` fallen
weg (dead code).

### Entschiedene offene Punkte

- **#1 Last.fm gebündelter Key (API-Fit, selbst entschieden):** Bundled Key+Secret
  als Default-Sign-in („no API key needed", nur Browser-Handshake) **+ versteckte
  Advanced-BYO-Key-Option** als Fallback. Grund: friktionsarme Mock-UX, aber der
  gebündelte Key ist in einem OSS-Binary extrahierbar und ein Single-Point-of-
  Failure (Ban/Rate-Limit bricht Scrobbling für alle) — der BYO-Fallback gibt
  Power-Usern einen Ausweg. **Aktionspunkt Nutzer:** eigene „Reprise"-Last.fm-App
  registrieren + Rotation verantworten (blockiert erst den Release, nicht die
  Umsetzung).
- **#2 Backend-Zusätze (v1-Scope):**
  - **Test connection → v1 IN.** Testet gespeicherte Creds (`validate_token`
    existiert für beide Dienste), transientes Inline-Ergebnis; das persistente
    Badge/Status bleibt Sache des Hintergrund-Workers (eine Wahrheit). Stale-Guard
    (ureq blockiert bis ~10s).
  - **„N listens submitted"-Zähler → v1 IN.** Persistenter kumulativer Zähler je
    Provider über die Settings-K/V-Tabelle (keine Migration), Increment **innerhalb**
    der `acknowledge_for`-Transaktion, neues Feld an `ConnectionStatus`. Lifetime,
    überlebt Disconnect. Bestandsinstallationen starten bei 0.
  - **ListenBrainz „ratings as feedback" → VERTAGT (nicht v1).** Blockiert durch
    fehlenden Recording-MBID/MSID (Reprise speichert keinen), verlustbehaftetes
    5-Stern→love/hate-Mapping, `RefCell`-Reentrancy-Risiko im Rating-Callback,
    Konflikt mit „Netzwerk default-off". Eigener späterer Task.

---

## My Stats (entschieden)

Neue Tabelle `listen_events` (`track_id`, `played_at`, `ms_played`), geschrieben
am **bestehenden** Scrobble-Schwellwert in `play_tracking.rs`. Quellen-Dropdown
im Screen: **Lokal** (Default beim Öffnen) + verbundene Dienste.
- **Lokal (Hybrid):** Headline-Stunden + Top Artists/Albums/Tracks aus all-time
  `play_count×duration` (sofort voll, inkl. Rhythmbox-Import); **12-Monats-
  Balkenchart** + „diese Woche/Monat" aus `listen_events` (ab heute).
- **ListenBrainz:** `stats/user/.../artists` + `listening-activity`-Zeitreihe.
  **Last.fm:** `getTopArtists/Albums` + `getInfo`; Monatschart aus Weekly-Charts.
- Lokal + Remote **nie** summieren. Remote-Fehler → Banner + Fallback Lokal.
- Screen-Elemente (Frame): „312 hours of listening", plays, new artists, most
  active day, Top-Listen, 12-Monats-Chart, Jahr-Selektor.

---

## Lieferstruktur — Fundament-first, je eigener Merge

- **P0 · Design-System:** `tokens.rs` + zentrale CSS: **Multi-Theme** (N benannte
  Dark-Paletten + Picker + Persistenz + Live-Switch), Glow, Blur, Radius,
  Spacing, **neue Hover-Effekte**; eine Referenz-Oberfläche validiert. Jede
  spätere Oberfläche gegen alle Themes prüfen.
- **P1 · Player-Bar + Toasts:** volle Breite (Cover+Titel, glühender Play-Button,
  Volume, Queue), Waveform-Seek-Platzhalter bis P5; Toast = dunkles Pill +
  Akzent-Aktion (interaktiv bleibt).
- **P2 · Header + Views:** `Tracks | Albums | Artists`-Switcher; **Artists-View
  neu**; Fenster-Chrome.
- **P3 · Dialoge & Chrome:** Settings (vertikale Nav), Kontextmenüs, Tag-Editor
  (inkl. Mixed-Multi-Edit), Spaltenlayout-Editor, Equalizer/Audio-Settings-Restyle,
  **Scrobbling-Inline-Expander** (siehe oben), **Copy-Rewrite** + gettext.
  MPRIS: **kein Toggle** (immer an) — bewusst entgegen dem Mock; „Integration"-
  Sektion entfällt, `MPRIS_MODULE` raus, `mpris::start` unbedingt.
- **P4 · My Stats.**
- **P5 · Subsysteme:** echte Waveform (GStreamer-Peaks + Cache + Migration +
  Ladezustände) · Accent-aus-Cover (Dominantfarbe → Waveform/Play/Now-Playing,
  Fallback Petrol) · Artists-Tiefe.

## Offen (in Phasenplanung klären)

Artists-View-Inhalt · Kontextmenü-Treue (custom vs. Adwaita-Popover) · konkrete
Paletten je Theme · exakte Copy-Strings · Artist-&-Album-News-Doppel-Toggle
(Plugins + In-Panel schreiben denselben Flag → auf eine Quelle reduzieren).
