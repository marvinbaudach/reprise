# Zum laufenden Element springen — auch wenn es keine Musikdatei ist

Die Player-Bar verlinkt heute nur Bibliothekstitel. Läuft eine Podcast-Folge,
ein YouTube-Video oder ein Radiosender, sind Titel, Kanal und Cover tote
Flächen: `reveal_playing_track` steigt bei `current_track_id() == None` aus,
und genau das ist in jedem externen Modus der Fall. `Ctrl+L` verhält sich
identisch — es hängt an derselben Funktion.

Das ist kein fehlendes Reveal-Verfahren, sondern ein fehlender Einstieg. Die
Aufdeck-Mechanik existiert vollständig (`SRC-13`): `podcasts_reveal.rs` klappt
die Kanalgruppe und, wenn nötig, das Zehner-Episodenfenster auf und zentriert
die Zeile; `radio_reveal.rs` zentriert die Senderzeile. Sie greift nur, wenn
der Nutzer die Ansicht **selbst** betritt.

Diese Spec baut die Brücke: die Player-Bar-Links und `Ctrl+L` navigieren zur
Quellenansicht und lösen dort dasselbe Reveal aus.

---

## Geltungsbereich

| Oberfläche | betroffen |
| --- | --- |
| Podcasts (`PodcastsView`, `PodcastKind::Rss`) | ja |
| YouTube (`PodcastsView`, `PodcastKind::Youtube`) | ja |
| Radio (`RadioView`) | ja |
| Concerts | **nein** — dort läuft nichts, es gibt kein „gerade gespielt" |

`PodcastsView` ist eine Klasse, zweimal instanziiert (`window/source_views.rs`).
Podcasts und YouTube sind deshalb dieselbe Änderung, kein Portierungsschritt.

Nicht im Scope: eine Kanal-Detailseite als Sprungziel. Die YouTube-Kanalseite
(`youtube_channel_detail.rs`) existiert, ist aber nicht das Ziel dieser Links —
der Kanal wird in der Liste aufgedeckt, wo auch die Episode liegt.

---

## Teil A — Was die drei Bedienstellen tun

Drei Bedienstellen, zwei Ziele. Das spiegelt die Musikbibliothek, wo Titel den
Track zeigt und Cover das Album.

| Bedienstelle | Musik (heute) | Podcast/YouTube (neu) | Radio (neu) |
| --- | --- | --- | --- |
| Titel | Track aufdecken | **Episode** aufdecken | Sender aufdecken |
| Kanal-/Interpretenzeile | Interpret öffnen | **Kanal** aufdecken | Sender aufdecken |
| Cover | Album öffnen | **Kanal** aufdecken | Sender aufdecken |
| `Ctrl+L` | Track aufdecken | **Episode** aufdecken | Sender aufdecken |

Radio kennt keine Gruppierung: Sender, „Kanal" und „Album" sind dieselbe Zeile.
Alle drei Bedienstellen führen dorthin. Das ist keine Doppelung, sondern die
ehrliche Antwort auf eine flache Liste — jeder Klick landet dort, wo etwas ist.

### A.1 „Episode aufdecken"

Unverändert das, was `podcasts_reveal::reveal_target` bereits leistet:
Kanalgruppe aufklappen, bei Bedarf das Zehner-Fenster öffnen, Zeile zentrieren.
Ohne Fokus- oder Selektionswechsel (`SRC-13`).

### A.2 „Kanal aufdecken" — neu

Die Gruppe des Kanals wird aufgeklappt und **die Gruppenkopfzeile** zentriert,
nicht die Episode. Das Episodenfenster bleibt unangetastet: wer den Kanal
anspringt, will den Kanal von oben sehen, nicht eine Zeile in seiner Mitte.

Das ist die einzige neue Mechanik dieser Spec. Sie braucht das Kopfzeilen-Widget
zur `subscription_id`; `podcasts_groups` baut die Kopfzeilen, `podcasts_view`
hält heute nur die Episodenzeilen (`download_widgets`). Wie die Kopfzeile
adressierbar wird, entscheidet der Plan — eine zweite Map ist zulässig, wenn
sie denselben Lebenszyklus wie `download_widgets` hat.

