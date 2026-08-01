# Android-Spike — Befunde (2026-08)

Spec: `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
Plan: `docs/superpowers/plans/2026-08-01-multi-surface-p0-s1.md`

Dieser Bericht beantwortet die Fragen aus Spec §4/S1. Jeder Abschnitt endet
mit einem Urteil: TRÄGT / TRÄGT MIT AUFLAGEN / TRÄGT NICHT.

Stand: Frage 5 ist beantwortet (Recherche, 2026-08-01). Die Fragen 0 bis 4
brauchen den lokalen Prototyp und stehen noch aus.

---

## Frage 5 — Ist die Rust-NDK-Toolchain im F-Droid-Buildserver baubar?

**Urteil: TRÄGT MIT AUFLAGEN.** Die Abbruchbedingung aus B10 ist damit
aufgehoben; die Fragen 0 bis 4 des Spikes werden ausgeführt.

### Präzedenzfall

**Delta Chat** (`com.b44t.messenger`) baut eine Rust-Kernbibliothek über das
NDK aus einem produktiven, laufend gepflegten F-Droid-Rezept. Version 2.57.0
wurde am **2026-07-31** veröffentlicht — der Fall ist aktuell, nicht
historisch. Die Rust-Integration reicht in derselben Metadatei bis Version
1.1.2 (2019) zurück, über NDK r14b → r27.

Der reale Build-Eintrag (`metadata/com.b44t.messenger.yml`, `master`):

```yaml
  - versionName: 2.57.0
    versionCode: 7544
    timeout: 20000
    sudo:
      - apt-get update
      - apt-get install -y make g++ cmake rustup
    prebuild: sed -i -e 's/abiFilters .*/abiFilters "x86_64"/' ...
    build:
      - rustup default $(cat scripts/rust-toolchain)
      - rustup target add x86_64-linux-android
      - PATH=$PATH:$$NDK$$/toolchains/llvm/prebuilt/linux-x86_64/bin/ \
        ANDROID_NDK_ROOT=$$NDK$$ scripts/ndk-make.sh x86_64
    ndk: r27
```

Vier solche Einträge existieren je Version — **einer je ABI, mit eigenem
`versionCode`**. F-Droid unterstützt keine App Bundles oder native
Split-APKs; das ist die etablierte Umgehung.

Ein zweiter, teilweise passender Beleg ist **RiseupVPN**
(`se.leap.riseupvpn`): dasselbe Muster „`sudo:` installiert
Fremdsprachen-Toolchain, `build:` kompiliert nativ über NDK" — dort für Go
statt Rust.

### Die vier Teilfragen

**NDK-Verfügbarkeit.** Die Beschaffung ist **versionsagnostisch**:
`fdroidserver/common.py` (`auto_install_ndk`) ruft
`sdkmanager.build_package_list(use_net=True)` und danach
`sdkmanager.install(f'ndk;{ndk}')` — es gibt keine Whitelist. NDK
29.0.14206865 ist eine offizielle Google-Veröffentlichung und sollte damit
genauso beziehbar sein wie r27. **Bewiesen ist aber nur bis r27.**
→ Auflage: r27 als Rückfallebene einplanen.

**Toolchain-Beschaffung.** `rustup` darf aufgerufen werden und Targets
nachinstallieren; das ist der heute dominante Weg, keine Ausnahme. Debian
liefert inzwischen ein `rustup`-Paket, und die im Repo gepinnte
`rust-toolchain`-Datei bestimmt die Compilerversion — Reprises Rust-1.92-
Anforderung ist damit vom Debian-Basisimage entkoppelt.

**Netzzugang während des Builds.** Vorhanden. F-Droids Blogpost von 2022
behauptet das Gegenteil, wird aber von zwei Primärquellen widerlegt:
`fdroidserver` lädt das NDK selbst mit `use_net=True`, und Delta Chats
aktives Rezept ruft `apt-get install` und `rustup target add` auf. Die
formale Inclusion Policy enthält keine Netzregel für den Build — ihre
Netzregeln betreffen die Laufzeit der fertigen App.
→ **`cargo vendor` ist nicht nötig.** Zur Einordnung: ein Vendor-Lauf über
den heutigen Desktop-Workspace ergab 670 MB, davon ~300 MB reine
Windows-Crates, die für Android nie gezogen würden.

**Zeitgrenzen.** Das `timeout:`-Feld ist dokumentiert, Vorgabe 7200 s,
`0` = unbegrenzt. Delta Chat setzt 20000 s **je ABI-Eintrag**. Das eigentliche
Risiko liegt woanders: die **GitLab-CI-Pipeline**, die jede
`fdroiddata`-Merge-Request testweise baut, hat bei vergleichbaren
From-Source-Projekten (Godot, `com.controlloid`) bereits manuelles
Nachjustieren durch Maintainer gebraucht.

### Auflagen für P8

1. **Ein Build-Eintrag je ABI** mit eigenem `versionCode` (arm64-v8a,
   armeabi-v7a, x86_64) statt einer Fat-APK.
2. **Großzügiges `timeout:`**, an Delta Chats 20000 s orientiert.
3. **NDK zuerst mit der aktuellen Version versuchen, r27 als Rückfall.**
4. **Die Erstaufnahme-Pipeline früh mit den F-Droid-Maintainern klären**,
   nicht erst beim Einreichen.

### Entwurf eines Builds-Eintrags

Angelehnt an Delta Chat, aber mit `cargo-ndk` (lokal bereits im Einsatz)
statt handgeschriebener Linker-Konfiguration. Ein Eintrag je ABI:

```yaml
  - versionName: '1.0.0'
    versionCode: 1001
    commit: <tag>
    subdir: android/app
    timeout: 21600
    sudo:
      - apt-get update
      - apt-get install -y make g++ cmake rustup pkg-config
    prebuild: sed -i 's/abiFilters .*/abiFilters "arm64-v8a"/' build.gradle
    build:
      - rustup default $(cat rust-toolchain)
      - rustup target add aarch64-linux-android
      - cargo install cargo-ndk --locked --version 4.1.2
      - cd ../../<bindings-crate>
      - cargo ndk -t arm64-v8a -o ../android/app/src/main/jniLibs build --release
    ndk: r27
    gradle:
      - foss
