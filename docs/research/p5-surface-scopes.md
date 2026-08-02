# P5 — Geltungsbereich je UX-Regel (Vorschlag, noch nicht angewandt)

Erzeugt 2026-08-01 aus vier parallelen Durchgängen über `docs/ux-rules.md`.
**Noch nicht in die Regeldatei eingetragen** — siehe „Warum noch nicht angewandt“.

455 Regeln, Verteilung:

| Marker | Anzahl |
| --- | --- |
| `[surface:gtk]` | 332 |
| `[surface:gtk][surface:desktop]` | 62 |
| `[surface:all]` | 61 |

| Regel | Abschnitt | Geltungsbereich | Begründung |
| --- | --- | --- | --- |
| P-1 | A–E | `[surface:gtk]` | nennt AdwStatusPage als GTK-Mechanik namentlich; Prinzip evtl. universell, Text bindet an libadwaita-Vokabular |
| P-2 | A–E | `[surface:all]` | „Klick reagiert sofort" verallgemeinert zu Tap; Prinzip gilt für jede Eingabeart |
| P-3 | A–E | `[surface:gtk][surface:desktop]` | Hover hat auf Touch keine Entsprechung |
| P-4 | A–E | `[surface:all]` | kein ungebetenes Layout-Shifting ist universelles Prinzip, auch wenn Beispiele gtk-spezifisch sind |
| P-5 | A–E | `[surface:all]` | Datenmodell-Trennung Historie/Katalog, keine gtk-spezifische Mechanik im Resttext (Nachfolger BROWSE-6 außerhalb meines Bereichs) |
| P-6 | A–E | `[surface:all]` | Core-Level Beweisregel (Mount/Eject), plattformunabhängige Logik |
| NAV-1 | A–E | `[surface:gtk]` | listet gtk-exklusive Places (Devices, Issues) und My Stats; Nachfolger BROWSE-1 muss Places pro Fläche neu fassen |
| NAV-2 | A–E | `[surface:all]` | globaler History-Stack ist Core-Logik, jede Fläche mit Detail-Navigation schuldet Back |
| NAV-2a | A–E | `[surface:all]` | Teil derselben Core-Stack-Logik wie NAV-2 |
| NAV-3 | A–E | `[surface:all]` | Klick-Navigation auf Artist/Album gilt überall (Tap-Äquivalent); Hover-Unterstreichung darin ist zeigergebunden (gtk/desktop only) |
| NAV-4 | A–E | `[surface:gtk][surface:desktop]` | Doppelklick-vs-Einzelklick-Select-Paradigma + Enter-Taste existieren auf Touch nicht in dieser Form |
| NAV-5 | A–E | `[surface:gtk]` | Scroll+Selection-Gedächtnis an gtk-Sidebar/Place-Modell gekoppelt; Nachfolger BROWSE-2 evtl. breiter gefasst |
| NAV-6 | A–E | `[surface:all]` | Live-Filter-Suche+Esc-Verhalten ist universell, Ctrl+F ist nur die gtk/desktop-Tastenkombination dafür |
| NAV-7 | A–E | `[surface:gtk]` | konkrete GNOME-Hamburger-Menüstruktur (u.a. Keyboard-Shortcuts-Overlay) namentlich benannt |
| NAV-8 | A–E | `[surface:gtk][surface:desktop]` | My Stats ist laut Vorgabe ohne Mobile-Scope, Desktop hat Statistiken lesend |
| NAV-9 | A–E | `[surface:gtk][surface:desktop]` | Ctrl+L-Tastenkürzel; Verweis-Stub, ersetzt durch NAV-9a/GRID-5 |
| NAV-9a | A–E | `[surface:gtk][surface:desktop]` | Ctrl+L ist Tastatur-Mechanik ohne Touch-Äquivalent |
| NAV-9b | A–E | `[surface:gtk][surface:desktop]` | wie NAV-9a; App-weite Intent-Trennung evtl. universeller unter BROWSE-4, Ctrl+L-Bindung bleibt gtk/desktop |
| NAV-11 | A–E | `[surface:gtk]` | beschreibt gtk-Sidebar-Baum in konkreter AT-SPI-Mechanik |
| NAV-12 | A–E | `[surface:gtk]` | benennt Header-Bar-‹-Button als libadwaita-Chrome-Element |
| NAV-13 | A–E | `[surface:gtk][surface:desktop]` | Selection/Keyboard-Focus/Viewport-Erhalt ist Desktop-Paradigma ohne Touch-Entsprechung |
| PLAY-1 | A–E | `[surface:all]` | WYSIWYG-Queue-Aufbau aus sichtbarer Liste ist Kernverhalten; Doppelklick steht hier für „Aktivieren" (Tap auf Mobile) |
| PLAY-1a | A–E | `[surface:all]` | Core-Logik für Container-Play, plattformunabhängig |
| PLAY-2 | A–E | `[surface:all]` | Aktivieren einer Zeile hängt Rest der sichtbaren Liste an — gilt für Tap wie Doppelklick |
| PLAY-3 | A–E | `[surface:all]` | reiner Verweis-Stub auf 3a/3b, gleiche Einstufung wie diese |
| PLAY-3a | A–E | `[surface:all]` | Core-Queue-Logik „Filter/Suche schränkt Shuffle ein" gilt überall, wo eine sichtbare Trefferliste existiert |
| PLAY-3b | A–E | `[surface:all]` | Queue-Snapshot-Verhalten ist Core-Logik |
| PLAY-4a | A–E | `[surface:all]` | „Missing" beim Wiedergeben stillschweigend überspringen ist Core-Bibliothekslogik |
| PLAY-4b | A–E | `[surface:gtk]` | „Show in Missing files" verweist auf Import-Fehler-Verwaltung, laut Vorgabe gtk-exklusiv |
| PLAY-5 | A–E | `[surface:all]` | reiner Verweis-Stub auf 5a/5b/5c |
| PLAY-5a | A–E | `[surface:all]` | Core-Queue-Hygiene bei gelöschten Dateien, plattformunabhängig |
| PLAY-5b | A–E | `[surface:gtk]` | „unmounted"/Mount-Event betrifft Wechseldatenträger-Bibliotheken; für neue Flächen (nur einfacher Bibliothekspfad) nicht im Scope |
| PLAY-5c | A–E | `[surface:gtk]` | Episoden/Podcasts sind laut Vorgabe gtk-exklusiv |
| PLAY-6 | A–E | `[surface:all]` | Shuffle/Repeat als globaler Player-State gehört zu „Wiedergabe und Transport", Kernscope aller Flächen |
| PLAY-7 | A–E | `[surface:gtk]` | Verweis-Stub; Nachfolger PLAY-7a ist gtk-spezifische Chrome-Mechanik |
| PLAY-7a | A–E | `[surface:gtk]` | „Glass zones"/Scroll-Inset ist konkrete gtk-CSS-Overlay-Technik |
| PLAY-7b | A–E | `[surface:all]` | Player-Bar beansprucht eigene Höhe statt Content zu überlagern — allgemeines Layoutprinzip |
| PLAY-8 | A–E | `[surface:all]` | Immutable-Snapshot-Wiedergabemodell ist Core-Engine-Logik |
| PLAY-9 | A–E | `[surface:all]` | Play/Pause startet Zufallswiedergabe — Core-Transport-Verhalten für jede Fläche mit Play/Pause-Control |
| ALB-1 | A–E | `[surface:gtk][surface:desktop]` | Hover-Overlay+Kontextmenü sind zeigergebunden; Aktivierung/Container-Play wandert in Nachfolgeregeln |
| ALB-2 | A–E | `[surface:all]` | Album-Detail (Hero, Play/Shuffle, Tracklist, einheitliche Playing-Markierung) ist Kern-Screen jeder Fläche; „Accent Pipeline" ist nur Visual-Implementierungsdetail |
| GRID-1 | A–E | `[surface:gtk]` | nennt @reprise_player_accent und gtk-enable-animations als konkrete GTK-Mechanik |
| GRID-2 | A–E | `[surface:gtk]` | GtkGridView, Pfeiltasten-Fokus, Ctrl+Enter, Menu-Taste, Shift+F10 — durchgehend gtk-Mechanik |
| GRID-3 | A–E | `[surface:gtk]` | GTK-CSS-Variable @accent_color + Tastatur-Fokusring, konkrete Widget-Mechanik |
| GRID-4 | A–E | `[surface:gtk]` | Hover/Fokus-Gradient + GTK-CSS-Variable + Tooltip-Verzicht, gtk-spezifisch |
| GRID-5 | A–E | `[surface:gtk]` | GtkGridView/Adjustment und gtk-enable-animations namentlich genannt |
| GRID-6 | A–E | `[surface:gtk][surface:desktop]` | Tastaturfokus-Wiederherstellung ist Desktop-Paradigma ohne Touch-Entsprechung |
| GRID-7 | A–E | `[surface:gtk]` | Textur-Cache/Blur-Rendering-Optimierung ist gtk-spezifisches Implementierungsdetail |
| GRID-8 | A–E | `[surface:gtk]` | benennt konkrete Widget-Layer (Ambient-Layer, Grid-Page, Scroller) der gtk-Implementierung |
| ART-1 | A–E | `[surface:gtk][surface:desktop]` | Split-Pane „Detail rechts" ist Desktop-Layoutmuster; Mobile navigiert vermutlich vollflächig |
| ART-2 | A–E | `[surface:gtk]` | Hero-Glow-Textur/Crossfade ist gtk-Rendering-Detail, plus Hover und FIL-1c-Filterpille |
| FX-1 | A–E | `[surface:gtk]` | nennt gtk-enable-animations als GSetting-Schalter namentlich |
| MTP-1 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-2 | A–E | `[surface:gtk]` | Verweis-Stub innerhalb der gtk-exklusiven MTP-Fläche |
| MTP-3 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-4 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-5 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-6 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-7 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-8 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-9 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-10 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-11 | A–E | `[surface:gtk]` | Verweis-Stub innerhalb der gtk-exklusiven MTP-Fläche |
| MTP-12 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-13 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-14 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-15 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-16 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-17 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-18 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-19 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-20 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-21 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-22 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-23 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-24 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-25 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-26 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-27 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-28 | A–E | `[surface:gtk]` | Verweis-Stub innerhalb der gtk-exklusiven MTP-Fläche |
| MTP-29 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-30 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-31 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-32 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-33 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-34 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-35 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-36 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche (betrifft zusätzlich Podcasts, ebenfalls ausgeschlossen) |
| MTP-37 | A–E | `[surface:gtk]` | Verweis-Stub (ersetzt durch MTP-51) innerhalb der gtk-exklusiven MTP-Fläche |
| MTP-38 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-39 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-40 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche (betrifft zusätzlich Podcasts, ebenfalls ausgeschlossen) |
| MTP-41 | A–E | `[surface:gtk]` | Verweis-Stub (ersetzt durch MTP-45) innerhalb der gtk-exklusiven MTP-Fläche |
| MTP-42 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche (betrifft zusätzlich Podcasts, ebenfalls ausgeschlossen) |
| MTP-43 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-44 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche (betrifft zusätzlich Podcasts, ebenfalls ausgeschlossen) |
| MTP-45 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche (betrifft zusätzlich Podcasts, ebenfalls ausgeschlossen) |
| MTP-46 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche (betrifft zusätzlich Podcasts, ebenfalls ausgeschlossen) |
| MTP-47 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche (betrifft zusätzlich Podcasts, ebenfalls ausgeschlossen) |
| MTP-48 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-49 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-50 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| MTP-51 | A–E | `[surface:gtk]` | Geräte-Sync/MTP existiert in keiner neuen Oberfläche |
| SET-1 | F–R | `[surface:gtk]` | GTK-Preferences-Fenster-Navigationsmechanik, kein Äquivalent im neuen Scope beschrieben |
| SET-2 | F–R | `[surface:gtk]` | Subpage-Navigation im selben GTK-Fenster (‹-Back), reine Fenstermechanik |
| SET-3 | F–R | `[surface:gtk]` | Modal-Layer-Regel nennt explizit Tag Editor (gtk-only) und GTK-Fensterschichtung |
| SET-4 | F–R | `[surface:gtk]` | Auto-Clean-Dialog hängt an Device-Sync-Kaskade (MTP ausgeschlossen) + GTK-Settings-Sofort-Anwenden-Mechanik |
| SET-5 | F–R | `[surface:gtk]` | reine GTK-Preferences-Layout-Abstandsregel |
| SET-6a | F–R | `[surface:gtk]` | Plugins-Seiten-Gruppierung inkl. Scrobbling, GTK-Preferences-Struktur |
| SET-6b | F–R | `[surface:gtk]` | Scrobbling-Einrichtung ist laut Auftrag explizit ausgeschlossen |
| SET-7 | F–R | `[surface:gtk]` | Concerts/New-Releases als eigene Preferences-Seiten — beide Features ausgeschlossen |
| SET-8 | F–R | `[surface:gtk]` | „Online sources"-Seite bündelt YouTube/Podcasts/Radio — alle ausgeschlossen |
| SET-9 | F–R | `[surface:gtk]` | dito plus Phone-Sync/MTP (E-5/E-8) — ausgeschlossen |
| SET-10 | F–R | `[surface:gtk]` | Plugins als einzige Settings-Oberfläche für optionale Capabilities (Scrobbling, Online-Content) — durchweg gtk-only-Feature-Cluster |
| SET-11 | F–R | `[surface:gtk]` | Online-content-Gruppen-Masterswitch (Podcasts/Radio/YouTube) — ausgeschlossen |
| FB-1 | F–R | `[surface:gtk]` | „Pille, bottom-centered" ist konkrete Adw-Toast-Formmechanik; Timing/Undo-Intent wäre grundsätzlich auf alle übertragbar, aber so wie geschrieben ein Widget-Spec |
| FB-2 | F–R | `[surface:gtk]` | (replaced) Relink-Scan-Card im Issues-Bereich → Missing-files-Feature ausgeschlossen |
| FB-2a | F–R | `[surface:gtk]` | Relink-Scan-Card, Klick führt zu Missing files (ausgeschlossenes Feature) |
| FB-2b | F–R | `[surface:gtk]` | Scan/Sync/Playlist-Import-Card im selben Issues-Kontrakt wie FB-8 |
| FB-3 | F–R | `[surface:gtk]` | „N failed · Details"-Batching endet in ISSUES-Badge/Import-Errors — beides ausgeschlossene Fläche |
| FB-4 | F–R | `[surface:gtk]` | Badge-Zählung exklusiv für Missing/Import Errors — beide ausgeschlossen |
| FB-5 | F–R | `[surface:gtk]` | (replaced) StatusPage-Aufteilung, AdwStatusPage-Widget + Missing-Feature |
| FB-5a | F–R | `[surface:gtk]` | Missing-files-Leerzustand — ausgeschlossenes Feature |
| FB-5b | F–R | `[surface:gtk]` | AdwStatusPage-Widget für Root-Guard; die Situation „Root nicht verfügbar" könnte anderswo vorkommen, aber diese Regel spezifiziert das GTK-Widget dafür |
| FB-6 | F–R | `[surface:gtk]` | Missing-Row-Grauung + ISSUES-Badge sind ausgeschlossenes Feature; der Skip-Toast-Teil („Track unavailable — skipped") wäre für jeden Player sinnvoll, ist hier aber untrennbar mit der Missing-Logik verwoben |
| FB-7 | F–R | `[surface:gtk]` | „Remove from library" ist keine Mobile/Desktop-Aktion — beide Scopes sind bei der Bibliothek lesend (Mobile: browsen; Desktop: zusätzlich nur Filter/Smart-Playlists/Stats lesend) |
| FB-8 | F–R | `[surface:gtk]` | Scan/Relink/Doctor-Card-Stack im Issues-Bereich — Doctor und Missing/Import-Errors sind ausgeschlossen; Erstscan auf Mobile hat laut Auftrag nur „Fortschritt", nicht diesen Card-Stack-Mechanismus |
| OS-1 | F–R | `[surface:gtk]` | Dateizuordnung/Doppelklick-Öffnen ist in keinem der beiden neuen Scopes genannt |
| OS-2 | F–R | `[surface:gtk]` | Transient-Track-Konzept für Datei außerhalb der Bibliothek, OS-Dateiverknüpfung |
| OS-3 | F–R | `[surface:gtk]` | Mehrfachauswahl „Open with Reprise" — Desktop-Dateimanager-Feature, nicht im Scope |
| OS-4 | F–R | `[surface:gtk]` | Single-Instance/D-Bus-Aktivierung ist Linux-Desktop-Mechanik |
| OS-5 | F–R | `[surface:gtk]` | nennt MPRIS explizit — Linux-spezifisches Mediensteuerungsprotokoll |
| START-1 | F–R | `[surface:gtk]` | Scroll-Anchor-Restore und „Reconcile-Card" sind GTK-Mechanik; die Grundidee „letzter Zustand beim Start" wäre für Mobile/Desktop denkbar, aber so wie geschrieben an GTK-Mechanismen gebunden |
| START-2 | F–R | `[surface:gtk]` | AdwStatusPage + Root-Guard/Missing-Kopplung, ausgeschlossene Fläche |
| QUE-1 | F–R | `[surface:gtk]` | ColumnView+Panel-Dual-Surface-Design ist konkrete GTK-Mechanik, obwohl Queue selbst überall existiert |
| QUE-2 | F–R | `[surface:all]` | reine Verhaltens-/Gruppierungsregel („Next in Queue" vs. „Continuing from …"), kein GTK-Widget genannt, Queue ist Mobile/Desktop-Scope |
| QUE-3 | F–R | `[surface:all]` | Verhalten (gespielte manuelle Einträge verschwinden still), kein Widget genannt, gilt für jede Queue-UI |
| QUE-4 | F–R | `[surface:gtk]` | gemeinsame Rust-Formatierfunktion ist codebase-interne DRY-Regel, kein plattformübergreifender UX-Vertrag |
| QUE-5 | F–R | `[surface:all]` | reine Sprung-/Remove-Semantik der Queue, kein Widget genannt |
| QUE-6 | F–R | `[surface:gtk]` | ColumnView-Row-Recycling/Batch-Query-Implementierungsdetail, an QUE-1-Dual-Surface gebunden |
| QUE-9 | F–R | `[surface:gtk]` | RSS/YouTube-Episoden + MPRIS-Episode-Objektpfad — Podcasts sind ausgeschlossen |
| FIL-1a | F–R | `[surface:gtk]` | Filterzeile/Chip mit Pixel-Hit-Targets (≥20px), header-bar-Suche — GTK-Mechanik |
| FIL-1b | F–R | `[surface:gtk]` | dieselbe Chip-Zeilen-Mechanik für Grid-Modus |
| FIL-1c | F–R | `[surface:gtk]` | Place-Pill-Spezifika (Kontur, „‹"-Präfix, NAV-2-Historie) |
| FIL-2 | F–R | `[surface:gtk]` | FILTER-Label/Idle-Zustände/Status-Overlay unten rechts — GTK-Layout |
| FIL-3 | F–R | `[surface:gtk]` | nennt „ColumnView"-Virtualisierung explizit |
| FIL-4 | F–R | `[surface:gtk]` | Suchfeld-CSS-Styling (Akzent-Border/Tint) |
| FIL-5 | F–R | `[surface:gtk]` | nennt Pango explizit |
| FIL-6 | F–R | `[surface:gtk]` | AdwStatusPage-Leerzustand |
| FIL-7 | F–R | `[surface:gtk]` | Feature hängt am INST/Stem-Separation-Cluster (ausgeschlossen, INST-11) zusätzlich zur Filterzeilen-Chip-Mechanik |
| FIL-8 | F–R | `[surface:gtk]` | Sidebar-Place/Pill-Mechanik dominiert die Regel; der Scope-Gedanke „letzte 7 Tage, kein Limit" wäre für mobiles Browsen plausibel übertragbar — einer meiner unsichersten Fälle |
| FIL-9 | F–R | `[surface:gtk]` | ColumnView-Scroll-Anchor-Mechanik (ID+Offset) |
| TAG-1 | F–R | `[surface:gtk]` | Tag Editor existiert nicht außerhalb GTK |
| TAG-2 | F–R | `[surface:gtk]` | dito |
| TAG-3 | F–R | `[surface:gtk]` | dito |
| TAG-4 | F–R | `[surface:gtk]` | dito |
| TAG-5 | F–R | `[surface:gtk]` | dito |
| TAG-6 | F–R | `[surface:gtk]` | dito |
| TAG-7 | F–R | `[surface:gtk]` | dito |
| TAG-7a | F–R | `[surface:gtk]` | dito |
| TAG-7b | F–R | `[surface:gtk]` | dito |
| TAG-8 | F–R | `[surface:gtk]` | dito |
| TAG-9 | F–R | `[surface:gtk]` | dito |
| TIP-1a | F–R | `[surface:gtk][surface:desktop]` | Tooltip-Existenzregel für zeigergebundene Flächen; Mobile hat kein Hover |
| TIP-1b | F–R | `[surface:gtk][surface:desktop]` | Formulierungsregel für Tooltip-Text, gilt nur wo Tooltips existieren |
| TIP-1c | F–R | `[surface:gtk][surface:desktop]` | dito (replaced) |
| TIP-1d | F–R | `[surface:gtk][surface:desktop]` | dito |
| TIP-2a | F–R | `[surface:gtk][surface:desktop]` | disabled-icon-only-Tooltip-Begründung, zeigergebunden |
| TIP-2b | F–R | `[surface:all]` | fordert sichtbare Begründung STATT reinem Tooltip — genau die Brücke zu Touch, gilt überall |
| TIP-3 | F–R | `[surface:all]` | „touch operation never sees tooltips" ist explizit die Cross-Modality-Brückenregel — muss für alle drei Flächen gelten |
| TIP-4 | F–R | `[surface:gtk][surface:desktop]` | Menü-Tooltip-Verzicht, nur relevant wo Menüs Hover kennen |
| TIP-5 | F–R | `[surface:gtk]` | nennt „GTK default behavior" explizit — kein Desktop-Äquivalent beschrieben |
| TIP-6 | F–R | `[surface:gtk][surface:desktop]` | Shortcut-Hinweise sind tastaturgebunden; Desktop hat Tastatur, Mobile primär nicht |
| CTX-1 | F–R | `[surface:gtk]` | reine Builder-Implementierung (GMenu, Rust-Funktion `build_track_menu`), keine übertragbare UX-Aussage |
| CTX-2 | F–R | `[surface:gtk][surface:desktop]` | Selektions-Scoping-Prinzip wäre auf Mobiles Bottom-Sheet übertragbar, aber Rechtsklick-Vorselektion + Shift+F10 dominieren die Regel — einer meiner unsichersten Fälle |
| CTX-3 | F–R | `[surface:all]` | Menüinhalt-Semantik (kein doppeltes „Play", „Play next" zuerst) — Aktion, keine Auslösung, portiert auf Mobile-Bottom-Sheet |
| CTX-4 | F–R | `[surface:all]` | Navigationseintrags-Logik (grau bei uneindeutigem Ziel) ist reine Aktions-Semantik, keine Auslösungsmechanik |
| CTX-5a | F–R | `[surface:gtk]` | mischt portable Playlist/Queue-Removal (gälte als [alle]) mit gtk-only „Remove from library"/„Move to Trash" — Mehrheit der Regel ist library-only-Aktion, daher gtk |
| CTX-5b | F–R | `[surface:gtk]` | reine Remove-from-library-Änderung, gtk-only |
| CTX-6 | F–R | `[surface:gtk]` | Zähl-Konvention mischt Playlist/Queue (portabel) mit Library/Trash/„Edit tags…" (gtk-only); letzteres überwiegt |
| CTX-7 | F–R | `[surface:gtk]` | nennt „GTK popover" explizit als Flip-Mechanik |
| CTX-8 | F–R | `[surface:gtk]` | Missing-files-Feature (ausgeschlossen) + „Show in Files"/Nautilus + „Edit tags…" (Tag Editor) |
| CTX-9 | F–R | `[surface:all]` | „Add to playlist ▸"-Untermenü-Semantik (alphabetisch, „New playlist…" am Ende, aktuelle Playlist grau) portiert direkt auf Mobile |
| CTX-10 | F–R | `[surface:gtk]` | nennt Nautilus explizit — GNOME-Dateimanager |
| CTX-11 | F–R | `[surface:gtk]` | Podcast-Episoden im Queue-Menü — Podcasts ausgeschlossen |
| CTX-12 | F–R | `[surface:gtk]` | Podcast-Episode-Revalidierung — ausgeschlossen |
| MOT-1 | F–R | `[surface:gtk]` | Token-Datei `ui/motion.rs` + `AdwSpringAnimation` konkret benannt; Intent „endliches Set benannter Duration-Tokens statt Magic Numbers" wäre für jede Fläche sinnvoll, aber diese Regel spezifiziert die GTK-Werte |
| MOT-2 | F–R | `[surface:all]` | Prinzip „User-Aktion animiert, Hintergrund schaltet hart" ist reine P-4-Verhaltensregel; die genannte „process card" ist nur ein GTK-Beispiel, keine Voraussetzung |
| MOT-3 | F–R | `[surface:gtk]` | `adw::OverlaySplitView`/StatusPage explizit benannt — codebase-interne Widget-Symmetrie, keine übertragbare Aussage |
| MOT-4 | F–R | `[surface:gtk]` | (replaced) Podcasts⇄Music-Beispiel (ausgeschlossen) + „200-item window" GTK-Virtualisierung |
| MOT-5 | F–R | `[surface:gtk]` | waveform-draw-time-Desaturierung, `cover_accent`-Pipeline, EQ-Indikatoren sind Implementierungsdetails; Intent „Player-Statuswechsel animiert weich statt hart" wäre für Mobile Now-Playing sinnvoll |
| MOT-6 | F–R | `[surface:all]` | Prinzip „Modell ändert sich bei Frame 0, Animation blockiert nie, laufende Animation ist jederzeit sicher unterbrechbar" ist plattformunabhängig zentral; `AdwAnimation::skip()` ist nur die GTK-Mechanik dafür |
| MOT-7 | F–R | `[surface:gtk]` | `gtk-enable-animations=false` ist ein GNOME-Setting; das Prinzip „OS-Reduced-Motion zentral statt an 30 Stellen respektieren" müsste pro Plattform neu verdrahtet werden (prefers-reduced-motion / Android Animator-Scale) |
| MOT-8 | F–R | `[surface:gtk]` | windowed-model/Podcasts-YouTube-Beispiel GTK-spezifisch; Intent „Listen faden nicht zeilenweise ein" wäre für virtualisierte Listen auf jeder Fläche sinnvoll |
| NPP-1 | F–R | `[surface:gtk]` | `AdwOverlaySplitView`-Pixel-Kontrakt (240px/300px) |
| NPP-2 | F–R | `[surface:gtk]` | pixelgenaues Panel-Layout (Cover 168px, Pill-Toggle-Segmente, Footer) |
| NPP-3 | F–R | `[surface:gtk]` | Rendering-Detail (radialer Glow-Gradient, kein Live-Blur) |
| NPP-4 | F–R | `[surface:gtk]` | Tab-Struktur mit Pill-Toggle-Segmenten ist GTK-Panel-spezifisch; Mobile-Now-Playing hat laut Auftrag nur Cover+Lyrics ohne beschriebene Tab-Struktur |
| NPP-5 | F–R | `[surface:gtk]` | pixelgenaue Farb-/Größenhierarchie der Lyrics-Zeilen; der Intent (aktive Zeile hervorgehoben, Nachbarn gestuft gedimmt, ganze LRC-Zeilen statt Karaoke-Wörter) ist aber sehr wahrscheinlich auch für Mobile-Lyrics gewollt — einer meiner unsichersten Fälle |
| NPP-6 | F–R | `[surface:gtk]` | Motion-Tokens (Micro/Standard) GTK-spezifisch; das Scroll-zu-Mitte-Verhalten beim Zeilenwechsel könnte als Intent portieren |
| NPP-7 | F–R | `[surface:all]` | reines Zeit-/Verhaltensprinzip (manuelles Scrollen pausiert Auto-Scroll für 4s, Timer-Reset-Regeln), kein GTK-Widget genannt, Lyrics ist Mobile-Scope |
| NPP-8 | F–R | `[surface:all]` | Klick/Tap-zum-Seek ist universell und für Touch direkt übertragbar; der Hover-Lift-Teil ist zeigergebunden (Desktop ja, Mobile nein) — Aktionskern trägt trotzdem [alle] |
| NPP-9 | F–R | `[surface:all]` | reine Fallback-Zustandslogik für Lyrics (unsynced/leer/Fehler/Instrumental-Lücke), kein Widget genannt, gilt für jede Lyrics-Ansicht |
| NPP-10 | F–R | `[surface:gtk]` | (replaced) Crossfade-Kontrakt fest an MOT-5/MOT-7-Tokens gebunden |
| SEARCH-1 | F–R | `[surface:gtk]` | GTK-Header-Bar + „zweite, eingeklappte Top-Bar" ist ein konkretes libadwaita-Layout, kein abstraktes Prinzip |
| SEARCH-2 | F–R | `[surface:gtk]` | (replaced) dito plus clamp-Werte |
| SEARCH-2a | F–R | `[surface:gtk]` | Glass-Blur-Zone — GTK-Rendering-Detail |
| SEARCH-2b | F–R | `[surface:gtk]` | Pixel-/Motion-Token-Spezifikation (450px clamp, Standard-Token) |
| SEARCH-3 | F–R | `[surface:gtk]` | nennt `ToggleButton` + `:checked`-CSS-Pseudoklasse explizit |
| SEARCH-4 | F–R | `[surface:gtk][surface:desktop]` | Esc-Taste ist tastaturgebunden; Desktop hat Tastatur, Mobile typischerweise nicht |
| SEARCH-5 | F–R | `[surface:all]` | Verhalten „Schließen der Eingabe verwirft nicht die aktive Suche" ist universell; Esc ist nur einer von drei genannten Entfernungswegen (neben Chip, „Clear all") |
| NR-1 | F–R | `[surface:gtk]` | New-Releases-Feature-Cluster, s.o. |
| NR-1a | F–R | `[surface:gtk]` | dito |
| NR-2 | F–R | `[surface:gtk]` | dito |
| NR-3 | F–R | `[surface:gtk]` | dito |
| NR-3a | F–R | `[surface:gtk]` | dito |
| NR-4 | F–R | `[surface:gtk]` | dito |
| NR-5 | F–R | `[surface:gtk]` | dito |
| NR-5a | F–R | `[surface:gtk]` | dito |
| NR-5b | F–R | `[surface:gtk]` | dito, koppelt explizit an Concerts |
| NR-6 | F–R | `[surface:gtk]` | dito |
| NR-7 | F–R | `[surface:gtk]` | dito |
| NR-8 | F–R | `[surface:gtk]` | dito |
| NR-9 | F–R | `[surface:gtk]` | dito |
| NR-9a | F–R | `[surface:gtk]` | dito, koppelt explizit an Concerts |
| NR-10 | F–R | `[surface:gtk]` | dito |
| NR-11 | F–R | `[surface:gtk]` | dito |
| NR-12 | F–R | `[surface:gtk]` | dito |
| NR-12a | F–R | `[surface:gtk]` | dito |
| NR-13 | F–R | `[surface:gtk]` | dito |
| NR-14 | F–R | `[surface:gtk]` | dito |
| NR-15 | F–R | `[surface:gtk]` | dito |
| NR-16 | F–R | `[surface:gtk]` | dito |
| NR-17 | F–R | `[surface:gtk]` | dito |
| NR-18 | F–R | `[surface:gtk]` | dito |
| NR-19 | F–R | `[surface:gtk]` | dito |
| NR-20 | F–R | `[surface:gtk]` | dito |
| NR-21 | F–R | `[surface:gtk]` | dito |
| NR-22 | F–R | `[surface:gtk]` | dito |
| STYLE-1 | S–AA | `[surface:gtk]` | beschreibt konkrete GTK/libadwaita-Fallstricke (AdwToolbarView, AdwHeaderBar, GtkLabel-Ellipsize, AdwOverlaySplitView) |
| ACC-1 | S–AA | `[surface:gtk][surface:desktop]` | „Tastatur allein" ist ein Maus+Tastatur-Interaktionsmodell; Touch/TalkBack auf mobil folgt eigener Logik |
| ACC-2 | S–AA | `[surface:all]` | Name/Rolle/Zustand pro Element ist die universelle Screenreader-Zusage (TalkBack statt Orca), der native-GTK-Passus ist nur die gtk-Ausprägung |
| ACC-3 | S–AA | `[surface:all]` | Fokusreihenfolge folgt sichtbarer Bedeutung ist laut Vorgabe explizit die „dahinterliegende Zusage"; Tab/Shift+Tab ist deren Desktop-Ausdrucksform (unsicherster Fall, siehe unten) |
| ACC-4 | S–AA | `[surface:gtk][surface:desktop]` | ersetzt durch ACC-4a, gleiche Einordnung |
| ACC-4a | S–AA | `[surface:gtk][surface:desktop]` | reine Tastaturmechanik (Pfeile, Home/End, Page Up/Down, Enter, Space, Menu/F10, Esc) |
| ACC-5 | S–AA | `[surface:gtk][surface:desktop]` | Fokus-Lebenszyklus hängt an Ctrl+F/Esc-Tastenkombinationen und Dialog-Fokusfallen |
| ACC-6 | S–AA | `[surface:all]` | Fokus bei dynamischen Updates nicht stehlen/verlieren ist allgemeine A11y-Zusage, kein Tasten-Detail |
| ACC-7 | S–AA | `[surface:gtk][surface:desktop]` | kontrastiert Fokus- vs. Hover-Zustand — ein Zeiger+Tastatur-Modell, das Touch/TalkBack so nicht kennt |
| ACC-8 | S–AA | `[surface:gtk][surface:desktop]` | Tastatur-Alternative zu Drag-and-Drop plus Pfeile/PageUp/Home-Wertebereiche sind Tastaturmechanik |
| ACC-9 | S–AA | `[surface:gtk]` | folgt explizit GNOME-Standardbindungen (Ctrl+F/W/Q/,/?, F1/F10) — GNOME-HIG-exklusiv |
| NET-1 | S–AA | `[surface:all]` | „automatische Netz-Abrufe sind opt-in" ist eine plattformübergreifende Verhaltenszusage, auch wenn einzelne Module (Portraits/New Releases) gtk-exklusiv sind |
| NET-1a | S–AA | `[surface:gtk]` | beschreibt die Plugins-Einrichtungsoberfläche (SET-11) und Module (New Releases/YouTube/Podcasts/Radio), die im neuen Zuschnitt fast vollständig fehlen |
| NET-2 | S–AA | `[surface:gtk]` | ersetzt durch NET-2a, gleiche Einordnung |
| NET-2a | S–AA | `[surface:gtk]` | Update-Grandfathering hängt an gtk-exklusiven Modulen (Podcast/Radio/YouTube-Abos, Portraits) |
| NET-4 | S–AA | `[surface:gtk]` | Banner + Deep-Link zu Preferences → Plugins ist reine gtk-Einrichtungsoberfläche für ausgeschlossene Quellen |
| NET-3 | S–AA | `[surface:all]` | der 7-Zustands-Vertrag gilt für „jede netzwerkgestützte Ansicht" (auch Suche/Lyrics); die sechs Detailpunkte sind zwar Podcasts/Radio/MTP-spezifisch (unsicherster Fall, siehe unten) |
| NET-3a | S–AA | `[surface:gtk]` | Brücke von Podcasts/YouTube-Downloadstatus in die Connectivity-Projektion; beide Quellfeatures ausgeschlossen |
| NET-3b | S–AA | `[surface:gtk]` | die Radio-Ausnahme — Radio ist im neuen Zuschnitt ausgeschlossen |
| NET-3c | S–AA | `[surface:gtk]` | wanted_on_device/MTP-Queue und Podcast-Downloads sind beide ausgeschlossene Features |
| NET-3d | S–AA | `[surface:gtk]` | SourceErrorBanner-Widget sowie Concerts/New-Releases/Podcasts/YouTube sind gtk-exklusiv |
| LYR-1 | S–AA | `[surface:all]` | lokale .lrc/eingebettete Lyrics unabhängig vom Online-Modul zeigen ist Kernverhalten jeder Now-Playing-Lyrics-Ansicht |
| LYR-2 | S–AA | `[surface:all]` | wann der interaktive Online-Lookup startet ist eine Verhaltenszusage, kein gtk-Mechanik-Detail |
| LYR-3 | S–AA | `[surface:gtk]` | AdwStatusPage + Deep-Link zur Plugins-Zeile ist libadwaita/GTK-Preferences-Mechanik |
| LYR-5 | S–AA | `[surface:all]` | Provider-Reihenfolge + Fußzeilen-Quellennennung ist reine Core-/Anzeigelogik ohne gtk-Bindung |
| LYR-6 | S–AA | `[surface:gtk]` | hängt an der geteilten ScanControls-Fortschrittskarte, einem gtk-UI-Baustein |
| LYR-7 | S–AA | `[surface:gtk][surface:desktop]` | Sidecar-Schreibzugriff aufs Dateisystem braucht normalen FS-Zugriff (Desktop ja, Android-Scoped-Storage nein); die Gerätesync-Klausel ist ohnehin gtk-exklusiv |
| DISCOVER-1 | S–AA | `[surface:gtk]` | Hinweis-Kacheln für Portraits/New Releases — beides ausgeschlossene Module |
| DISCOVER-2 | S–AA | `[surface:gtk]` | Deep-Link zur Plugins-Seite, gtk-Einrichtungsoberfläche |
| SEARCH-6 | S–AA | `[surface:gtk]` | Ctrl+F + GTK-Suchleisten-Widget-Mechanik |
| SEARCH-7 | S–AA | `[surface:gtk]` | Fokusverlust-Kollaps der gtk SearchBar, referenziert SEARCH-3/5 |
| LYR-4 | S–AA | `[surface:all]` | Zeilen-Zentrierung der aktiven Lyricszeile ist reine Anzeigelogik der Now-Playing-Lyrics, relevant für mobil/desktop gleichermaßen |
| STYLE-2 | S–AA | `[surface:gtk]` | verwendet Adwaita-CSS-Ebenen (.view, sidebar_bg) direkt |
| STYLE-3 | S–AA | `[surface:gtk]` | benennt libadwaita-CSS-Variablen @accent_color/@reprise_player_accent |
| STYLE-4 | S–AA | `[surface:gtk]` | GL/NGL/Vulkan-Renderer-Fallback ist ein GTK-Rendering-Detail |
| STYLE-5 | S–AA | `[surface:gtk][surface:desktop]` | „Player-Bar überlebt Fenster-Schrumpfen" ist generischer Bedarf jedes größenveränderlichen Desktopfensters mit Transportleiste |
| STYLE-6 | S–AA | `[surface:gtk]` | „Show columns" + Spaltenkollaps ist GTK-ColumnView-Mechanik |
| STYLE-7 | S–AA | `[surface:gtk]` | Sidebar+NPP als zwei unabhängig togglebare Flanken mit 10s-Undo-Toast ist GTKs eigenes Layoutkonzept, nicht bestätigt für Desktop |
| CONTRAST-1 | S–AA | `[surface:gtk]` | definiert Textstufen über Adwaita Named Colors |
| CONTRAST-2 | S–AA | `[surface:gtk]` | gtk-spezifische Statuszeile der Track-Tabelle |
| CONTRAST-2a | S–AA | `[surface:gtk]` | Pill-Overlay an der GTK-Track-Tabelle |
| CONTRAST-3 | S–AA | `[surface:all]` | Mindestkontrast 4,5:1 ist universelle WCAG-Zusage, unabhängig vom Toolkit |
| CONTRAST-4 | S–AA | `[surface:gtk]` | bezieht sich auf die Glass-Chrome-Blur-Ebene, gtk-Rendering-exklusiv |
| NAV-10 | S–AA | `[surface:all]` | ersetzt durch NAV-10a; Grundidee (Marker + History-Anker) ist plattformunabhängig |
| NAV-10a | S–AA | `[surface:all]` | Marker-Anzeige und Auto-Scroll-Zentrierung sind allgemeine Track-Listen-UX; „Doppelklick" ist nur die Desktop-Ausprägung derselben Aktivierung |
| QUE-7 | S–AA | `[surface:all]` | Queue-Komposition (manuell + Kontext-Tail mit Zähler) ist Kernverhalten; Queue existiert auf allen drei Oberflächen |
| QUE-8 | S–AA | `[surface:gtk]` | nennt explizit die Queue-ColumnView; Drag+Kontextmenü-Mechanik ist GTK-Widget-gebunden |
| NPP-11 | S–AA | `[surface:gtk]` | AdwViewSwitcher/AdwViewSwitcherBar/AdwBreakpoint sind libadwaita-Widgets |
| NPP-12 | S–AA | `[surface:gtk]` | das rechte Now-Playing-Panel als andockbares Fenster-Pane ist GTKs Layoutkonzept; mobil hat einen eigenen Now-Playing-Screen statt eines Panels |
| NPP-13 | S–AA | `[surface:gtk]` | Tab-Leiste, Cover-Crossfade-Token und rechte Spalte sind gtk-NPP-Mechanik |
| STATS-0 | S–AA | `[surface:gtk][surface:desktop]` | My Stats existiert nur auf gtk+desktop, nicht mobil |
| STATS-1 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-1a | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-2 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-3 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-4 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-5 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-6 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-6a | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-6b | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-6c | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-7 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-8 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-9 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-10 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-11 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-11a | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-12 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-13 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-14 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-15 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-16 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-17 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-18 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-19 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-20 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| STATS-21 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| BTN-1 | S–AA | `[surface:gtk]` | GTK4-CSS-Eigenheit: keine `cursor`-Property, `style::buttons::arm`-Workaround |
| BTN-2 | S–AA | `[surface:gtk]` | GtkToggleButton/`:checked`-Semantik plus App-eigene CSS-Priorität |
| BTN-3 | S–AA | `[surface:gtk]` | Lautstärke-Tiers sind an konkrete GTK-Button-Klassen gebunden |
| BTN-4 | S–AA | `[surface:gtk]` | `STYLE_PROVIDER_PRIORITY_APPLICATION`, `currentColor`-Alphas, `gtk-enable-animations` — Implementierungsdetail dieser Codebasis |
| AC-7 | S–AA | `[surface:gtk]` | Fläche nicht im mobilen/Desktop-Zuschnitt enthalten |
| AC-8 | S–AA | `[surface:gtk]` | dito |
| AC-10 | S–AA | `[surface:gtk]` | dito |
| AC-11 | S–AA | `[surface:gtk]` | dito |
| AC-19 | S–AA | `[surface:gtk]` | dito |
| AC-20 | S–AA | `[surface:gtk]` | dito |
| AC-21 | S–AA | `[surface:gtk]` | dito |
| AC-22 | S–AA | `[surface:gtk]` | dito |
| AC-23 | S–AA | `[surface:gtk]` | dito (Core-DSP portabel, aber die Visual-Tab-Fläche selbst fehlt im neuen Zuschnitt) |
| DOC-1a | S–AA | `[surface:gtk]` | Library Doctor komplett ausgeschlossen |
| DOC-1b | S–AA | `[surface:gtk]` | dito |
| DOC-1c | S–AA | `[surface:gtk]` | dito |
| DOC-1d | S–AA | `[surface:gtk]` | dito |
| DOC-2a | S–AA | `[surface:gtk]` | dito |
| DOC-2b | S–AA | `[surface:gtk]` | dito |
| DOC-2c | S–AA | `[surface:gtk]` | dito |
| DOC-3a | S–AA | `[surface:gtk]` | dito |
| DOC-3b | S–AA | `[surface:gtk]` | dito |
| DOC-4a | S–AA | `[surface:gtk]` | dito |
| DOC-4b | S–AA | `[surface:gtk]` | dito |
| DOC-5a | S–AA | `[surface:gtk]` | dito |
| DOC-5b | S–AA | `[surface:gtk]` | dito |
| DOC-5c | S–AA | `[surface:gtk]` | dito |
| DOC-5d | S–AA | `[surface:gtk]` | dito |
| DOC-6a | S–AA | `[surface:gtk]` | dito |
| DOC-6b | S–AA | `[surface:gtk]` | dito |
| DOC-6c | S–AA | `[surface:gtk]` | dito |
| DOC-7a | S–AA | `[surface:gtk]` | dito |
| DOC-7b | S–AA | `[surface:gtk]` | dito |
| BROWSE-1 | S–AA | `[surface:all]` | ein Track-List-Modell mit Album/Artist als Scopes ist Grundarchitektur des Browsers auf allen drei Oberflächen |
| BROWSE-2 | S–AA | `[surface:all]` | Navigationszustand pro Standort (Source/Scope/Suche/Sortierung/Anker) ist universelles Back/Forward-Verhalten |
| BROWSE-3 | S–AA | `[surface:gtk]` | „Sidebar" als feste linke Navigationsleiste ist GTK/Desktop-Fensterlayout; mobil nutzt ein eigenes Navigationsmuster |
| BROWSE-4 | S–AA | `[surface:all]` | einheitliche RevealTrack/OpenAlbum/OpenArtist-Intents gelten überall, mit entsprechend weniger Ursprungsorten auf mobil (kein NPP/My Stats) |
| BROWSE-5 | S–AA | `[surface:all]` | Session-Restore von Standort/Bibliotheksroot/Playback-Ursprung ist universelles Neustart-Verhalten |
| BROWSE-6 | S–AA | `[surface:gtk][surface:desktop]` | Zweck der Regel ist laut Text explizit die Stabilität von My Stats, das mobil nicht existiert |
| BROWSE-7 | S–AA | `[surface:gtk]` | Bibliotheks-Kuration (Remove/Trash) steht nicht in der engen mobilen/Desktop-Einschlussliste, die nur Durchsuchen/Browsen nennt (unsicherster Fall, siehe unten) |
| BROWSE-8 | S–AA | `[surface:gtk]` | ersetzt durch BROWSE-11, gleiches Scope-Argument (Lösch-Verhalten) |
| BROWSE-9 | S–AA | `[surface:gtk]` | „Spalten-Editor" ist explizit GTK-ColumnView-Mechanik |
| BROWSE-10 | S–AA | `[surface:all]` | Cover-Kanonisierung passiert beim Scan selbst (Kernlogik), nicht als Nutzeraktion — jede Oberfläche mit eigenem Scan (auch mobil/desktop) trifft darauf |
| BROWSE-11 | S–AA | `[surface:gtk]` | hängt an Remove/Trash-Aktionen, die wie BROWSE-7 nicht im engen mobilen/Desktop-Zuschnitt stehen |
| COVER-1 | S–AA | `[surface:gtk][surface:desktop]` | Cover-Sidecar-Schreibzugriff aufs Dateisystem wie LYR-7; Android-Scoped-Storage schließt mobil aus |
| EXT-1a | S–AA | `[surface:gtk][surface:desktop]` | externe Prozess-Schreibzugriffe auf dieselbe DB sind ein Desktop-Szenario (gleiche Maschine), kein Mobile-Szenario |
| EXT-1b | S–AA | `[surface:gtk][surface:desktop]` | gleiche DB-Sharing-Prämisse wie EXT-1a |
| EXT-2 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| EXT-3 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| EXT-4 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| EXT-5 | S–AA | `[surface:gtk][surface:desktop]` | dito |
| INST-1 | AB–AG | `[surface:gtk]` | historisches Signpost der entfernten GTK-Konversions-UI; Feature komplett aus mobil/desktop ausgeschlossen |
| INST-2 | AB–AG | `[surface:gtk]` | Konversions-Playlist/Progressbar war GTK-exklusiv, Feature ausgeschlossen |
| INST-3 | AB–AG | `[surface:gtk]` | Zeilenzustände der (entfernten) Konversions-Ansicht |
| INST-4 | AB–AG | `[surface:gtk]` | reine Aufsplittungs-Markierung → INST-4a/4b, gleicher Ausschluss |
| INST-4a | AB–AG | `[surface:gtk]` | View-seitige „playable"-Markierung der ausgeschlossenen Konversions-Ansicht |
| INST-4b | AB–AG | `[surface:gtk]` | Staging-Wiedergabe der ausgeschlossenen Konversions-Ansicht |
| INST-5 | AB–AG | `[surface:gtk]` | reine Aufsplittungs-Markierung → INST-5a/5b, gleicher Ausschluss |
| INST-5a | AB–AG | `[surface:gtk]` | View-Model „wait, nie Play" gilt nur der ausgeschlossenen Konversions-Ansicht, keine allgemeine Queue-Regel |
| INST-5b | AB–AG | `[surface:gtk]` | App-Interaktion der ausgeschlossenen Konversions-Ansicht |
| INST-6 | AB–AG | `[surface:gtk]` | Save/Discard-Entscheidung der ausgeschlossenen Konversions-Ansicht |
| INST-7 | AB–AG | `[surface:gtk]` | „Clear playlist"-Warnung ist spezifisch für die ausgeschlossene Konversions-Playlist |
| INST-8 | AB–AG | `[surface:gtk]` | Persistenz/Diskkosten-Anzeige der ausgeschlossenen Staging-Ansicht |
| INST-9 | AB–AG | `[surface:gtk]` | Dedup-Hinweis beim Draggen in die ausgeschlossene Konversions-Playlist |
| INST-10 | AB–AG | `[surface:gtk]` | bestätigt bestehende Markierung; AI-Badge gehört zur ausgeschlossenen Instrumental/Stem-Funktion |
| INST-11 | AB–AG | `[surface:gtk]` | Master-Gate des entfernten, GTK-exklusiven Experimental-Togglers |
| INST-12 | AB–AG | `[surface:gtk]` | Modell-Provisionierung (`ensure_weights`) ist Teil der ausgeschlossenen GTK-Konversions-Oberfläche |
| INST-13 | AB–AG | `[surface:gtk]` | Sidebar-Eintrag der ausgeschlossenen Konversions-Ansicht |
| INST-14 | AB–AG | `[surface:gtk]` | Sidebar-Drop-Target der ausgeschlossenen Konversions-Ansicht |
| MINI-1 | AB–AG | `[surface:all]` | Fenstermechanik (Ctrl+M, 430×76-Fenster, Transparenz) ist GTK-exklusiv, aber der beschriebene Karteninhalt — Cover/Titel-Interpret-Zeile/Mini-Waveform mit Tap=Seek, Drag=Scrub, Play/Pause, bewusst ohne Volume/Prev/Next — ist genau der Vertrag einer Android-Bottom-Bar |
| MINI-2 | AB–AG | `[surface:gtk]` | Restore/Quit nur über Rechtsklick-Menü, Tastatur, Doppelklick sowie GtkWindowHandle-Drag — reine Desktop-Fenstermechanik ohne Mobil-Äquivalent |
| MINI-3 | AB–AG | `[surface:gtk]` | Rechtsklick-Kontextmenü inkl. X11-only „Always on Top" ist reine Desktop-Fenstermechanik |
| MINI-4 | AB–AG | `[surface:gtk]` | Tastenkürzel (Ctrl+M/Ctrl+Q/Ctrl+Pfeile) setzen Tastatur und Fenstertoggle voraus |
| MINI-5 | AB–AG | `[surface:gtk]` | Angebot „Use Compact Mode" reagiert auf ein zu klein werdendes Desktop-Fenster, kein Android-Konzept |
| CONC-1 | AB–AG | `[surface:gtk]` | Concerts-Modul komplett aus mobil/desktop ausgeschlossen; Sidebar/Badge nur SMART/GTK |
| CONC-2 | AB–AG | `[surface:gtk]` | Filterzeilen-UI der ausgeschlossenen Concerts-Ansicht |
| CONC-3 | AB–AG | `[surface:gtk]` | externe Ziel-Öffnung ohne Play-Pfad, spezifisch für die ausgeschlossene Concerts-Tabelle |
| CONC-4 | AB–AG | `[surface:gtk]` | historische Vorgängerfassung von CONC-4b, gleicher Ausschluss |
| CONC-4a | AB–AG | `[surface:gtk]` | historische Vorgängerfassung von CONC-4b, gleicher Ausschluss |
| CONC-4b | AB–AG | `[surface:gtk]` | Leerzustände/Updates-Popover der ausgeschlossenen Concerts-Ansicht |
| CONC-5 | AB–AG | `[surface:gtk]` | historische Vorgängerfassung von CONC-5a, gleicher Ausschluss |
| CONC-5a | AB–AG | `[surface:gtk]` | Netzwerk-Trigger nur für das ausgeschlossene Concerts-Feature, obwohl in `reprise-core` implementiert (Layer ≠ Geltungsbereich) |
| CONC-6 | AB–AG | `[surface:gtk]` | „Similar to …"-Zeilen sind Concerts-spezifisch |
| CONC-7 | AB–AG | `[surface:gtk]` | Updates-Popover-Sektion existiert nur für das ausgeschlossene Concerts-Modul |
| CONC-8 | AB–AG | `[surface:gtk]` | Credential-Check-UI der ausgeschlossenen Concerts-Einstellungen |
| CONC-9 | AB–AG | `[surface:gtk]` | Ticketmaster/Bandsintown-Credentials betreffen nur das ausgeschlossene Concerts-Modul |
| CONC-10 | AB–AG | `[surface:gtk]` | Zeilenlayout der ausgeschlossenen Concerts-Tabelle |
| CONC-11 | AB–AG | `[surface:gtk]` | Fehlerbanner ist an die ausgeschlossene Concerts-Ansicht gebunden, auch wenn das Bannermuster mit Podcasts geteilt wird |
| SRC-1 | AB–AG | `[surface:gtk]` | Podcasts/Radio-Modul komplett aus mobil/desktop ausgeschlossen; Sidebar-Platzierung nur GTK |
| SRC-2 | AB–AG | `[surface:gtk]` | Add-Button/Filterzeilen-Grammatik der ausgeschlossenen Quellen |
| SRC-3 | AB–AG | `[surface:gtk]` | historische Vorgängerfassung von SRC-3a, gleicher Ausschluss |
| SRC-4 | AB–AG | `[surface:gtk]` | historische Vorgängerfassung von SRC-4a/4b, gleicher Ausschluss |
| SRC-5 | AB–AG | `[surface:gtk]` | RSS/YouTube/Radio-Ortsstruktur ist Teil der ausgeschlossenen Quellen |
| SRC-3a | AB–AG | `[surface:gtk]` | Add-Dialog-Verhalten der ausgeschlossenen Quellen |
| SRC-6 | AB–AG | `[surface:gtk]` | quellengebundene Add-Dialoge existieren nur für das ausgeschlossene Modul |
| SRC-7 | AB–AG | `[surface:gtk]` | Add/„Added"-Zustand der ausgeschlossenen Add-Dialoge |
| SRC-8 | AB–AG | `[surface:gtk]` | Scroll-Layout der ausgeschlossenen Add-Dialoge |
| SRC-9 | AB–AG | `[surface:gtk]` | Abonnentenzahl in Channel-Suchergebnissen, ausgeschlossenes Modul |
| SRC-10 | AB–AG | `[surface:gtk]` | Empty-State-Geometrie ist spezifisch für die ausgeschlossenen Podcasts/YouTube/Radio-Ansichten |
| SRC-11 | AB–AG | `[surface:gtk]` | Bildanzeige der ausgeschlossenen Quellen (Anzeige-Teil; reine Fetch/Cache-Policy liegt separat in core, aber ohne dortige Feature-Oberfläche unsichtbar) |
| SRC-12 | AB–AG | `[surface:gtk]` | Bulk-Auswahl/Batch-Aktionen der ausgeschlossenen Episoden-Ansicht |
| SRC-4a | AB–AG | `[surface:gtk]` | Radio-Entfernung/Undo, ausgeschlossenes Modul |
| SRC-4b | AB–AG | `[surface:gtk]` | „Play Next"/„Add to Queue" für Episoden ist an das komplett ausgeschlossene Podcasts/YouTube-Modul gebunden — da Episoden auf mobil/desktop nie existieren, erreicht dort nie ein Eintrag die Queue |
| SRC-13 | AB–AG | `[surface:gtk]` | Playback-Marker-Reveal in Quelllisten, spezifisch für ausgeschlossene Podcast/YouTube-Listen |
| SRC-14 | AB–AG | `[surface:gtk]` | Auswahlverhalten der ausgeschlossenen Episoden-Zeilen |
| POD-1 | AB–AG | `[surface:gtk]` | Episodenstatus-Ableitung existiert nur für das ausgeschlossene Podcasts-Modul |
| POD-2 | AB–AG | `[surface:gtk]` | RSS-Datenmodell/Refresh-Worker des ausgeschlossenen Moduls |
| POD-3 | AB–AG | `[surface:gtk]` | yt-dlp-Anbindung des ausgeschlossenen YouTube-Teils |
| POD-4 | AB–AG | `[surface:gtk]` | Wiedergabeposition/Autoplay-Angebot der ausgeschlossenen Episoden |
| POD-5 | AB–AG | `[surface:gtk]` | Download-Policy des ausgeschlossenen Moduls |
| POD-6 | AB–AG | `[surface:gtk]` | Entfernen/Undo einzelner Episoden, ausgeschlossenes Modul |
| POD-7 | AB–AG | `[surface:gtk]` | Downloadzustand-Anzeige der ausgeschlossenen Episodenzeile |
| POD-8 | AB–AG | `[surface:gtk]` | ersetzte Vorgängerfassung (→ POD-12), gleicher Ausschluss |
| POD-9 | AB–AG | `[surface:gtk]` | Gruppierung/Zähler-Header der ausgeschlossenen Show-Listen |
| POD-10 | AB–AG | `[surface:gtk]` | YouTube-Kanalseite ist Teil des ausgeschlossenen Moduls |
| POD-11 | AB–AG | `[surface:gtk]` | Download-Spalte/Header-Summe der ausgeschlossenen Kanalseite |
| POD-12 | AB–AG | `[surface:gtk]` | Android-Gerätesync-Auswahl für Episoden — Geräte-Sync/MTP ist ebenfalls explizit ausgeschlossen, Episoden erst recht |
| POD-13 | AB–AG | `[surface:gtk]` | Fehlerklassifizierung der ausgeschlossenen Download-Zeile |
| POD-14 | AB–AG | `[surface:gtk]` | „Only Shorts here"-Zustand der ausgeschlossenen Kanalseite |
| POD-15 | AB–AG | `[surface:gtk]` | Header-Wortwahl der ausgeschlossenen Podcasts/YouTube-Seiten |
| POD-16 | AB–AG | `[surface:gtk]` | ersetzte Vorgängerfassung (→ POD-19), gleicher Ausschluss (Tag im Dokument bereits vorhanden) |
| POD-17 | AB–AG | `[surface:gtk]` | Tagging beim Download ist reine Hintergrundlogik des ausgeschlossenen Moduls, nichts, das eine andere Oberfläche anzeigt |
| POD-18 | AB–AG | `[surface:gtk]` | Upload-Datum-Übernahme des ausgeschlossenen YouTube-Teils |
| POD-19 | AB–AG | `[surface:gtk]` | Fehler-Footer der ausgeschlossenen Podcasts/YouTube-Bibliothek |
| POD-20 | AB–AG | `[surface:gtk]` | Playback-Marker/Toggle-Verhalten der ausgeschlossenen Episodenzeile |
| POD-21 | AB–AG | `[surface:gtk]` | Nachbar-Navigation/Lyrics-Tab-Ausblendung hängen an einer „external session" (Podcast/YouTube), die auf mobil/desktop nie existiert |
| POD-22 | AB–AG | `[surface:gtk]` | Browser-Cookie-Auswahl für YouTube-Signin, ausgeschlossenes Modul |
| POD-23 | AB–AG | `[surface:gtk]` | lokalisierte YouTube-Metadaten, ausgeschlossenes Modul |
| RAD-1 | AB–AG | `[surface:gtk]` | Verbunden-Akzent der Radio-Tabelle, ausgeschlossenes Modul |
| RAD-2 | AB–AG | `[surface:gtk]` | Live-Wiedergabe ohne Seek/Dauer, ausgeschlossenes Radio-Modul |
| RAD-3 | AB–AG | `[surface:gtk]` | radio-browser-Serverwahl/Reconnect, ausgeschlossenes Modul |
| RAD-4 | AB–AG | `[surface:gtk]` | URL-Auflösung für Radiostreams, ausgeschlossenes Modul |
| RAD-5 | AB–AG | `[surface:gtk]` | Add-Station-Dialog-Chips, ausgeschlossenes Modul |
| RUN-1 | AB–AG | `[surface:all]` | „ein Runtime-Owner, zweite Oberfläche verbindet sich oder scheitert" — keine D-Bus/systemd/XDG-Nennung, gilt für jede Oberfläche als Client derselben Runtime |
| RUN-2 | AB–AG | `[surface:all]` | exakt die im Auftrag genannte Zusage „kein geratener Zustand bei fehlender Verbindung" |
| RUN-3 | AB–AG | `[surface:all]` | Reconnect-als-Hintergrundereignis ohne Toast/Fokusklau — keine plattformspezifische Technologie genannt |
| RUN-4 | AB–AG | `[surface:all]` | Idle-Shutdown-Politik der Runtime selbst, ohne systemd zu nennen; gilt für jeden verbundenen Client gleichermaßen |
| RUN-5 | AB–AG | `[surface:all]` | „externe Änderung folgt still, ohne Toast/Fokusklau" ist eine reine Client-Zusage ohne D-Bus/systemd/XDG-Erwähnung |
| RUN-6 | AB–AG | `[surface:all]` | „Fenster" ist GTK-Sprache, aber das Prinzip „nur die selbst gestartete Wiedergabe stoppen, fremde nicht" nennt keine Desktop-Technologie und passt auf jede Oberfläche, die ihre eigene Sitzung schließt |

