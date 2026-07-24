# Reprise

[English](README.md) · [Deutsch](README.de.md)

Reprise ist ein Musikplayer für Menschen, die ihre Musik noch als eigene
Dateien besitzen — und für Entwickler, die sehen wollen, wie native
Desktop-UX, ein portabler Core und messbare Systemarbeit in einer
Rust-Codebase zusammenkommen. Eine GTK4-/libadwaita-App für GNOME sitzt auf
einem GUI-freien Core; alles Linux-Spezifische bleibt hinter klaren Verträgen.

> **Status:** aktive Alpha. Reprise ist noch kein öffentliches Release.

## Warum Reprise

- **Alles funktioniert lokal.** Scans grosser Bibliotheken, Metadaten, Suche,
  Playlists, Hörverlauf, Android-Sync und Dateisicherheit — nichts davon macht
  aus deiner Musiksammlung ein Cloudkonto.
- **Nativ, kein Web-View.** GTK4/libadwaita prägt das GNOME-Erlebnis;
  GStreamer, MPRIS, MTP, Keyring und Papierkorb liegen in einer eigenen
  Linux-Schicht.
- **Geprüft statt versprochen.** Architektur, UX, Accessibility, Performance
  und Delivery werden von Skripten und Tests erzwungen — nicht nur im README
  beschrieben.

## Architektur

![Reprise-Architektur: ein portabler Rust-Core, ein Linux-Plattformadapter und ein natives GTK4-/libadwaita-Frontend mit erzwungener Abhängigkeitsrichtung.](docs/assets/reprise-architecture.svg)

| Crate | Verantwortet | Darf nicht verantworten |
|---|---|---|
| `reprise-core` | Bibliothek, SQLite-Queries, Queue-Semantik, Scanner, Playlists, Settings und Plattformverträge | Abhängigkeiten zu GTK, libadwaita, GStreamer, zbus oder GLib |
| `reprise-platform-linux` | GStreamer-Wiedergabe und -Analyse, MPRIS/D-Bus, MTP, Papierkorb und weitere Linux-Adapter | Produkt-UI oder duplizierte Fachregeln |
| `reprise-gnome` | GTK4-/libadwaita-Präsentation, Interaktionszustand, Accessibility und Desktop-Komposition | Produktives SQL, blockierendes HTTP oder direkte GStreamer-Orchestrierung |
| `reprise-cli` | Headless-CLI über Core-Fassaden: Playlists, Suche, Library-Summary, Scan und Instrumental-Jobs | Andere Workspace-Crates als reprise-core (ausser den feature-gegateten mpris-/worker-Ausnahmen) oder produktives SQL |
| `reprise-mcp` | Lokaler stdio-MCP-Server: read-only Library-Resources und capability-gegatete Create-Tools für Agenten | Andere Workspace-Crates als reprise-core, produktives SQL oder Playback-/Queue-/Tag-/Delete-Tools |
| `reprise-stems` | Portables Stem-Separation-Backend (ML-Inferenz) für die experimentellen Instrumental-Jobs | Andere Workspace-Crates als reprise-core oder GUI-/Engine-Kopplung |

Die gesamte Anwendungslogik und alle Daten liegen in der gemeinsamen Engine;
die Plattform-Crates implementieren nur die schmalen Verträge, die der Core
vorgibt, und jedes Frontend bleibt nativ. Die `reprise-cli`- und
`reprise-mcp`-Frontends laufen als eigene Prozesse auf derselben Datenbank,
und ein Change-Log-Notifier zeigt ihre Änderungen live in einer laufenden
GTK-App an — ohne Neustart. `scripts/check-architecture.sh` erzwingt die
Abhängigkeitsrichtung, die Reinheit des Cores, Dateigrössen-Limits und bekannte
Kopplungsfallen der Präsentationsschicht.

## Engineering-Verträge

- **Jede UX-Regel hat einen Test.** Das verbindliche
  [UX-Regelwerk](docs/ux-rules.md) ordnet jeder aktiven Regel einen nach ihr
  benannten Rust- oder CUA-Test zu — inklusive Tastatur, Fokus, Accessibility,
  Feedback und Reduced Motion.
- **Grosse Bibliotheken bleiben schnell und sparsam.** Das Trackmodell
  kombiniert GTK-Widget-Virtualisierung mit lazy geladenen
  200-Zeilen-SQLite-Fenstern und einem festen Cache-Budget. Akzeptierte
  Vergleiche laufen auf generierten Profilen mit 10.000 und 100.000 Tracks.
