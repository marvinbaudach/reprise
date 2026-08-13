---
slug: doctor-progress-and-window-chrome
worktree: ~/Projects/reprise/.worktrees/doctor-progress
branch: fix/doctor-progress-and-chrome
phase: coded
codex_session:
created: 2026-08-09
---
# Der Doctor zeigt echten Fortschritt, und das Fenster behält seine Leiste

Zwei gemeldete Fehler aus derselben Ansicht (Library Doctor, laufender Scan mit
Remote-Prüfung). Beide sind statisch belegt; die Fundstellen unten sind gegen
`origin/dev` = `edd458e8df` geprüft.

## Symptom 1 — der Balken springt in einer Sekunde auf 100 % und steht dann minutenlang

Gemeldet: `2250/2251 tracks`, Titel „Checking against MusicBrainz…", vier Minuten
ohne Bewegung.

### Ursache

`crates/reprise-core/src/library/library_doctor/scan.rs`

Beim **Eintritt** in die Remote-Phase wird der Zähler hart auf `total - 1`
gesetzt — bevor eine einzige Netzabfrage läuft:

```rust
// scan.rs:248-253
if progress(DoctorScanProgress {
    phase: DoctorScanPhase::CheckingRemote,
    completed_tracks: tracks.len().saturating_sub(1),   // 2250
    total_tracks: tracks.len(),                          // 2251
    summary: preview_summary,
}) == ScanControl::Cancel
```

Sämtliche Fortschrittsrückrufe **innerhalb** der Arbeitsschleife melden denselben
konstanten Wert (`scan.rs:285`, `:307`, `:330` — jeweils
`tracks.len().saturating_sub(1)`). Erst nach der kompletten Schleife geht der
Zähler auf `tracks.len()` (`scan.rs:382-386`).

Die Schleife (`scan.rs:292-375`) läuft über **Album-Gruppen**
(`group_album_tracks`, `scan.rs:657-669`) und macht pro Gruppe eine
`resolve_album`-Abfrage plus je Track eine `resolve_track`-Abfrage. MusicBrainz
ist global auf **eine Anfrage pro Sekunde** gedrosselt
(`crates/reprise-core/src/musicbrainz.rs:21`, `wait_for_request_slot` ebd.
`:202-219`). Der gesamte Netzteil ist damit für den Nutzer unsichtbar.

