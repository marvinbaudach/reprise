---
slug: downloads-for-linux-mac-and-windows
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Fertige Downloads auf der Projektseite — Linux jetzt, macOS und Windows als eigenes Vorhaben

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„MacOS-Unterstützung. ich möchte die aktuellste Main-App auch auf der Git-Page
anbieten, damit Mac-User sie sofort installieren können. und das gleiche für
Linux-User. Am besten in der CI für main hinzufügen. Prüfe um die beste
kompatibilität für Mac noch zu gewährleisten. Das gleiche für MS-Windows"*

**Der Wunsch zerfällt in drei sehr ungleiche Teile.** Linux ist ein
CI-Vorhaben von überschaubarer Größe. macOS und Windows sind es nicht — dort
fehlt nicht der Workflow, sondern die Portierung. Das steht unten mit Belegen,
damit die Reihenfolge eine Entscheidung wird und keine Überraschung.

## 1. Ist-Zustand, gemessen am 16.08.2026 gegen `origin/dev` @ `95b4b30016`

**1.1 Es gibt noch gar keine Veröffentlichung.**
`gh release list` ist **leer**. Versionstags existieren nicht — die einzigen
Tags sind `backup/…`-Sicherungen. Es gibt also nichts, worauf eine
Download-Schaltfläche heute zeigen könnte.

**1.2 Kein Workflow baut ein Artefakt.** Die vier Workflows auf `dev`:

| Workflow | Zweck |
| --- | --- |
| `ci.yml` | Qualitäts-Gate + Android-JVM-Suite |
| `cross-target.yml` | `cargo check` für Windows- und Android-*Targets* — **kein Build** |
| `pages.yml` („Showroom") | baut `showroom/` mit Node und deployt es auf GitHub Pages |
| `delete-merged-branch.yml` | Aufräumen |

`cross-target.yml:83` prüft ausdrücklich nur `-p reprise-core` für
`x86_64-pc-windows-msvc`. Das ist ein Kompilierbarkeits-Nachweis für den
**Kern**, nicht für die App — die GUI-Crate ist dort nie beteiligt.

**1.3 Die Seite existiert bereits und wird automatisch deployt.**
`pages.yml` baut `showroom/dist` und schiebt es über `deploy-pages@v4`. Ein
Download-Bereich hat also schon ein Zuhause; es fehlt nur, was er anbieten
soll.

**1.4 Verpacken kann das Projekt heute nur für Linux.**
`RELEASING.md:179-217` beschreibt den Meson-Installbaum
(`/usr/bin/reprise`, `.desktop`, `metainfo.xml`, Icons, `.mo`), dazu
`flatpak/README.md` und `packaging/aur/`. Für macOS oder Windows gibt es
nichts — kein Bundle, kein Installer, keine Signaturkette.

**1.5 Die README positioniert das Projekt heute anders.**
> „**Status:** active alpha. Reprise is not a public release yet."

Downloads anzubieten heißt, diesen Satz zu ändern. Das ist eine
Positionierungsentscheidung, keine technische — aber sie gehört vor die
Umsetzung, nicht danach.

## 2. Wie portabel die Architektur wirklich ist

Hier ist die Lage **besser als erwartet**, und das verschiebt den Aufwand.

**2.1 Es gibt echte Plattformverträge.** `reprise-platform-linux`
implementiert Traits, die in `reprise-core` definiert sind — unter anderem
`Player`, `DeviceMonitor`, `DatabaseLibrary` sowie die Fingerprint- und
Waveform-Backends. Eine macOS-Schicht wäre also eine **zweite Implementierung
bestehender Verträge**, kein Umbau des Kerns. Genau das behauptet die README
mit „a portable Rust core … everything Linux-specific kept behind explicit
contracts", und der Trait-Befund stützt es.

**2.2 Was `reprise-platform-linux` abdeckt**, und damit die Liste dessen, was
eine zweite Plattform beibringen muss: `player` (GStreamer), `mpris`,
`device_sync`/`device_transfer` (MTP), `trash`, `fingerprint`, `waveform`,
`location`, `runtime_service`, `spectrogram_backfill`, `diagnostics`.

**2.3 Der harte Punkt sitzt nicht im Kern, sondern im Frontend.**
`crates/reprise-gnome/Cargo.toml` hängt unbedingt an:

```
reprise-platform-linux = { path = "../reprise-platform-linux" }   # Zeile 29
gtk4       = { version = "0.11.4", features = ["v4_22"] }          # Zeile 39
libadwaita = { version = "0.9.2",  features = ["v1_9", "gtk_v4_22"] }  # Zeile 42
```

Zwei Folgen:

- Die Linux-Plattform ist **fest verdrahtet**, nicht über ein Feature oder ein
  Trait-Objekt gewählt. Vor jeder zweiten Plattform muss diese Kante
  austauschbar werden.
- **`libadwaita` ist der eigentliche Engpass.** GTK4 selbst läuft auf macOS
  (Quartz-Backend) und Windows; libadwaita ist auf GNOME zugeschnitten und
  außerhalb davon bestenfalls geduldet. Die App benutzt es nicht am Rand,
  sondern durchgängig — `AdwPreferencesPage`, `AdwSwitchRow`, `AdwComboRow`,
  `AdwDialog`, `AdwAboutDialog` sind über die gesamte Oberfläche verteilt.
  Das ist die Frage, die vor allen anderen beantwortet werden muss.

## 3. Teil 1 — Linux-Download: machbar, konkret

Der kleinste ehrliche Schritt, der den Wunsch für Linux erfüllt:

1. **Versionstag und GitHub-Release einführen.** Ohne Tag kein Release, ohne
   Release kein stabiler Download-Link. Die Versionsnummer bewegt sich schon
   bei jedem Merge nach `dev` (`land.sh` ruft `scripts/bump-version.sh`) — die
   Zahl ist also da, sie wird nur nie festgeschrieben.
2. **Ein Release-Workflow auf `main`**, der den bestehenden Meson-Weg fährt
   (`RELEASING.md:181-186`) und ein Artefakt anhängt. Zur Wahl:
   - **Flatpak-Bundle** (`.flatpak`) — das Manifest existiert bereits, die
     Abhängigkeiten sind darin festgeschrieben, und es ist der Weg, den die
     Zielgruppe (GNOME) erwartet. **Empfohlen.**
   - **AppImage** — läuft ohne Flatpak-Installation, muss aber GTK4,
     libadwaita und GStreamer selbst mitbringen; das ist neue Arbeit.
   - Das Meson-Installtar allein ist kein Nutzer-Download.
3. **Der Showroom bekommt einen Download-Bereich**, der auf das jeweils
   neueste Release zeigt (`releases/latest`), statt auf eine feste Datei.
4. **`check-release.sh` läuft ohnehin schon** (`scripts/check-release.sh`,
   `RELEASING.md:7`) — der Workflow sollte ihn als Torwächter benutzen, nicht
   daneben laufen.

Realistischer Rahmen: ein Vorhaben, nicht ein Nachmittag — aber ohne
Portierungsarbeit.

## 4. Teil 2 und 3 — macOS und Windows

**Das ist kein CI-Punkt.** Ein Workflow kann nur bauen, was baubar ist; heute
kompiliert für Windows nur `reprise-core`, und für macOS ist nichts geprüft —
`cross-target.yml:59` fügt `x86_64-pc-windows-msvc` und
`aarch64-linux-android` hinzu, kein Apple-Target.

Was zuerst beantwortet werden muss, in dieser Reihenfolge:

1. **Die libadwaita-Frage.** Drei Auswege, alle mit Preis:
   - *libadwaita mitliefern* — auf macOS/Windows unüblich und schlecht
     getestet; die App sähe zudem überall aus wie GNOME, was auf dem Mac als
     Fremdkörper gilt.
   - *Ein zweites Frontend* auf einem plattformnahen Toolkit. Das Projekt hat
     die Trennung dafür bereits (`reprise-view` als portable Sichtschicht,
     `reprise-core` ohne GUI) und mit der Android-App schon einmal bewiesen,
     dass ein zweites Frontend auf demselben Kern funktioniert.
   - *Nur GTK4 ohne libadwaita* — hieße, jede `Adw*`-Fläche zu ersetzen.
2. **Die Plattformschicht.** MPRIS hat auf macOS kein Gegenstück (dort:
   `MPNowPlayingInfoCenter`, auf Windows: `SMTC`); MTP-Gerätesync, Papierkorb
   und Keyring brauchen je eine eigene Antwort. GStreamer selbst läuft auf
   beiden Systemen.
3. **Auslieferung und Vertrauen.** Ein `.app`-Bundle ohne Apple-Signatur und
   Notarisierung wird von Gatekeeper blockiert — der Nutzer sieht „beschädigt,
   in den Papierkorb". Das kostet ein Apple-Developer-Programm (99 USD/Jahr)
   und einen Signaturschritt in der CI. Windows: SmartScreen verlangt
   praktisch ein Code-Signing-Zertifikat. **Das ist der Teil von „beste
   Kompatibilität", der am häufigsten übersehen wird und am wenigsten
   verhandelbar ist.**
4. **Wer testet das?** GitHub bietet `macos-latest`- und
   `windows-latest`-Runner, aber niemand im Projekt benutzt die App dort. Ein
   Download, den niemand startet, ist ein Versprechen ohne Deckung.

## 4a. Android: die APK auf der Seite anbieten

**Nachtrag des Nutzers vom 16.08.2026:** *„auch APK anbieten auf der Page"*.

Das ist der **zweitleichteste** Teil nach Linux — die App existiert, ist
gebaut, und seit #471 prüft die CI sie sogar. Drei Dinge stehen dem Download
aber konkret im Weg, und alle drei sind heute im Repo belegbar:

**4a.1 Das Build-Skript baut genau ein ABI — und zwar das falsche.**
`scripts/android-build.sh:22-24`:

```
# The emulator is x86_64; arm64 is added when a real device is in play.
target_triple="${ANDROID_TARGET:-x86_64-linux-android}"
abi="${ANDROID_ABI:-x86_64}"
```

Der Standard ist der **Emulator**. Eine APK zum Herunterladen braucht
`arm64-v8a` — das haben praktisch alle realen Telefone —, sinnvollerweise
zusammen mit `x86_64` in einer Universal-APK oder über getrennte Artefakte.
Das Skript kann das per Umgebungsvariable, tut es im Standardfall aber nicht;
der Release-Workflow müsste die ABIs explizit durchlaufen und die `.so`-Dateien
einsammeln.

**4a.2 Der Release-Build signiert mit dem Debug-Schlüssel.**
`android/app/build.gradle.kts:44-53` — im `release`-Block steht
`signingConfig = signingConfigs.getByName("debug")`. Für einen öffentlichen
Download ist das aus zwei Gründen untragbar:

- Der Debug-Keystore ist **maschinenlokal**. Wird die APK das nächste Mal auf
  einem anderen Rechner (oder einem CI-Runner) gebaut, hat sie eine andere
  Signatur — und Android verweigert das Update auf jeder Installation, die
  noch die alte trägt („App not installed"). Der Nutzer müsste erst
  deinstallieren und verlöre dabei seine Daten.
- Der Debug-Schlüssel ist **allgemein bekannt**. Jeder könnte eine gefälschte
  Aktualisierung signieren, die sich sauber über die echte installiert.

Nötig ist also ein eigener Release-Keystore, als GitHub-Secret hinterlegt und
im Workflow eingespielt. **Und er muss aufbewahrt werden:** ein verlorener
Android-Signaturschlüssel bedeutet, dass nie wieder ein Update für die
installierte App ausgeliefert werden kann.

**4a.3 `versionCode` ist hartkodiert.**
`build.gradle.kts:27` steht auf `versionCode = 13`, während `:28` den
`versionName` korrekt aus der Workspace-Version zieht. Android akzeptiert eine
Aktualisierung nur bei **streng steigendem** `versionCode` — bei einem festen
Wert ist die erste heruntergeladene APK zugleich die letzte. Der Wert muss aus
derselben Quelle kommen wie die Version, die `land.sh` ohnehin bei jedem Merge
hochzählt.

**4a.4 Was danach noch zu entscheiden ist.** Ein direkter APK-Download ist auf
Android üblich und zulässig (F-Droid und GitHub Releases sind der Normalweg),
kostet den Nutzer aber die Bestätigung „Installation aus unbekannter Quelle".
Ob es dabei bleibt oder ob später F-Droid oder Play dazukommen, ist eine
eigene Frage — für den Wunsch „auf der Seite anbieten" reicht der direkte
Download.

## 5. Empfehlung zur Reihenfolge

1. **Positionierung klären** (§1.5): Ab wann darf man das herunterladen?
2. **Linux liefern** (§3): Tag, Release, Flatpak-Bundle, Download-Bereich im
   Showroom. Das erfüllt einen Teil des Wunsches vollständig und schafft die
   Release-Mechanik, die alle anderen Plattformen später ohnehin brauchen.
3. **Android gleich mitnehmen** (§4a): dieselbe Release-Mechanik, dieselbe
   Seite. Die drei Hindernisse — ABI, Signaturschlüssel, `versionCode` — sind
   klein und benannt, und die App ist die einzige neben Linux, die es schon
   gibt. Der Signaturschlüssel sollte früh entstehen, weil er nicht
   nachträglich austauschbar ist.
4. **Die libadwaita-Frage als eigene Untersuchung** (§4.1) — sie entscheidet
   über Aufwand und Aussehen aller Nicht-Linux-Ziele und ist billig zu
   beantworten, verglichen mit dem, was daran hängt.
5. Erst danach macOS, dann Windows — jeweils als eigenes Vorhaben mit
   Signaturkette.

Damit stünden nach Schritt 3 **zwei** der vier gewünschten Downloads auf der
Seite (Linux und Android), und zwar die beiden, die es als lauffähige App
wirklich gibt.

**Was ausdrücklich nicht empfohlen wird:** einen `macos-latest`-Job in die CI
zu hängen, in der Hoffnung, dass er baut. Er wird scheitern, und zwar an
`reprise-platform-linux` in Zeile 29 der GUI-Crate — lange bevor libadwaita
überhaupt zu Wort kommt.

## Berührte Stellen

| Datei | Rolle |
| --- | --- |
| `.github/workflows/pages.yml` | deployt den Showroom — hier hinge der Download-Bereich |
| `.github/workflows/cross-target.yml:59,83` | die heutige Target-Liste, ohne Apple |
| `crates/reprise-gnome/Cargo.toml:29,39,42` | die fest verdrahtete Linux-Plattform und libadwaita |
| `crates/reprise-platform-linux/src/lib.rs:10-23` | die Liste dessen, was eine zweite Plattform beibringen muss |
| `scripts/android-build.sh:22-24` | baut standardmäßig das Emulator-ABI, nicht `arm64-v8a` |
| `android/app/build.gradle.kts:27,44-53` | fester `versionCode`, Release mit Debug-Schlüssel signiert |
| `RELEASING.md:179-217` | der bestehende Linux-Artefaktweg |
| `flatpak/`, `packaging/aur/` | die zwei vorhandenen Verpackungen |
| `README.md` | der Alpha-Satz aus §1.5 |