- **Veraltete Async-Ergebnisse treffen nie die falsche Zeile.** Recycelte
  Zeilen und langlaufende Worker tragen Generation Tokens; verspätete Cover,
  Metadaten, Lyrics oder Fortschrittswerte können kein anderes sichtbares
  Element übermalen.
- **Alles Riskante ist Opt-in.** Netzwerkmodule sind standardmässig aus,
  Credentials liegen im System-Keyring, Dateien ändern sich nur nach einer
  Benutzeraktion, und automatische Prüfungen laufen auf isolierten Profilen
  statt auf einer echten Musiksammlung.

Benchmarkmethoden, ihre Grenzen und die akzeptierten Ergebnisse stehen in
[TESTING.md](TESTING.md) und im [Engineering-Showcase](docs/showcase.md) —
dieses README verzichtet bewusst auf Zahlen, die schnell veralten.

## Mitentwickeln

**Such dir deinen Einstieg:** reine Bibliotheks-, Scanner-, Queue- oder
Playlist-Logik in `reprise-core`; native Interaktion und Accessibility in
`reprise-gnome`; oder Audio-, Desktop- und Geräteadapter in
`reprise-platform-linux`.

Starte mit [AGENTS.md](AGENTS.md) und dem [UX-Regelwerk](docs/ux-rules.md).
Jede Änderung beginnt mit einem fehlschlagenden Test, respektiert die
Core-Grenze und landet per Pull Request über `feature → dev → main`. Das Ziel
ist nicht mehr Code, sondern ein besserer Musikplayer — mit Belegen, dass jede
Änderung korrekt ist.

## Bauen und starten

Voraussetzungen: Rust 1.92+, Meson 1.3+, Ninja, GTK 4.22+, libadwaita 1.9+,
SQLite, gettext, GStreamer 1.x und GVfs mit MTP-Volume-Monitor.

Installiere zusätzlich die GStreamer-Codecs für die Musikformate, die du
abspielen möchtest.

```sh
cargo build --locked --workspace
cargo run --locked -p reprise-gnome
cargo test --locked --workspace
```

Installation über Meson:

```sh
meson setup _build --prefix="$HOME/.local" -Dprofile=release
meson compile -C _build
meson install -C _build
```

Das Flatpak-Manifest zielt auf GNOME 50 und löst Cargo-Abhängigkeiten aus
gepinnten Checksums auf. Details stehen in [flatpak/README.md](flatpak/README.md).

## Verifikation

Der schnelle lokale Check:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
```

Vor einem Merge durchläuft ein Kandidat das vollständige Pull-Request-Gate —
warnungsfreies Rustdoc, Dependency-Audit, isolierte GTK-Display-Suites sowie
die Accessibility- und Eingabe-Verträge:

```sh
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch
```

Release-Kandidaten validieren über `scripts/check-release.sh` zusätzlich
Desktop-Metadaten, Flatpak-Quellen, Übersetzungen und einen optimierten
Meson-Install.

Display-Tests schlagen schlicht fehl, wenn ihre privaten
D-Bus-/Xvfb-/AT-SPI-Dienste fehlen — sie greifen nie auf den laufenden Desktop
oder dein Benutzerprofil zurück.

## Dokumentation

| Dokument | Zweck |
|---|---|
| [AGENTS.md](AGENTS.md) | Repository-Workflow, Sicherheitsgrenzen und Pflicht-Gates |
| [TESTING.md](TESTING.md) | Testebenen, Benchmarkmethode und Evidenzgrenzen |
| [docs/ux-rules.md](docs/ux-rules.md) | Verbindliche Interaktions- und Accessibility-Verträge |
| [docs/agents/branching.md](docs/agents/branching.md) | Pull-Request-Flow `feature → dev → main` |
| [docs/showcase.md](docs/showcase.md) | Portfolio-Positionierung und vertiefende Engineering-Evidenz |
| [RELEASING.md](RELEASING.md) | Packaging- und Release-Checkliste |

## Lizenz

Die portable Engine (`reprise-core`, `reprise-platform-linux`) steht unter
**MIT**. Das native GTK4-Frontend (`reprise-gnome`) steht unter
**GPL-3.0-or-later**. [LICENSING.md](LICENSING.md) erklärt die Gründe und
Komponentengrenzen.