Zentriert wird mit derselben Tick-Callback-Mechanik wie in `podcasts_reveal`
(`compute_point` + `motion::timed`, Sprung statt Animation bei ausgeschaltetem
Motion-Gate). Kein zweiter Zentrier-Weg.

### A.3 Radio

`radio_reveal` zentriert die verbundene Senderzeile. Keine neue Mechanik.

---

## Teil B — Der Weg dorthin: zwei neue Intents

`BROWSE-4` verlangt, dass Metadaten app-weit über zentrale Intents navigieren.
Ein Sonderpfad direkt aus der Player-Bar in die View wäre genau die
Prädikat-Doppelung, die in diesem Projekt schon zweimal ein hörbarer Bug war.
Also: zwei neue Varianten in `reprise_core::browser::NavigationIntent`.

```rust
RevealEpisode {
    subscription_id: i64,
    /// `None` = nur den Kanal aufdecken (Kanal- und Cover-Klick).
    episode_id: Option<i64>,
    kind: PodcastKind,
},
RevealStation {
    station_id: i64,
},
```

`kind` ist **kein** `PodcastKind`. Ein browser-eigenes `SourceKind { Podcasts,
Youtube }` hält die Navigations-Grammatik frei von der Podcast-Domäne;
`reprise_core::browser` importiert heute nur `BrowseFilter` und `ViewSource`,
und das soll so bleiben.

`kind` wählt den Ziel-Place: `BrowserPlace::Podcasts` bzw. `BrowserPlace::Youtube`.
`RevealStation` zielt auf `BrowserPlace::Radio`. Beide laufen durch dieselbe
`go_metadata_scope`-Logik wie `RevealTrack`: kommt man von woanders, ein `New`
mit Verlauf, sodass `Back` zurückführt.

**Kein `Replace`.** Steht man bereits in der Zielansicht, liefert
`go_metadata_scope` für diese zustandslosen Places `None` — es gibt keinen
Übergang. Das ist richtig so und wird nicht umgebogen: die Navigation *ist* am
Ziel. Der Reveal-Auftrag (Teil C) wird deshalb **unabhängig vom
Übergangsergebnis** erteilt, auch wenn `navigate` `None` zurückgibt. Genau
dieser Fall ist der Grund, aus dem es Teil C gibt.

Nebenbefund, hier mitzufixen: `same_destination` kennt `BrowserPlace::Youtube`
nicht — zwei YouTube-Places gelten einander heute als verschiedene Ziele. Das
ist ohne diese Arbeit folgenlos und stünde ihr direkt im Weg.

Ungültige Eingaben (`subscription_id <= 0`, `station_id <= 0`) sind No-ops, wie
`RevealTrack` es mit `track_id <= 0` hält.

### B.1 Woher der Player die Daten nimmt

`ExternalMedia::Podcast` trägt heute `episode_id`, aber weder `subscription_id`
noch `kind` — beides steckt in `PodcastSession` und erreicht den
`ExternalPlaybackSnapshot` nicht. `PlayerController` bekommt deshalb einen
synchronen Accessor in der Form von `album_identity.rs` (einen
`PodcastController` gibt es nicht; die gesamte externe Wiedergabe liegt als
`impl PlayerController`):

```rust
enum LoadedSourceItem {
    Episode { subscription_id: i64, episode_id: i64, kind: PodcastKind },
    Station { station_id: i64 },
}

fn current_source_item(&self) -> Option<LoadedSourceItem>
```

Ob `kind` dafür in `PodcastSession` mitgeführt oder beim Start der Sitzung aus
`podcast_subscriptions` gelesen wird, entscheidet der Plan. Ein DB-Query im
Klick-Handler ist die schlechtere von beiden Möglichkeiten.

### B.2 Wer entscheidet, welcher Intent fliegt