Das ist bewusst so gebaut — der Kommentar `scan.rs:257-261` begründet es mit dem
Fingerprinting, das einen einzelnen Track minutenlang halten kann. Die gewählte
Lösung („Zähler nicht anfassen") erzeugt aber genau das gemeldete Bild: die
lokale Lesephase füllt den Balken in einer Sekunde, die Remote-Phase erbt die
volle Skala und bewegt sich nie.

### Was zu tun ist

Der Balken muss **je Phase** seine eigene Skala haben und in der Remote-Phase
echten Fortschritt melden.

1. **Test zuerst** (`library_doctor/phase_scan_tests.rs`, in
   `crates/reprise-core`): Ein Scan mit ≥ 2 Album-Gruppen und aktivierter
   Remote-Prüfung, gegen einen Fake-Provider. Erwartung:
   - Der erste Fortschrittswert der Phase `CheckingRemote` ist **nicht**
     `total - 1` und nicht `total` — die Phase beginnt bei 0.
   - Nach jeder abgeschlossenen Album-Gruppe wächst `completed_tracks` um die
     Trackzahl genau dieser Gruppe.
   - Der letzte Wert der Phase ist `total`.
   - Monotonie bleibt (die bestehenden Zusicherungen `phase_scan_tests.rs:135`
     und `tests.rs:347` müssen grün bleiben).
2. **Umsetzen:** `completed_tracks` in der Remote-Phase ist die Summe der Tracks
   aller **fertig abgearbeiteten** Album-Gruppen — inklusive der
   wiederverwendeten (`reusable`-Zweig, `scan.rs:293-301`), die sofort
   gutgeschrieben werden dürfen, weil sie tatsächlich fertig sind. Der Eintritts-
   Rückruf (`scan.rs:248`) meldet 0.
3. Die Rückrufe innerhalb einer laufenden Gruppe (`scan.rs:285`, `:307`, `:330`)
   dienen weiterhin nur dem Abbruch-Handshake und melden den zuletzt erreichten
   Gruppenstand — sie dürfen ihn nicht erhöhen. Damit bleibt die Begründung aus
   `scan.rs:257-261` gewahrt: ein langes Fingerprinting lässt den Zähler stehen,
   statt ihn zu fälschen.
4. Der Text bleibt „N/M tracks"; die Einheit stimmt, weil in Trackzahlen
   gutgeschrieben wird.

**Nicht-Ziel:** keine Restzeitschätzung, keine Änderung an Drosselung,
Reihenfolge, Caching oder Auflösungslogik.

## Symptom 2 — in der Doctor-Ansicht fehlt die Fensterleiste

Gemeldet: nur ein zentrierter Titel „Library Doctor", keine Leiste, keine
Fensterknöpfe.

### Ursache

Die globale Chrome wird für dieses eine Stack-Kind ausgeblendet:

```rust
// crates/reprise-gnome/src/ui/window/library_chrome.rs:64-66
fn sync_content_chrome(root: &adw::ToolbarView, visible_child: Option<&str>) {
    root.set_reveal_top_bars(visible_child != Some("library-doctor"));
}
```

Die Doctor-Seite bringt zwar eine eigene Kopfzeile mit, aber eine **leere**:

```rust
// crates/reprise-gnome/src/ui/library_doctor/summary_page.rs:220-226
let toolbar = adw::ToolbarView::new();
toolbar.add_top_bar(&adw::HeaderBar::new());
```

Entscheidend ist die Dekorations-Architektur: die App ist client-drawn
(`window_decorations.rs:164` setzt `toplevel.set_decorated(false)`), und die
Fensterknöpfe werden **ausschließlich** an die Bibliotheks-Kopfzeile gehängt:

```rust
// crates/reprise-gnome/src/ui/window/window_decorations.rs, sync_controls()
self.library_header.set_show_start_title_buttons(visible);
self.library_header.set_show_end_title_buttons(visible);
```

Wird diese Leiste versteckt, verschwinden die Fensterknöpfe mit ihr. Die
Doctor-eigene `adw::HeaderBar` bekommt sie nie zugewiesen — sie zeigt nur den
`NavigationPage`-Titel. Das ist exakt das gemeldete Bild.

Das war im bestehenden Plan bereits anders entschieden:
`docs/plans/library-doctor-fix-round-3.md`, **OD-13, Variante A** verlangt
wörtlich, dass das Doctor-Kind „eine vollwertige `adw::HeaderBar`
(**Fensterknöpfe inklusive**)" trägt, mit der Abnahmebedingung „genau eine
Kopfzeilenreihe, gleiche Höhe wie auf den übrigen Seiten, **Fensterknöpfe
vorhanden, Fenster weiter verschiebbar**". Umgesetzt wurde nur die Ausblendung,
nicht der Ersatz.

### Was zu tun ist

1. **Test zuerst** (Display-Test in `crates/reprise-gnome`, neben den
   vorhandenen `doc_7c_*`-Tests): Nach dem Öffnen des Library Doctor trägt die
   sichtbare Kopfzeile der Doctor-Seite Fensterknöpfe (`show-start-title-buttons`
   bzw. `show-end-title-buttons` entsprechend dem Dekorationsmodus), und es ist
   genau **eine** Kopfzeilenreihe sichtbar.
2. **Umsetzen:** Die Doctor-Kopfzeile übernimmt die Fensterknöpfe, solange die
   Bibliotheks-Chrome verborgen ist. `sync_controls` in `window_decorations.rs`
   ist die eine Stelle, die heute über die Sichtbarkeit der Knöpfe entscheidet —
   sie muss die aktuell sichtbare Kopfzeile bedienen, nicht fest
   `library_header`. Keine zweite Kopie dieser Entscheidung anlegen.
3. Der Dekorationsmodus (`WindowDecorationMode::System` mit separater
   GTK-Titlebar) muss weiter funktionieren: in diesem Modus trägt die separate
   Titlebar die Knöpfe, die Doctor-Kopfzeile keine.
4. Gilt für **alle** Doctor-Seiten mit eigener Kopfzeile — Start/Ergebnis
   (`summary_page.rs:222`), Review (`review_page.rs:477-510`) und die laufende
   Ansicht.

**Nicht-Ziel:** keine Umgestaltung der Doctor-Seiten, keine neuen Bedienelemente
in der Kopfzeile, kein Zurückholen der Bibliotheks-Chrome über den Doctor.

## Runde 2 — Symptom 2 ist NICHT behoben (gemessen, nicht vermutet)

Die visuelle Abnahme der ersten Runde ist **negativ**. Der Fix aus
`window_decorations.rs` greift verdrahtungsseitig vollständig, erzeugt aber
keine sichtbaren Fensterknöpfe. Gemessen an der laufenden App (headless, eigener
Xvfb, isoliertes Profil, Doctor über das Primärmenü geöffnet):

```
PROBE sync_content_chrome visible_child=Some("library-doctor")
PROBE sync_controls integrated=true doctor_visible=true has_visible_page=true found_headers=1
PROBE all headers i=0 shows_end=true  mapped=true  w=1211 h_px=46   <- Doctor-Kopfzeile
PROBE all headers i=1 shows_end=false mapped=false w=1752 h_px=54   <- Library-Kopfzeile
```

Also: der Stack wechselt korrekt, `sync_controls` läuft mit `doctor_visible=true`,
die Doctor-Kopfzeile wird gefunden, ist dargestellt (1211×46) und trägt
`show-end-title-buttons=true`. Der Screenshot zeigt an ihrem rechten Rand
trotzdem **nichts** — vergrößert geprüft, die Fläche ist leer.

**Schlussfolgerung:** `show-title-buttons` auf einer `adw::HeaderBar`, die tief in
der Hierarchie sitzt (Fenster → ToolbarView → ToastOverlay → OverlaySplitView →
NavigationView → Stack → NavigationView → NavigationPage → ToolbarView →
HeaderBar), lässt libadwaita keine Fensterknöpfe zeichnen. Die
Bibliotheks-Kopfzeile kann es nur, weil sie in der äußersten ToolbarView des
Fensters sitzt. Variante A aus OD-13 ist damit auf diesem Weg nicht erreichbar.

### Was zu tun ist

`docs/plans/library-doctor-fix-round-3.md` (OD-13) hat diesen Fall vorgesehen:

> **Variante B**, falls A die Fensterknöpfe oder das Ziehen verliert: die
> bibliotheksspezifischen Bedienelemente (`search_toggle`, Scan-Button,
> Quellentitel) werden für dieses eine Kind auf `set_visible(false)` gesetzt und
> der Doctor-Titel in denselben Header gehängt.

Genau das ist jetzt umzusetzen — die Messung oben ist die von OD-13 verlangte
Begründung für den Wechsel auf B.

1. **Zurücknehmen:** Commit `e689bfdcb3` (`fix(doctor): keep controls on the
   visible header`) wird rückgängig gemacht — der Ansatz trägt nicht. Der
   Fortschritts-Commit `57df2ab47e` bleibt unangetastet.
2. `library_chrome.rs:65` blendet die Chrome-Leiste **nicht mehr** aus. Die
   Leiste bleibt stehen und behält damit die Fensterknöpfe.
3. Stattdessen werden die bibliotheksspezifischen Bedienelemente in dieser Leiste
   verborgen, solange `"library-doctor"` das sichtbare Stack-Kind ist, und der
   Titel der Leiste wird auf „Library Doctor" gesetzt.
4. Die Doctor-Seiten tragen dann **keine** eigene Kopfzeile mehr
   (`summary_page.rs:222`, `review_page.rs`) — sonst stehen zwei Reihen
   übereinander. Die Bedienelemente der Review-Seite („All"/„None") wandern in
   die Chrome-Leiste, solange die Review-Seite sichtbar ist.
5. **Eine** Stelle entscheidet, was die Leiste im Doctor zeigt — keine zweite
   Kopie dieser Bedingung.

### Test

Der bestehende Test `doc_7c_the_visible_doctor_header_owns_the_window_controls`
prüft Variante A und ist damit gegenstandslos — er wird durch einen Test ersetzt,
der Variante B festhält: im Doctor bleibt `reveals_top_bars` **wahr**, die
Chrome-Leiste behält ihre Fensterknöpfe, die bibliotheksspezifischen Elemente
sind unsichtbar, und es ist genau **eine** Kopfzeilenreihe sichtbar.

**Wichtig:** Ein Property-Test allein hat den Fehler nicht gefangen — er war grün,
während die App den Fehler zeigte. Die Abnahme dieser Runde ist der Screenshot,
nicht der Test.

## Abnahme

- `cargo test -p reprise-core library_doctor` grün, inklusive der neuen Tests.
- `cargo test -p reprise-gnome` grün (Display-Tests werden separat isoliert
  nachgefahren, nicht in der Codex-Sandbox).
- Beide neuen Tests schlagen **vor** der jeweiligen Änderung fehl — das ist
  nachzuweisen, nicht zu behaupten.
