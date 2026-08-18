---
slug: youtube-download-auto-retry
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Fehlgeschlagene YouTube-Downloads automatisch 3–5× wiederholen, solange wir online sind

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026:

> „beim Runterladen von YT-Episoden kommt oft mal der Fehler, yt-dlp konnte es
> nicht runterladen. Nach 1–2 Retries klappt es aber dann doch. Gern automatisch
> noch 3–5 mal Retries machen, wenn wir online sind."

Der entscheidende Beleg steckt in der Beobachtung selbst: **derselbe Abruf
gelingt beim zweiten oder dritten Versuch.** Der Fehler ist also überwiegend
vorübergehend (Drosselung, abgelaufene Stream-URL, Netzhänger) und kein
dauerhafter Zustand der Episode.

## Ist-Zustand: im Download-Pfad gibt es keinen Wiederholversuch

- **Download:** `crates/reprise-core/src/podcasts/pipeline_download.rs:30`
  (`download_episode`) — ein Versuch, danach `persist_download` (`:157`).
  Schlägt yt-dlp fehl, landet die Episode direkt in `DownloadState::Failed`.
- **Fehlertext:** `crates/reprise-core/src/podcasts.rs:196-201` —
  `PodcastError::YtDlpFailure { .. }` wird zu
  *„YouTube source could not be read with yt-dlp"*; der Sonderfall
  `VerificationRequired` wird davor abgefangen und zu
  `YOUTUBE_BROWSER_RECOVERY_MESSAGE` (`:38`, `:189-195`).
- **Ein Backoff existiert bereits — aber woanders.**
  `crates/reprise-core/src/podcasts/pipeline_retry.rs` (`NET-3d`) hält
  Wiederhol-Zustände im Prozess, **je Subscription** (`RetryKey { connection,
  subscription_id }`), und wird ausschließlich vom **Refresh**-Pfad benutzt
  (`pipeline.rs:350`, `:481`). Für einzelne Episoden-Downloads greift er nicht.

## Was gebaut werden soll

Ein begrenzter Wiederholzyklus um `download_episode`, mit vier Bedingungen:

1. **3–5 Versuche**, dann endgültig `Failed` — Zahl als benannte Konstante.
2. **Nur bei vorübergehenden Fehlern.** `VerificationRequired` (Browser-Login
   nötig) und ein dauerhaft entferntes Video dürfen **nicht** wiederholt
   werden — das wären 5 sinnlose Läufe und eine irreführende Wartezeit. Die
   Unterscheidung existiert schon als `ytdlp::YtDlpFailureKind`.
3. **Nur online.** Vor jedem weiteren Versuch die Erreichbarkeit prüfen
   (`NET-3`) und die Netz-Einwilligung (`NET-1a`) — offline sofort abbrechen
   statt fünfmal ins Leere zu laufen.
4. **Mit Abstand dazwischen**, nicht in einer engen Schleife. Der bestehende
   Backoff aus `pipeline_retry.rs` ist das Muster; ob er wiederverwendet oder
   ein zweiter, download-spezifischer Zustand angelegt wird, ist offen — sein
   Schlüssel trägt heute eine `subscription_id`, gebraucht würde eine
   `episode_id`.

## Offene Fragen

- **Sichtbarkeit:** Soll die Zeile während der Wiederholungen „lädt" anzeigen
  oder „Versuch 2 von 5"? Ein stiller Retry lässt einen Download minutenlang
  hängen aussehen; ein sichtbarer Zähler erklärt die Wartezeit.
- **Bekannte Gefahr aus `pipeline_retry.rs`:** dessen Schlüssel enthält die
  *Adresse* der `Connection`, nicht ihre Identität — im laufenden Programm
  harmlos, in Tests eine Falle (der Kommentar oben in der Datei sagt es
  ausdrücklich). Wer den Zustand dort mitbenutzt, erbt das Problem.
- **Zusammenhang mit dem Streaming-Fehler:** `docs/plans/youtube-streaming-internal-data-stream-error.md`
  beschreibt einen Fehlschlag beim *Abspielen* derselben Quelle. Ob beides
  dieselbe Wurzel hat (yt-dlp liefert keine oder eine abgelaufene URL), ist
  **nicht** geprüft — lohnt sich, vor zwei getrennten Fixes einmal
  nachzusehen.