`window_runtime_wiring.rs` — dort, wo `reveal_playing_track`,
`reveal_playing_album` und `reveal_playing_artist` schon liegen. Jede der drei
Closures fragt zuerst `current_source_item()`; liefert es `Some`, fliegt der
neue Intent, sonst der bisherige Track-/Album-/Artist-Pfad. Ein Zweig pro
Closure, keine zweite Verdrahtungsstelle.

Das Info-Panel (`set_on_track_reveal`, `set_on_album_reveal`,
`set_on_artist_reveal`) und das Now-Playing-Panel teilen sich diese Closures
bereits und erben das Verhalten, ohne angefasst zu werden — genau das verlangt
`BROWSE-4`s „regardless of origin".

---

## Teil C — Ein expliziter Sprung deckt immer auf

`SRC-13`s `ViewEntered` hängt an `connect_map`. Steht der Nutzer bereits in der
Zielansicht, wird nichts neu gemappt — der Klick liefe ins Leere, und zwar
ausgerechnet in dem Fall, in dem der Nutzer die Liste vor sich hat.

`source_reveal.rs` bekommt deshalb eine vierte Variante:

```rust
LoadedItemChange::RequestedByUser  // ⇒ immer RevealPolicy::Reveal
```

Sie ignoriert `USER_SCROLL_GRACE` bewusst. Die Grace-Periode schützt den
lesenden Nutzer vor einem Viewport, der ihm unter der Hand wegspringt; hier hat
er den Sprung selbst ausgelöst. Ihn dann zu verweigern, weil er eine Sekunde
vorher gescrollt hat, wäre die Regel gegen ihren eigenen Zweck gewendet.

Die Views bekommen dafür je einen Eingang — `PodcastsView::request_reveal(
subscription_id, episode_id: Option<i64>)` und, weil `RadioReveal`
ausschließlich die verbundene Station aufdecken kann und eine fremde ID gar
nicht anspringen könnte, `RadioView::request_reveal_connected()`. Der
Navigations-Router ruft sie nach dem Seitenwechsel auf. Die View führt den
Auftrag aus, sobald sie gemappt und gerendert ist; die vorhandene
Tick-Callback-Schleife mit `MAX_LAYOUT_FRAMES` deckt den Fall „Seite gerade erst
umgeschaltet" bereits ab.

Radio hat zusätzlich eine Wache, die nur bei *gewechselter* verbundener Station
aufdeckt. Ein Nutzersprung zur bereits verbundenen Station ist der Normalfall
und muss an ihr vorbei — sonst wäre der Radio-Sprung tot geboren.

**Wettlauf mit `ViewEntered`:** Beim Seitenwechsel mappt sich die Zielansicht
und feuert `ViewEntered` auf die *geladene Episode* — beim Kanal-Klick zielt das
im selben Frame auf eine andere Zeile als der angeforderte Reveal. Der
angeforderte Reveal gewinnt: liegt beim Mappen ein Auftrag vor, unterdrückt er
das `ViewEntered` dieses einen Durchlaufs.

### C.1 Ausgeblendetes Ziel: nur die verbergende Facette weicht

`SRC-13` sagt für passive Reveals: ein durch den aktiven Filter verborgenes
Element wird nicht aufgedeckt, und der Filter wird nie geräumt. Für einen
**expliziten** Sprung ist das eine Sackgasse — der Nutzer klickt und nichts
passiert, was exakt der Bug ist, den diese Spec behebt. Und es ist nicht der
Randfall: die laufende Episode ist gerade *nicht* mehr „Unplayed", ein aktiver
Unplayed-Filter verbirgt sie also im Normalbetrieb.

Entscheidung: Ein `RequestedByUser`-Reveal schaltet **genau die Facetten** ab,
die das Ziel verbergen — nicht alle. Verbirgt „Unplayed" die Episode, geht
dieser Chip aus; „Downloaded" bleibt stehen. Ist das Ziel ohnehin sichtbar,
bleibt der Filter vollständig unangetastet.

