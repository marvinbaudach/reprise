# Android-Sync (MTP) — Implementation Plan

Status: **gegrilled, ready to implement**
Branch: `feat/synch-android-settings`
Date: 2026-07-16

---

## Architektur-Entscheidungen

| Frage | Entscheidung | Begründung |
|-------|-------------|------------|
| Per-Device-Settings | SQLite (`device_settings`-Tabelle) | Codebase nutzt kein GSettings für App-Settings; Relocatable Schemas für dynamische Device-Serials wäre unnötige Komplexität |
| State-Modell | `Vec<DeviceState>` | Multi-Device von Anfang an, nur 1 gleichzeitiger Sync. Verhindert späteres Refactoring |
| Ratings-back | Raus aus V1 | Braucht Android-Companion-App die state.json schreibt; ohne die ist Merge-Logik toter Code. Toggle existiert aber disabled |
| Remove-Semantik | Selection-basiert | Alles was in keiner gewählten Playlist vorkommt, fliegt. Absicherungen: Delta-Preview (−14 removed) + „Keep on device"-Pin + Toggle aus = nie entfernen |
| Pin-Storage | SQLite (`device_files.pinned`) | Querybar, offline verfügbar, Delta-Berechnung berücksichtigt es direkt |
| Transcode-Timing | Just-in-time Pipeline | 2 Encoder-Threads → Ringpuffer (VecDeque) → 1 Kopier-Thread. Max ~200 MB tmp |
| „Sync settings…"-Button | Öffnet Preferences → Sync-Tab (16a) | Settings leben an einem Ort, kein neues UI-Pattern |
| Device-Filter | Nur MTP (`mtp://` URI-Präfix) | Android-Geräte. Kein Rauschen durch USB-Sticks, SD-Karten, Kameras |
| M3U-Scope | Nur benannte Playlists | Kein M3U für „Entire library" — Android-Player scannt sowieso den ganzen Ordner. Smart Playlists als Snapshot materialisiert |
| „Entire library"-Modus | Exklusiv | Playlist-Checkboxen werden disabled wenn „Entire library" gewählt — echte Teilmenge, kein doppelter Zustand |
| Geräteinhalt-Tracking | DB vertrauen + manueller Rescan | DB-Tabelle ist Wahrheit. Delta = DB-Diff (instant). Full MTP-Listing nur beim ersten Pairing und bei manuellem Rescan |
| Opus-Encoder | GStreamer-Pipeline | `filesrc → decodebin → audioconvert → opusenc → oggmux → filesink`. GStreamer 0.25 ist schon Dependency |

---

## V1-Scope

### Enthalten

- Device-Erkennung (MTP only) → Toast + Sidebar-DEVICES-Karte
- Device-View (17a): flache Track-Liste, Status-Chips, Speicherbalken, Sync now / Sync settings… / Eject
- Preferences Sync-Tab (16a): Selection (Playlist-Checkboxen + Entire library exklusiv), Delta-Karte, Settings (3 Toggles, Ratings-back disabled)
- Sync-Flow: Removals → Pipeline-Transcode+Kopie → M3U → Abschluss
- Fortschritt: Sidebar-Karte mutiert zur Fortschrittskarte, Device-View Delta-Karte mutiert, Headerbar-Spinner (Sidebar collapsed)
- Error-Handling: Device full (Pause), Disconnect (.part-Cleanup + Resume), Einzeldatei-Fehler (Skip), Cancel
- DB-Migration V9: `device_settings` + `device_files`
- „Keep on device"-Pin (Rechtsklick-Menü in Device-View)
- FAT-safe Dateinamen-Sanitizing
- .part-Dateien (rename nach Abschluss, verwaiste .part beim nächsten Sync löschen)

### Nicht in V1 (→ V2)

- Ratings-back (braucht Android-Companion-App)
- „grouped by playlist ▾"-Gruppierung in Device-View Track-Liste
- Rescan-Device (manueller Trigger)
- Scan + Sync gleichzeitig im gemeinsamen Bottom-Slot
- Sidebar-Bottom-Slot-Architektur (einheitlicher Fortschritts-Slot)