## Warum noch nicht angewandt

Zwei Gründe, beide handfest.

**1. Die Marker müssen die dritte Klammer sein.**
`scripts/check-ux-traceability.sh` liest die Testebene positionsgebunden:

```
^- \*\*ID\*\* \[(active|planned)\] \[(core|gtk|e2e|manual)\]
```

Ein `[surface:*]` **zwischen** Status und Ebene lässt die Ebenenerkennung
stillschweigend leerlaufen — `[manual]`-Regeln würden dann gegen einen Test
statt gegen `RELEASING.md` geprüft, und das Gate wäre falsch, ohne rot zu
werden. Angehängt **hinter** der Ebene ist es unschädlich, weil das `grep -oE`
nicht am Zeilenende ankert. Reihenfolge also immer:
`[status] [ebene] [surface:…]`.

**2. Zwei blockierende Pläne fassen dieselbe Datei an.**
`docs/plans/ux-rules-motion.md` (reviewed) und `docs/plans/motion-player.md`
(planned) arbeiten in `docs/ux-rules.md`. Das Anwenden berührt **jede**
Regelzeile — ein Konflikt mit Ansage. Der Eintrag wartet deshalb auf die
Entscheidung über diese beiden Pläne.

## Fälle, die eine menschliche Entscheidung brauchen

