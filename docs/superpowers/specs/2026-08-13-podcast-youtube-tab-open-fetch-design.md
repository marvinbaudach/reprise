# Podcasts und YouTube holen beim Öffnen des Tabs neue Folgen

Date: 2026-08-13
Status: design approved, not yet implemented
Baseline: `origin/dev` @ 6521524489

## Problem

Wer den Podcasts- oder den YouTube-Tab öffnet, will die neuen Folgen sehen.
Heute sieht er den Stand von bis zu sechs Stunden vorher, ohne dass etwas
darauf hinweist.

Der Grund steht in `library_shell.rs:243` und `:250`: das Routing ruft beim
Öffnen `podcasts_view.refresh()` bzw. `youtube_view.refresh()`, und dieses
`refresh` liest ausschließlich die Datenbank neu. Es geht dabei kein einziges
Byte ins Netz.

Ein Netz-Fetch entsteht heute nur auf zwei Wegen:

1. **Der Refresh-Knopf** (`podcasts_view.rs:352`) ruft
   `request_refresh(true)` — `force`, also ohne jede Fälligkeitsprüfung.
2. **Der Zeitplan** (`window/podcast_refresh_scheduler.rs`) feuert einmal beim
   Fensterstart über `idle_add_local_once` und danach stündlich. Er fetcht nur,
   wenn `automatic_refresh_allowed(enabled, subscription_count, metered, due)`
   zustimmt, und `due` heißt: mindestens ein Abo ist nach
   `sources.refresh_hours` (Default **6**) plus Jitter fällig.

Das Öffnen eines Tabs ist in dieser Rechnung kein Ereignis. Ein Nutzer, der um
09:00 den Tab öffnet, während der letzte Fetch um 08:30 lief, bekommt korrekt
nichts Neues — aber einer, der um 14:00 öffnet und dessen Stundentimer wegen
Jitter erst um 14:20 fällig wird, sieht bis dahin alte Daten, ohne es zu
erfahren.

Ein zweites Problem liegt im Zuschnitt: `pipeline::refresh` läuft über
`active_subscriptions_in` — **alle** aktiven Abos, RSS und YouTube gemeinsam.
Nur *ob* ein Abo dran ist, wird pro Abo entschieden (`pipeline.rs:342` für die
Fälligkeit, `:366` für die Netz-Erlaubnis pro Art). Es gibt keinen Weg, nur
eine der beiden Arten zu aktualisieren. Ein Fetch, der am Podcasts-Tab hängt,
würde also für jeden abonnierten YouTube-Kanal einen yt-dlp-Subprozess starten.

## Was gebaut wird

Das Öffnen des Podcasts- oder YouTube-Tabs löst einen Fetch **nur der eigenen
Quelle** aus, wenn deren letzter Fetch länger als **15 Minuten** her ist, das
Gerät online und das Netz nicht getaktet ist. Läuft der Fetch, ist er sichtbar
— im Footer und am Refresh-Knopf. Ist nichts zu holen, passiert nichts: kein
Spinner, keine Statuszeile, kein Netzverkehr.

### 1. Core: eine Refresh-Politik statt `force: bool`

`pipeline::refresh` kennt heute genau zwei Modi, kodiert in einem `bool`:
`force` (alles) und `!force` (was nach `refresh_hours` fällig ist). Der
Tab-Fetch ist ein dritter Modus, und er braucht zusätzlich einen Kind-Filter.
Ein zweiter und dritter `bool`-Parameter wären an dieser Stelle nicht lesbar,
deshalb tritt ein Anforderungstyp an die Stelle des Bools:

```rust
// podcasts/refresh.rs
pub enum RefreshPolicy {
    /// Zeitplan: fällig nach `sources.refresh_hours` plus Jitter.
    /// Wort für Wort das heutige `!force`.
    Due,
    /// Tab-Öffnen: der letzte Fetch ist länger als dieses Intervall her.
    StaleFor { seconds: i64 },
    /// Refresh-Knopf: unabhängig von jeder Fälligkeit. Das heutige `force`.
    Force,
}

pub struct RefreshRequest {
    pub policy: RefreshPolicy,
    /// `None` aktualisiert beide Arten — der Zeitplan tut das weiterhin.
    pub kind: Option<PodcastKind>,
}
```