---

## State-Modell

```rust
/// Pro Gerät — lebt in Vec<DeviceState> im Runtime
pub struct DeviceState {
    pub device: Device,
    pub phase: SyncPhase,
    pub delta: Option<SyncDelta>,
    pub error: Option<SyncError>,
}

pub struct Device {
    pub name: String,
    pub serial: String,
    pub connected: bool,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub music_bytes: u64,
    pub last_sync: Option<DateTime<Utc>>,
}

pub enum SyncPhase {
    Idle,
    ComputingDelta,
    Syncing {
        step: SyncStep,
        done: u32,
        total: u32,
        current_track: String,
        bytes_done: u64,
        bytes_total: u64,
    },
    Finishing,
}

pub enum SyncStep {
    Removing,
    Copying,      // includes transcoding
    WritingPlaylists,
}

pub struct SyncDelta {
    pub to_copy: Vec<TrackId>,
    pub to_remove: Vec<TrackId>,
    pub bytes: u64,
    pub est_secs: u32,
}
```

Ein Observable (wie `ScanProgress`); alle Flächen binden daran:
- Sidebar-Gerätekarte
- Device-View (17a)
- Settings-Delta-Karte (16a)
- Toasts
- Headerbar-Spinner (Sidebar collapsed)

Keine getrennten Update-Pfade.

---

## DB-Schema (Migration V9)

```sql
CREATE TABLE device_settings (
    device_serial TEXT PRIMARY KEY,
    device_name   TEXT NOT NULL,
    selection_json TEXT NOT NULL DEFAULT '[]',
    -- selection_json: ["playlist:42", "playlist:7", "smart:3"]
    -- oder: "entire_library"
    opus_bitrate  INTEGER NOT NULL DEFAULT 0,
    -- 0 = kein Transcode, sonst kbit/s (64, 96, 128, 160, 192, 256)
    ratings_back  INTEGER NOT NULL DEFAULT 0,
    -- V1: immer 0 (disabled), V2: 1 = enabled
    remove_deleted INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE device_files (
    device_serial TEXT NOT NULL,
    track_id      INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    device_path   TEXT NOT NULL,
    size          INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    pinned        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (device_serial, track_id)
);

CREATE INDEX idx_device_files_serial ON device_files(device_serial);
```

---

## Sync-Flow

```
1. Connect
   └─ VolumeMonitor mount-added, URI starts with mtp://
   └─ Toast „Pixel 8 connected"
   └─ DEVICES-Karte faded in (150 ms crossfade)
   └─ Delta-Berechnung im Hintergrund starten (DB-Diff, instant)
      └─ Sidebar-Karte zeigt „checking…" während ComputingDelta

2. User klickt Sync (3 gleichwertige Trigger → Action app.sync-device)
   └─ Sidebar-Karte „Sync"-Pill
   └─ Device-View (17a) „Sync now"-Button
   └─ Preferences (16a) Delta-Karte „Sync now"-Button
   └─ Button sofort → „Starting…" + 12px Spinner (max 400 ms)

3. Phase: Syncing
   a) Removals
      └─ device_files WHERE pinned=0 AND track NOT IN (selected playlists union)
      └─ Dateien auf Gerät löschen (MTP delete)
      └─ device_files-Rows löschen
      └─ Device-View: −-Rows faden aus + kollabieren (200 ms)

   b) Kopien/Transcodes (Pipeline)
      └─ 2 GStreamer-Encoder-Threads:
         └─ Lossless? → filesrc→decodebin→audioconvert→opusenc(bitrate)→oggmux→filesink(tmp)
         └─ Lossy/bereits Opus? → direkt in Copy-Queue
      └─ VecDeque<ReadyFile> als Ringpuffer
      └─ 1 Kopier-Thread:
         └─ MTP copy (gio::File::copy) mit .part-Name
         └─ Rename .part → final nach Abschluss
         └─ device_files-Row einfügen
      └─ Fortschritt = bytes_done / bytes_total (nicht Dateianzahl)

   c) M3U-Dateien schreiben
      └─ Für jede benannte Playlist in Selection:
         └─ Music/Reprise/<PlaylistName>.m3u8
         └─ Relative Pfade zu den kopierten Dateien
      └─ Nicht für „Entire library"

   d) Abschluss
      └─ Toast „Sync complete · 82 copied, 14 removed"
         └─ Bei Fehlern: „…3 failed" + Details-Button
      └─ Delta-Karte → „Everything in sync ✓" (Haken teal, Text weiß 60%)
      └─ Kein künstlicher 100%-Zwischenzustand

4. Disconnect (Kabel gezogen)
   └─ Toast „Pixel 8 disconnected — sync incomplete (54 of 82)"
   └─ Aktuelle .part-Datei verwaist → beim nächsten Sync gelöscht + neu kopiert
   └─ device_files bleibt — DB ist Wahrheit für nächsten Connect
   └─ Sidebar-Karte + Device-View verschwinden (150 ms crossfade)
```

