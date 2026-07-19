# Netz-Features opt-in — Beschlussdokument (Grilling 2026-07-18)

Normativer Kontext für den Folge-Branch `feat/network-opt-in`. Er macht alle
vier Netz-Features zu bewussten Entscheidungen, ohne Bestandsnutzern etwas
wegzunehmen, und löst die Entdeckbarkeit, die dadurch verloren geht.

> **Reihenfolge:** setzt `feat/search-and-new-releases` voraus (dort entsteht
> das `new_releases`-Modul und die NR-Oberfläche). Erst mergen, dann hier
> starten — beide fassen `modules.rs` und die Plugins-Seite an.

> **Regelwerk:** NET-1/2, LYR-1..3 und DISCOVER-1/2 gehen als **Sektion S**
> nach `docs/ux-rules.md`, `[geplant]` beim Anlegen, Flip im
> Implementierungs-Commit, jede aktive Regel mit regelbenanntem Test.

## Audit-Befunde

- **Vier ungegatete Netz-Features**, nicht drei: `cover_download.rs`
  (coverartarchive.org + MusicBrainz), `artist_portrait/deezer.rs` (Deezer),
  `lyrics.rs` (LRCLIB) und der New-Releases-Fetch. Keiner hat heute einen
  `is_enabled`-Check, während Last.fm und ListenBrainz opt-in sind.
- **Module brauchen keine Migration** (`modules.rs`): der Schalter lebt als
  `module.<id>.enabled` in der `settings`-Tabelle, `is_enabled` fällt auf
  `default_enabled` zurück. Eine Migration wird nur für den *Bestandsschutz*
  gebraucht, der explizite `true`-Werte schreibt.
- **Cover- und Portrait-Caches sind auf der Platte nachweisbar**
  (`<XDG cache>/reprise/covers/downloaded`, Portrait-Cache analog) — daraus
  lässt sich „wurde bisher genutzt" ableiten. Lyrics und New Releases haben
  keinen solchen Bulk-Cache.

## Gegrillte Beschlüsse

1. **NET-1 — das Kriterium ist automatisch + massenhaft.** Ein Netz-Feature
   ist opt-in, wenn es **selbsttätig und in Menge** Daten holt
   (Hintergrund-Polling, library-weiter Bild-Download). Ein **on-demand
   ausgelöster Einzelabruf** braucht kein Toggle — die Nutzeraktion *ist* die
   Zustimmung. Daraus folgt: New Releases, Cover-Download und
   Portrait-Download sind opt-in; Online-Lyrics wären es nach diesem Kriterium
   **nicht** (LYR-2 macht sie strikt on-demand) — sie bekommen trotzdem ein
   Toggle, weil Nutzer, die gar keinen Netzverkehr wollen, eine Abschaltung
   brauchen.
2. **NET-2 — Bestandsschutz ist evidenzbasiert, nicht pauschal.** Die
   Migration schreibt `enabled = true` nur, wo die bisherige Nutzung
   nachweisbar ist:
   - **Cover-Download** → gecachte Cover vorhanden.
   - **Portrait-Download** → gecachte Portraits vorhanden.
   - **Online-Lyrics** → **die Existenz der Datenbank selbst** ist die
     Evidenz. Lyrics waren bisher ungated, also hatte sie jeder an; wer schon
     eine Library hat, behält sie. (Die ursprüngliche Vorgabe nannte hier ein
     Vorgänger-Flag `artist_news.enabled` — das war ein Verschreiber, es hat
     mit Lyrics nichts zu tun und hätte allen Bestandsnutzern eine laufende
     Funktion genommen.)
   - **New Releases** → kein Bulk-Cache, kein Vorgänger: bleibt auch im
     Bestand **aus**, außer das alte `module.artist_news.enabled` war an
     (dann war es eine bewusste Entscheidung, die erhalten bleibt).
   Frische Installationen starten in allen vier Fällen aus. Ziel ist
   ausnahmslos: **kein „plötzlich fehlt etwas" nach einem Update.**
   Test: `net_2_migration_preserves_existing_cover_usage` [core].
