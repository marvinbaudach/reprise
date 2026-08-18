---
slug: jump-always-centers-the-current-track
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Der Sprung zum laufenden Titel soll immer zentrieren

**Beobachtung des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„scrolling: manchmal erscheint der aktuelle Song auf den ich springe
zentralisiert und manchmal ist er ganz oben im Sichtfeld. am besten immer
zentrieren, (Reiterwechsel, Titel-Fokus). weniger überraschung"*

## Die Absicht ist bereits Zentrieren — der Weg dorthin ist ein Wettlauf

Der Code will unbestritten die Mitte. `scroll_center.rs:1-10` sagt es im
Modulkopf: *„a plain `scroll_to` only edge-snaps"* — deshalb rechnet
`centered_scroll_target()` den Adjustment-Wert selbst und schreibt ihn direkt.
`track_reveal.rs:156-177` nutzt genau das und gleitet zur Mitte.

**Aber ein zweiter Pfad kommt zuerst.**
`track_list/centered_scroll_restore.rs:55-59` feuert sofort:

```rust
let scroll = gtk4::ScrollInfo::new();
scroll.set_enable_vertical(true);
shared.column_view.scroll_to(position, None, gtk4::ListScrollFlags::NONE, Some(scroll));
```

Das ist GTKs eigenes `scroll_to` — und das bringt die Zeile **minimal** in
Sicht, also an den oberen oder unteren Rand. Erst *danach* versuchen zwei
nachgelagerte Korrekturen, das Ergebnis auf die Mitte zu ziehen:

- `:28-38` — `after_changed_once()` auf dem Adjustment, und
- `:42-53` — ein `idle_add_local_once()` für den Fall, dass der Bereich stabil
  bleibt und deshalb gar kein `changed` kommt.

Beide stehen unter Vorbehalt: Sie laufen nur, wenn
`shared.model.generation() == generation` noch gilt **und** `apply()` `true`
liefert. `apply()` `:62-72` steigt aus, wenn keine Vertikal-Adjustment da ist,
wenn der Inhalt in den Viewport passt — oder wenn `live_row_height()` die
Zeilenhöhe noch nicht messen kann.

**Greift eine Korrektur, steht die Zeile in der Mitte. Greift keine, bleibt der
Edge-Snap stehen — und die Zeile klebt oben.** Das ist genau die Zweiteilung,
die der Nutzer sieht, und sie hängt am Timing, nicht an der Bedienung. Deshalb
wirkt sie zufällig.

Der zweite Pfad (`track_reveal::reveal_position`) hat dieses Problem **nicht**:
findet er keine Geometrie, scrollt er gar nicht und versucht es im nächsten
Leerlauf erneut (`:167-176`). Er snapped nie an die Kante. Zwei Pfade, zwei
Verhalten — das ist die eigentliche Wurzel.

## Was zu klären ist

1. **Kann der Edge-Snap einfach entfallen?** Wenn `scroll_to` nur dazu da ist,
   die Zeile überhaupt realisieren zu lassen (GTK baut Zeilen außerhalb des
   Viewports nicht), dann braucht es ihn — aber dann darf sein Ergebnis nicht
   sichtbar werden. Zu prüfen: ob ein `ScrollInfo` mit gesetzter
   Vertikalausrichtung dasselbe leistet, oder ob der Snap in denselben Frame
   fällt wie die Korrektur.
2. **Warum zwei Pfade?** `centered_scroll_restore.rs` (Wiederherstellung nach
   Reload) und `track_reveal.rs` (Sprung zum laufenden Titel) lösen dieselbe
   Aufgabe unterschiedlich. Der Nutzer nennt beide Anlässe — „Reiterwechsel"
   und „Titel-Fokus". Die naheliegende Auflösung ist, dass beide durch
   `reveal_position()` gehen und `centered_scroll_restore` nur noch die
   Zeilenrealisierung besorgt.
3. **Die Retry-Grenze.** `reveal_position(shared, position, attempts)` gibt
   nach `attempts` auf und scrollt dann **gar nicht**. Das ist die bessere
   Sorte Fehlschlag als ein Edge-Snap, aber sie ist auch nicht sichtbar. Wird
   der Pfad vereinheitlicht, gehört ein Blick darauf, ob die Zahl reicht.
4. **`SRC-13` gilt hier nicht.** `ui/source_reveal.rs` entscheidet *ob*
   überhaupt bewegt wird, und zwar für Podcasts/YouTube/Radio. Seine
   `RevealPolicy::Reveal` ist im Doc-Kommentar schon als „center it"
   beschrieben. Die Trackliste hat ihre eigene Mechanik (`NAV-10b`,
   `current_track_selection`), und dort sitzt der Befund. Beim Vereinheitlichen
   nicht die eine Regel für die andere halten.

## Verwandt

- `docs/plans/jump-to-playing-track-drops-the-filter.HANDOFF.md` — derselbe
  Sprung, anderer Aspekt.
- `docs/plans/queue-section-anchor-landing.HANDOFF.md` und
  `docs/plans/queue-anchor-grill-followups.md` — Ankerlogik derselben Tabelle.

## Berührte Stellen

| Datei | Rolle |
| --- | --- |
| `crates/reprise-gnome/src/ui/track_list/centered_scroll_restore.rs:28-72` | der Edge-Snap und die zwei Korrekturversuche |
| `crates/reprise-gnome/src/ui/track_list/track_reveal.rs:156-177` | der saubere Pfad: zentrieren oder erneut versuchen |
| `crates/reprise-gnome/src/ui/scroll_center.rs:19-58` | die geteilte Zentriermathematik |
| `crates/reprise-gnome/src/ui/window/window_playing_source_wiring.rs:163-166` | `Ctrl+L` löst den Sprung aus |
| `crates/reprise-gnome/src/ui/source_reveal.rs` | die *andere* Regel — nicht verwechseln |