---

## Dateisystem auf dem Gerät

```
Music/
└─ Reprise/
   ├─ AlbumArtist/
   │  └─ Album/
   │     ├─ 01 Title.opus       (transkodiert)
   │     ├─ 02 Title.mp3        (1:1 kopiert)
   │     └─ 03 Title.opus.part  (unvollständig → nächster Sync löscht)
   ├─ Late Night.m3u8
   ├─ Gym.m3u8
   └─ .sync/
      └─ state.json             (V2: Companion-Daten)
```

**Pfadschema:** `Music/Reprise/<AlbumArtist>/<Album>/<NN Title>.<ext>`
**FAT-safe Sanitizing:** `? * : " < > |` → `_`
**Naming-Konflikte:** Track-Nummer voranstellen (`01 `, `02 `), bei Duplikaten Suffix ` (2)`

---

## UI-Verhalten (Mockup-Details)

### Sidebar DEVICES-Karte (Idle)

- Phone-Icon (13×21 px border-Zeichnung) + „Pixel 8" (12.5 bold) + „82 queued · 45 GB free" (10.5, weiß 55%)
- „Sync"-Pill (11.5 bold, teal #4fc3ab, Background teal 18%)
- Karte: border-radius 10px, Background teal 14%, Border teal 25%
- Hover: Fläche auf weiß 7%
- Klick auf Karte → Device-View öffnen
- Klick auf „Sync"-Pill → Sync starten (stopPropagation, öffnet NICHT die View)

### Sidebar DEVICES-Karte (Syncing)