3. **LYR-1 — lokale Songtexte sind nie vom Toggle betroffen.** Eingebettete
   Lyrics (Tag) und Sidecar-Dateien (`.lrc`) werden **immer** angezeigt, ohne
   jeden Netzzugriff, auch bei ausgeschaltetem Toggle. Das Toggle regelt
   ausschließlich den LRCLIB-Abruf.
4. **LYR-2 — der Abruf ist strikt on-demand.** LRCLIB wird nur kontaktiert,
   wenn der Lyrics-Tab **offen** ist, der Song **keine lokalen** Lyrics hat
   **und** das Toggle an ist. Kein Prefetch, kein Batch, kein Nachladen im
   Hintergrund für kommende Queue-Einträge.
5. **LYR-3 — der Leerzustand ist die Aktivierungsfläche.** Tab offen, keine
   lokalen Lyrics, Toggle aus → zentrierte StatusPage: Icon, „Online-Songtexte
   sind deaktiviert", Untertitel „Aktiviere sie, um fehlende Texte automatisch
   zu laden", Button „In den Einstellungen aktivieren" (Deep-Link auf die
   Plugins-Zeile, hebt sie kurz hervor), Fußnote „Eingebettete Songtexte
   werden immer angezeigt." Toggle an, aber nichts gefunden → eigener Zustand
   „Keine Songtexte gefunden."
   **Konsequenz:** Der Lyrics-Tab ist dauerhaft sichtbar und damit seine
   eigene Entdeckbarkeit — er braucht **keinen** DISCOVER-Hinweis.
6. **DISCOVER-1 — kontextueller Einmal-Hinweis, nur mit Evidenz.** Features
   ohne dauerhaft sichtbare Fläche (New Releases, Cover, Portraits) bekommen
   je einen dezenten Inline-Hinweis mit ×, gesteuert durch ein Settings-Flag:
   **einmal gezeigt oder weggeklickt = nie wieder**, auch ohne Aktivierung.
   Platziert **an der Stelle der sichtbaren Lücke** und erst **nach dem ersten
   abgeschlossenen Scan**:
   - **Cover** → Kopfzeile im Album-Grid, nur wenn etwa **> 20 %** der Alben
     die Fallback-Kachel zeigen.
   - **Portraits** → Kopfzeile in der Artists-Ansicht, getriggert durch
     Initialen-Avatare.
   - **New Releases** → Zeile am Kopf der Artists-Ansicht.
   Der Hinweis ist **keine Badge-Bitte** (P-1): dezente Inline-Zeile, kein
   Punkt, kein Toast.
7. **DISCOVER-2 — nie stapeln, maximal eine Zeile pro View.** Treffen in der
   Artists-Ansicht Portrait- und NR-Hinweis gleichzeitig zu, erscheint **eine
   kombinierte** Zeile: „Netz-Features für Interpreten aktivieren (Bilder &
   neue Releases) →", Deep-Link auf die Plugins-Seite. Nie zwei
   „aktivieren"-Zeilen gleichzeitig.
