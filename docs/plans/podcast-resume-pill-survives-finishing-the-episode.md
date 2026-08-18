---
slug: podcast-resume-pill-survives-finishing-the-episode
worktree: /home/marvin/Projects/reprise-podcast-resume-pill-survives-finishing-the-episode
branch: feature/podcast-resume-pill-survives-finishing-the-episode
phase: shipped
codex_session:
created: 2026-08-18
---
# Plan: Die Episodenliste zieht nach, wenn eine Episode durchläuft

Der Befund vom 16.08.2026 hat die Ursache belegt: Die Datenbank ist richtig
(`played_at` gesetzt, `position_ms = 0`), die Statusableitung ist richtig — nur
zeichnet die Liste aus einem Speicherstand vom Ladezeitpunkt. Dieser Plan macht
daraus Aufgaben, geprüft am 18.08.2026.

## Der fehlende Draht

Der Weg des **manuellen** „als gespielt markieren" ist vollständig —
`podcasts_view_actions.rs:91-105`:

```rust
podcasts::store::mark_played(&view.conn, id, …);
view.refresh();                          // ← lädt aus der Datenbank neu
(view.callbacks.on_sidebar_refresh)();
```

Der Weg des **Durchhörens** endet auf halber Strecke — `window.rs:224-228`:

```rust
player.add_on_episode_played(move || {
    if let Some(sidebar) = sidebar_for_played.upgrade() {
        sidebar.refresh("episode played");
    }
});
```

Nur die Seitenleiste. `render()` zeichnet danach zwar neu
(`podcasts_view.rs:385-392`), aber aus `self.rows` — dem Stand vom Laden.

Es fehlt **kein Mechanismus, nur ein Abnehmer.**

## Die Entscheidung: chirurgisch, nicht neu laden

Im Grilling am 18.08.2026 entschieden, gegen den vollen `refresh()`.

`refresh()` lädt die Datenbank neu und baut die Liste über
`podcasts_groups::replace` wieder auf. Der Expander-Zustand überlebt das
(`expanded_sources` wird durchgereicht, `podcasts_view.rs:415-416`) — die
Scrollposition ist ungeprüft. Ein vom Nutzer nicht ausgelöster Neuaufbau, der
ihm womöglich die Scrollposition wegzieht, ist der falsche Preis für eine
Statuspille. `podcasts_view_marker.rs:197-208` begründet für den Pausenfall
bereits genau diese Zurückhaltung.

Also: die eine betroffene Zeile patchen. Dem vorhandenen Muster folgen —
`update_download_state(episode_id, state)` (`podcasts_view.rs:603`) und
`update_playback_state` (`podcasts_view_marker.rs:203-207`) tun bereits genau
das.

```
notify_episode_played(id)
   └► view.update_played_state(id)
        ├─ rows[id].played_at = now
        ├─ rows[id].position_ms = 0
        └─ Statuspille neu — nur diese Zeile
```

## Aufgaben

1. **`notify_episode_played` reicht die Episoden-ID durch.** Signatur von
   `Fn()` auf `Fn(i64)` (`external_media.rs:67-82`). Beide Aufrufstellen haben
   die ID bereits als lokale Variable vorliegen
   (`external_media_completion.rs:61-68` und `:115-124`) — es ist ein
   Durchreichen, keine neue Ermittlung.
2. **Neue Methode `update_played_state(episode_id)` auf `PodcastsView`.**
   Sie setzt in `self.rows` für diese ID `played_at` und `position_ms = 0`,
   erneuert die Statuspille dieser einen Zeile über die vorhandenen
   Zeilen-Widgets und rührt sonst nichts an. Kein `render()`, kein
   `podcasts_groups::replace`.
3. **`window.rs:224-228` bekommt die beiden Quellansichten als weitere
   Abnehmer** — `source_views.podcasts` und `source_views.youtube`
   (`window/source_views.rs:11-12`), als `Weak` gehalten wie die Seitenleiste.
   Die Seitenleiste behält ihren bestehenden Abnehmer unverändert.
4. **Kennt eine Ansicht die ID nicht, tut sie nichts.** Damit erübrigt sich
   jede Fallunterscheidung zwischen Podcast- und YouTube-Ansicht: der Aufruf
   geht an beide, genau eine trifft zu, und keine von beiden fragt die
   Datenbank.
5. **`youtube_channel_detail` braucht keinen eigenen Draht** — es wird aus
   `render()` mit versorgt (`podcasts_view.rs:403`). Zu prüfen ist nur, ob es
   für die chirurgische Änderung eine eigene Aktualisierung braucht, analog zu
   `update_download_state`, das es über `:603` schon bekommt.

## Abgrenzung

Eine **Abschlussschwelle** („Rest < 30 s gilt als durchgehört") ist nicht Teil
dieses Fehlers. Hier läuft die Episode nachweislich bis zum Ende durch,
`mark_played()` schreibt korrekt. Das ist ein eigener Wunsch.

## Nachweis

Die Datenbank ist im Fehlerfall **richtig** — ein Test gegen die Datenbank misst
also nichts. Der Nachweis muss an der Oberfläche geführt werden.

1. Eine Episode bis zum Ende hören (oder nahe ans Ende springen und auslaufen
   lassen), während die Podcast-Liste sichtbar ist: die Pille „Resume xx %"
   verschwindet, ohne Ansichtswechsel.
2. Dasselbe in der YouTube-Ansicht.
3. Scrollposition und aufgeklappte Kanäle stehen danach unverändert. Das ist
   der Grund für den chirurgischen Weg und gehört ausdrücklich beobachtet.
4. Der Zähler „ungespielt" in der Seitenleiste zieht weiterhin nach — die
   bestehende Wirkung darf nicht verloren gehen.
5. Läuft die Episode aus, während eine ganz andere Ansicht sichtbar ist, zeigt
   die Podcast-Liste beim nächsten Öffnen den richtigen Zustand.
6. Regressionstest auf der Verdrahtung: `add_on_episode_played` hat mehr als
   einen Abnehmer, und `notify_episode_played` wird an beiden Stellen mit der
   ID aufgerufen. `external_media_completion.rs:212-221` führt eine solche
   Zusicherung bereits als Quelltexttest — diesem Muster folgen, statt ein
   GTK-Fenster zu bauen.

## Parallelität

**Nicht teilbar.** Signaturänderung, neue Methode und Verdrahtung greifen
ineinander: die Verdrahtung baut ohne die Methode nicht, die Methode ist ohne
die ID nicht aufrufbar. Drei Dateien, eine Änderung.
