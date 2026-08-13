# Visuelle Abnahme — About-Dialog (libadwaita-Standard)

Stand: 2026-08-12, 19:30 CEST. Plan: `docs/plans/about-dialog-libadwaita-standard.md`
(`phase: shipped`, `ea4ebb7846` auf `dev`). Handover:
`docs/plans/about-dialog-libadwaita-standard.HANDOFF.md`, offener Punkt 1.

## Wie gemessen

Headless, viermal gestartet, nie auf dem echten Desktop: `Xvfb` (eigene
Displaynummer) + `openbox` + eigener Session-Bus (`dbus-daemon --session`) +
Scratch-`XDG_*`, `GDK_BACKEND=x11`, leeres `WAYLAND_DISPLAY`,
`GSK_RENDERER=cairo`, `REPRISE_AUDIO_SINK=fakesink`. Datenbank: WAL-konsistente
`.backup`-Kopie der echten Bibliothek (2.132 Titel), nie das Original.

Binary: `target/debug/reprise` aus dem Branch-Worktree (Build 17:02, Inhalt
deckungsgleich mit `dev` für alle beteiligten Dateien). Der Dialog wurde über
die echte Fensteraktion geöffnet — `org.gtk.Actions.Activate about` auf
`/io/github/marvinbaudach/Reprise/window/1` —, alles Weitere per Klick
(`xdotool`), Belege per `import -window root`.

Bilder und die kopierte Berichtsfassung: `.tmp/about-visual/`.

## Bestätigt

- **„What's New" erscheint** und zeigt „Version 0.1.1" plus den Release-Notes-Satz
  aus `data/io.github.marvinbaudach.Reprise.metainfo.xml`. Die Kopplung an
  `<release version="0.1.1">` trägt.
- **Die Versionspille ist klickbar und kopiert exakt `0.1.1 (1600f14273)`** —
  aus der Zwischenablage zurückgelesen, nicht geschlossen aus dem Code.
- **Die Problembehandlung ist zweistufig**, genau wie in E1 beschlossen: Seite
  „Troubleshooting" mit libadwaitas eigenem Fließtext, darunter die Zeile
  „Debugging Information ›", erst dahinter der Berichtstext mit „Copy Text" und
  „Save As…".
- **Website fehlt, Issue-Zeile ist da**: keine tote Adresse, „Report an Issue"
  mit Extern-Symbol.