- Spinner 13px + „Syncing Pixel 8" (12 bold) + „34%" rechts (tabular-nums)
- 3px-Balken: Track weiß 12%, Fill #1CA98F. Fortschritt = bytes, nicht Dateien
- Untertext (10.5, weiß 45%, ellipsized): „↑ Gone Too Soon — Late 9" oder „transcoding · Gone Too Soon"
- „Sync"-Pill → „Cancel" (flat, rosa #f38ba8 Text)

### Device-View Header (17a)

- Phone-Icon (38×64 px, border 2.5px, radius 9px)
- „Pixel 8" (22 bold) + „MTP · connected" (11 bold, teal-Badge) + „last sync yesterday 18:40" (11.5, weiß 40%)
- Speicherbalken: 7px height, radius 4px, max-width 560px
  - Music: #1CA98F (48%)
  - After sync: teal 45% opacity (5%)
  - Other: weiß 28% (16%)
  - Free: weiß 10% (Rest)
- Legende: 10.5, weiß 50%. „after sync" in #6fd7c2
- Buttons: „Sync now" (teal filled, shadow 0 5 14 teal 30%) + „Sync settings…" (weiß 9% bg) + Eject ⏏ (36px circle, weiß 9% bg)

### Status-Chips (17a)

- All · 894 (weiß 70%, bg weiß 7%)
- ↑ Queued · 82 (teal #6fd7c2, bg teal 14%, bold — aktiver Chip)
- − To remove · 14 (rosa #f38ba8, bg rosa 10%)
- ✓ Synced · 798 (weiß 70%, bg weiß 7%)
- Hover: weiß +4%. Klick filtert die Liste. Aktiver Chip = bold + gefüllter Hintergrund

### Track-Liste (17a)

- Grid: `40px 44px 1.4fr 1fr 1fr 90px 70px`
- Spalten: Sync | Cover | Title | Artist | From (playlist) | Size | Length
- Row-Höhe: 44px, Border-bottom weiß 4.5%
- Sync-Glyph: ↑ (teal bold), − (rosa, strikethrough auf Title), ✓ (weiß 60%)
- Cover: 30px, radius 6px, inset border weiß 7%
- Sizes + Lengths: tabular-nums, weiß 50%

### Track-Liste während Sync

- Wartende Rows: ↑ bleibt
- Aktuelle Row: 12px Mini-Spinner statt ↑, Background Akzent 6%
- Fertige: ↑→✓ mit 150 ms Crossfade
- Entfernte (−): Opacity→0, Höhe 44→0 (200 ms Kollaps)
- Status-Chips zählen live (Queued 82→54, Synced 798→826)

### Delta-Karte (16a)

- Border-radius 12px, Background teal 8%, Border teal 22%
- „Next sync: +82 tracks · −14 removed · 12 rating updates back" (13 bold)
  - +N in #6fd7c2, −N in #f38ba8
- „1.2 GB will be copied · ~4 min via USB · lossless → Opus 128" (11.5, weiß 50%)
- „Sync now"-Button (teal filled, shadow)

### Delta-Karte während Sync

- Gleicher Container, Inhalt = Balken + „34% · 28 of 82 · ~2 min left" + aktueller Track
- „Sync now" → „Cancel"

### Button-Feedback

- Pill-Buttons: Hover +8% Helligkeit (weiß-Overlay), :active scale 0.97 (100 ms)
- Akzent-Button: shadow bleibt, Hover hebt shadow auf 40%
- „Sync now" Klick → sofort „Starting…" + 12px Spinner (max 400 ms sichtbar)
- Disabled (Opacity 0.4): wenn Delta leer oder ComputingDelta, Tooltip nennt Grund

### Eject

- Klick → Button wird Spinner → gvfs unmount
- Toast „Pixel 8 can be unplugged"
- Karte + View verschwinden (150 ms crossfade, View → zurück zur Library)
- Während Syncing: Eject disabled, Tooltip „Sync in progress"

### Headerbar-Fallback (Sidebar collapsed)

- 14px Spinner rechts neben ⋮
- Tooltip „Syncing Pixel 8 · 34%"
- Gleiches Muster wie beim Scan

### Preferences Sync-Tab (16a)

- Prefs-Sidebar: Navigation-Tabs (Playback, Appearance, Library, **Sync**, Plugins)
- Device-Header: Phone-Icon (26×44 px) + Name + Badge + Last-sync + Speicherbalken (gleiche Farben wie 17a, kleiner)
- SELECTION-Karte: Checkbox-Liste aller Playlists + „Entire library"
  - Header: „SELECTION" (11, letter-spacing 0.8px, weiß 45%) + „305 tracks · 3.1 GB selected" (11, weiß 45%)
  - Rows: Checkbox (16px, radius 5px, checked=teal mit ✓) + Name (13) + „184 tracks · 1.9 GB" (11.5, weiß 45%)
  - Smart Playlists: „✦ updates weekly" Hinweis (10, teal)
  - „Entire library" gewählt → alle Playlist-Checkboxen disabled
- Delta-Karte (s.o.)
- Sync Settings:
  - „Sync Ratings & Play Counts back" — Toggle (V1: disabled, Tooltip „Requires companion app")
  - „Convert to Opus" — Dropdown: kein Transcode, 64/96/128/160/192/256 kbit/s
  - „Remove deleted tracks from device" — Toggle (default: on)

### Error-States

- **Device full:** Sync pausiert. Delta-Karte → Warnkarte (gelber Punkt #e5a50a): „Device full — 3.1 GB needed, 0.8 GB free. Deselect playlists or enable Opus conversion." Buttons: „Open selection" · „Cancel rest". Bereits Kopiertes bleibt.
- **Disconnect:** Toast „Pixel 8 disconnected — sync incomplete (54 of 82)". .part verwaist, nächster Sync räumt auf + macht weiter.
- **Einzeldatei-Fehler:** Skip, weiterlaufen. Toast am Ende „…3 failed" + Details-Dialog.
- **Cancel:** Aktuelle Datei sauber beenden (oder .part löschen). Toast „Sync cancelled · 28 copied".

### Animations

- Alle Fades/Crossfades: 150 ms
- Row-Kollaps (Removals): 200 ms
- `gtk-enable-animations=false`: alles hart schalten
- Kein Layout-Shift außer: DEVICES-Sektion erscheint beim ersten Gerät (user-initiiert)

---

## Betroffene Dateien (geschätzt)

### Neue Dateien

| Datei | Zweck |
|-------|-------|
| `reprise-core/src/device_sync/settings.rs` | Device-Settings CRUD (SQLite) |
| `reprise-core/src/device_sync/delta.rs` | Delta-Berechnung (DB-Diff) |
| `reprise-core/src/device_sync/transfer.rs` | Kopier-/Transcode-Logik |
| `reprise-core/src/device_sync/sanitize.rs` | FAT-safe Pfad-Sanitizing |
| `reprise-core/src/device_sync/m3u.rs` | M3U-Export für Gerät |
| `reprise-gnome/src/ui/device_view/mod.rs` | Device-View Widget (17a) |
| `reprise-gnome/src/ui/device_view/device_header.rs` | Header + Speicherbalken |
| `reprise-gnome/src/ui/device_view/track_list.rs` | Device Track-Liste mit Status |
| `reprise-gnome/src/ui/device_view/delta_card.rs` | Delta-/Fortschrittskarte |
| `reprise-gnome/src/ui/preferences/sync_page.rs` | Preferences Sync-Tab (16a) |
| `reprise-gnome/src/ui/sidebar/device_card.rs` | Sidebar DEVICES-Karte |

### Geänderte Dateien

| Datei | Änderung |
|-------|----------|
| `reprise-core/src/db.rs` | Migration V9 (2 neue Tabellen) |
| `reprise-core/src/view_source.rs` | `ViewSource::Device { serial }` Variante |
| `reprise-gnome/src/ui/sidebar/sidebar.rs` | DEVICES-Sektion + Karte einbauen |
| `reprise-gnome/src/ui/window/library_shell.rs` | Device-View in content_stack |
| `reprise-gnome/src/ui/window/window.rs` | wire_source_routing für Device |
| `reprise-gnome/src/ui/device_sync/device_sync_runtime.rs` | Sync-Orchestrierung erweitern |
| `reprise-gnome/src/ui/device_sync/device_sync_backend.rs` | Transcode-Pipeline |
| `reprise-gnome/src/ui/preferences/mod.rs` | Sync-Tab registrieren |
| `reprise-platform-linux/src/device_sync.rs` | MTP-Filter, Transcode, .part-Handling |

---

## Offene Punkte für die Implementierung

1. **Exact GStreamer-Pipeline testen** — `opusenc` Element muss auf Manjaro installiert sein (`gst-plugins-base` oder `gst-plugins-good`). Verify mit `gst-inspect-1.0 opusenc`.
2. **MTP-Copy mit Progress-Callback** — `gio::File::copy_async` mit `G_FILE_COPY_OVERWRITE` und Progress-Callback für Byte-Fortschritt. Testen ob MTP-Backend den Callback sinnvoll feuert.
3. **Device-Serial-Stabilität** — Prüfen was `gio::Volume` als stabile ID liefert (`get_uuid()` vs. `get_identifier("uuid")`). Muss über Reconnects stabil sein.
4. **FAT-Pfadlänge** — FAT32 hat 255-Zeichen-Limit pro Pfadkomponente. Lange Album-/Tracknamen truncaten.