`refresh`, `refresh_with_download_progress`, `refresh_to_root` und
`refresh_to_root_with_download_progress` nehmen `RefreshRequest` statt
`force: bool`. Im Schleifenkopf über die Abos (`pipeline.rs:335` ff.) gilt
dann:

- **Kind-Filter zuerst.** Ein Abo fremder Art wird übersprungen, bevor
  irgendetwas anderes geprüft wird, und zählt damit auch nicht in
  `summary.attempted`. Wer den Podcasts-Tab öffnet, startet keinen einzigen
  yt-dlp-Prozess.
- **`Force`** verhält sich genau wie heute `force == true`.
- **`Due`** verhält sich genau wie heute `force == false`: erst
  `pending_retry`, sonst `refresh_due_with_hours` mit `config.refresh_hours`
  und dem Datenbank-Jitter.
- **`StaleFor`** respektiert **denselben Retry-Backoff** wie `Due` und prüft
  danach nur, ob `last_fetch_at` älter als `seconds` ist. Der Backoff ist hier
  keine Kosmetik: ohne ihn würde ein Abo, dessen Feed dauerhaft 404 liefert,
  bei jedem Tab-Wechsel erneut angefragt werden und die Fehlermeldung bei jedem
  Öffnen neu produzieren. Jitter entfällt bei `StaleFor` — Jitter existiert,
  um automatische Läufe über die Zeit zu verteilen, und der Nutzer hat hier
  gerade selbst gehandelt.

`StaleFor` liest `last_fetch_at`, das die Pipeline nach jedem Fetch ohnehin
schreibt. Damit braucht die Drosselung **keinen neuen Zustand und kein
Schema**: sie gilt pro Abo, überlebt einen App-Neustart und kann nicht mit dem
Zeitplan aus dem Takt geraten.

### 2. Ein Entscheider für beide Auslöser

`podcast_refresh_scheduler::decision_inputs` beantwortet heute schon fast die
richtige Frage, aber nur ungefiltert und nur mit `config.refresh_hours`. Diese
Entscheidung zieht in einen eigenen kleinen Helfer neben der View — sie gehört
zu den Podcasts, nicht zum Fenster — und bekommt beide Parameter:

- den Zuschnitt (`Option<PodcastKind>`),
- das Intervall, gegen das „fällig" gemessen wird.

Der Zeitplan ruft ihn mit `None` und `config.refresh_hours`, der Tab-Fetch mit
`Some(self.kind)` und 15 Minuten. Beide Auslöser teilen damit dieselbe
Buchhaltung, statt sie zweimal zu formulieren — die Sorte Doppelung, die im
Projekt schon einmal auseinandergedriftet ist.

### 3. Der Tab-Fetch prüft, bevor er etwas anstößt

Neu am View: `request_tab_open_refresh()`. Es stellt nur dann eine Anfrage,
wenn **alle** Bedingungen erfüllt sind:

1. Das Netz für `self.kind` ist erlaubt (Modul an **und** globale
   Online-Quellen-Freigabe) — dieselbe Frage, die
   `config::source_network_allowed` schon beantwortet.
2. Das Gerät ist online: `self.connectivity.get() == Connectivity::Online`.
   Der View hat diesen Seam bereits (`podcasts_connectivity_ui.rs`).
3. Das Netz ist nicht getaktet (`NetworkMonitor::is_network_metered`), wie
   beim Zeitplan.
4. Es gibt mindestens ein Abo dieser Art, dessen `last_fetch_at` älter als 15
   Minuten ist.

Die Vorprüfung ist der Grund, warum ein Tab-Wechsel nichts kostet und nichts
flackert: `request_refresh` startet den Footer-Spinner, sobald die Anfrage in
der Queue liegt, unabhängig davon, ob die Pipeline anschließend etwas zu tun
findet. Ohne Punkt 4 würde jeder Tab-Wechsel einen Spinner zeigen, der eine
leere Runde begleitet.

**Offline wird nichts vorgemerkt.** `request_load_more` stellt offline auf
`DeferredAction` um und meldet „später"; für einen Refresh wäre das falsch. Es
würde bedeuten, dass ein Tab, den man offline geöffnet hat, Minuten später
beim Reconnect überraschend zu fetchen anfängt. Der Offline-Zustand hat mit
`should_show_offline_notice` schon seine eigene, richtige Anzeige.

