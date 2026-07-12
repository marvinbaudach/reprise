# Release Readiness: Flatpak, Integration und deutsche Übersetzung — Design

## Ziel

Reprise wird aus dem reinen Entwicklungs-Workspace zu einer lokal installierbaren,
Flatpak-baubaren GNOME-Anwendung mit vollständiger Desktop-Integration und einer
echten deutschen UI-Übersetzung. Die Veröffentlichung selbst bleibt getrennt:
Der Auftrag verbietet Pushes, und das Repository besitzt weder öffentlichen Remote
noch eine nachweisbar kontrollierte Domain für die aktuelle App-ID
`org.reprise.Reprise`. Das Ergebnis ist deshalb **submission-ready**, aber wird nicht
zu Flathub hochgeladen.

## Build und Installation

Meson ist die Installationsschicht und ruft Cargo für das bestehende Workspace-Binary
`reprise` auf. Direkte Cargo-Entwicklung bleibt unverändert möglich. Meson installiert:

- das Release-Binary nach `bin`;
- Desktop-Datei und AppStream-Metainfo unter den standardisierten XDG-Pfaden;
- ein vollfarbiges skalierbares und ein symbolisches SVG im hicolor-Theme;
- kompilierte gettext-Kataloge unter `share/locale`.

Die Build-Konfiguration übergibt `GETTEXT_PACKAGE` und `LOCALEDIR` an Rust. Direkte
Cargo-Läufe verwenden sichere Fallbacks; ein isolierter Übersetzungs-Smoke kann ein
Scratch-Locale-Verzeichnis explizit überschreiben.

## Desktop- und Store-Integration

Die Desktop-Datei verwendet exakt `org.reprise.Reprise`, startet `reprise`, nennt die
Audio-/Player-Kategorien und enthält deutsche Name/Kommentar/Keywords. Das
AppStream-Dokument beschreibt die tatsächlich vorhandenen Funktionen, Lizenz,
Entwickler, Startziel, Kategorien, Eingabegeräte und Release `0.1.0`. Es behauptet
keine noch nicht vorhandenen Online-Dienste, Screenshots oder Veröffentlichung.

Das App-Icon folgt der GNOME-Metapher „Schallplatte + Reprise/Repeat“: einfache
geometrische 128×128-Komposition, reduzierte Details und eine separate monochrome
16×16-Symbolic-Variante. Es wird als repo-natives SVG gepflegt, nicht als generiertes
Rasterbild.

## gettext und Deutsch

Englisch bleibt Quellsprache. Jede nutzersichtbare Zeichenkette aus `ui/strings.rs`
wird als gettext-msgid markiert und am Verwendungspunkt übersetzt. Dynamische Texte
verwenden übersetzbare Templates und echte Singular-/Pluralformen, nicht das
Zusammenkleben übersetzter Satzfragmente. `po/reprise.pot` ist die Vorlage;
`po/de.po` enthält die vollständige, von uns gepflegte deutsche Erstübersetzung.

Die Initialisierung (`setlocale`, `bindtextdomain`, UTF-8, `textdomain`) geschieht vor
GTK-/Adwaita-Initialisierung. Fehlt ein Katalog, bleibt die Oberfläche vollständig
englisch. Tests prüfen die deterministische Template-Ersetzung; `msgfmt --check`
und ein isolierter `LANGUAGE=de`-Smoke beweisen den realen Katalogpfad.

## Flatpak-Sandbox

Die stabile GNOME-50-Runtime ist die Basis (Stand 2026-07-12). Das Manifest baut
Cargo-Abhängigkeiten offline aus einer generierten, gepinnten Quellenliste. Die
Runtime bringt GTK/libadwaita/GStreamer; der seit Freedesktop 25.08 automatisch
bereitgestellte Codec-Zusatz wird nicht als veraltete ffmpeg-full-Erweiterung
deklariert.

Minimal notwendige Finish-Args:

- Wayland plus Fallback-X11, IPC und DRI;
- PulseAudio für Wiedergabe;
- Netzwerk, weil der explizit opt-in geschaltete Cover-Download sonst nie arbeiten
  könnte;
- ausschließlich der eigene MPRIS-Busname.

Es gibt **kein** `--filesystem=home`, kein `--talk-name=org.freedesktop.*` und keine
pauschale Session-Bus-Freigabe. `GtkFileDialog` bezieht die vom Nutzer gewählte
Bibliothek dauerhaft über FileChooser/Documents-Portale; Portalnamen sind in Flatpak
standardmäßig gefiltert zugänglich.

## Sicheres Löschen im Sandbox-Betrieb

Der bisherige Freedesktop-Trash-Spec-Pfad würde in Flatpak in den privaten
App-Daten-Trash schreiben, den der Host-Dateimanager nicht als Papierkorb zeigt.
Darum wandert die konkrete Linux-Löschentscheidung in `reprise-platform-linux`:

- auf dem Host bleibt die bewährte `trash`-Crate aktiv;
- in Flatpak öffnet der Worker die ausdrücklich freigegebene Datei read/write und
  ruft `org.freedesktop.portal.Trash.TrashFile` mit dem Dateideskriptor auf;
- nur Portal-Ergebnis `1` gilt als Erfolg; kein Fallback auf permanentes Löschen;
- Core behält ausschließlich den injizierbaren, plattformfreien
  `trash_tracks_with`-Workflow und seine DB-Invarianten.

## Flathub-Grenze

Das Top-Level-Manifest ist lokal reproduzierbar und linter-/builder-fähig. Für einen
echten Flathub-PR muss später genau eine externe Voraussetzung erfüllt werden: ein
öffentlicher, unveränderlich referenzierbarer Source-Release unter einer zur App-ID
passenden, nachweisbaren Projektidentität. Codex pusht oder veröffentlicht nichts.

## Verifikation

- bestehende Rust-Gates plus Core-Purity;
- `meson setup/compile/install` in Scratch-Prefix;
- `desktop-file-validate`, `appstreamcli validate --pedantic`, SVG/XML-Prüfung;
- `msgfmt --check` und deutscher isolierter GTK-Smoke;
- Host-Trash bleibt durch Unit-Smoke injiziert; Portalwahl und Resultat werden pure
  getestet, der echte Portalaufruf nur mit Scratch-Datei;
- Flatpak-Manifest-Parse/Lint und, wenn Builder/Runtime lokal verfügbar sind, ein
  vollständiger Offline-Build und Start-Smoke.

## Nicht Teil dieses Schritts

- Push, Tag, GitHub-/GitLab-Release oder Flathub-PR;
- Signierschlüssel, Store-Account oder Domainübernahme;
- Screenshots ohne echte manuelle Aufnahme einer befüllten Bibliothek;
- automatische Wiedergabe, Telemetrie oder ungefragter Netzwerkzugriff.

