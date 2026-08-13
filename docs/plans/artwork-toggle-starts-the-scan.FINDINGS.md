# Review-Befunde: Artwork einschalten heißt jetzt laden

> Erhoben am 13.08.2026 in der Check-Phase gegen
> `feature/artwork-toggle-starts-the-scan` @ `95974b56b8`, Merge-Basis
> `4912275130`. Vier Reviewer: drei `rust-reviewer` (Core, Wirkkette, sichtbare
> Flächen) und ein Spec-Abgleich gegen `artwork-toggle-starts-the-scan.md`.
> Vom Nutzer angenommen: **alle 13**. Befunde 1–5 und 9 wurden zusätzlich am
> Code nachgeprüft, die übrigen stehen als Reviewer-Behauptung.
>
> Zeilennummern gelten für `95974b56b8`. Wo eine nicht mehr passt, gilt der
> genannte Funktionsname.

## Kritisch

### 1 — Eine kaputte MusicBrainz-Antwort brennt das Album permanent fest

`crates/reprise-core/src/cover_download.rs:259-266`

`parse_best_release` gibt `None` in drei verschiedenen Lagen zurück: bei
unparsbarem JSON (`serde_json::from_str(json).ok()?`), bei gültigem JSON ohne
`releases`-Feld (`v.get("releases")?.as_array()?`) und bei einem echten
Fehlschlag der Titelsuche. Alle drei laufen in denselben Arm
`None => { write_negative(&key); return CoverFetchOutcome::NotFound }`. Der
Album-Negativmerker läuft nie ab (siehe Kommentar bei `cover_download.rs:21`).

Ein Track ohne eingebettete MBID nimmt den Suchpfad. Antwortet MusicBrainz mit
HTTP 200 und einer Proxy-Zwischenseite, einem Deploy-Fehler oder geändertem
Schema, dann liefert `mb_fetch` ein `Some(body)` — der neue Transient-Zweig bei
`:256-258` greift also gar nicht — und das Album gilt für immer als „nichts zu
holen". Künftige Durchläufe überspringen es über
`cover::download_marked_unavailable`.

Das ist dieselbe Korruption, die P3 abstellen sollte, nur über den Sucheingang
statt über den Abrufeingang. Ein Parse- oder Formfehler ist kein „gibt es
nicht"; nur eine wohlgeformte Antwort ohne passenden Treffer ist einer.

### 13 — Übergroßes oder unlesbares CAA-Bild wird ebenfalls endgültig

`crates/reprise-core/src/cover_download.rs:318-324`

Dieselbe Klasse, andere Stelle: eine Antwort über `MAX_IMAGE_BYTES` und ein
vollständig geladener, aber nicht als Bild erkennbarer Rumpf
(`validated_image_extension` → `None`) werden beide zu
`CaaFetchResult::NotFound` und schreiben damit jetzt den permanenten Merker.

> **Umfangshinweis:** Dieser Befund liegt im Bestand und wird von diesem Zweig
> nicht berührt. Der Nutzer hat ihn am 13.08. bewusst mit aufgenommen; er
> erweitert den Umfang des Plans. Er gehört sachlich zu Befund 1 und sollte mit
> ihm zusammen bearbeitet werden.

## Major

### 2 — `start()` überfährt einen laufenden Nutzerlauf

`crates/reprise-gnome/src/ui/cover/cover_download_batch.rs:139-150` gegen `:155-165`

`start_user_triggered` prüft `self.running` und steigt bei einem laufenden Pass
aus. `start()` prüft nur `runtime.enabled` und die Fälligkeit. `start()` ist
aber nicht auf den Programmstart beschränkt:
`crates/reprise-gnome/src/ui/cover/main_cover_download_progress.rs:116` hängt es
über `scan_controls.add_on_complete(move || batch.start())` an das Ende **jedes**
Bibliotheksscans.

Ablauf: Nutzer schaltet Artwork ein, der Durchlauf steht bei 40 von 300; ein
Rescan (manuell oder durch den Watcher) endet; `add_on_complete` feuert
`start()`; die Signatur hat sich durch den Scan mit hoher Wahrscheinlichkeit
geändert, die Fälligkeitsprüfung sagt also ja; `generation` springt und der
Nutzerlauf beginnt bei null. Die vorhandene `generation`-Mechanik verhindert
den Doppelschreiber, nicht den Verlust des Fortschritts.

Der Doc-Kommentar bei `:152-154` behauptet das Gegenteil dessen, was der Code
leistet: „A second request joins the active pass instead of replacing it with
overlapping work." Das gilt heute nur für `start_user_triggered` gegen sich
selbst, nicht für `start()` gegen einen laufenden Nutzerlauf.

### 3 — Der LYR-6-Regeltest prüft toten Code

`crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs:95-96`,
`crates/reprise-gnome/src/ui/lyrics/lyrics_batch_tests.rs:33`

Die Entscheidung „Einschalten startet den Lauf" ist in diesem Zweig nach
`preference_online_module_effects.rs:56-71` gewandert. `recompute_enabled` ist
dabei auf `#[cfg(test)]` heruntergestuft worden und hat repo-weit nur noch
Aufrufer in `lyrics_batch_tests.rs`.

`docs/ux-rules.md` führt LYR-6 als `[active]`; laut `AGENTS.md` sichern
`[active]`-Regeln den Merge über ihren namensgleichen Test. Dieser Test,
`lyr_6_enabling_the_module_starts_the_batch_once_and_nothing_else_does`, ruft
weiterhin `batch.recompute_enabled()` — also einen Weg, den die Produktion nicht
mehr geht. Fiele der `PermissionEffect::Start`-Arm im neuen Code weg oder
brächte `effect_for_transition` die Aus-nach-Ein-Erkennung durcheinander, bliebe
LYR-6 grün, während Online Lyrics in der ausgelieferten App stumm nicht mehr
startet.

Der Regeltest muss den Weg prüfen, den die Produktion nimmt.

### 4 — Kein UX-Regeleintrag für das neue Verhalten

`docs/ux-rules.md` hat im gesamten Zweig **null** geänderte Zeilen (geprüft:
`git diff --name-only 4912275130..HEAD -- docs/ux-rules.md`).

Der Sofortstart beim Einschalten ist sichtbares Nutzerverhalten. `AGENTS.md`
verlangt, dass eine Regel `[planned]` → `[active]` in demselben Commit kippt,
der das Verhalten baut und seinen Test mitbringt. So wie der Zweig steht, hängt
das neue Verhalten an keiner Regel-ID und ist für
`scripts/check-ux-traceability.sh` unsichtbar.

Betroffen ist auch P4 (bereits gezeichnete Flächen fordern nach) — das ist
eigenes sichtbares Verhalten und braucht seine eigene Zusage.

### 5 — Kein Test fährt die echte Wirkkette

`crates/reprise-gnome/src/ui/window/window_artwork_permission_wiring.rs:18-39`,
`crates/reprise-gnome/src/ui/preferences/preference_online_module_effects.rs:41-85`

Von drei Reviewern unabhängig gefunden. Die Kette

```
set_module_enabled → refresh_online_module_state → effect_for_transition
                   → apply_artwork_effect → Rückruf → refresh_visible_artwork
```

hat keinen einzigen Test. Was existiert, prüft ausschließlich die Enden:

- `effect_for_transition` als reine Funktion
  (`preference_online_module_effects.rs:88-121`),
- `batch.start_user_triggered()` / `batch.cancel()` direkt am nackten
  `CoverDownloadBatch` (`main_cover_download_progress.rs`, Test
  `an_artwork_enable_starts_one_fresh_pass_and_disable_stops_it`, zudem
  `#[ignore]`),
- `StatsBandsRow::set_data` direkt an der Zeile
  (`stats_bands_row_tests.rs:105`).

Verifikationspunkt 3 des Plans verlangt aber ausdrücklich den Weg **über**
`set_module_enabled`. Ein Fehler in der Verdrahtung selbst — `wire()` wird nie
gerufen, ein `Weak::upgrade` schlägt still fehl, `effect_for_transition` bekommt
die falschen Eingaben — bliebe von allem Grünen unbemerkt.

Ebenfalls ungedeckt: `StatsView::refresh_visible_artwork` (`stats_view.rs:331`),
`RadioView::refresh_visible_artwork` (`radio_artwork_refresh.rs:7`) und
`PodcastsView::refresh_visible_artwork` (`podcasts_view.rs:350`) werden von
keinem Test aufgerufen — samt ihrer `is_mapped()`-Riegel und, bei Stats, der
`current_snapshot`-Verkabelung.

Zur Einordnung: der Stats-Test beweist auf **Zeilenebene** echt, dass eine
bereits gezeichnete Zeile nach dem Öffnen des Riegels fünf Porträts neu
anfordert. Er geht nur nie durch `StatsView`. Beide Aussagen der Reviewer sind
richtig, auf verschiedenen Ebenen — der Test ist nicht falsch, er reicht nur
nicht bis zur Naht.

### 6 — Der Radio-Test schaltet den Riegel gar nicht um

`crates/reprise-gnome/src/ui/radio/radio_columns.rs:158-197`

