---
slug: artwork-toggle-starts-the-scan
worktree: /home/marvin/Projects/reprise-artwork-toggle-starts-the-scan
branch: feature/artwork-toggle-starts-the-scan
phase: shipped
codex_session:
created: 2026-08-13
---
# Artwork einschalten heißt: jetzt laden, nicht beim nächsten Start

> Alle Zeilennummern wurden gegen `origin/dev` @ `ca85fedffd` erhoben
> (13.08.2026, nach dem Merge von #452). Der lokale Hauptcheckout liegt weit
> zurück und taugt nicht als Basis. Geprüft und gültig auf diesem Stand:
> `preference_module_state.rs:45` (`"artwork"`-Arm) und `:78`
> (`set_module_enabled`), `preferences.rs:464`
> (`refresh_online_module_state`), `cover_download_batch.rs:136` (`start`) und
> `:141` (`begin_exact`), `window_runtime_wiring.rs:486`
> (`start_after_cover`). Wo unten eine Zeilennummer nicht mehr passt, gilt der
> genannte Funktionsname.

## Warum

Wer in den Einstellungen unter *Plugins* den Schalter **Artwork** einschaltet,
erwartet, dass die App anfängt zu arbeiten: fehlende Album-Cover suchen,
Künstlerporträts für *My Stats* holen, Quellbilder für Podcasts, YouTube und
Radio nachladen. Tatsächlich passiert nichts Sichtbares. Der Schalter öffnet
nur den Riegel — geladen wird erst, wenn ein anderer Anlass es auslöst.

### Was der Schalter heute tut

`set_module_enabled` (`preference_module_state.rs:72`) schreibt das Flag und
ruft `refresh_online_module_state` (`preferences.rs:458`). Für `"artwork"`
(`preference_module_state.rs:45`) heißt das: `modules::set_enabled`, dann
`cover_download.recompute_enabled` und `artist_portrait.recompute_enabled`, dazu
`source_image::recompute_gate`. Das sind ausschließlich *Erlaubnis*-Updates.
Kein Bibliotheksdurchlauf, keine neue Bildanfrage.

Die drei Verbraucher warten deshalb auf einen Anlass, der nie kommt:

- **Album-Cover** — der einzige echte Bibliotheksdurchlauf. Er wird an genau
  einer Stelle angestoßen: beim Fensteraufbau, hinter der Startruhe
  (`window_runtime_wiring.rs:476-491` → `lyrics_batch.start_after_cover`).
- **Künstlerporträts in My Stats** — laden bedarfsgesteuert je sichtbarer
  Kachel (`stats_artwork.rs`: Porträt → Album-Cover → Initialen). Bereits
  gezeichnete Kacheln fragen nach dem Einschalten nichts nach.
- **Quellbilder** — dieselbe Lage pro Renderdurchgang
  (`source_image.rs:94`, jeder Aufrufer berechnet den Riegel selbst).

### Und ein Neustart hilft auch nicht zuverlässig

`CoverDownloadBatch::start` (`cover_download_batch.rs:136`) läuft nur, wenn
`startup_tasks::begin_exact(…, SignatureTask::CoverDownload)`
(`crates/reprise-core/src/library/startup_tasks.rs:226`) den Lauf für fällig
hält. Fällig ist er nur, wenn sich die Bibliothekssignatur seit dem letzten
abgeschlossenen Durchlauf geändert hat. Wer Artwork ausschaltet, später wieder
einschaltet und die Bibliothek dabei nicht anfasst, bekommt also auch beim
nächsten Start `Skip` — der Riegel ist offen, gescannt wird trotzdem nie.

Das ist kein Fehler der Fälligkeitslogik: sie ist für *Startaufgaben* gebaut.
Ein Schalterdruck ist ein manueller Anlass, und für den gilt ihr eigener
Grundsatz (`startup_tasks.rs:87`): „Der Live-Watcher und jeder manuelle
Auslöser sind von dieser Entscheidung unabhängig." Nur gibt es für den
Cover-Durchlauf bisher keinen solchen Einstieg.

### Nebenbefund, der dieses Feature scharf macht

Der Durchlauf schreibt jede Spur, deren Ergebnis `Unavailable` lautet, als
erledigt fest (`cover_download_batch.rs:225-245`,
`cover::remember_download_unavailable`). `Unavailable` bedeutet aber zweierlei:
„es gibt nichts" **und** „der Abruf ist gescheitert" — `fetch_and_cache` gibt
bei `CaaFetchResult::TransientFailure` und bei fehlgeschlagenem
MusicBrainz-GET dasselbe `None` zurück wie bei einem sauberen Treffer-Fehlschlag
(dort allein wird ein negativer Merker geschrieben). Ein Durchlauf ohne Netz
brennt damit die halbe Bibliothek als „nichts zu holen" fest, und künftige
Durchläufe überspringen sie (`cover::download_marked_unavailable`).

Solange der Durchlauf nur beim Start lief, war das selten. Ein Sofortstart
direkt auf den Schalterdruck trifft genau die Lage, in der das passiert —
frisch eingeschaltet, Netz vielleicht noch nicht da. Deshalb gehört die
Trennung in dieses Paket und nicht in einen Folgetask.

## Was gebaut wird

### P1 — Ein ereignisgetriebener Durchlauf ist überhaupt möglich

`startup_tasks` bekommt einen Einstieg für Läufe, die aus einer Nutzeraktion
kommen: die Fälligkeitsprüfung entfällt, die Signaturbuchführung bleibt (der
Lauf darf sein Ergebnis festschreiben, damit der nächste Start nicht dieselbe
Arbeit wiederholt). `record_completed_at` bleibt, was es ist — ein
`#[doc(hidden)]`-Testeinstieg; Produktion bekommt einen eigenen, benannten Weg.

`CoverDownloadBatch` bekommt daneben einen zweiten Einstieg für genau diesen
Fall. `start()` bleibt der Startpfad und behält seinen Riegel unverändert.
Ein zweiter Schalterdruck während eines laufenden Durchlaufs darf keinen
Parallellauf erzeugen — die vorhandene `generation`-Mechanik trägt das bereits,
sie muss nur auch für den neuen Einstieg greifen.

### P2 — Der Schalter löst aus

Nach erfolgreichem Persist in `set_module_enabled` und **nur** beim Einschalten:

- `"artwork"` → Cover-Durchlauf sofort, plus P4.
- `"online_lyrics"` → `LyricsBatch::start()` (`lyrics_batch.rs:130`) — der
  erzwungene Einstieg existiert dort bereits, `start_automatically`
  (`:189`) ist der mit Fälligkeitsprüfung.

Dieselbe Regel gilt, wenn der globale Online-Riegel eingeschaltet wird
(`set_online_sources_enabled`, `preferences.rs`) und das jeweilige Modul schon
an ist. Das ist derselbe Übergang „darf nicht → darf", nur von der anderen
Seite.

`PreferencesContext` kennt den Cover-Durchlauf bisher nicht (nur die
`CoverDownloadRuntime`). Er wird über einen Rückruf angebunden, den das
Fenster-Wiring setzt — dem Muster von `on_source_modules_changed` folgend
(`preferences.rs`), damit der ohnehin 21-argumentige Konstruktor nicht wächst
und die Tests ohne Durchlauf weiter bauen. Sowohl der Schalter auf der
Plugins-Seite als auch die Modulaktion in der Seitenleiste laufen durch
`set_module_enabled` — beide erben das Verhalten damit automatisch.

### P3 — Nur online, und kein Schaden bei Netzverlust

Vorbedingung für den Sofortstart ist eine tatsächlich verfügbare Verbindung.
Die Projektion des `gio::NetworkMonitor` existiert bereits an genau einer
Stelle (`window/source_connectivity.rs`) und ist der Ort, an dem diese Auskunft
zu holen ist — kein zweiter Monitor-Leser. Ohne Netz: kein Lauf, keine
Markierung, keine Fehlermeldung in der Scan-Karte (das ist kein Fehlschlag,
sondern ein „später").

> Bewusst **nicht** im Paket: ein Nachholen, sobald das Netz zurückkommt (vom
> Nutzer am 13.08. abgewählt). Der nächste Start erledigt es.

Dazu die Trennung aus dem Nebenbefund: ein gescheiterter Abruf darf nicht
dasselbe Ergebnis liefern wie ein sauberes „gibt es nicht", und nur Letzteres
darf `remember_download_unavailable` auslösen. Der Kern kennt den Unterschied
schon (`CaaFetchResult::TransientFailure` vs. `NotFound`,
`cover_download.rs:236-244`) — er geht nur auf dem Weg nach oben verloren.

### P4 — Sichtbare Flächen laden nach

Beim Übergang „darf nicht → darf" fordern die schon gezeichneten Flächen ihre
Bilder neu an, statt bis zum nächsten Renderdurchgang bei Glyph und Initialen
zu bleiben: die Kacheln in *My Stats* und die Zeilen in Podcasts, YouTube und
Radio. `refresh_online_module_state` (`preferences.rs:458`) ist der Sammelpunkt,
an dem das gehört — es republiziert die Riegel bereits, es fehlt der Anstoß zum
Nachfordern.

## Abgrenzung

- Kein neues Fortschritts-Element. Der Cover-Durchlauf meldet sich über die
  vorhandene Scan-Karte in der Seitenleiste
  (`main_cover_download_progress.rs`); sie ist sichtbar, sobald der
  Einstellungsdialog geschlossen ist.
- Der Startpfad (Startruhe, Fälligkeit, `start_after_cover`) bleibt unberührt.
- Kein Vorwärmen unsichtbarer Bilder über die bestehende Bedarfssteuerung
  hinaus. Album-Cover haben ihren Bibliotheksdurchlauf, Porträts und
  Quellbilder laden weiter nach Sichtbarkeit.

## Verifikation

Headless, ohne ein App-Fenster auf dem echten Desktop zu öffnen.

1. **Kern:** Der ereignisgetriebene Einstieg läuft auch bei unveränderter
   Signatur und schreibt danach eine Abschlussmarke, die den nächsten Start
   überspringen lässt.
2. **Trennung transient/endgültig:** Ein Abruf, der am Netz scheitert,
   hinterlässt keine „erledigt"-Marke; ein sauberes „nicht gefunden" schon.
   Das ist der Test, der den Bestand schützt.
3. **GNOME:** Modul aus, dann über `set_module_enabled` an → der Durchlauf
   wechselt nach `Running` (`progress_for_test`), obwohl die Fälligkeitsprüfung
   `Skip` sagen würde. Ausschalten während des Laufs beendet ihn. Zweimal
   einschalten erzeugt keinen zweiten Lauf.
4. **Ohne Netz:** kein Lauf, kein `Failed`-Zustand, keine Marken in der
   Bibliothek.
5. **Sichtbar:** ein Display-Test über die My-Stats-Kacheln — nach dem
   Einschalten fordert eine bereits gezeichnete Kachel ihr Bild neu an.
   Ein grüner Testlauf allein beweist die Oberfläche nicht; die Abnahme
   braucht den Screenshot-Harness.

## Risiko

`feature/source-artwork-never-reloads` (`phase: coded`, Worktree
`/home/marvin/Projects/reprise-source-artwork-never-reloads`, acht Commits)
ist noch nicht auf `dev` und arbeitet am selben Riegel: Nachhol-Migration für
`module.artwork.enabled`, geerbte Zustimmung, Ladepfad und Cache der
Quellbilder. Berührungspunkte sind `source_image.rs` und die Artwork-Erlaubnis.
Dieser Strang sollte zuerst landen; danach ist die Basis hier neu zu erheben.

> **Nachtrag 13.08.2026:** Das Risiko ist erledigt — der Nachbarstrang ist als
> #452 (`ca85fedffd`) auf `dev` gelandet, also genau auf der Basis, gegen die
> dieser Plan erhoben wurde.
>
> **Wiederhergestellt am 13.08.2026, 20:50.** Diese Datei war nur ungetrackt im
> geteilten Hauptcheckout vorhanden und wurde dort von einer fremden Sitzung
> gelöscht (zusammen mit rund einem Dutzend anderer Planungsdokumente). Der
> Inhalt oben ist aus dem Sitzungskontext rekonstruiert und entspricht dem
> Stand, der zu Sitzungsbeginn gelesen wurde; einzige Änderung ist `phase`.
> Die Befunde der Review-Phase liegen in
> `artwork-toggle-starts-the-scan.FINDINGS.md`.
