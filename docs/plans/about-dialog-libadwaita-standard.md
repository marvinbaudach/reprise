---
slug: about-dialog-libadwaita-standard
worktree: ~/Projects/reprise-about-dialog-libadwaita-standard
branch: feature/about-dialog-libadwaita-standard
phase: planned
codex_session:
created: 2026-08-12
---

# About-Dialog: libadwaita-Slots füllen und Debug-Bericht bauen

## Ausgangslage (verifiziert gegen origin/dev, nicht gegen den lokalen Checkout)

`crates/reprise-gnome/src/ui/about.rs` baut **bereits** eine `adw::AboutDialog` —
der Dialog ist kein Eigenbau. Er ist nur unterbefüllt: gesetzt sind heute
`application_icon` (= `crate::APP_ID`), `application_name`, `version`,
`developer_name`, `developers`, `copyright`, `license_type` (`Gpl30`, das ist in
GTK bereits „or later") und **eine** pauschale Legal-Sektion.

Es fehlen: `release_notes`, `issue_url`, `debug_info`, `debug_info_filename`,
`translator_credits` und die Legal-Sektionen je Abhängigkeit.

Weitere geprüfte Fakten, auf die dieser Plan aufsetzt:

- **Die App-ID ist bereits umbenannt.** Auf `origin/dev` heißt die Datei
  `data/io.github.marvinbaudach.Reprise.metainfo.xml`, die Konstante `APP_ID` in
  `crates/reprise-gnome/src/main.rs` ist mitgezogen. Plan
  `flathub-app-id-and-packaging` ist `phase: shipped`. Überall dort, wo der
  Auftragstext noch `org.reprise.Reprise` sagt, gilt
  `io.github.marvinbaudach.Reprise`.
- **Die Versionspille ist geschenkt.** In `adw-about-dialog.ui` ist
  `version_button` ein `GtkButton` mit `action-name="about.copy-property"` und
  `action-target="version"`. Ein Klick kopiert exakt den Wert der
  `version`-Property. `version_string()` liefert bereits `0.1.1 (8d062859de)`.
  **Es wird kein eigener Klick-Handler gebaut** — dieser Punkt ist bereits erfüllt.
- **Ein GResource existiert**: `crates/reprise-gnome/resources/reprise.gresource.xml`.
- **Es gibt keinerlei Diagnose-Sammlung**: kein `debug_info`, keine
  Flatpak-Erkennung, keine GStreamer-/GTK-Laufzeitabfrage, keinen Log-Puffer.
  Das ist der eigentliche Neubau in diesem Plan.
- Logging läuft über `tracing_subscriber` mit `REPRISE_LOG`-Filter in
  `crates/reprise-gnome/src/main.rs`.

## Getroffene Entscheidungen

**E1 — Der Standard gewinnt, der Entwurf wird nachgezogen.** libadwaita rendert
die Problembehandlung **zweistufig**: Seite „Problembehandlung" mit eigenem
Fließtext und einer Zeile „Fehlerdiagnoseinformationen ›", dahinter erst der
Textblock mit „Kopieren" und „Speichern unter…". Der Zeilentitel „Neu" und der
Hinweistext sind in der `.ui` fest verdrahtet und **nicht** setzbar; einen
Untertitel unter „Neu" gibt es dort nicht. Es wird **nicht** in den Widget-Baum
gegriffen, um das Mockup exakt zu treffen.

**E2 — Alles puffern, beim Rendern redigieren.** Alle `WARN`/`ERROR` der Sitzung
laufen in einen Ringpuffer. Beim Bauen des Debug-Strings werden Pfade, URIs und
Dateinamen zu Platzhaltern ersetzt. Vollständigkeit schlägt Kuratierung — auch
unerwartete Fehler müssen im Bericht landen.

---

## Aufgabe 1 — Diagnose-Kern in `reprise-core`

Neues Modul `crates/reprise-core/src/diagnostics/` (Dateien nach Zuschnitt, keine
Datei über 400 Zeilen).

**Reine Sammelfunktion.** Eine Struktur mit den Fakten als Felder, plus eine
Funktion, die daraus den String rendert. Kein I/O, keine Umgebungsabfrage im
Kern — die Werte werden hineingereicht. Genau das macht sie testbar.

Jedes Feld ist optional; ein fehlender Wert rendert als `unknown`, **niemals** als
leeres Feld oder weggelassene Zeile.

Zielformat, zeilenweise lesbar (Reihenfolge ist verbindlich):

```
reprise 0.1.1 (8d062859de, release)
flatpak io.github.marvinbaudach.Reprise
os fedora 43 · gnome 49 · wayland
gtk 4.20.1 · libadwaita 1.8.2
rust 1.91 · gstreamer 1.28 (pipewire)
locale de_DE.UTF-8
db schema 41 · wal · 2165 tracks · 18.4 MiB
mtp libmtp 1.1.22 · 1 device remembered

last warnings
09:25:14 mtp: device detached mid-transfer, run 41 cancelled
09:11:02 scanner: 3 files without inventory entry, skipped
08:58:37 lyrics: no .lrc for 12 tracks
```

Bei nativer Installation steht statt der `flatpak`-Zeile `native`. Der Block
`last warnings` entfällt vollständig, wenn der Puffer leer ist — keine leere
Überschrift.

**Redaktion.** Eine Funktion, die eine Log-Nachricht säubert, bevor sie in den
String geht:

- absolute Pfade unterhalb des Musikordners → `$XDG_MUSIC_DIR/…`
- absolute Pfade unterhalb von `$HOME` → `$HOME/…`
- sonstige absolute Pfade → `…`
- `file://`- und andere URIs → Schema plus `…`
- der nackte Benutzername → `$USER`

Zusätzlich gilt für den gesamten Bericht: keine Bibliothekspfade, keine
Zugangsdaten, keine Scrobbling-Benutzernamen. Der Text muss ohne Nachbearbeitung
in ein öffentliches Issue kopierbar sein.

**Ringpuffer.** Feste Kapazität (200 Ereignisse), nimmt Zeitstempel, Level, Ziel
und Nachricht. Gerendert werden die letzten 10, neueste zuerst, mit Uhrzeit
`HH:MM:SS`. Der Puffer ist Teil des Kerns und rein; das Einspeisen macht die
GNOME-Schicht.

**Tests** (neben dem Modul):

- fehlende Angaben rendern durchgängig `unknown`
- leerer Puffer → kein `last warnings`-Block
- Redaktion: Ereignisse mit absolutem Pfad, `$HOME`-Pfad, `file://`-URI,
  Dateiname mit Endung und Benutzername einspeisen; die gerenderte Ausgabe darf
  **keinen** davon enthalten. Das ist der Regel-Test aus dem Auftrag — er prüft
  die Ausgabe, nicht die Redaktionsfunktion allein.
- Puffer läuft über: nur die jüngsten Ereignisse überleben, Reihenfolge stimmt

## Aufgabe 2 — Fakten sammeln in `reprise-gnome`

Die unreine Gegenseite zu Aufgabe 1: liest die Umgebung und füllt die Struktur.

- **Paketform**: Flatpak erkennen (`/.flatpak-info`), sonst `native`. Bei Flatpak
  die App-ID mit ausgeben.
- **OS/Desktop**: Distribution aus `/etc/os-release`, GNOME-Version, Wayland vs.
  X11 (im Repo existiert bereits eine `is_x11()`-Prüfung in
  `crates/reprise-gnome/src/ui/compact/compact_mode_controls.rs` — Logik
  wiederverwenden statt zweiter Implementierung), Locale.
- **Toolkit**: GTK- und libadwaita-Version **zur Laufzeit** (nicht die
  Compile-Zeit-Konstante).
- **Laufzeit**: Rust-Version (Build-Zeit über `build.rs`, analog zu
  `REPRISE_GIT_SHA`), GStreamer-Version und aktives Audio-Backend.
- **Build-Profil**: `debug` oder `release`, in die erste Zeile.
- **Datenbank**: Schemaversion, Journalmodus, Titelzahl, Dateigröße.
- **Geräte**: libmtp-Version, Anzahl erinnerter Geräte.

Wo ein Wert nicht ermittelbar ist, wird `None` gereicht — der Kern macht daraus
`unknown`. Ein fehlgeschlagener Sammelschritt darf den Dialog **nie** am Öffnen
hindern.

**Log-Puffer anschließen**: ein `tracing`-Layer im Setup in
`crates/reprise-gnome/src/main.rs`, der `WARN` und `ERROR` in den Ringpuffer aus
Aufgabe 1 schreibt. Der Puffer ist prozessweit und lebt so lange wie die Sitzung.

## Aufgabe 3 — Den Dialog befüllen

In `crates/reprise-gnome/src/ui/about.rs`:

1. **`from_appdata` als Basis.** Den Dialog über
   `adw::AboutDialog::from_appdata(<gresource-pfad>, Some(env!("CARGO_PKG_VERSION")))`
   bauen. Der erste Parameter ist ein **GResource-Pfad, kein Dateisystempfad** —
   die metainfo.xml muss dafür in `crates/reprise-gnome/resources/reprise.gresource.xml`
   eingetragen und mit eingebettet werden (Aufgabe 4). Als Release-Notes-Version
   die **reine** Crate-Version übergeben, nicht den String mit Commit-Hash.
2. **`website` aktiv leeren.** `from_appdata` liest `<url type="homepage">` aus
   der metainfo und setzt daraus `website`. Die GitHub-Pages-Seite ist **noch
   nicht live**, deshalb muss `website` nach dem Bau explizit geleert werden,
   damit der Dialog die Zeile weglässt. Ohne diesen Schritt zeigt der Dialog eine
   tote Adresse.
3. **`issue_url`** explizit auf den Issue-Tracker setzen (nicht darauf verlassen,
   dass `from_appdata` den `bugtracker`-Eintrag überträgt — verifizieren und
   danach entscheiden).
4. **`application_icon`** auf `crate::APP_ID` belassen, `version` weiterhin aus
   `version_string()`, `license_type` bleibt `Gpl30`.
5. **`debug_info`** aus Aufgabe 1+2 setzen, `debug_info_filename` auf
   `reprise-debug-info.txt`. libadwaita baut daraus selbst beide Unterseiten mit
   „Kopieren" und „Speichern unter…" — **keine eigenen Knöpfe, kein eigenes
   Fenster.**
6. **Credits**: `developers`, `designers`, `artists`, `documenters` als Listen im
   Format `Name <mail@example.org>` bzw. `Name https://…`. `translator_credits`
   aus dem übersetzbaren String, damit Übersetzer automatisch erscheinen — der
   String läuft über `super::strings`, wie im Modul bereits üblich.
7. **Legal**: die heutige Sammel-Sektion ersetzen durch **je eine**
   `add_legal_section()` pro Abhängigkeit — GStreamer, libmtp, TagLib,
   Phosphor-Icons und was sonst mitgeliefert wird, jeweils mit der zutreffenden
   Lizenz.

Der Debug-String wird **beim Öffnen** des Dialogs gebaut, nicht beim Start —
sonst sind die Warnungen veraltet.

## Aufgabe 4 — AppStream-Datei und GResource

- `data/io.github.marvinbaudach.Reprise.metainfo.xml` in
  `crates/reprise-gnome/resources/reprise.gresource.xml` aufnehmen, damit
  `from_appdata` sie findet. Der Build muss die Datei aus `data/` einsammeln.
- Die Datei bleibt die **einzige** Quelle für „Neu in Reprise". Kein zweiter,
  handgepflegter Änderungstext im Code.
- Erlaubtes Markup ist der AppStream-Teilsatz (`p`, `ul`, `ol`, `li`, `em`,
  `code`) — mehr rendert das Widget nicht. Bestehende Einträge 0.1.1 und 0.1.0
  inklusive `xml:lang="de"` bleiben unverändert.
- Fehlt für die laufende Version ein `<release>`-Eintrag, zeigt der Dialog die
  Zeile nicht — das ist gewolltes Verhalten, kein Fehlerfall.

## Aufgabe 5 — Tests

Zusätzlich zu den Kern-Tests aus Aufgabe 1, im bestehenden Testmodul von
`about.rs` (die vorhandenen Anzeige-Tests tragen `#[ignore]` + xvfb-Hinweis —
diesem Muster folgen):

- Der Dialog öffnet **ohne** passenden Release-Eintrag ohne Fehler und ohne
  Release-Notes-Zeile.
- `version()` liefert exakt `version (commit)`, solange `REPRISE_GIT_SHA` gesetzt
  ist, und die reine Version sonst.
- `website()` ist leer, `issue_url()` gesetzt.
- `debug_info()` ist nicht leer und enthält die erste Zeile im erwarteten Format.

## Was ausdrücklich nicht hineingehört

- **Datenordner öffnen** — gehört in die Einstellungen.
- **Bibliotheksstatistiken** — dafür gibt es „My Stats".
- **Spenden-Link**, solange das Projekt nicht öffentlich ist.
- **Reiter** — libadwaita blättert in Unterseiten mit Zurück-Pfeil.
- **Ein eigener Klick-Handler für die Versionspille** — libadwaita hat ihn schon.
- **Eingriffe in den libadwaita-Widget-Baum**, um das Mockup exakt zu treffen.

## Hinweis zu den Dateilisten

Die genannten Dateien sind **Startpunkt, kein Zaun**. Angrenzende Dateien dürfen
minimal geändert und in der Commit-Message genannt werden. Anhalten nur, wenn der
*Vertrag* dieses Plans falsch ist — nicht, wenn eine Datei fehlt oder der Zustand
woanders liegt als hier vermutet.