`artwork_permission_rebinds_visible_radio_images_without_resetting_the_model`
ruft `artwork_cells.reapply()` bei `:190` unbedingt auf, ohne den
Artwork-Riegel (`gate_open()` / `GATE_OPEN`) zwischen dem Vorher- und dem
Nachher-Zustand umzulegen. Der Test beweist, dass `reapply()` eine neue
Bildzelle einsetzt, ohne `selection.selected()` zu verlieren — das ist eine
echte und nützliche Zusicherung. Er beweist nicht, dass der Übergang
„darf nicht → darf" das Neuanfordern auslöst, und auch nicht, dass die neue
Zelle überhaupt etwas anderes anfordert als die alte.

Der Testname verspricht Riegelverhalten, das der Rumpf nicht ausübt. Als
P4-Beleg für Radio taugt er so nicht.

### 7 — Der `startup_tasks`-Test kann „festgeschrieben" nicht von „nichts getan" unterscheiden

`crates/reprise-core/src/library/startup_tasks.rs:472-484`

`a_user_triggered_pass_ignores_due_state_and_settles_the_current_signature`
schreibt die Bibliothekssignatur nie fort — `advance_library_signature_in`
(`:314`) steht bereit, wird aber nicht benutzt, die Signatur bleibt `0`. Das
einleitende `record_completed_at(&db, CoverDownload, 123)` versetzt die DB
bereits für Signatur `0` in den `Skip`-Zustand. Die Schlussbehauptung
`assert!(begin_exact(&db, CoverDownload).is_none())` hielte deshalb auch dann,
wenn `begin_user_triggered` gar keine Signatur mehr erfasste und festschriebe —
`record_completed_or_warn` würde nur warnen, und der alte Skip-Zustand bliebe
unangetastet.

Der Test kann seine eigene Namensbehauptung nicht belegen.

### 8 — Der Transient-Zweig der MusicBrainz-Suche ist ungetestet

`crates/reprise-core/src/cover_download.rs:256-258`

Der neue Zweig
`let Some(body) = mb_fetch(...) else { return CoverFetchOutcome::TransientFailure }`
wird von keinem Test erreicht. Beide neuen Tests
(`transient_album_fetch_does_not_write_a_negative_marker`,
`definitive_album_miss_writes_a_negative_marker`, `:529-566`) übergeben
`Some(mbid)` und lassen die `mb_fetch`-Closure panicken, betreten den Suchpfad
also bewusst nie. Es gibt im ganzen Modul keinen Test mit `mbid: None` und
scheiterndem `mb_fetch`.

Das ist derselbe Codebereich wie Befund 1.

## Medium

### 9 — Podcasts baut die ganze Seite neu, statt die Bildzellen nachzubinden

`crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs:348-354`

`refresh_visible_artwork` ruft `self.render()`. Über `podcasts_groups::replace`
(`podcasts_groups.rs:108-110`) werden dabei sämtliche Gruppen- und Zeilenwidgets
der Seite zerstört und neu gebaut — nicht nur die Bildzellen. Radio löst
dieselbe Aufgabe chirurgisch über die `RadioLiveCells`-Registratur.

Ablauf: ein Nutzer mit mehreren aufgeklappten Podcast-Quellen und vielen
Episodenzeilen schaltet Artwork ein; die gesamte sichtbare Seite wird synchron
abgerissen und neu aufgebaut, um eine Handvoll Vorschaubilder aufzufrischen.