8. **Plugins-Seite trägt alle vier** mit Privacy-Untertitel („contacts
   MusicBrainz" / „contacts coverartarchive.org" / „contacts LRCLIB"), Toggle
   und ggf. ComboRow darunter (New Releases: „nur Top-Artists / alle").
   Deep-Links aus den DISCOVER-Hinweisen und aus LYR-3 scrollen dorthin und
   **heben die Zielzeile kurz hervor** — sonst landet der Nutzer auf einer
   Liste und sucht.

## Offene Punkte für die Umsetzung

- Der 20-%-Schwellwert für den Cover-Hinweis braucht eine billige Zählung
  („wie viele Alben haben keine Kachel") — die Album-Ansicht kennt das beim
  Aufbau ohnehin. Nicht bei jedem Redraw neu rechnen.
- Die Hervorhebung der Zielzeile auf der Plugins-Seite braucht einen
  kurzlebigen CSS-Zustand; MOT-Token beachten, kein Dauerblinken.

---

## Korrekturen nach dem Code-Audit (2026-07-18, abends)

Der Audit hat drei Annahmen dieses Dokuments widerlegt und einen Regressionsfund
beigesteuert. Die folgenden Fassungen gehen vor.

1. **LYR-1 wird vertagt.** Reprise liest heute **keine** lokalen Songtexte —
   weder eingebettete Tags (USLT/Vorbis) noch `.lrc`-Sidecars; alles kommt von
   LRCLIB. LYR-1 ist damit kein Schutz für Vorhandenes, sondern neue
   Dateiformat-Arbeit. Sie gehört nicht in einen Opt-in-Umbau: LYR-1 bleibt
   `[geplant]`, dieser Branch baut nur Gating und Leerzustand.
   **Folge für LYR-3:** Die Fußnote „Eingebettete Songtexte werden immer
   angezeigt" darf **erst** erscheinen, wenn LYR-1 gebaut ist — sonst
   verspricht der Leerzustand etwas, das es nicht gibt.
2. **LYR-2 ist heute nicht erfüllt.** `sync_lyrics_track` läuft bei jedem
   Trackwechsel, unabhängig davon, ob der Lyrics-Tab sichtbar ist. „Nur bei
   offenem Tab" ist also eine eigene Änderung, nicht bloß eine Formulierung.
3. **DISCOVER-1 triggert am Sichtfenster, nicht bibliotheksweit.** Die Annahme
   „die Album-Ansicht kennt die Quote beim Aufbau" ist falsch — die
   Kachel-Entscheidung fällt pro sichtbarer Karte, off-thread. Neue Fassung:
   Der Hinweis erscheint, sobald **≥ 3 Fallback-Kacheln gleichzeitig sichtbar**
   sind; er **rastet ein** (Latch) und verschwindet nicht wieder beim Scrollen
   in cover-reiche Bereiche. Dismiss ist dauerhaft (Flag). Symmetrisch für den
   Portrait-Hinweis (sichtbare Initialen-Avatare). Keine teure Vorberechnung
   für einen Hinweis.
4. **Regression aus dem heutigen NR-Umbau:** `artist_news` wurde zu
   `new_releases` umgewidmet, aber `module.artist_news.enabled` ist verwaist —
   niemand liest es, keine Migration überträgt es. Wer das Feature früher
   eingeschaltet hatte, hat es nach dem Update stillschweigend aus. Die
   Migration dieses Branches trägt den Wert nach (das ist zugleich NET-2s
   Evidenz für New Releases).

### Selbstentscheidungen (Implementierungsebene)

- **Gecachte Portraits bleiben sichtbar.** Ein Gate an
  `ArtistPortraitRuntime::request` würde auch Cache-Treffer unterdrücken —
  gegen NET-2s Grundsatz „nichts verschwindet". `reprise-core` bekommt einen
  reinen Cache-Pfad; das Gate wählt, **welche** Core-Funktion der Worker ruft,
  statt den Versand zu unterdrücken. Cover haben diese Eigenschaft schon
  (`resolve_source` prüft den Download-Cache lokal).
- **Alle drei Module wirken live**, per gemeinsamem `Rc<Cell<bool>>` wie
  `ArtistNewsRuntime` — `CoverLoader` kopiert seinen Schalter heute einmalig
  bei der Konstruktion, deshalb würde eine Einstellungsänderung sonst nicht
  ankommen. Kein `Restart required` in den Untertiteln.
- **`ModuleDescriptor` bekommt ein `applies_live`-Feld**, statt die dritte
  String-ID-Sonderlocke in `plugin_applies_live` zu ergänzen.
- **Bestandsschutz als v13-Schritt mit Rust-Funktion** in derselben
  Transaktion (die bisherige reine `execute_batch`-Form kann kein Dateisystem
  prüfen). Die beiden Cache-Verzeichnisse werden als Parameter injiziert, damit
  Tests nicht vom echten `~/.cache` des Entwicklers abhängen; `.notfound`-
  Marker zählen **nicht** als Nutzungsnachweis.
- **Das Cover der New-Releases-Karten gehört zum `new_releases`-Modul**, nicht
  zu `cover_download` — es rendert ausschließlich im NR-Popover.
- **Sektionsbuchstabe ist T**, nicht S: S ist seit heute von STYLE-1 belegt.