Alle vier Durchgänge sollten ihre unsichersten Fälle benennen. Diese hier
sind keine Flüchtigkeitsfehler, sondern echte Grenzfälle:

| Regel | Vorschlag | Der Zweifel |
| --- | --- | --- |
| `RUN-2` … `RUN-6` | `[surface:all]` | Der Laufzeitdienst trägt alle Oberflächen, also gelten seine Zusagen überall. Heute sind sie `[gtk]` markiert — das war historische Alleinstellung, kein Argument. Die wichtigste Verschiebung überhaupt. |
| `RUN-6` | `[surface:all]` | „Closing the **window** stops the playback that window started" — das Prinzip passt auf jede Oberfläche, das Wort „Fenster" nicht. |
| `MINI-1` | `[surface:all]` | Mischt GTK-Fenstermechanik (Ctrl+M, 430×76) mit einem Karteninhalt, der exakt einer Android-Bottom-Bar entspricht. Kandidat für eine Aufspaltung in a/b. |
| `NPP-5` | `[surface:gtk]` | Lyrics-Zeilenhierarchie: pixelgenaue Werte sind GTK, die Absicht (aktive Zeile hervor, Nachbarn gestuft) ist ausdrücklich Mobil-Scope. |
| `CTX-2` | `[surface:gtk][surface:desktop]` | Kontextmenü-Selektionsregel: die Aktionssemantik passt auf ein Android-Bottom-Sheet, die Auslösung (Rechtsklick, Shift+F10) nicht. 50/50. |
| `ACC-3` | `[surface:all]` | Fokusreihenfolge ist eine plattformübergreifende Zusage, der Regeltext ist aber ganz in Tab/Shift-Tab formuliert. |
| `NET-3` | `[surface:all]` | Der Offline-Zustandsvertrag ist allgemein formuliert, seine sechs Punkte drehen sich aber fast nur um ausgeschlossene Flächen. Aufspaltung wäre sauberer. |
| `BROWSE-7` | `[surface:gtk]` | Remove/Trash-Unterscheidung: fundamentale Hygiene, aber der mobile Zuschnitt nennt nur „durchsuchen und browsen", nicht Löschen. |
| `P-1` | `[surface:gtk]` | Feedback-Architektur: das Prinzip ist universell, der Text bindet namentlich an libadwaita-Widgets. |
| `PLAY-5b` | `[surface:gtk]` | Wechseldatenträger-Hygiene — offen, ob die Tauri-App je externe Laufwerke unterstützt. |
| `FIL-8` | `[surface:gtk]` | „Recently added" ist Sidebar-Mechanik, der Gedanke wäre für mobiles Browsen aber eine eigene Ansicht wert. |
| `ALB-2` | `[surface:all]` | Album-Detail ist Kernscope, die „Accent Pipeline" zur Farbextraktion könnte GTK-spezifisch sein. |
| Abschnitt **R** (New Releases, 28 Regeln) | `[surface:gtk]` | Steht **nicht** auf der Ausschlussliste, wurde aber komplett ausgeschlossen, weil es architektonisch untrennbar am ausgeschlossenen Concerts-/Artist-News-Cluster hängt. Die weitreichendste Einzelentscheidung dieses Durchgangs. |

## Der eigentliche Befund

**61 von 455 Regeln gelten für alle drei Oberflächen — 13 %.**

Das Regelwerk beschreibt weit überwiegend das GTK-Vollprodukt. Die neuen
Oberflächen erben also wenig und werden den Großteil ihres Verhaltens als
**neue** Regeln brauchen. Das ist kein Mangel des Regelwerks, sondern eine
Aussage über den Zuschnitt: was ausgeschlossen ist, macht den Löwenanteil
der Regeln aus (MTP allein 51, New Releases 28, Library Doctor 20,
Podcasts/Radio 45).

Für P7 heißt das: `docs/ux-rules.md` ist als Abnahmespezifikation für die
Android-App **nicht ausreichend**. Sie liefert 61 verbindliche Zusagen und
einen Rahmen — den Rest schreibt P7 selbst.