Der Doc-Kommentar („Rebinds the rows already held by a visible source page")
beschreibt nicht, was der Code tut.

Nebenbefund, unbestätigt: dabei wird jedes `SourceImage` jeder sichtbaren Zeile
neu gebaut, auch solche, deren Bild vor dem Schalterdruck schon aufgelöst war.
`cached_texture` in `source_image.rs` begrenzt das bei Cache-Treffern auf keinen
neuen Netzverkehr, ein echter Anfragensturm konnte nicht nachgewiesen werden.

## Minor

### 10 — Der Konnektivitäts-Riegel gilt jetzt auch für Online Lyrics

`crates/reprise-gnome/src/ui/preferences/preference_online_module_effects.rs:16-26`,
angewandt bei `:43`, `:56`, `:63-71`

`effect_for_transition` entscheidet inzwischen für Artwork **und** für Lyrics.
Vor diesem Zweig startete der Lyrics-Lauf bei jedem Aus-nach-Ein-Übergang und
war nur durch die gespeicherten `network_allowed_or_off`-Flags gebremst, nie
durch die Live-Projektion des `gio::NetworkMonitor`. Jetzt ergibt ein
Einschalten bei `Connectivity::Offline` ein `PermissionEffect::None`, der Lauf
unterbleibt ersatzlos — ein Nachholen bei zurückkehrendem Netz ist laut Plan
bewusst nicht gebaut.

**Entscheidung des Nutzers vom 13.08.2026: zurücknehmen.** Der Plan begründet
die Netzvorbedingung ausschließlich mit dem Cover-Pfad und
`remember_download_unavailable`; Lyrics kennt keinen solchen zerstörerischen
Merker, ein Fehlversuch kostet dort nichts. Online Lyrics startet beim
Einschalten also wieder unabhängig von der Live-Verbindung, wie vor dem Zweig.
LYR-6 bleibt damit unverändert gültig und braucht keine Netzbedingung.

### 11 — Doppelte Signaturerfassung in `startup_tasks`

`crates/reprise-core/src/library/startup_tasks.rs:230-242` und `:260-275`

`begin_exact` und `begin_user_triggered` tragen denselben Block
`match current_signature_in(db.conn()) { Ok(..) => Some(..), Err(..) => { warn; None } }`
wortgleich, unterschieden nur durch den Logtext. Ändert sich die
Fehlerbehandlung später an einer Stelle (Wiederholung, anderer Loglevel), driftet
die andere mit. Genau das Muster, das dieses Projekt schon getroffen hat.

### 12 — Die Klassifikation eines Cache-Schreibfehlers ist ungetestet

`crates/reprise-core/src/cover_download.rs:279-282`

`store_album_downloaded(...).map_or(CoverFetchOutcome::TransientFailure, CoverFetchOutcome::Downloaded)`
wird von keinem Test durch `fetch_and_cache_with` gefahren. Die vorhandenen
Schreibfehler-Tests rufen `store_album_downloaded_with` direkt auf und prüfen
damit nicht die Einstufung, um die es in diesem Paket geht.

## Offen, aber kein Befund

Die visuelle Abnahme (Verifikationspunkt 5 des Plans) steht aus. Beide
einschlägigen Display-Tests
(`an_already_rendered_row_requests_portraits_after_artwork_is_enabled`,
`artwork_permission_rebinds_visible_radio_images_without_resetting_the_model`)
tragen `#[ignore]` und laufen im Normalbetrieb nicht mit. Der CUA-Versuch ist
laut Codex-Protokoll und `.superpowers/sdd/progress.md` an einer degradierten
AT-SPI-Bridge gescheitert. Der Plan hat diesen Rest ausdrücklich vorweggenommen —
erledigt ist er trotzdem nicht.

## Als geliefert bestätigt

Nicht erneut anfassen:

- **P1** — `record_completed_at` bleibt `#[doc(hidden)]` und testintern
  (`startup_tasks.rs:279-292`); `begin_user_triggered` (`:260-275`) ist der
  eigene, benannte Produktionsweg. `start()` behält seinen Riegel.
- **P2 mechanisch** — Plugins-Schalter (`preference_plugins.rs:190,216`) und
  Seitenleistenaktion (`preference_module_state.rs:66`) laufen beide über
  `set_module_enabled`; der globale Riegel `set_online_sources_enabled`
  (`preferences.rs:461-467`) benutzt dieselbe `refresh_online_module_state`.
  Der Konstruktor von `PreferencesContext` ist bei 21 Argumenten geblieben, die
  Anbindung läuft über `set_on_artwork_permission_changed` — genau nach dem
  `on_source_modules_changed`-Muster.
- **P3 teilweise** — `window/source_connectivity.rs:127` bleibt einziger
  `NetworkMonitor::default()`-Leser des Zweigs; `set_connectivity` ist ein
  reiner Setzer ohne Nebenwirkung, ein Nachholen bei Netzrückkehr existiert
  korrekterweise nicht. `outcome_settles_track`
  (`cover_download_batch.rs:352-357`) schließt `TransientFailure` richtig aus.
- **P4 mechanisch** — ein Rückruf
  (`window_artwork_permission_wiring.rs:18-39`) bedient My Stats, Podcasts,
  YouTube und Radio, jeweils hinter `is_mapped()`.
- **Abgrenzung** — kein neues Fortschritts-Element, der Startpfad
  (Startruhe, Fälligkeit, `start_after_cover`) ist unberührt, kein Vorwärmen
  unsichtbarer Bilder, kein Nachholen bei Netzrückkehr. Kein Scope Creep.

## Auftrag an die Refactor-Phase

Codex baut, nicht Opus — auch dort, wo der Review den Fix schon benennt.
Änderungen bleiben auf die Befunde beschränkt. Für jeden reparierten Befund
gehört ein Test dazu, der ohne die Reparatur rot wäre; Befunde 5, 6, 7, 8 und 12
**sind** Testbefunde und haben keinen anderen Inhalt.

Zwei Fallen dieses Repos, die für die Testarbeit hier gelten:

- In `reprise-gnome` gibt es kein lib-Target: `cargo test --lib` führt **nichts**
  aus, nur `--bin reprise` fährt die Tests.
- Ein `test result: ok` beweist nichts, wenn der Filter ins Leere lief. Die Zahl
  der ausgeführten Tests ist mitzulesen.