`request_refresh(force: bool)` bleibt als Weg des Knopfes bestehen; intern
bauen beide Wege denselben `RefreshRequest`. Die Worker-Operation wird von
`Refresh { force }` zu `Refresh { policy, kind }` — sie ist Teil der
`request_generation`-Identität, was weiterhin trägt: zwei Anfragen mit
unterschiedlicher Politik sind auch unterschiedliche Anfragen.

Der Zeitplan bleibt inhaltlich unverändert: `RefreshPolicy::Due`, `kind: None`.

### 4. Ein laufender Fetch ist sichtbar

Zwei Anzeigen, beide an „ein Refresh läuft" gebunden — nicht daran, wer ihn
ausgelöst hat. Ein Fetch vom Zeitplan sieht damit genauso aus wie einer vom
Knopf oder vom Tab-Öffnen:

- **Footer:** `footer_spinner` plus Status `PODCAST_REFRESHING`, wie heute.
- **Refresh-Knopf:** solange der Fetch läuft, trägt der Knopf einen Spinner
  statt seines Icons und ist insensitiv; danach steht wieder das Icon da. Der
  Knopf muss dafür als Feld am View liegen — `wire_controls` bekommt ihn
  aktuell nur geliehen. Das Insensitiv-Setzen folgt dem Muster, das
  `concerts_view.rs:500`/`:559` für seinen Fetch-Knopf schon verwendet; der
  Spinner im Knopf ist die sichtbare Hälfte, die dort fehlt.

Beim Tab-Wechsel schaut der Blick auf Liste und Kopfzeile, nicht in den
Footer — deshalb reicht der Footer allein hier nicht.

Wichtig für den Knopf: er wird in **jedem** Ausgang wieder freigegeben, auch
im `Err`-Arm und wenn die Antwort zu einer veralteten Generation gehört. Ein
Knopf, der nach einem fehlgeschlagenen Fetch insensitiv bleibt, nimmt dem
Nutzer genau die Handlung weg, die er dann braucht.

## Was ausdrücklich nicht gebaut wird

- **Keine Einstellung für das Tab-Intervall.** 15 Minuten sind eine Konstante.
  Wer öfter aktualisieren will, hat den Knopf.
- **Kein neuer Zustand, keine Migration.** Die Drosselung liest
  `last_fetch_at`.
- **Kein Kind-Filter für den Zeitplan.** Er aktualisiert weiter beides in
  einem Lauf; das ist für einen Hintergrundlauf richtig.
- **Kein Deferred-Refresh für den Offline-Fall.**
- **Der Radio-Tab bleibt unberührt.**

## Verifikation

**Core** (`pipeline_refresh_tests.rs`, `pipeline_youtube_tests.rs`):

- `kind: Some(Rss)` fetcht kein YouTube-Abo an, und `Some(Youtube)` kein
  RSS-Abo — beide Richtungen, jeweils mit einem Abo der anderen Art als
  Kontrollzeile, dessen `last_fetch_at` unberührt bleiben muss.
- `StaleFor` unterhalb der Schwelle fetcht nichts, oberhalb fetcht es.
- `StaleFor` fetcht ein Abo mit offenem Retry-Backoff nicht an.
- `Force` ignoriert die Fälligkeit weiterhin, `Due` verhält sich unverändert —
  die vorhandenen Tests dieser beiden Pfade wandern auf den neuen Typ um und
  müssen ohne Änderung ihrer Erwartungen grün bleiben.

**GTK:** Der Entscheider ist eine reine Funktion und wird als solche getestet
— getaktetes Netz, offline, keine Abos dieser Art, gerade gefetcht: jeweils
kein Fetch; alles erfüllt: Fetch. Dass beide Routing-Zweige ihn aufrufen, ist
eine zweizeilige Änderung in `library_shell.rs`, die die vorhandenen
Routing-Tests mitnehmen.

Für den Knopf-Zustand gilt die Hausregel: grüne Tests beweisen keine UI. Der
sichtbare Beweis ist ein Screenshot des laufenden Fetches (Spinner im Knopf,
Footer-Status) über die headless Harness — kein geöffnetes App-Fenster.