```

`rusqlite`/`bundled` braucht nur einen funktionierenden C-Compiler fürs
Ziel; `cargo-ndk` setzt `CC_<target>`/`AR_<target>` aus der NDK-Toolchain,
und `cmake`/`make`/`g++` kommen wie bei Delta Chat über `sudo:`.

### Restunsicherheiten

1. **NDK 29.0.14206865 ist konkret ungetestet.** Der Mechanismus spricht
   dafür, bewiesen ist r27.
2. **Die Erstaufnahme-Pipeline** ist eine vom `timeout:`-Feld unabhängige
   Hürde und für ein Workspace dieser Größe ungetestet.
3. **Reale Laufzeit und Hardware** der Produktionsflotte sind nicht
   dokumentiert; ein aktueller Diskussionsfaden deutet auf ältere
   Build-Maschinen hin. Ohne echten Lauf bleibt die Dauer eine Hochrechnung.

### Quellen

- `https://gitlab.com/fdroid/fdroiddata/-/raw/master/metadata/com.b44t.messenger.yml`
- `https://f-droid.org/en/packages/com.b44t.messenger/`
- `https://raw.githubusercontent.com/f-droid/fdroidserver/master/fdroidserver/common.py`
- `https://gitlab.com/fdroid/fdroid-website/-/raw/master/_docs/Build_Metadata_Reference.md`
- `https://gitlab.com/fdroid/fdroid-website/-/raw/master/_docs/Inclusion_Policy.md`
- `https://forum.f-droid.org/t/pipeline-timeout-building-a-new-app/32809`
- `https://forum.f-droid.org/t/build-timeout-for-com-controlloid/7833`

---

## Frage 0 — Baut der Rust-Baum überhaupt für Android?

Offen. Braucht den lokalen Prototyp (Plan Task 4).

## Frage 3 — Trägt UniFFI die Typen von reprise-view?

Offen. Braucht den lokalen Prototyp (Plan Task 6). Prüfobjekt ist
`ui/track_list/queue_sections.rs` (`QueueViewModel`, `QueueSection`,
`QueueSectionKind`, `VirtualContextTail`), nicht mehr
`podcasts_presentation.rs` — siehe Spec B13.

## Frage 1 — Erfüllt Media3 den playback-Vertrag?

Offen. Braucht den lokalen Prototyp (Plan Task 7).

## Frage 2 — Kann ein MediaSessionService die Runtime beherbergen?

Offen. Braucht ein echtes Gerät (Plan Task 8).

## Frage 4 — Trägt SAF den Scanner?

Offen. Braucht den lokalen Prototyp (Plan Task 9).