Das ist bewusst mehr Arbeit als ein `clear_all`, und zwar aus zwei Gründen:
`clear_all` schreibt persistent in die Datenbank, und die Filterschlüssel sind
global statt pro `kind` — ein Klick auf die Player-Leiste würfe damit dauerhaft
auch den Filter der *anderen* Podcast-Ansicht weg. Ein Sprung darf keine
Aufräumaktion in einer Ansicht auslösen, die der Nutzer gar nicht ansieht.

Der Plan braucht dafür ein reines Prädikat („welche Facette verbirgt dieses
Element?") auf demselben Filter-Prädikat, das die Liste schon benutzt — kein
zweites, das driften kann. `SRC-13`s Satz für die passiven Auslöser bleibt
wörtlich stehen.

---

## Teil D — Beschriftung

Die drei Bedienstellen tragen heute feste Tooltips und Accessible-Labels
(`JUMP_TO_NOW_PLAYING`, `GO_TO_PLAYING_ARTIST`, `REVEAL_PLAYING_ALBUM`). Im
externen Modus stimmen zwei davon nicht mehr: dort gibt es keinen Interpreten
und kein Album.

Die Beschriftung folgt dem Modus. Neue Strings über die vorhandenen
`strings_*`-Fassaden mit `N_!`, mitsamt `.pot`/`.po`-Aktualisierung:

| Bedienstelle | Podcast/YouTube | Radio |
| --- | --- | --- |
| Titel | „Zur laufenden Episode" | „Zum laufenden Sender" |
| Kanalzeile | „Zum Kanal" | „Zum laufenden Sender" |
| Cover | „Zum Kanal" | „Zum laufenden Sender" |

Die Kanalzeile ist im externen Modus immer bedienbar, solange eine Sitzung
läuft — die heutige `set_sensitive(!artist.trim().is_empty())`-Regel darf einen
Kanal-Link nicht deaktivieren, nur weil das Label leer gerendert wurde.

`Ctrl+L` behält seine eine Beschriftung in der Tastenkürzel-Hilfe
(`help.rs`); es hat keinen modusabhängigen Text, weil es keine sichtbare Fläche
hat.

### D.1 Wo die Beschriftung herkommt

`set_track` schreibt heute bei jedem Wechsel `REVEAL_PLAYING_ALBUM` auf den
Cover-Button, und `set_external_snapshot` ruft `set_track` als erstes. Die
modusabhängigen Labels danach drüberzuschreiben wäre von der Aufrufreihenfolge
abhängig und damit still zerbrechlich. Stattdessen bekommt `set_track` den Modus
mit und setzt die Beschriftung einmal richtig.

Der Quelltext-Test `tip_1d` behauptet, ein bestimmtes Tooltip-Literal komme in
`player_bar_layout.rs` genau einmal vor. Er bewacht damit künftig eine Aussage,
die nicht mehr stimmt — die Beschriftung ist nicht mehr im Bau festgelegt. Der
Test wird umgeschrieben, nicht umgangen.

### D.2 Auch das Now-Playing- und Info-Panel

Die Panels teilen sich die Klick-Closures mit der Player-Leiste und erben das
neue Zielverhalten automatisch. Ihre Beschriftung erben sie **nicht**: sie sagen
während einer Podcast-Wiedergabe weiterhin „Go to playing artist", obwohl der
Klick zum Kanal führt. Ein Link, der etwas anderes sagt als er tut, ist kein
erfülltes „keine toten Flächen" — die modusabhängige Beschriftung gilt deshalb
auch dort.

### D.3 Ein Sprung, der nicht landen kann, sagt es

Für Tracks gibt es bereits einen Toast, wenn das Ziel nicht mehr in der Liste
ist. Episoden und Sender bekommen sein Gegenstück: ist die geladene Episode aus
dem Feed verschwunden oder der Sender nicht mehr in den Favoriten, sagt ein
Toast das, statt dass der Klick still verpufft. Das ist der zweite Halbsatz der
Player-Leisten-Regel — „nie ins Leere" heißt auch „nie stumm".

---

## Teil E — Regelwerk

**Neue Regel: die Player-Leiste hat keine toten Flächen.** Das ist die
allgemeine Form des Befunds — dieser Bug ist nur ihr erster bekannter Verstoß.

> Titel, Kanal-/Interpretenzeile und Cover in der Player-Leiste sind in **jedem**
> Wiedergabemodus Links. Was gerade läuft, ist auffindbar: jede der drei Flächen
> führt zu dem Ort, an dem das laufende Element in einer Liste steht. Gibt es zu
> einer Fläche in einem Modus kein eigenes Ziel, führt sie zum nächstgelegenen
> vorhandenen — nie ins Leere. Eine Fläche darf nur dann unbedienbar sein, wenn
> überhaupt nichts geladen ist; sie ist dann sichtbar inaktiv, nicht stumm.
> Beschriftung und Tooltip nennen das tatsächliche Ziel des jeweiligen Modus.

Diese Regel gilt für künftige Wiedergabequellen ohne erneute Entscheidung: wer
eine neue Quelle einbaut, schuldet ihr drei landende Links. Der Plan verankert
sie mit einem Test, der über die Modi iteriert, statt nur die heute bekannten
drei abzuhaken.

**`BROWSE-4`** wird erweitert. Der Satz über Track, Album und Artist bekommt
seine Entsprechung für die Quellenlisten:

> Läuft eine Episode oder ein Sender, verlinken dieselben Bedienstellen deren
> Entsprechungen: Titel und `Ctrl+L` decken die Episode auf, Kanalzeile und
> Cover den Kanal; bei Radio führen alle drei zur Senderzeile, weil die Liste
> flach ist. Ziel ist immer die Quellenliste, in der das Element liegt, nie
> eine Detailseite.

**`SRC-13`** bekommt einen Satz für den expliziten Auslöser:

> Ein vom Nutzer angeforderter Sprung deckt immer auf, auch in der bereits
> sichtbaren Ansicht und ungeachtet der 1,5-Sekunden-Frist; er räumt die Filter
> der Zielansicht genau dann, wenn das Ziel sonst verborgen bliebe.

`scripts/check-ux-traceability.sh` prüfen und die Regel-IDs als Kommentare an
die neuen Intent-Arme und die Policy-Variante setzen.

---

## Tests (ohne Display)

- Beide neuen Intents: Ziel-Place je `PodcastKind`, `Replace` in der bereits
  aktiven Ansicht, `New` mit Verlauf von außerhalb, No-op bei ungültiger ID.
- `reveal_policy(RequestedByUser, true)` ⇒ `Reveal`.
- Kanal-Reveal (`episode_id: None`) klappt die Gruppe auf und lässt das
  Episodenfenster unangetastet — auch wenn die geladene Episode dahinter liegt.
- Episoden-Reveal verhält sich unverändert (Regressionsschutz für `SRC-13`).
- Filter-Räumung: verborgenes Ziel ⇒ Filter geräumt; sichtbares Ziel ⇒ Filter
  unverändert.
- `current_source_item()` liefert `None` bei Bibliothekswiedergabe und im
  Leerlauf, `Some` mit korrekter `kind` in beiden Podcast-Sorten.

Display-Tests dürfen ergänzt werden, tragen aber
`#[ignore = "requires a display; run via xvfb-run"]` und gelten nicht als
Beweis — die Display-Suite ist im Rudel flaky.

---

## Randbedingungen

- Basis ist `origin/dev`, **nicht** der lokale Hauptcheckout (203 Commits
  zurück; `ViewSource::Youtube`, `source_reveal.rs` und `podcasts_reveal.rs`
  existieren dort gar nicht).
- Keine neuen Abhängigkeiten, keine neuen Farbtokens.
- Alle nutzersichtbaren Strings über `strings_*` mit `N_!`.
- Fokussierte Commits: Core-Intents, Player-Accessor + Verdrahtung,
  Kanal-Reveal, Beschriftung, Regelwerk.
- Vor Abschluss grün: `cargo fmt`, `cargo clippy --all-targets`,
  `cargo test -p reprise-core`, `cargo test -p reprise-gnome` ohne
  Display-Tests.