- **Legal listet sieben Sektionen**: GTK, libadwaita, GStreamer, GVfs (LGPL 2.1),
  Lofty, CAVA (MIT), SQLite („Public Domain").
- **Die Redaktion hält am sichtbaren Ende.** Vier Warnungen mit Musikpfad,
  Fremdpfad, `file://`-URI und nacktem Benutzernamen provoziert: alle vier
  erscheinen als `value=$REDACTED`. Der über „Copy Text" geholte Bericht enthält
  weder Benutzernamen noch Pfad noch URI (13 Zeilen, gegengeprüft).
- **Hell und dunkel tragen beide.** Die Teal-Pille bleibt in beiden lesbar.

## Befunde

### B1 — Die Warnzeilen brechen vierfach um (Format weicht vom Zielbild ab)

Der Plan gibt als Zielformat `09:25:14 mtp: device detached mid-transfer` vor —
kurzes Ziel, eine Zeile je Ereignis. Gerendert wird das volle Rust-Modul:

```
19:16:19 reprise::ui::track_list::track_list_menu_smoke: REPRISE_SMOKE_MENU_ACTION: unrecognized value; ignoring; value=$REDACTED
```

Im schmalen Dialog sind das **vier umgebrochene Zeilen pro Ereignis**. Schon
vier Warnungen füllen zwei Drittel der sichtbaren Fläche; bei den vorgesehenen
zehn wären es rund vierzig Zeilen Fließtext. Als Textdatei bleibt es eine Zeile
je Ereignis — das Problem ist rein die Anzeige.

Naheliegend: nur das letzte Segment des Targets rendern (`track_list_menu_smoke:`)
oder eine kurze Zuordnung pflegen (`scanner:`, `mtp:`, `lyrics:`).

### B2 — `os manjaro unknown` auf Rolling-Distributionen

`/etc/os-release` hat hier `ID=manjaro` und `BUILD_ID=rolling`, aber **kein**
`VERSION_ID`. Der Bericht macht daraus laut Spezifikation `unknown` — korrekt
umgesetzt, liest sich in der Zeile aber wie ein Fehler, und zwar für jeden
Arch-/Manjaro-/Gentoo-Nutzer. Besser: auf `BUILD_ID` ausweichen (`os manjaro
rolling`) oder das Token weglassen, wenn keine Version existiert.

### B3 — Die Gewährleistungsformel steht siebenmal untereinander

Jede Legal-Sektion bekommt `copyright: None`, also rendert libadwaita darunter
jedes Mal „This application comes with absolutely no warranty. See the GNU
Lesser General Public Licence…". Sieben Wiederholungen derselben Aussage über
*diese* App, obwohl die Sektion je eine Fremdbibliothek ausweisen soll. Ein
kurzer Copyright- oder Custom-Text je Komponente (wie bei SQLite, das deshalb
sauber nur „Public Domain" zeigt) bricht das auf.

### B4 — Der Dialog bleibt englisch, `translator_credits` wird nie sichtbar

Lauf mit `LANG=LC_ALL=de_DE.utf8`, `LANGUAGE=de`: der Dialog zeigt weiter
„What's New", „Troubleshooting", „Credits" — obwohl
`/usr/share/locale/de/LC_MESSAGES/libadwaita.mo` „Troubleshooting" →
„Fehlerbehandlung" enthält. Eine Sektion „Translated by" erscheint nicht,
obwohl `po/de.po` `translator-credits` gefüllt hat.

Damit ist der in dieser Änderung neu gesetzte `set_translator_credits`-Slot
praktisch wirkungslos. Die Ursache liegt außerhalb dieser Änderung (i18n-Wiring:
`crates/reprise-gnome/src/i18n.rs` bindet die Textdomain, `DEFAULT_LOCALE_DIR`
ist `/usr/share/locale`; das installierte `reprise.mo` liegt unter
`~/.local/share/locale`) und ist hier **nicht abschließend diagnostiziert** —
festgehalten als gemessenes Verhalten.

### B5 — Ohne installiertes Icon zeigt der Dialog einen Platzhalter

Mit reinem Scratch-`XDG_DATA_HOME` rendert das App-Icon als
Fehlbild-Platzhalter. `main.rs` registriert zwar
`/io/github/marvinbaudach/Reprise/icons` in der Icon-Theme-Suche, die GResource
enthält aber nur zwei symbolische Aktions-Icons und kein App-Icon. Sobald
`~/.local/share/icons` im Suchpfad liegt (installierte App, Flatpak), erscheint
das richtige Icon — betrifft also nur nicht-installierte Entwicklungsbauten.

### B6 — App-Icon und In-App-Logo sind zwei verschiedene Marken

Der Dialog zeigt die abstrakte Balkenmarke aus
`~/.local/share/icons/hicolor/*/apps/io.github.marvinbaudach.Reprise.png`,
während die App unten links das Notenzeichen führt. Vorbestehend, nicht Teil
dieser Änderung — aber im Dialog stehen beide erstmals nah beieinander.

### B7 — Die Dialoghöhe folgt nicht dem Seiteninhalt

Alle Unterseiten behalten die Höhe der Hauptseite. Die Problembehandlung besteht
aus drei Sätzen und einer Zeile und hat darunter rund 450 px Leerraum. Kosmetisch,
libadwaita-Standardverhalten.

## Nicht geprüft

- „Report an Issue" und die Lizenz-Links öffnen einen Browser — im Harness
  bewusst nicht ausgelöst.
- „Save As…" (Dateidialog).
- Reihenfolge „neueste zuerst" im Warnblock: die vier provozierten Ereignisse
  fielen in dieselbe Sekunde.
