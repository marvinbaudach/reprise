---
slug: dependabot-covers-every-ecosystem
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Automatische Abhängigkeits-PRs für **alle** Ökosysteme

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„automatische PRs die abhängigkeiten updaten/Upgraden für alle Deps"*

## Ist-Zustand: es läuft schon — für zwei von drei Ökosystemen

`.github/dependabot.yml` ist vorhanden und ordentlich gebaut:

| Ökosystem | Verzeichnis | Takt | Ziel | Gruppen |
| --- | --- | --- | --- | --- |
| `github-actions` | `/` | wöchentlich, montags | `dev` | — |
| `cargo` | `/` | wöchentlich, montags | `dev` | `gstreamer*`, `gtk4`/`gdk4-*`/`libadwaita`/`gio`, `serde`/`serde_json` |

Beides zielt korrekt auf `dev` statt `main`, trägt Commit-Präfixe (`ci` bzw.
`chore`) und das Label `dependencies`. Die Gruppierung ist genau richtig
gewählt: GTK- und GStreamer-Bindings müssen im Verbund steigen, sonst
zerbricht die Übersetzung an einer halb angehobenen Bindungsreihe.

## Die Lücke: Android/Gradle fehlt vollständig

`android/` ist ein vollwertiges Gradle-Projekt mit gepflegten Abhängigkeiten —
und keine davon wird überwacht:

```
android/build.gradle.kts
android/app/build.gradle.kts
android/gradle/wrapper/gradle-wrapper.properties
```

Was dort hängt (Auszug aus `android/app/build.gradle.kts:76-84`):

```
androidx.compose:compose-bom:2026.06.01
androidx.navigation:navigation-compose:2.9.8
androidx.activity:activity-compose:1.13.0
androidx.core:core-ktx:1.19.0
org.jetbrains.kotlin.plugin.compose
```

Das sind festgenagelte Versionen inklusive einer Compose-BOM — genau die Art
Abhängigkeit, die still veraltet, weil niemand sie täglich anfasst. Dependabot
kann das: `package-ecosystem: gradle` mit `directory: /android`. Es erkennt
sowohl die `.kts`-Dateien als auch den Wrapper.

## Was zu klären ist

1. **Gruppierung für Compose.** Dieselbe Logik wie bei GTK/GStreamer: die
   `androidx.compose.*`-Artefakte hängen an der BOM und sollten gemeinsam
   steigen, nicht einzeln. Ohne Gruppe erzeugt Dependabot pro Artefakt einen
   PR — bei Compose sind das schnell fünf bis acht pro Woche.
2. ~~**Wer prüft die Android-PRs?**~~ **Geklärt am 16.08.2026 — die Prüfung
   existiert bereits.** `origin/dev` @ `95b4b30016` fährt seit #471
   (`5721ade95e`, 14.08.2026) einen eigenen CI-Job `android-unit-suite`
   („Android JVM unit suite", `ci.yml:19-49`) auf `ubuntu-24.04` mit
   `actions/setup-java@v5` / JDK 21, der `scripts/check-android-suite.sh`
   ausführt — inklusive Frischeprüfung der JUnit-XMLs und einem gemessenen
   Testboden (`ANDROID_TEST_FLOOR=334`). Dazu prüft `cross-target.yml`
   inzwischen auch `reprise-android-ffi` für `aarch64-linux-android`. Letzter
   `dev`-Lauf (15.08.2026): beide Jobs grün.
   *Der Hinweis „CI fährt kein Gradle" in der ersten Fassung dieses Dokuments
   stammte aus dem geteilten Hauptcheckout (`be5f014d3b`), der vor diesem
   Stand liegt — gegen `origin/dev` gelesen war er falsch.*
   Damit fällt das Hindernis weg: ein Gradle-Dependabot-PR landet in einer
   Kette, die ihn tatsächlich prüft.
3. **Kein weiteres Ökosystem nötig.** Geprüft am 16.08.2026: keine
   `requirements.txt`, kein `pyproject.toml`, kein `setup.py` — die 78
   Python-Skripte unter `scripts/` haben kein Manifest, also nichts für
   Dependabot. `flatpak/cargo-sources.json` ist generiert und folgt der
   `Cargo.lock`. Damit ist Gradle die einzige echte Lücke.
4. **Sicherheitsmeldungen laufen getrennt.** `cargo-audit` ist in der
   CI-Umgebung installiert (`ci.yml:38`), aber es gibt weder `deny.toml` noch
   `audit.toml` im Repo. Ob Dependabot Security Updates zusätzlich aktiv sein
   sollen (das ist eine Repo-Einstellung, keine YAML-Zeile), gehört in
   dieselbe Runde entschieden — das ist der Mechanismus, der auch außerhalb
   des Montagstakts feuert.
5. **Wöchentlich für drei Ökosysteme heißt mehr PR-Verkehr.** Wenn der Montag
   zu voll wird, ist `groups` mit einem `patterns: ["*"]`-Eintrag pro
   Ökosystem der übliche Ausweg: ein Sammel-PR statt zwanzig einzelner.

## Berührte Stellen

| Datei | Rolle |
| --- | --- |
| `.github/dependabot.yml` | die zwei bestehenden Einträge, hier käme `gradle` dazu |
| `android/app/build.gradle.kts:76-84` | die unüberwachten Abhängigkeiten |
| `android/gradle/wrapper/gradle-wrapper.properties` | Wrapper-Version, ebenfalls von Dependabot pflegbar |
| `.github/workflows/ci.yml` | fährt heute kein Gradle — siehe Punkt 2 |
