---
slug: device-page-on-this-device-when-not-connected
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: „On this device" bleibt bedienbar, obwohl das Telefon nicht verbunden ist

**Nur ein Befund, kein Plan.** Festgehalten am 16.08.2026, 08:03, gemeldet vom
Nutzer, belegt durch einen Screenshot der Geräteseite `Pixel 10 Pro XL`
(laufender Build: 0.1.13, gebaut 15.08.2026 23:00, entspricht dem `dev`-Kopf `95b4b30016`). Fußzeile der Seite: **„Not connected · Automatic sync is on"**.

## Symptom

Auf der Geräte-Detailseite eines **nicht verbundenen** Telefons ist der obere
Teil konsequent deaktiviert, der Abschnitt **„On this device"** dagegen nicht:

| Element | Zustand im Screenshot |
| --- | --- |
| Music transfer profile (Dropdown) | ausgegraut |
| Set limit… | ausgegraut |
| **Check again** | **klickbar** |
| **Change folder…** | **klickbar** |
| **Review playlists above** (Link) | **klickbar** |
| **Remove from phone when removed from a playlist** (Schalter) | **schaltbar** |
| **Sync automatically when this phone connects** (Schalter) | **schaltbar** |

Dazu zeigt der Abschnitt durchweg Leerwerte: „Available space unknown",
„0 playlists · 0 tracks · 0 B", „Device contents never verified", oben
„Write access unknown · Storage projection is unavailable until the selection
is valid."

## Der Wunsch des Nutzers

> „eigentlich braucht es *on device* nicht, wenn wir nicht mal verbunden sind."

Also: den Abschnitt bei fehlender Verbindung **gar nicht zeigen** — nicht bloß
ausgrauen. Offen ist, was mit den beiden **Regel-Schaltern** passiert: die sind
inhaltlich Einstellungen für den nächsten Anschluss („Sync automatically **when
this phone connects**") und wären auch ohne Verbindung sinnvoll bedienbar. Sie
liegen aber heute *innerhalb* von „On this device". Das ist die eigentliche
Design-Entscheidung: Abschnitt aufteilen (Bestand vs. Regeln) oder komplett
verbergen.

## Code-Verortung (erhoben im lokalen Hauptcheckout, ungeprüft gegen `origin/dev`)

- Abschnitt: `crates/reprise-gnome/src/ui/device_sync/device_sync_on_device.rs`
  - `Change folder…` `:147-153` — **keine** Sensitivitätsregel
  - `Set limit…` `:154-159` — dauerhaft `set_sensitive(false)` (unabhängig von
    der Verbindung, „no size limit" als Tooltip)
  - `Review playlists above` `:142-146` — **keine** Sensitivitätsregel
  - Schalter `:177-200`, angehängt `:213-215` — **keine** Sensitivitätsregel
  - `update()` `:242-253`: gesetzt wird nur `check_button.set_sensitive(can_scan)`
    aus `verification_copy(...)`; der Verbindungszustand geht hier nicht ein
- Der Rest der Seite hängt an `device.page.controls.editable`:
  `device_sync_page.rs:216`, `device_sync_playlist_card.rs:178` — genau dieses
  Flag fragt „On this device" nie ab.
- Zustandstexte: `device_sync_strings.rs:201` (`Not connected`), `:210`
  (`Automatic sync is on`), `:394` (`On this device`), `:399`
  (`Rules for this phone`).

**Warum die Seite überhaupt im „verbunden"-Layout steht:** Es gibt bereits
eine Stack-Umschaltung `connected` / `disconnected`
(`device_sync_page.rs:200-208`). Die Entscheidung trifft
`device_sync_remembered::apply()`
(`device_sync_remembered.rs:11-13`):

```rust
device.connected || device.session_state == DeviceSessionState::Remembered
```

Ein *gemerktes* Gerät bekommt also die volle Seite, auch ohne Verbindung — der
Screenshot ist genau dieser Fall. Der Doc-Kommentar dort nennt „Plan E", der
diese schmale Fassung durch eine echte Remembered-Projektion ersetzen soll;
dieser TODO ist vermutlich ein Teil davon. Vor dem Umbau prüfen, ob es zu
Plan E schon einen Plan in `docs/plans/` gibt.

## Offene Fragen

- Verbergen oder deaktivieren? (Nutzer tendiert zu verbergen.)
- Wohin mit „Rules for this phone", wenn der Abschnitt verschwindet — eigener
  Abschnitt oben, oder mitverschwinden?
- Was ist mit **Check again** bei getrenntem Gerät: `can_scan` erlaubt es
  offenbar; scheitert der Scan dann sichtbar oder still?
- Display-Tests: für den Zustand „disconnected" gibt es **keinen** Test — der
  `disconnected`-Stack-Child (`device_sync_page.rs:90-97`) wird nirgends
  geprüft. Sensitivität wird nur an einer Stelle festgenagelt:
  `device_sync_page_display_tests.rs:684`
  (`unrememberable_device_disables_hero_rename_with_the_identity_explanation`,
  Assertion in `:700`). Der Umbau braucht also einen neuen Test, nicht nur
  einen angepassten.
