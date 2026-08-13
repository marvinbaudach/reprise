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

**Urteil: TRÄGT.** Ohne Auflagen, ohne Nacharbeit an einer einzigen Zeile
Rust.

Umgebung: NDK **29.0.14206865** (`/opt/android-sdk/ndk`), `cargo-ndk` 4.1.2,
Rust 1.92, Targets `aarch64-linux-android`, `armv7-linux-androideabi`,
`x86_64-linux-android`.

| Crate | ABI | Release-Build |
| --- | --- | --- |
| `reprise-core` | arm64-v8a | **OK**, 43 s |
| `reprise-core` | armeabi-v7a | **OK**, 42 s |
| `reprise-core` | x86_64 | **OK**, 45 s |
| `reprise-runtime` | arm64-v8a | **OK**, 7 s |

Drei Befunde:

1. **`rusqlite` mit `bundled` übersetzt SQLite aus C für alle drei
   Android-Architekturen**, ohne dass an der Toolchain etwas eingerichtet
   werden musste. `cargo-ndk` setzt `CC_<target>`/`AR_<target>` aus dem NDK
   selbst. Das war das größte Einzelrisiko dieser Frage und es ist keins.
2. **`reprise-runtime` baut in 7 Sekunden auf einem fremden Target.** Damit
   ist Spec §2.1 („die Runtime ist transportfrei") nicht mehr nur eine
   Behauptung über den Dependency-Baum, sondern auf Android bewiesen — sie
   zieht nichts Linux-Spezifisches nach.
3. **Erzeugt werden `.rlib`-Dateien, nicht `.so`.** Das ist erwartet: beide
   sind reine Bibliotheks-Crates. Die gemeinsame Bibliothek entsteht später
   aus einer Bindings-Crate mit `crate-type = ["cdylib"]` — das ist Teil von
   Frage 3 (UniFFI), nicht dieser Frage.

Zur Einordnung für P8: knapp 45 s je ABI im Release-Build auf einem
Entwicklungsrechner mit warmem Cache. Der F-Droid-Buildserver baut kalt und
auf schwächerer Hardware, aber die Größenordnung liegt weit unter dem
Timeout-Budget, das Delta Chat dort verwendet.

## Frage 3 — Trägt UniFFI die Typen von reprise-view?

**Urteil: TRÄGT MIT EINER AUFLAGE**, und die Auflage prägt P1a.

Prüfobjekt war `ui/track_list/queue_sections.rs`, nachgebaut in
`spikes/uniffi-shapes` gegen UniFFI 0.29 (Proc-Macro-Modus, plus ein
`uniffi-bindgen`-Binärziel).

### Was trägt

Verschachtelte Records, Listen, `u32`/`i64`/`String` und — bemerkenswert —
**Enums mit Datenvariante**. `QueueSectionKind::UpNext { source_label }`
wird auf der Kotlin-Seite zu einer `sealed class`, also genau der
idiomatischen Entsprechung:

```kotlin
data class QueueRow(...)
data class QueueSection(...)
data class QueueViewModel(...)
sealed class QueueSectionKind { ... }
fun queueViewModel(rows: UInt): QueueViewModel
fun queueWindow(start: UInt, len: UInt): List<Long>
```

### Was nicht trägt — und das ist der eigentliche Befund

`QueueViewModel` hält heute über `VirtualContextTail` einen **boxed
Closure**:

```rust
window: Rc<dyn Fn(usize, usize) -> Vec<i64>>
```

Das ist die Naht, an der die GTK-Liste beim Scrollen in Rust zurückruft, um
den Kontext-Schwanz fensterweise nachzuladen. Über eine FFI-Grenze ist sie
nicht darstellbar. Gegenprobe gefahren, drei Fehler:

```text
the trait `uniffi::TypeId<UniFfiTag>` is not implemented for
  `Rc<dyn Fn(usize, usize) -> Vec<i64>>`
the trait `uniffi::Lower<UniFfiTag>` is not implemented for ...
the trait `Lift<UniFfiTag>` is not implemented for ...
```

Die geteilte Form kompiliert dagegen sauber: das ViewModel trägt nur noch
`context_len`, und das Fenster holt der Aufrufer über einen **expliziten**
`queue_window(start, len)`.

### Konsequenz für P1a

**ViewModels in `reprise-view` dürfen keine Closures halten.** Faules
Nachladen wird von Rückruf zu Anfrage/Antwort. Das betrifft **auch GTK** —
beide Oberflächen konsumieren dieselbe Schicht, also verliert das
GTK-Frontend an dieser Stelle seinen Closure-Rückruf und bekommt denselben
expliziten Fensteraufruf. Das ist kein Nebenschauplatz von P1a, sondern eine
seiner Kernumbauten.

### Noch nicht gemessen

Die Kosten für eine lange Liste über die Grenze (Plan Task 6 Step 4: 10.000
Zeilen). Dafür braucht es eine Kotlin-Laufzeit, nicht nur die Generierung.
Die Frage ist offen, ob das ViewModel am Stück oder seitenweise geholt
werden muss — die Antwort ändert die Auflage oben nicht, nur ihre
Dimensionierung.

## Frage 1 — Erfüllt Media3 den playback-Vertrag?

**Urteil: TRÄGT**, mit einer benannten Lücke.

Gemessen am 2026-08-01 auf einem **Pixel 10 Pro XL, Android 17 (API 37)** —
also am strengsten verfügbaren API-Level — mit Media3 1.10.1, gegen zwei
fünfminütige FLAC-Proben.

| Vertragsteil | Beleg |
| --- | --- |
| Laden | `existiert=true bytes=10066662`, dann `BUFFERING → READY` |
| Dauer | `dur=300000` — korrekt aus der Datei, nicht geraten |
| Abspielen | `isPlaying=true`, `pos` wächst |
| Position lesen | `pos=4667`, später `pos=38325` |
| Suchen | `BUFFERING pos=35989 → READY pos=36048`, Wiedergabe läuft weiter |
| Pausieren | `isPlaying=false`, `pos=39512` eingefroren |
| **Gapless** | `itemTransition reason=1` (`AUTO`) bei `pos=283873`, direkt gefolgt von `pos=20164` im nächsten Titel — **ohne** `BUFFERING` und **ohne** `ENDED` dazwischen |

Der Gapless-Beleg ist der wertvollste: Media3 wechselt automatisch und ohne
Pufferstillstand. Ein Zwischenzustand hätte sich im Log gezeigt.

**Die Lücke: Crossfade.** ExoPlayer bringt kein Überblenden mit; das
GTK-Frontend kann es. Für P4a heißt das entweder Eigenbau über zwei
Player-Instanzen oder ein bewusster Verzicht auf Android. **Nicht gemessen**,
weil es nichts zu messen gab — die Funktion existiert schlicht nicht.

**Ebenfalls nicht gemessen:** `STATE_ENDED` am Ende der letzten Queue-Position
(der Lauf endete vorher).

## Frage 2 — Kann ein MediaSessionService die Runtime beherbergen?

**Urteil: TRÄGT** für alles, was ohne Handgriff am Gerät prüfbar war.

Ein Befund vorweg, der B7 stützt: **Service und Activity laufen im selben
Prozess** (dieselbe PID im Log). Der Service ist damit ein tragfähiger Wirt
für eine eingebettete Runtime — sie müsste nicht über eine Prozessgrenze
angesprochen werden.

| Prüfung | Ergebnis |
| --- | --- |
| Foreground-Service-Typ (AUD-8) | `isForeground=true`, `types=0x00000002` = `MEDIA_PLAYBACK`. Auf API 37 ist das die Pflichtangabe, ohne die `startForeground()` abstürzt |
| Medienbenachrichtigung | `category=transport`, `actions=2`, `NO_CLEAR｜FOREGROUND_SERVICE`, `vis=PUBLIC` |
| Audio-Focus angefordert | Im System-Focus-Stack: `pack: dev.reprise.spike`, `gain: GAIN`, `loss: none`, `usage=USAGE_MEDIA`, angefordert durch `androidx.media3.common.audio.AudioFocusManager` |
| Hintergrundwiedergabe | 60 s Bildschirm aus: **null Ereignisse**, Prozess lebt, Wiedergabe ununterbrochen |
| `POST_NOTIFICATIONS` | erteilt; die Wiedergabe hing zu keinem Zeitpunkt daran (AUD-13) |

`setAudioAttributes(…, handleAudioFocus = true)` und
`setHandleAudioBecomingNoisy(true)` genügen also, um die Anforderung
überhaupt zu stellen — nachgewiesen im Focus-Stack des Systems, nicht nur im
App-Code.

### Die drei Handgriffe am Gerät — nachgeholt

**Focus-Verlust: die Runtime erfährt davon.** Eine fremde App mit Ton
gestartet, und der Listener feuerte:

```text
19:14:44.288  EVENT isPlaying=false
```

**Das ist der wertvollste Einzelbefund dieses Spikes.** Media3 pausiert nicht
still hinter dem Rücken der Anwendung — der Zustandswechsel läuft über
`onIsPlayingChanged`, also über denselben Rückruf, den `reprise-runtime`
ohnehin abonnieren wird. Die befürchtete Konsequenz aus B7 (ein neu zu
erfindender Zustandsübergang) **entfällt**.

Bemerkenswert und regelkonform: **kein Auto-Resume** nach dem Ende der
Störung. Das entspricht `AUD-2` — bei dauerhaftem Verlust ist
Nicht-Fortsetzen das richtige Verhalten, kein Versäumnis.

**`ACTION_AUDIO_BECOMING_NOISY`: greift.** Manuell beobachtet (Kopfhörer im
USB-C-Port, deshalb ohne adb): „beim Abziehen stoppt der Ton". Für diese
Frage ist die Wahrnehmung das Kriterium; `setHandleAudioBecomingNoisy(true)`
tut, was es soll.

**Aus Recents gewischt:**

```text
19:16:28.570  LIFECYCLE onTaskRemoved
19:16:28.620  LIFECYCLE onDestroy
```

Der Service wurde beendet — **weil die Wiedergabe zu diesem Zeitpunkt
pausiert war**. Media3s Standard ist genau diese Unterscheidung: laufende
Wiedergabe überlebt das Wischen, pausierte nicht.

Das ist dieselbe Frage, die `RUN-6` auf dem Desktop stellt („Fensterschließen
beendet die Wiedergabe, die dieses Fenster gestartet hat"). Androids
Standardantwort deckt sich mit der Regel — P4a muss hier nichts erfinden,
nur bewusst bestätigen.

### Weiterhin offen

**Doze.** Braucht eine lange Leerlaufphase; nicht gemessen.

### Betriebsnotiz für künftige Gerätearbeit

Drahtloses adb war in diesem Netz **nicht** herstellbar. Ursache ist nicht
das VPN des Telefons — mit abgeschaltetem NordVPN blieb der Ping bei 100 %
Verlust —, sondern die **Client-Isolation des WLAN-Routers**. Zwei
WLAN-Geräte dürfen dort nicht miteinander sprechen. Für Tests, die den
USB-C-Port brauchen (Kopfhörer), heißt das: entweder ein anderes Netz oder
manuelle Beobachtung.

## Frage 4 — Trägt SAF den Scanner?

**Zwischenstand aus Code-Analyse (2026-08-01): TRÄGT NICHT ohne
Storage-Abstraktion.** Der Prototyp aus Plan Task 9 steht noch aus und muss
das Ausmaß bestätigen — die Richtung ist aber bereits klar, und sie ist die
teuerste Nachricht dieses Spikes.

Vier Mechanismen in `reprise-core` hängen strukturell an einem echten
Dateipfad:

1. **Der Scanner selbst.** `walkdir::WalkDir::new(root)` in
   `library/scanner.rs:264` und `library/scanner_progress.rs:15` — beide über
   `&Path`.
2. **Die Unmounted-gegen-Deleted-Klassifikation.** `library/mounts.rs` ist der
   heikelste Punkt: sein Modulkommentar hält ausdrücklich fest, der
   Mechanismus komme *„without any platform trait, GVolumeMonitor, or
   `/proc/mounts` parsing"* aus. Er ruht ganz auf `tracks.device` — dem
   `st_dev` der Datei aus Schema v2. Auf SAF gibt es kein `st_dev`. Das ist
   keine Lücke, sondern eine **bewusste Architekturentscheidung, die Android
   zunichtemacht**.
3. **Geschwisterdateien und alle drei Writeback-Pfade** (Cover, Tags,
   `.lrc`-Sidecars). Gemessen: **49 Produktivdateien** in
   `crates/reprise-core/src` nutzen `std::fs` direkt (90 inklusive Tests).
4. **Der `notify`-Watcher.** Auf SAF gibt es keine Entsprechung — das ist
   zugleich die Antwort auf offenen Punkt O3.

### Der Tag-Pfad ist handle-fähig — gemessen, nicht vermutet

Die teuerste Teilfrage war, ob `lofty` überhaupt ohne Pfad arbeiten kann.
SAF liefert `content://`-URIs und daraus Dateideskriptoren, nie Pfade;
`reprise-core` ruft heute ausnahmslos `read_from_path`/`save_to_path`.

Gemessen am 2026-08-01 gegen `lofty` 0.24 mit
`crates/reprise-core/tests/fixtures/sine.flac`:

```text
LESEN AUS HANDLE: OK    Format: Flac, Tags: 1
SCHREIBEN IN HANDLE: OK
```

`lofty::read_from(&mut File)` und `AudioFile::save_to(&mut File,
WriteOptions)` existieren beide und funktionieren. Damit ist der gesamte
Tag-Lese- und Schreibpfad über einen Dateideskriptor bedienbar — der Umbau
ist ein Signaturwechsel, kein Ersatz der Bibliothek.

Das verkleinert den Befund deutlich. Übrig bleiben drei Stellen, die
wirklich neu gedacht werden müssen: das **Auflisten** eines Baums (SAF hat
mit `DocumentsContract` eine eigene API dafür), die
**Unmounted-Klassifikation** über `st_dev`, und der **Watcher**.

### Was daraus folgt

Eine Android-Oberfläche über unverändertem Core reicht nicht. Es braucht eine
echte **Storage-Abstraktion** — ein `LibrarySource`-artiges Trait mit
Auflisten, Lesen, Schreiben und Erreichbarkeitsprüfung — mit zwei
Implementierungen, durchgezogen durch Scanner, Mount-Klassifikation und alle
Writeback-Pfade. Dazu ein Ersatz für die `tracks.device`-Spalte, weil SAF
kein `st_dev` kennt.

**Rückwirkung auf die Planung:** Das ist Arbeit an `reprise-core`, nicht an
Android — sie gehört damit vor P4a und berührt Code, den auch P1a anfasst.
Die Größenordnung ist noch nicht seriös schätzbar; Task 9 muss den
Übergabemechanismus (Dateideskriptoren nach Rust gegen Pfad-Trait) klären,
bevor daraus ein Paket wird.

**Was dabei nicht verlorengeht:** Die Zweiteilung „vorübergehend
unerreichbar" gegen „endgültig weg" ist auf dem Desktop bereits als Konzept
vorhanden (`MissingReason::Unmounted` / `Deleted` / `Unknown`). Sie trägt auf
Android weiter, nur die Feststellung braucht einen anderen Weg als `st_dev`.

## Frage 6 — Läuft gettext auf Android? (nachgetragen 2026-08-02)

**Urteil: JA, statisch, für 24 kB je ABI.** Der Befund gehört zu Spec-Punkt
O2 und wurde beim Ausmessen von Welle 1 nötig, weil die Strings an
`crate::i18n::gettext` hängen.

Ausgangslage: `reprise-gnome` bindet `gettext-rs 0.7.7` mit
`features = ["gettext-system"]`, also gegen die **System**-libintl. Bionic hat
keine — die Annahme „Strings ziehen einfach nach `reprise-view`" trägt so
nicht.

Gemessen an einer `cdylib`-Probe gegen `aarch64-linux-android`, NDK r29,
`cargo-ndk`, `gettext-rs` mit `default-features = false`:

| Prüfung | Ergebnis |
| --- | --- |
| `gettext-sys` baut libintl aus Quellen | **OK** — `libintl.a` mit NDK-Clang, ~28 s |
| Link einer `cdylib` | **OK**, Exit 0 |
| `NEEDED` der erzeugten `.so` | nur `libdl.so`, `libc.so` — **kein externes libintl** |
| undefinierte `gettext`/`intl`-Symbole | **keine**, statisch aufgelöst |
| `libintl_gettext`, `_libintl_gettext_extract_plural`, `_nl_expand_alias` | im Binary vorhanden; Referenzprobe ohne gettext: null Treffer |
| Größe gestrippt, release | 331.664 B mit gettext gegen 307.584 B ohne → **+24.080 B je ABI** |

**Messfalle, die zweimal zuschlug:** Eine `rlib` wird nicht gelinkt, beweist
also nichts über fehlende Symbole. Und eine `cdylib` exportiert nur
`#[no_mangle] extern "C"`-Symbole — ein bloßes `pub fn` wird samt gettext
wegoptimiert, worauf beide Proben byte-identisch herauskamen. Erst der echte
C-Export macht die Messung gültig.

**Was damit noch nicht entschieden ist:** nur die Bauseite. Wie Compose und
Tauri die `po`-Kataloge konsumieren — Katalog in `reprise-view` gegen
`strings.xml`-Erzeugung aus `po` — bleibt offen. Der Unterschied ist ab jetzt
eine Werkzeug- und Übersetzer-Workflow-Frage, kein Build-Hindernis.

**Auflage für die Welle, die `strings.rs` bewegt:** `reprise-view` darf
`gettext-rs` nur ohne `gettext-system` führen, sonst bricht der Android-Build.
Das Architektur-Gate fängt das nicht ab — es verbietet `gtk4|libadwaita|glib|
gstreamer|zbus` und fremde `reprise-*`-Kanten, und `gettext-rs` ist keins von
beidem.

## Frage 0b — Läuft der Spike auf dem Emulator? (nachgetragen 2026-08-02)

**Urteil: JA — und das war vorher nicht bewiesen.** Frage 0 hat gezeigt, dass
`reprise-core` für alle drei Android-ABIs *baut*. Was danach tatsächlich lief,
war die arm64-Variante auf dem angeschlossenen Pixel. Der headless-Emulator
aus der Betriebsnotiz ist `x86_64` — der Spike trug diese ABI gar nicht
(`abiFilters += "arm64-v8a"`, eine ABI, um die APK-Größe ehrlich zu messen).
Ein Start dort wäre am `UnsatisfiedLinkError` gescheitert.

Behoben und gemessen auf `emulator-5554` (`pixel10xl_api37`, Android 17,
x86_64, headless, Software-Rendering):

| Schritt | Ergebnis |
| --- | --- |
| `cargo ndk -t x86_64 build --release -p reprise-android-ffi` | **OK**, 3 min 59 s kalt |
| `libreprise_android_ffi.so` x86_64 | 5.272.992 B (arm64 zum Vergleich: 5.508.168 B) |
| APK mit beiden ABIs | 22.730.147 B debug; `lib/x86_64/` und `lib/arm64-v8a/` beide enthalten |
| Installation und Start | **OK**, Prozess lebt, **kein `UnsatisfiedLinkError`, kein FATAL** |
| `reprise-core` im App-Sandbox | DB öffnen + migrieren **157 ms**, Scan 4 ms, Count 1 ms, Fenster 1 ms |
| Scan-Ergebnis | `added=3 errors=0`, Bibliothek hält 3 Tracks |
| UniFFI → Compose | Titelliste als **Text** im Accessibility-Baum gelesen, nicht als Pixel |

Damit ist die Kette lückenlos: Rust-Kern über NDK gebaut, per UniFFI nach
Kotlin, von Compose gerendert, auf einem Gerät, das niemand bereithalten muss.
Die 157 ms für Öffnen und Migrieren sind unter Software-Rendering gemessen und
damit eine Obergrenze, keine Zielgröße.

**Betriebsfolge:** Der Emulator ist ab jetzt ein vollwertiges Testziel für
Android-Arbeit an Reprise, nicht nur für Fremd-Apps. Wer eine ABI hinzufügt,
muss `abiFilters` mitziehen — der Build schlägt sonst nicht fehl, die App
stürzt erst beim Start ab.

## Frage 7 — Wie groß ist die Storage-Abstraktion? (nachgetragen 2026-08-02)

Die Spec sagt an dieser Stelle ausdrücklich **„Nicht geschätzt"** und nennt
die Frage, an der die Größe hängt: wie viele der Dateisystem-Stellen in
`reprise-core` müssen wirklich über `LibrarySource`, und wie viele nicht.

Vermessen wurden alle Produktivdateien mit Dateisystem-Zugriff, je Bereich von
einem Agenten eingeordnet; anschließend hat ein zweiter Durchgang jeden
**Freispruch** angegriffen — die Richtung ist bewusst so gewählt, weil beim
Schätzen das Unterzählen der teure Fehler ist.

| Urteil | Stellen |
| --- | --- |
| **must-abstract** | 21 behauptet → **27 nach der Gegenprüfung** |
| nur ein Dateihandle nötig | 6 → 3 |
| app-eigener Pfad (Cache/Config) | 3 |
| nur in Tests | 11 |

**6 von 20 Freisprüchen wurden zurückgenommen** — eine Fehlerquote von 30 % in
genau der Richtung, die zählt. Alle sechs lagen im Tag-Schreiben
(`scanner_meta.rs`, `tag_edit.rs`, `tag_mutation.rs`, `tag_mutation_guarded.rs`,
`tag_edit_write.rs`, `trash_tracks.rs`): sie galten als „arbeitet auf einem
Handle", öffnen die Nutzerdatei aber selbst per Pfad.

### Was das Paket kleiner macht

1. **Das Tag-Schreiben bündelt sich bereits in einer Naht.**
   `tag_mutation.rs` nennt `apply_tag_patch_to_file` im eigenen Doc-Kommentar
   „the sole production Lofty tag-save path". Tag-Editor, geschützter Editor
   und der Batch-Job laufen alle dort hindurch. Da `lofty` laut Spike auf
   Handles arbeitet, deckt **ein** umgestelltes Modul die gesamte
   Tag-Schreib-Oberfläche ab.
2. **Zwei Muster für den Schnitt existieren bereits im Code.**
   `trash_tracks.rs` nimmt die plattformspezifische Löschung schon als
   injizierten `Fn(&Path)`-Closure entgegen, und `device_sync/machine.rs` gibt
   einen `Effect`-Enum aus, den eine Plattformschicht ausführt. Beides sind
   funktionierende Präzedenzfälle — die Abstraktion muss nicht erfunden,
   sondern verallgemeinert werden.
3. **Die echten Caches sind app-privat.** `artist_portrait/cache.rs` und
   `remote_image` liegen im App-Verzeichnis, für das Android einen echten
   Dateisystempfad vergibt. SAF greift dort nicht, dort ändert sich nichts.
4. **Viel weniger Dateien als vermutet fassen überhaupt an.** In `concerts`
   und `radio` tut es außer `http.rs` keine einzige; in `lyrics` nur 2 von 11;
   in `queries`/`device_sync` nur 5 von 10 — der Rest manipuliert `PathBuf`
   als Zeichenkette, was nach der eigenen Regel des Crates keine
   Dateisystemstelle ist.

### Was das Paket größer macht

1. **`mounts.rs` hat auf Android kein Gegenstück — das ist der teure Befund.**
   Die Unterscheidung „Laufwerk nicht eingehängt" gegen „Datei gelöscht" steht
   vollständig auf POSIX-`st_dev` und `lstat` über die Pfad-Vorfahren. Unter
   SAF gibt es kein `/proc/mounts`, keine stabile Geräte-ID unter einem
   `content://`-Baum und kein Vorfahren-`lstat`. **Das ist kein
   Signaturwechsel.** Entweder bekommt Android einen anderen Mechanismus
   (etwa: ist die Berechtigung für diesen Tree-URI noch erteilt?), oder die
   Unterscheidung wird dort bewusst aufgegeben.
   `mounts.rs`, `scanner_mount.rs`, die Identitätsfelder in `scanner.rs` und
   `relink.rs` bilden dabei **eine zusammenhängende Einheit**, keine
   unabhängigen Stellen.
2. **Drei Bereiche haben keine Bündelung.** `walkdir::WalkDir::new(root)` läuft
   an vier Stellen unabhängig über denselben Bibliotheksbaum (`scanner.rs`,
   `scanner_progress.rs` für den Fortschrittsbalken, `relink.rs` zweimal).
   Bei den Podcasts gibt es **keinen** gemeinsamen Weg für „einen geladenen
   Download abschließen" — drei eigenständige Implementierungen. Und in
   `lyrics` leiten `local.rs` und `sidecar_write.rs` jeweils selbst
   `track_path.with_extension("lrc")` ab.
3. **Die `.exists()`-Prüfungen sind verstreut.** „Ist dieser Track noch da?"
   steht als Einzeiler in `scanner_vanish.rs`, `scanner_move.rs`, dreimal in
   `relink.rs` und in `rhythmbox_import.rs` — jeweils trivial, aber ohne
   gemeinsame Stelle.

### Urteil

**Das Paket ist zweigeteilt, und nur eine Hälfte ist Fleißarbeit.**

Die 27 Stellen zerfallen in rund 20 mechanische — `.exists()`,
`metadata()`, `walk`, Handle statt Pfad — die über das Trait der Spec
laufen, sobald es existiert, und die dank der Tag-Naht und der beiden
vorhandenen Präzedenzfälle weniger Aufwand sind, als ihre Zahl vermuten lässt.

Der Rest ist **ein einziger Entwurf**: der Ersatz für `st_dev`. Er entscheidet,
ob `MissingReason::Unmounted` auf Android eine Bedeutung hat, und er berührt
Scanner, Relink und die Mount-Klassifikation gemeinsam. Diese eine Frage sollte
beantwortet sein, bevor das Paket geschnitten wird — alles andere daran ist
absehbar.

### Korrektur zu Frage 7 (2026-08-02, nach dem Lesen von `classify_missing`)

Oben steht, `mounts.rs` habe „auf Android kein Gegenstück" und sei „ein
Neuentwurf". **Das war zu scharf**, und der Fehler ist meiner: Ich habe die
Einordnung des messenden Durchgangs übernommen, ohne den Klassifikator selbst
zu lesen. Er ist zehn Zeilen lang und bereits plattformneutral:

```rust
pub(crate) fn classify_missing(stored_device: Option<i64>, path: &Path) -> MissingReason {
    let Some(stored_device) = stored_device else { return MissingReason::Unknown };
    match nearest_existing_ancestor_dev(path) {
        Some(current) if current == stored_device as u64 => MissingReason::Deleted,
        Some(_) => MissingReason::Unmounted,
        None => MissingReason::Unknown,
    }
}
```

Die Logik fragt nicht, **was** das Merkmal ist — nur, ob es mit dem
gespeicherten übereinstimmt. Das ist wörtlich `residence_token` aus dem
Spec-Trait, und die Spec hat es richtig vorhergesehen: die Spalte wird von
„`st_dev`" zu einem generischen Aufenthaltsmerkmal umgedeutet, ohne Migration.

**Der tatsächliche Umfang, nachgezählt:**

| | |
| --- | --- |
| Unix-spezifische Produktivstellen (`MetadataExt`, `.dev()`, `.ino()`, `symlink_metadata`) | **15**, in vier Dateien |
| davon in `mounts.rs` | 9 |
| Aufrufstellen von `classify_missing` | **2** (`scanner_vanish.rs:148`, `queries/maintenance.rs:365`) |
| plattformabhängige Funktion | **eine**: `nearest_existing_ancestor_dev` |

**Unter SAF wird die Feststellung sogar einfacher, nicht schwerer.** Der
Vorfahren-Aufstieg existiert dort nicht — er muss auch nicht: die Wurzel des
gewählten Baums **ist** der feste Vorfahr. Ist sie abfragbar, steht das Volume
und eine fehlende Datei ist `Deleted`; ist sie es nicht — Berechtigung
entzogen, Karte gezogen —, dann `Unmounted`. Dieselbe Dreiteilung, ein
Vergleich weniger.

**Die Fehlerrichtung stimmt bereits.** `classify_missing` läuft nur für
Dateien, die ohnehin schon als fehlend gelten. Ein veraltetes Merkmal — unter
Linux können sich Gerätenummern über einen Neustart ändern — führt zu
`Unmounted` statt `Deleted`, also zur datenerhaltenden Antwort. Diese
Großzügigkeit trägt unverändert nach SAF.

**Was offen bleibt, ist eine andere Frage als die, die ich oben gestellt habe:**
nicht die Erreichbarkeit, sondern die **Umzugserkennung**. `scanner.rs`
erkennt eine verschobene Datei an `(device, inode)`. SAF kennt keine Inodes,
sondern Dokument-IDs, und deren Stabilität über einen Umzug hinweg garantiert
der DocumentsProvider, nicht die Plattform. Das ist der Punkt, der einen
eigenen Entwurf braucht — nicht `classify_missing`.

### Paket 1 umgesetzt: Aufenthalt und Erreichbarkeit (2026-08-02)

`LibrarySource: Send + Sync` besitzt jetzt genau die zwei zuerst benötigten
Operationen: `residence_token` und die darauf aufgebaute
Vorgabe-Implementierung `reachability`. `UnixLibrarySource` liefert weiterhin
den `st_dev` des mit `lstat` gefundenen nächsten existierenden Vorfahren; eine
Quelle ohne stabiles Merkmal liefert ehrlich `None`. Die zwei bisherigen
`classify_missing`-Aufrufe und die Root-Guard-Prüfung laufen über dieses Trait.
Der Linux-Pfad bleibt dabei identisch: der aktuelle `u64`-Gerätewert und der
gespeicherte SQLite-`i64`-Wert werden wie zuvor über dasselbe Bitmuster
verglichen. Die alten Mount-Tests bleiben grün; eine zweite Testquelle leitet
ihr Merkmal stattdessen aus einer Provider-Tree-ID ab und liefert dieselbe
Dreiteilung `Deleted` / `Unmounted` / `Unknown`.

**Gewonnene Zuführungsform: ein `&dyn LibrarySource`-Parameter.** Die heutigen
öffentlichen Scan- und Query-Grenzen behalten schmale Unix-Vorgaben, die den
Parameter an die drei betroffenen Helfer reichen.

**Damit ist die Naht gelegt, aber noch nicht angeschlossen.** `UnixLibrarySource`
steht fest verdrahtet in drei Vorgabe-Wrappern (`mark_vanished`,
`any_candidate_confirms_root_residence`, `mark_track_missing_if_current`). Die
Wahl der Quelle wird also weiterhin *innerhalb* von `reprise-core` getroffen,
nicht an dessen Rand. Für Paket 1 ist das Absicht — die öffentliche Scan- und
Query-API bliebe sonst nicht stabil, und der erste Schnitt wäre nicht mehr
isoliert prüfbar. Aber eine SAF-Quelle kann heute an keiner Produktionsstelle
eingesetzt werden, nur im Test. Das Hochziehen dieser Wahl an die Crate-Grenze
ist offene Arbeit und gehört in dasselbe Paket, das die erste echte SAF-Quelle
mitbringt. Ein Closure hätte die
zusammengehörigen Operationen `residence_token` und `reachability` wieder
auseinandergerissen; ein `Effect`-Enum hätte für drei synchrone Abfragen die
Scanner-Transaktion und die Query unnötig in einen Plattform-Rundlauf zerlegt.
Die Folgepakete reichen deshalb dieselbe Quelle als Parameter weiter, statt
pro Operation neue Closures oder Effects einzuführen.

Die ursprüngliche **27er-Messung zählte Abstraktionsstellen und Nähte, nicht
Rohaufrufe**. Eine neue Rohmessung des damaligen Clusters „Baum und Präsenz"
auf `dev` fand **53 Produktivstellen in 22 Dateien** (`.exists()`,
`fs::metadata`, `symlink_metadata`, `is_file`, `is_dir`, `read_dir` und
`WalkDir::new`; Tests ausgeschlossen). Diese Zahl ist keine belastbare
Paketgröße: derselbe Cluster vermischt Bibliotheksquellen mit app-privatem
Speicher. `cover.rs` zeigt den Fehler besonders klar: Von fünf Treffern ist
genau das `read_dir` im Albumordner bibliotheksbezogen; vier `out.exists()`
prüfen ausschließlich Thumbnails im XDG-Cache und dürfen nie in
`LibrarySource` wandern.

Der korrigierte Stand nach Paket 2 ist deshalb absichtlich keine weitere
Dateiliste:

| Cluster | Stand |
| --- | --- |
| Traversierung (Paket 2) | **umgesetzt**: Die drei nach dem Entfernen des Scanner-Vorlaufs verbliebenen Baumläufe gehen über `LibrarySource::walk`; nur `UnixLibrarySource` kennt noch `walkdir`. |
| Präsenz und Metadaten (Paket 3) | **noch nicht klassifiziert**: Die 53 Rohstellen müssen **stellenweise** als bibliotheksbezogen oder app-privat eingeordnet werden. Eine Datei kann beides enthalten; eine Dateiliste ist daher keine zulässige Arbeitsgrundlage. |
| Quellnahe Lese-/Schreib-Handles (danach) | Die bestehende Inventur bleibt ein Hinweis auf Nähte, wird aber erst nach Paket 3 neu gegen den dann verbleibenden Bestand geprüft. `tag_mutation.rs` bleibt die erwartete gemeinsame Tag-Schreib-Naht. |
| Separater späterer Vertrag | `library/watcher.rs`: `notify` hat unter SAF kein gleichwertiges Gegenstück und bleibt die ausdrücklich optionale Watcher-Fähigkeit. |

**Offener Vorbereitungsschritt für Paket 3:** eine appendierbare Liste aller 53
Fundstellen mit Datei, Zeile/Operation und Klassifikation
`Bibliotheksquelle`/`app-privat`. Erst diese Liste darf den Paketschnitt
bestimmen. Bis sie existiert, ist weder die Zahl 53 noch eine Liste der 22
Dateien eine Implementierungsfreigabe. App-private Cache-, Konfigurations- und
Staging-Pfade bleiben weiterhin außerhalb von `LibrarySource`.

### Paket 2 umgesetzt: Traversierung ohne Scanner-Doppellauf (2026-08-02)

`scan_folder_with_progress` zählt den Baum nicht mehr vor. Stattdessen dient
die Zahl der gegenwärtigen Katalogzeilen unter der gewählten Wurzel als billige
Schätzung aus dem letzten Scan; findet der Lauf mehr Dateien, wächst der
Nenner mit. Neue Dateien können ihn anheben, bei entfernten Dateien kann der
Lauf enden, bevor die Schätzung 100 Prozent erreicht. Das Scan-Ergebnis bleibt
unverändert, aber SAF spart einen vollständigen Satz von
DocumentsProvider-Auflistungen.

**Beim ersten Scan gibt es keine Schätzung, und das sagt die App auch.**
`ScanProgress::Scanning::total` ist `Option<u64>`; ohne vorherigen Katalog
bleibt es `None`, und die Oberfläche zeigt einen unbestimmten Balken mit
„Finding music files…" statt einer Prozentzahl. Der naheliegende Kurzschluss —
den Nenner auf die bereits gesehenen Dateien zu heben — ergäbe `n von n` und
damit einen vollen Balken über die gesamte Dauer des längsten Scans, den ein
Nutzer je abwartet. Ein Test hält das fest; eine Schätzung, die künftig wieder
auf `processed` zurückfällt, schlägt dort fehl.

Die Schätzung nutzt denselben `LIKE '<wurzel>/%'`-Vorfilter wie
`scanner_vanish::candidates_under_root`, mit `Path::starts_with` als
maßgeblicher Prüfung dahinter. Ohne ihn zöge ein Rescan eines kleinen
Unterbaums jedes vorhandene Stück der ganzen Bibliothek durch Rust, nur um
einen Fortschrittsbalken zu bemaßen.

`LibrarySource::walk` ist objekt-sicher und stromorientiert: Ein benanntes
Visitor-Interface trägt benannte Einträge und Fehler sowie ein Stoppsignal;
weder `walkdir::DirEntry`, Closure, anonymes Tupel noch `impl Iterator` gehört
zum Trait. Die Unix-Quelle kapselt `walkdir` mit `follow_links(false)`, der
Scanner behält die native Reihenfolge und Relink die Sortierung nach Dateiname.
Ein im Test aufgebauter DocumentsProvider-artiger Baum ohne Dateisystem liefert
dieselbe sortierte Audio-Datei-Projektion wie der Unix-Adapter.

Bewiesen ist damit der *Vertrag*. Dass auch die *Verbraucher* quellenneutral
sind, zeigen zwei Scanner-Tests, die `scan_folder_with_source` mit einer
skriptgesteuerten Quelle fahren: einmal ein vollständiger Scan über einen Baum,
den niemand abgelaufen ist, einmal ein Traversierungsfehler, der als
`import_errors`-Zeile ankommt und den Rest des Laufs nicht abbricht. Der
bestehende Rechte-basierte Test deckt denselben Pfad ab, überspringt sich aber
überall dort, wo Verzeichnisrechte nicht durchgesetzt werden — als root etwa.

### Die Klassifikation, die Paket 3 freigibt (2026-08-02)

Paket 2 hat festgehalten, dass eine Dateiliste keine zulässige
Arbeitsgrundlage ist. Hier ist die stellenweise Einordnung, die sie ersetzt.

**Die Rohzahl war dreimal falsch, und jedes Mal zu hoch.** Der Spike schätzte
27 Stellen für die gesamte Abstraktion. Ein Grep über den damaligen Cluster
fand 53. Ein Grep über den ganzen Crate fand 94. Die 94 enthielten Treffer in
inline `#[cfg(test)]`-Modulen und sogar Kommentarzeilen, die `read_dir` bloß
erwähnen. Nach Ausschluss von Testcode und Kommentaren bleiben **58
Produktivstellen** — und davon gehören **21** hinter `LibrarySource`.

| Klasse | Stellen | Was damit geschieht |
| --- | --- | --- |
| **A — Bibliotheksquelle** | **21** | Paket 3. Die Musikdateien unter einer Scan-Wurzel. |
| **B — app-privat** | **31** | Nie abstrahiert. Unser eigener XDG-Cache und `dirs::data_dir()/reprise`. |
| **C — fremde App-Daten** | **2** | Nicht abstrahiert, sondern plattformweise ausgeschlossen. |
| **E — der Adapter selbst** | **4** | Nichts zu tun; das *ist* die Implementierung. |

Bemerkenswert ist das Verhältnis: **mehr Stellen dürfen nicht in die
Abstraktion als hinein.** Wer den Cluster als Block umbaut, zieht 31
app-private Zugriffe in einen Vertrag, der die Musikquelle beschreibt — und
Android bekäme einen SAF-Umweg für seinen eigenen Cache.

#### A — Bibliotheksquelle (Paket 3)

| Datei | Zeilen | Ergebnis Paket 3 |
| --- | --- | --- |
| `cover.rs` | 82 | 1/1 über `LibrarySource` |
| `cover_writeback.rs` | 22, 50 | 2/2 über `LibrarySource` |
| `device_sync/snapshot.rs` | 105 | 1/1 über `LibrarySource` |
| `library/relink.rs` | 40, 77, 244 | 3/3 über `LibrarySource` |
| `library/rhythmbox_import.rs` | 501 | 1/1 über `LibrarySource` |
| `library/scanner.rs` | 32, 55, 266 | 3/3 über `LibrarySource` |
| `library/scanner_move.rs` | 56 | 1/1 über `LibrarySource` |
| `library/scanner_vanish.rs` | 167 | 1/1 über `LibrarySource` |
| `lyrics/local.rs` | 21 | 1/1 über `LibrarySource` |
| `lyrics/sidecar_write.rs` | 17 | 1/1 über `LibrarySource` |
| `queries/issues.rs` | 244 | 1/1 über `LibrarySource` |
| `queries/maintenance.rs` | 294, 376 | 2/2 über `LibrarySource` |
| `stem_separation.rs` | 164 | 1/1 über `LibrarySource` |
| `writeback_publish.rs` | 198, 207 | 2/2 über `LibrarySource` |

`cover.rs` steht in A **und** in B: Zeile 82 liest den Albumordner nach einem
Sidecar-Bild, die vier `out.exists()` prüfen Thumbnails im XDG-Cache. Genau
dieser Fall war der Grund, keine Dateiliste zu akzeptieren.

**Ergebnis Paket 3 (2026-08-02): 21/21 umgestellt, keine Ausnahme.** Der Walk
trägt jetzt optional genau die Metadaten mit, die eine Quelle beim Auflisten
ohnehin schon besitzt. Der Unix-Adapter setzt dieses Feld bewusst auf `None`:
dadurch fragt der Scanner jede Audiodatei einmal per `probe`, Nicht-Audiodateien
gar nicht, und bereits im Walk gesehene Pfade werden beim anschließenden
Verschwunden-Abgleich nicht erneut gefragt. Ein SAF-Adapter kann dagegen
Größe, Änderungszeit und stabile Identität direkt aus seiner Cursor-Zeile
mitgeben und braucht für dieselbe Datei keinen weiteren Binder-Rundlauf.

Die drei flachen Albumordner-Zugriffe verwenden die eigene objekt-sichere
Operation `read_directory` statt einer Tiefengrenze am rekursiven `walk`.
Damit bleiben ihre bisherigen Semantiken — nur unmittelbare Kinder, weder
Wurzel noch Nachfahren — ausdrücklich erhalten. Auch deren Einträge dürfen
bereits vorhandene Metadaten mittragen; Unix lässt sie weg und vermeidet so
einen vorsorglichen `stat` für jedes Kind. Fehlende Antworten und einzelne
fehlende Fakten bleiben in beiden Operationen `None`; kein Adapter erfindet
Größe, Zeitstempel oder Identität.

Die Gegenprobe über die ursprünglichen Zugriffsmuster lässt ausschließlich
die klassifizierten Stellen übrig: Klasse B in den Reprise-eigenen XDG-Daten,
Klasse C an Rhythmboxʼ eigenen Dateien und Klasse E im Unix-Adapter selbst.
Diese drei Klassen wurden in Paket 3 nicht verändert.

#### B — app-privat (nie in `LibrarySource`)

| Datei | Zeilen |
| --- | --- |
| `ai_promotion.rs` | 160 |
| `ai_staging.rs` | 106, 127 |
| `artist_portrait/cache.rs` | 31, 49 |
| `artist_portrait/mod.rs` | 92, 134 |
| `cover.rs` | 168, 199, 237, 254 |
| `cover_download.rs` | 83, 144, 208 |
| `db_grandfather.rs` | 148, 150 |
| `device_sync/podcasts.rs` | 156, 159 |
| `lyrics/lrclib.rs` | 189 |
| `podcasts/download_state.rs` | 56, 58 |
| `podcasts/downloads.rs` | 174, 182, 223, 229, 303 |
| `podcasts/pipeline.rs` | 765 |
| `podcasts/ytdlp_download.rs` | 266 |
| `remote_image/cache.rs` | 40, 113, 116 |

Podcast-Downloads liegen unter `dirs::data_dir()/reprise/podcasts`
(`downloads::default_download_root`) und sind damit app-privat, nicht
Bibliothek — auch dann, wenn `device_sync/podcasts.rs` sie später auf ein
angeschlossenes Gerät kopiert. Die Zielseite eines Sync ist ein eigener
Speicherbereich und bekommt, wenn überhaupt, einen eigenen Vertrag.

#### C — fremde App-Daten, plattformweise ausgeschlossen

`rhythmbox_import` liest Rhythmboxʼ eigene `rhythmdb.xml` und `playlists.xml`
unter `~/.local/share/rhythmbox`. Das ist weder unsere Bibliothek noch unser
Speicher, und unter Android existiert Rhythmbox nicht. Daraus folgt keine Frage,
die jede `LibrarySource` beantworten muss. Die Gegenmessung nach dem ersten
P3-Schnitt fand zwoelf verpflichtete Implementierungen bei nur zwei
Produktionsaufrufen: Die GNOME-Surface fragte einen konkreten
`UnixLibrarySource`, und der zweite Aufruf war der Core-Guard selbst. Die
versuchte `RhythmboxImportCapability` war deshalb spekulative Allgemeinheit und
wurde samt `UnsupportedSource` wieder entfernt.

Die Surface behaelt die Pfadentscheidung. Core oeffnet beide XML-Dateien
weiterhin ueber `LibrarySource::open_read` und gewinnt Anwesenheit sowie
Aenderungszeit aus `probe`. Der GNOME-Einstieg wertet nur
`LibraryPathPresence` aus: `Absent` beziehungsweise eine bestaetigte
Nicht-Datei widerlegen den konkreten Pfad, `Unknown` wird nicht als Abwesenheit
ausgegeben. `&dyn LibrarySource` bleibt der Leser, weil es die bereits echte
Speichernaht mit Produktions- und in-memory Adapter ist. Ein eigener
Rhythmbox-Trait wuerde `open_read` und `probe` duplizieren; ein konkreter
Unix-Parameter wuerde den quellenreinen Test ohne Dateisystem aufgeben.

Kein Typ-Guard verhindert einen kuenftigen nicht-GNOME-Aufrufer. Ein solcher
Aufrufer existiert nicht: Der gesamte Produktionsfluss liegt in
`preference_rhythmbox.rs`, CLI, MCP und Android verdrahten ihn nicht. Das ist
fuer den heutigen Umfang ausreichend und vermeidet eine Sperre, deren Kosten
jeder kuenftige Quellenadapter traegt.

Die erneute Messung fand neben den drei direkten `File::open` und dem
`std::fs::metadata` noch eine in der Ausgangszaehlung fehlende Stelle:
`playlists_path.is_file()`. Alle fuenf Zugriffe sind entfernt. Der bereits
vorhandene Name `prescan_rhythmdb_with_source` war nur teilweise wahr: Vor P3
lief allein die Anwesenheit der im XML genannten Musikdatei durch die Quelle,
nicht die beiden XML-Eingaben. Der in-memory Prescan-Test liest nun
`provider:/rhythmdb.xml` und `provider:/playlists.xml` ohne Dateisystempfad.
Der anfangs hinzugefuegte zweite Core-Test wies den Import vor jedem
Leseversuch ab, wenn die Quelle `Unsupported` meldete; dieser Test und der von
ihm belegte Guard wurden mit der spekulativen Faehigkeit entfernt. Die
Musikdatei, deren Anwesenheit der Prescan klassifiziert, bleibt Klasse A, weil
sie in der Bibliothek liegt.

#### E — der Unix-Adapter selbst

| Datei | Zeilen |
| --- | --- |
| `library/mounts.rs` | 75 |
| `library/source.rs` | 188, 197, 246 |

Diese Zeilen sind das Innere von `UnixLibrarySource` und `mount_point_of`. Sie
sollen `walkdir` und `lstat` benutzen; das ist ihr Zweck.

#### Woran die Einordnung hängt

Die Frage pro Stelle war nicht „liegt hier ein Pfad", sondern **wem gehört das,
worauf der Pfad zeigt**. Kommt er aus `tracks.path` oder einer Scan-Wurzel →
A. Aus `dirs::cache_dir()`/`dirs::data_dir()` unter unserem Namen → B. Aus dem
Verzeichnis einer anderen Anwendung → C.

#### Korrektur: die Zahl 21 war zu niedrig (2026-08-02, nachgetragen)

Die Messung oben zählte **direkte** Dateisystem-Aufrufe. Sie übersah zwei
Gruppen, beide bibliotheksbezogen.

**Neun indirekte Stellen in `reprise-core`**, die über hauseigene Wrapper
gingen statt über `std::fs` direkt:

| Wrapper | Aufrufstellen |
| --- | --- |
| `scanner::file_stat` | `relink.rs:92`, `relink.rs:204` |
| `scanner::file_mtime` | `relink.rs:129`, `relink.rs:267` |
| `mounts::mount_point_of` | `relink.rs:102`, `relink.rs:225`, `scanner_mount.rs:69`, `scanner_mount.rs:74`, `scanner_vanish.rs:183` |

Die vier `file_stat`/`file_mtime`-Stellen sind mit Paket 3 erledigt — `relink`
fragte jede Datei zweimal, genau wie der Scanner, und holt beide Fakten jetzt
aus einem `probe`.

**`mount_point_of` bleibt offen, und es ist keine mechanische Stelle.** Paket 1
hat `classify_missing` abstrahiert, aber die Gruppierung „was verschwindet
zusammen" blieb vollständig Unix — sie steigt pro Vorfahre mit
`symlink_metadata` auf, bis der Gerätewert wechselt. Unter SAF gibt es keinen
Mount-Punkt; die Frage müsste der DocumentsProvider-Baum beantworten, und
`mounts.rs`' eigener Modulkopf nennt btrfs-Subvolumes bereits als Fall, in dem
die Antwort auch unter Linux nur näherungsweise stimmt. **Das ist eine
Entwurfsfrage, kein Umzug.**

**Acht Stellen liegen in `reprise-gnome`**, außerhalb der Abstraktion, die
beansprucht die Musikquelle zu kapseln:

| Datei | Zeilen | Worauf |
| --- | --- | --- |
| `ui/device_sync/device_sync_effects.rs` | 335, 338, 393 | `track.source_path` |
| `ui/import_errors_view.rs` | 4, 404 | Track-Pfad, über `MetadataExt::mtime()` |
| `ui/file_open.rs` | 134 | geöffnete Datei, wird Track |
| `ui/mounts.rs` | 67 | Bibliothekswurzel |
| `ui/playback/playback_faults.rs` | 86 | `summary.path` |

Für Android hieße das: die Compose-Oberfläche baut jeden dieser Zugriffe gegen
SAF neu — oder sie ziehen vorher nach `reprise-core`. `MetadataExt::mtime()` in
einer GTK-Ansicht ist Unix-API im Frontend, und niemand hätte sie beim
P1a-Schnitt als „Präsentationslogik" gelesen.

**Korrigierter Stand:** 21 direkte + 9 indirekte = **30 bibliotheksbezogene
Stellen in `reprise-core`**, davon 25 mit Paket 3 erledigt und 5
(`mount_point_of`) als Entwurfsfrage offen; dazu **8 in `reprise-gnome`**, die
noch keinem Paket zugeordnet sind.

#### Erledigt: „weg" gegen „weiß nicht" ist ein Drei-Zustands-Vertrag

Der erste SAF-Adapter bestätigte Paket 3s offene Frage: Eine faktenfreie
`Some`-Antwort schützte `scanner_vanish::mark_vanished_with`, aber nicht
`queries::maintenance::mark_track_missing_if_current_with`, weil dessen
`is_file`-Prüfung danach trotzdem einen `missing_since`-Eintrag schrieb.

`LibrarySource::probe` liefert deshalb nun `LibraryPathPresence`: `Present`
trägt Fakten, `Absent` bestätigt Abwesenheit, und `Unknown` sagt ausdrücklich,
dass die Quelle nicht nachsehen konnte. **Nur `Absent` darf an den zwei
destruktiven Stellen bis zum Schreibzugriff gelangen.** Ein fehlgeschlagener
Binder-Aufruf wird direkt `Unknown`; die faktenfreie Ersatz-Metadatenstruktur
ist entfernt. Die übrigen Verbraucher behandeln `Unknown` konservativ und
behalten ihr bisheriges Ergebnis.

Auch Unix trennt die Zustände jetzt ehrlich: erfolgreicher `stat` ist
`Present`, `NotFound` ist `Absent`, jeder andere Fehler — etwa fehlende Rechte
oder E/A — ist `Unknown`. Damit ist ein Zugriffsfehler auch auf Linux keine
behauptete Abwesenheit mehr.

### Paket 4 umgesetzt: Bibliotheksinhalt über einen Lese-Griff (2026-08-02)

Die neue Rohmessung erfasste **84 Produktiv-E/A-Stellen in `reprise-core`**.
Davon sind **55 app-privat** (unter anderem Cache, Datenbank und
Podcast-Downloads) und bleiben bewusst außerhalb von `LibrarySource`.
**18 Stellen sind Klasse A**, weil sie Dateien in der Bibliothek selbst
betreffen; die sechs Rhythmbox-Stellen bleiben Klasse C und unverändert.

Von den 18 Klasse-A-Stellen sind in diesem Paket **vier reine Lesestellen
umgestellt**:

| Bereich | Rohstellen | Ergebnis |
| --- | ---: | --- |
| `lyrics/local.rs` | 2 | Sidecar und eingebettete Lyrics lesen über `LibrarySource::open_read`. |
| `provenance.rs` | 2 | Der reine Leser ist umgestellt; der Leser im anschließenden Tag-Schreiber bleibt mit diesem zusammen. |
| `cover.rs` | 1 | Das Ordnerbild liest seine Bytes über die Quelle. |
| `tag_mutation.rs` | 4 | Unverändert: beide `Vec<u8>`-Leser sind jeweils untrennbar mit dem folgenden Schreiben gekoppelt. |
| `library/scanner_repair.rs` | 4 | Unverändert: Lesen, Temp-Datei und abschließender Austausch bilden einen Schreibvorgang. |
| `writeback_publish.rs` | 5 | Unverändert: diese Stellen reservieren, veröffentlichen oder löschen; sie lesen keinen Bibliotheksinhalt. |

Die Messung der Verbraucher entscheidet die Signatur. Sidecar, Ordnerbild und
Tag-Reparatur wollen jeweils den ganzen Inhalt; Lofty 0.24 fordert für
`AudioFile::read_from` dagegen **`Read + Seek`**. Deshalb trägt das benannte,
konkrete `LibraryReadHandle` genau diese beiden Fähigkeiten. Weder
`std::fs::File` noch ein fremdes Trait-Objekt steht in der objekt-sicheren
`LibrarySource`-Signatur, und `open_read` hat keine Vorgabe-Implementierung.
Die Unix-Quelle verpackt einen `File`; ein Test fährt denselben Lyrics-Vertrag
über `Cursor<Vec<u8>>` an einem `content:/`-Pfad, dessen Inhalt nirgends als
Datei existiert.

**Paket 5 muss Zusicherungen entwerfen, nicht Handles umbenennen.**
`OpenOptions::create_new` ist das unteilbare Versprechen „beanspruche genau
diesen Namen oder scheitere mit `AlreadyExists`". `rename` ist das unteilbare
Versprechen, den Zielnamen durch den vollständig geschriebenen Inhalt zu
ersetzen. SAF gibt keines von beiden: `DocumentsContract.createDocument`
erzeugt bei einer Kollision einen anderen Namen, und der Provider garantiert
keinen gleichwertigen atomaren Austausch. Der Schreibvertrag muss diese beiden
Sicherheitswirkungen ausdrücklich neu formulieren; ein bloßes `create`/`move`
würde die heutigen Nicht-Überschreiben- und Ganz-oder-gar-nicht-Garantien
unbemerkt verlieren.

### Alle Inventuren zusammengeführt — und das Loch, das keine davon sah (2026-08-02)

In diesem Dokument standen drei Zählungen nebeneinander, am selben Tag von
verschiedenen Durchgängen erhoben, mit unvereinbaren Zähleinheiten und ohne
Querverweis: „27 abstraktionspflichtige Stellen" (Frage 7), „30
bibliotheksbezogene, davon 21 direkt Klasse A" (Paket 3) und „84 E/A-Stellen,
davon 18 Klasse A" (Paket 4). **Diese Zählungen sind hiermit ersetzt.**

**Alle drei waren zu niedrig, aus zwei Gründen — beide Messfehler, keine
Codefehler.**

**Erstens: der Testfilter schnitt zu früh.** Die Messskripte verwarfen jede
Zeile ab der ersten `#[cfg(test)]`-Zeile einer Datei. Die steht aber häufig an
einem einzelnen test-gated Helfer mitten im Produktivcode, nicht am Testmodul
— in `library/scanner_meta.rs` in Zeile 86 von 200+. **64 Dateien** waren so
teilweise unsichtbar. Der korrekte Schnitt greift nur bei `#[cfg(test)]`
unmittelbar vor einem `mod`.

**Zweitens, und schwerer: `lofty` öffnet Dateien selbst.**
`lofty::read_from_path(path)` und `lofty::probe::Probe::open(path)` rufen intern
`std::fs::File::open`. Kein Muster über `fs::`, `File::open` oder `.exists()`
findet sie je — sie sehen aus wie Bibliotheksaufrufe, sind aber
Dateisystemzugriffe.

Es sind **13 Stellen**, und darunter ist die folgenschwerste des ganzen Kerns:

| Datei | Zeilen | Bedeutung |
| --- | --- | --- |
| `library/scanner_meta.rs` | `read_meta`, `read_meta_content_based`, `read_meta_relaxed` | **`read_meta` ist die Metadatenlesung für jeden importierten Track**, dazu die beiden Reparatur-Fallbacks |
| `library/tag_mutation.rs` | 199, 289, 376, 486 | die einzige produktive Lofty-Speichernaht und ihre Lesehälften |
| `library/tag_mutation_guarded.rs` | 114, 201 | |
| `library/tag_edit.rs` | 125 | |
| `library/library_doctor/remote/metadata.rs` | 121 | |
| `podcasts/episode_tags.rs` | 106 | app-privat (Podcast-Downloads) |
| `provenance.rs` | 212 | Schreibseite |

**Was das heißt:** `scanner_meta::read_meta` wird für jeden Track aufgerufen,
den der Scanner importiert, und geht über `std::fs::File::open` — **unabhängig
davon, welche `LibrarySource` konfiguriert ist**. Ein SAF-gestützter Scan
liefe heute durch Traversierung, Präsenzprüfung und Klassifikation korrekt
über das Trait und würde dann an jedem einzelnen Track scheitern, sobald er
dessen Tags lesen will.

Die Abstraktion hat also ein Loch an ihrer meistbegangenen Stelle, und keine
der drei Inventuren hat es gezeigt — weil alle drei nach `std::fs` suchten und
lofty dazwischenstand.

#### Der ersetzende Stand (korrekt gemessen)

| Gruppe | Stellen |
| --- | --- |
| Präsenz und Metadaten | 44 |
| Direkte E/A | 95 |
| E/A über `lofty` nach Pfad | 13 |

Diese Rohzahlen umfassen weiterhin app-private Pfade. Maßgeblich bleibt die
**Klassenzuordnung** aus Paket 3 (Bibliothek / app-privat / fremde App / der
Adapter selbst), nicht die Rohzahl — mit der Ergänzung, dass die 13
lofty-Stellen bisher in **keiner** Klasse geführt wurden.

#### Folge für die Paketfolge

Paket 5 war als „die Schreibseite" geplant. **Vorher gehört `scanner_meta`
umgestellt** — es ist eine reine Lesestelle, sie gehörte in Paket 4 und fiel
nur durch den Messfehler heraus. `lofty::read_from_path(path)` wird zu
`AudioFile::read_from(&mut source.open_read(path)?)`, wofür der Griff seit
diesem Paket `Read + Seek` trägt.

Und beim Umstellen ist auf eine Falle zu achten, die dieses Paket bereits
einmal gestellt hat: `Probe::open(path)` setzt den Dateityp aus der Endung
vor, `Probe::new(reader)` nicht. Wer das übersieht, verliert stillschweigend
die Erkennung jeder Datei, deren Header-Schnüffelei scheitert (siehe
`lyrics/local.rs`s `synced_id3_from_source`).

### Paket 5 umgesetzt: Tag-Lesungen über die Bibliotheksquelle (2026-08-02)

Die 13 zuvor verborgenen Lofty-Pfadstellen sind vollständig klassifiziert.
Fünf reine Lesungen auf Bibliotheksdateien laufen jetzt über
`LibrarySource::open_read`; acht Stellen bleiben aus den jeweils angegebenen
Gründen absichtlich unverändert:

| Stelle der Inventur | Ergebnis |
| --- | --- |
| `library/scanner_meta.rs` — `read_meta` | **Umgestellt.** Der normale Tag- und Eigenschaftenlauf liest den Inhalt aus der aktiven Quelle. |
| `library/scanner_meta.rs` — `read_meta_content_based` | **Unverändert.** Die Reparatur liest eine Temp-Datei mit absichtlich fremder Endung und muss den Parser weiter aus dem Inhalt wählen. |
| `library/scanner_meta.rs` — `read_meta_relaxed` | **Umgestellt.** Auch der tolerante zweite Lauf liest aus derselben Quelle. |
| `library/tag_mutation.rs:199` — `apply_tag_patch_to_file` | **Unverändert.** Das Einlesen hält den Container für die unmittelbar folgende Speicheroperation. |
| `library/tag_mutation.rs:289` — `strip_and_rewrite_tag` | **Unverändert.** Der erneute Lesezugriff folgt auf eine Dateischreibung und mündet direkt in die Schreibnaht. |
| `library/tag_mutation.rs:376` — `save_loaded_tagged` | **Unverändert.** Das ist selbst die produktive Lofty-Speichernaht, keine reine Lesung. |
| `library/tag_mutation.rs:486` — `commit_tag_mutation` | **Unverändert.** Konfliktprüfung und Speichern verwenden denselben geladenen Container in einer Schreiboperation. |
| `library/tag_mutation_guarded.rs:114` — `read_tag_field_values` | **Umgestellt.** Die reine Vorablesung nimmt ihren Griff aus der Quelle. |
| `library/tag_mutation_guarded.rs:201` — `commit_guarded_tag_changes` | **Unverändert.** Prüfen, Ändern und Speichern gehören zu einer einzigen geschützten Schreiboperation. |
| `library/tag_edit.rs:125` — `read_editable_tags` | **Umgestellt.** Die reine Editor-Lesung nimmt ihren Griff aus der Quelle. |
| `library/library_doctor/remote/metadata.rs:121` — `read_remote_metadata` | **Umgestellt.** Die Aufrufer liefern Pfade vorhandener `tracks`-Zeilen; damit ist dies Bibliotheks-E/A, auch wenn die gelesenen Werte mit entfernten Metadaten verglichen werden. |
| `podcasts/episode_tags.rs:106` — `write_episode_tags` | **Unverändert.** Die Podcast-Downloaddatei ist app-privat und die Lofty-Stelle speichert Tags. |
| `provenance.rs:212` — `write_ai_tags` | **Unverändert.** Die Stelle ist die Schreibseite des Provenienz-Taggers; dessen separater reiner Leser wurde schon in Paket 4 umgestellt. |

Der Ersatz bildet Lofty 0.24 bewusst genau nach: Die Quelle öffnet zuerst den
Inhalt, danach setzt `FileType::from_path` den Probe-Typ nur dann, wenn die
Endung bekannt ist. Eine unbekannte Endung bleibt ungesetzt und führt erst in
`Probe::read` zu `UnknownFormat`. Damit bleibt auch
`import_errors::classify_lofty` gleich. Ein vollständiger Scan aus einem
`Vec<u8>`-Griff ohne Datei im Dateisystem schreibt die echten FLAC-Tags in die
Datenbank; `broken-tags.mp3`, `broken-front-id3v2-damaged-ape.mp3` und eine
unbekannte Endung ergeben über Pfad und Quelle dieselben gespeicherten
Fehlerverdikte.

#### Messung des doppelten Scanner-Zugriffs

Die zählende Quelle zeigt für eine Audiodatei ohne mitgelieferte
Walk-Metadaten **einen `probe`-Aufruf und einen `open_read`-Aufruf**; Nicht-Audio
erhält keinen von beiden. Trägt der Walk seine Metadaten bereits mit, sinkt das
auf **null `probe` und einen `open_read`**. Die Tags selbst brauchen damit in
beiden Fällen genau eine Öffnung.

Probe und Öffnung lassen sich im allgemeinen Vertrag nicht zu einem einzigen
Quellenzugriff verschmelzen: `open_read` liefert nur `Read + Seek`, während
`probe` die provider-eigenen Pfadmetadaten liefert. Androids
`openFileDescriptor` gibt keinen DocumentsProvider-Metadatensatz zurück; der
Cursor bleibt eine eigene Abfrage. Ein breiterer Rückgabetyp würde deshalb
unter SAF weiterhin beide Binder-Rundläufe ausführen und nichts einsparen.
Die wirksame Zusammenlegung liegt eine Ebene früher: Ein SAF-Walk kann die
Metadaten aus seinem ohnehin vorhandenen Cursor mittragen, worauf der Scanner
die zusätzliche Probe wie gemessen vollständig auslässt.

**Zu den Ortsangaben:** Diese Tabellen nennen **Funktionsnamen, keine
Zeilennummern**. Die frühere Fassung nannte Zeilen — und war schon falsch, als
sie geschrieben wurde, weil dasselbe Paket die Zeilen verschoben hatte, das sie
festhielt. Ein Name überlebt jede Umsortierung; eine Zahl überlebt den nächsten
Commit nicht.

Seit diesem Paket öffnet außerdem **eine einzige Stelle** Bibliotheksinhalte
für lofty: `library/tag_probe.rs`. Die Begründung — warum die Endung gesetzt
wird, warum eine unbekannte Endung die Probe absichtlich ungesetzt lässt, und
warum `read_meta_content_based` genau das Gegenteil braucht — steht dort einmal
statt in vier Kopien.

### Phase 3 belegt: ein Scan über SAF, auf dem Gerät (2026-08-03)

Gemessen auf `emulator-5554`, frisch installierte App, geleerte App-Daten,
Ordner `/sdcard/Music/Repriese` über `ACTION_OPEN_DOCUMENT_TREE` gewählt.

```
RepriseScan: Scan discovery started
RepriseScan: Scan progress: processed=1 total=unknown uri=content://…/sine.flac
RepriseScan: Scan progress: processed=2 total=unknown uri=content://…/broken-tags.mp3
RepriseScan: Scan call returned: tracks=2 added=2 updated=0 errors=0
```

Die App-Datenbank, gezogen mit `adb exec-out run-as … cat files/reprise.db`
(samt `-wal`), bestätigt es nicht nur, sondern zeigt Verhaltensgleichheit mit
dem Desktop:

| Track | `duration_ms` | `untagged` | `import_errors` | `mount_point` |
| --- | --- | --- | --- | --- |
| `sine.flac` | 1160 | 0 | — | *(leer)* |
| `broken-tags.mp3` | 52 | 1 | `unreadable_tags` | *(leer)* |

**Was damit auf Hardware belegt ist:**

- `lofty` parst über `open_read` → `File::from_raw_fd` auf einem
  SAF-Deskriptor. Pakete 4 und 5 tragen.
- `broken-tags.mp3` bekommt dasselbe Verdikt wie auf dem Desktop —
  Pass 1 scheitert, Pass 2 rettet als `untagged`, der Fehler bleibt als Hinweis
  stehen. Die Verdikt-Paritätstests aus Paket 5 haben das vorhergesagt.
- `total=unknown`: der Erstscan hat keine Schätzung und behauptet auch keine.
  Das `Option<u64>` aus Paket 2 reicht bis auf das Gerät durch.
- `mount_point` ist leer, weil die SAF-Quelle die Frage ablehnt. Die
  `mount_point`-Fähigkeit ersetzt den Plattform-Booleschen Wert erfolgreich.
- Content-URIs überleben Traversierung, Präsenzprüfung, Tag-Lesung und
  Datenbank ohne Sonderbehandlung.

**Ein Mangel, den erst das Gerät zeigt:** Der Titel lautet
`primary%3AMusic%2FRepriese%2Fsine`. Der Scanner fällt bei leerem Titel auf
den Dateistamm zurück, und der Dateistamm einer Content-URI ist die ganze
kodierte Dokument-ID. Eine Quelle muss einen **Anzeigenamen** liefern können —
SAF hat ihn in `DocumentsContract.Document.COLUMN_DISPLAY_NAME`. Das ist der
nächste Fund, den der MVP aus der Abstraktion herausdrückt, und er gehört vor
Phase 4.

### Der MVP, vollständig belegt (2026-08-03)

Gemessen auf `emulator-5554`, frische Installation, geleerte App-Daten.

| Schritt | Beleg |
| --- | --- |
| Ordner über SAF wählen | `ACTION_OPEN_DOCUMENT_TREE` auf `/sdcard/Music/Repriese` |
| Scannen | `Scan completed: added=2 updated=0 errors=0` |
| Katalog stimmt | Datenbank: `sine.flac` mit Tags, `broken-tags.mp3` als `untagged` mit `unreadable_tags` — dieselben Verdikte wie unter Linux |
| Album benannt | `album = "Repriese"` (Anzeigename des übergeordneten Dokuments), nicht `document` |
| Liste anzeigen | beide Tracks mit Dauer |
| Abspielen | `AudioTrack: stop(16): called with 51200 frames delivered` — bei 1160 ms und 44,1 kHz sind 51.156 Frames der ganze Track |
| Neustart | nach `force-stop` steht die Liste wieder da, **ohne zweiten Scan** |

Der letzte Punkt ist der wichtigste für die Abstraktion: Die Tracks kommen aus
dem Katalog, nicht aus einem erneuten Baumlauf. Ein Kaltstart, der den Baum
über Binder neu abliefe, wäre genau die Kosten, die die fünf Pakete zu
vermeiden gelernt haben.

Wird die Berechtigung entzogen oder der Ordner entfernt, zeigt die App
ausdrücklich „Access may have been revoked or the folder may have been
removed" und bietet neu zu wählen an — **keine stille leere Bibliothek und
kein Missing-Verdikt.** Das ist die Oberfläche, an der ein Mensch den
Unterschied zwischen `Absent` und `Unknown` bemerken würde.

## Frage 8 — Die Umzugserkennung, und was Tauri daran ändert (umgesetzt 2026-08-02)

**Status: umgesetzt.** `file_stat` liefert jetzt die echte Größe getrennt von
`Option<(device, inode)>`; `MoveLookup` trägt dieselbe Identität als ein
einziges optionales Tupel. Ohne Identität entfällt nur Strategie 1, während
der Fingerabdruck aus Strategie 2 weiterläuft. Der Nicht-Unix-Zweig erfindet
deshalb kein `(0, 0)` mehr. Die bestehenden Linux-Fälle bleiben unverändert:
bei vorhandener Identität läuft weiterhin zuerst die Inode-Strategie, danach
erst der Fingerabdruck.

Die Korrektur zu Frage 7 lässt eine Frage übrig: `scanner.rs` erkennt eine
verschobene Datei an `(device, inode)`, und SAF kennt keine Inodes. Nachgelesen
ist die Lage besser **und** schlechter als erwartet.

### Es gab bereits einen zweiten Weg — er war nur verriegelt

`find_move_candidate` versucht zwei Strategien: erst `(device, inode)` für ein
`rename` auf demselben Dateisystem, dann einen **Fingerabdruck aus Titel,
Interpret, Album, Dauer und Dateigröße** für den Fall „kopiert und gelöscht",
bei dem sich der Inode ändert, Inhalt und Tags aber nicht.

Die zweite Strategie braucht nichts, was SAF fehlt: Tags und Dauer liefert
`lofty` über ein Handle, die Größe gibt der DocumentsProvider als Spalte.

Vor dem Umbau war sie aber nicht einzeln erreichbar. Die Aufrufstelle
(`scanner.rs:444`) lautete
`match (device, inode) { (Some(device), Some(inode)) => …, _ => None }` —
**fehlte die Identität, entfielen beide Strategien.** Der Kommentar darüber
begründete das, und die Begründung war für einen fehlgeschlagenen `stat`
richtig: Dann war auch `file_size` ein Platzhalter-`0`, und der Fingerabdruck
hätte gegen Müll verglichen.

**Für SAF galt diese Begründung nicht.** Dort ist die Größe echt, nur die
Identität fehlt. Die Bedingung, die das Gate motivierte, traf also nicht zu —
und damit war der Umbau klein und klar umrissen: Das damalige
`Option<(size, device, inode)>` musste sich in „Größe bekannt" und „Identität
bekannt" trennen, sodass eine Quelle mit Größe, aber ohne Identität, Strategie
2 allein benutzen kann. Die Spec hat das im Trait bereits so modelliert —
`residence_token()` gibt `Option<i64>` zurück, getrennt von allem anderen.

### Derselbe Umbau räumte eine Annahme weg, die Tauri kippte

`file_stat`s damaliger Doc-Kommentar sagte über den Nicht-Unix-Zweig:

> off Unix identity degrades to `(0, 0)` — **never reached at runtime**

Das stimmte, solange die App nur auf Linux lief. **Ein Tauri-Desktop auf
Windows hätte die Aussage falsch gemacht.** Dort hätte `stat` nicht
fehlgeschlagen, sondern `Some((size, 0, 0))` geliefert: Das Gate hätte
geöffnet, und Strategie 1 hätte mit `WHERE device = 0 AND inode = 0` jede auf
Windows gescannte Zeile getroffen.

Genau diesen Fall beschrieb derselbe Kommentar als den Fehler, den Stage 3
Task 1 beseitigt hatte („could have coincidentally matched an unrelated
`(device, inode)` of `(0, 0)`"). Der damalige Fix griff jedoch nur, wenn
`stat` **fehlschlug**. Auf Windows wäre das nicht passiert.

**Wie weit hätte der Schaden getragen?** Nicht weit, aber er wäre echt
gewesen. Der Kandidatenfilter verlangt, dass der alte Pfad verschwunden oder
die Zeile bereits als fehlend markiert ist, und bei mehreren Treffern greift
die Mehrdeutigkeitssperre („not guessing"). Falsch wäre es also erst geworden,
wenn **genau eine** solche Zeile existiert hätte — dann hätte die Historie
eines fremden Titels am neuen gehangen. Selten, still und nicht rückgängig zu
machen.

### Folge

Der gemeinsame Core-Umbau für beide Plattformen ist umgesetzt: Die bekannte
Größe öffnet die Umzugserkennung, die optionale Identität nur deren erste
Strategie. Damit

- kann ein späterer SAF-Adapter Strategie 2 ohne Identität benutzen; die
  `LibrarySource`-/Handle-Anbindung selbst ist weiterhin nicht Teil dieses
  Umbaus,
- benutzt Windows Strategie 2 statt einer Abfrage auf `(0, 0)`,
- bleibt Linux samt Strategie-Reihenfolge unverändert.

Auf Android fehlt weiterhin die `rename`-Erkennung. Dafür braucht es einen
eigenen Entwurf für den Umgang mit stabilen
DocumentsProvider-Dokument-IDs; der hier freigeschaltete Fingerabdruck erkennt
nur den vorhandenen Kopieren-und-Löschen-Fall.

## Frage 4b — Tragen Content-URIs die Pfad- und SQL-Annahmen? (2026-08-03)

Diese Prüfung gehört zu Phase 1 des Android-MVP-Plans. Sie trennt die drei
Annahmen ausdrücklich, weil A1 in diesem Lauf nicht ausgeführt werden sollte.

### A1 — Seek auf einem SAF-Deskriptor

**Urteil: TRÄGT.** Gemessen am 2026-08-03 auf `emulator-5554`, Android-API 36,
mit einem echten über `ACTION_OPEN_DOCUMENT_TREE` gewählten Ordner unter
`/sdcard/Music/Repriese` und dem Anbieter
`com.android.externalstorage.documents`:

```
A1 descriptor probe: bytesRead=64 readError=null seekSucceeded=true
seekError=null bytesReadAfterSeek=32 readAfterSeekError=null bytesMatch=true
```

Der von `ContentResolver.openFileDescriptor(uri, "r").detachFd()` übergebene
Deskriptor, in Rust mit `File::from_raw_fd` übernommen, liest, springt, und
die Bytes nach dem Sprung stimmen mit denen an derselben Stelle aus dem ersten
Lesen überein. `LibraryReadHandle`s Zusage `Read + Seek` ist damit auf echtem
Gerät erfüllt, und `lofty` kann direkt darauf parsen — ohne Vorabkopie in den
App-Cache, die der Plan als Ausweichweg vorgesehen hatte.

**Was damit nicht geprüft ist:** nur der lokale Anbieter
`externalstorage`. Ein Netzwerk-Anbieter (Drive, SMB) darf laut
SAF-Vertrag eine Pipe liefern, und eine Pipe kann nicht springen. Für die
Musikbibliothek eines Telefons ist das der Randfall, nicht der Normalfall —
aber eine Quelle, die ihn treffen kann, braucht dort die Vorabkopie. Der
Befund gilt für lokalen Speicher, und nur dafür.

### A2 — `Path` auf einer realistischen Content-URI

**Urteil: TRÄGT für die beiden geprüften Operationen.** Der Test verwendet
unverändert:

```text
Baum:  content://com.android.externalstorage.documents/tree/primary%3AMusic
Datei: content://com.android.externalstorage.documents/tree/primary%3AMusic/document/primary%3AMusic%2Fsong.flac
```

`Path::starts_with` liefert `true`: Die Komponenten der Baum-URI sind ein
echtes Präfix der Komponenten der Datei-URI. Das kodierte `%2F` bleibt zwar
Teil der letzten Dokument-ID-Komponente und wird nicht zu einem
Pfadtrennzeichen, liegt aber erst hinter diesem gemeinsamen Präfix.

`Path::extension()` liefert `Some("flac")`: Der Punkt vor `flac` ist in der
letzten Komponente nicht kodiert. Der Test behauptet bewusst nicht, dass
`Path` die URI dekodiert oder die Struktur innerhalb der Dokument-ID kennt.

A2 fällt damit nicht; wegen A2 muss der Plan nicht neu geschnitten werden.

### A3 — `LIKE`-Vorfilter unter einer URI-Wurzel

**Urteil: TRÄGT.** Eine migrierte In-Memory-Datenbank enthält die Datei-URI
oben sowie eine ähnlich aussehende URI aus einem anderen Baum.
`scanner_vanish::candidates_under_root` gibt für die Baum-URI genau die
richtige Track-Zeile zurück.

Der `%` in `%3A` wird von `playlists::escape_like` als Literal für
`LIKE ? ESCAPE '\'` gebunden; das angehängte `/%` bezeichnet danach die
Nachfahren. Der anschließende autoritative `Path::starts_with`-Filter hält die
ähnlich aussehende fremde Baum-URI zusätzlich draußen. Der Test hält das
Escaping außerdem unabhängig als wörtliches `primary\%3AMusic` fest, damit der
autoritative Nachfilter eine Regression des SQL-Musters nicht verdecken kann.
Temporäre Mutationen wurden für beide Schichten rot beobachtet: Ohne den
Nachfahrenanteil `/%` war die Kandidatenliste leer; ohne das Prozent-Escaping
wich der tatsächliche Mustertext vom erwarteten Literal ab. Nach beiden
Wiederherstellungen liefen A2 und A3 grün.

Damit sind A2 und A3 festgehalten. A1 bleibt bis zum getrennten Emulator-Lauf
offen; aus diesem Durchgang folgt kein Urteil über die Seekbarkeit eines
echten Provider-Deskriptors.

## Phase 5 — Bilanz der Storage-Abstraktion (2026-08-03)

Der MVP hat die Bibliothek über eine echte SAF-Quelle gescannt. Sein Ertrag ist
nicht, dass `LibrarySource` als Ganzes bestätigt wäre. Vier der fünf
Storage-Pakete trugen für den geprüften lokalen DocumentsProvider; eine
Signatur musste geändert werden, und vier weitere Dateisystemannahmen lagen
außerhalb der Signaturen an Stellen, die ihre Quelle gar nicht befragen
konnten.

### Was von den fünf Paketen getragen hat

| Paket | Befund am zweiten Quelltyp |
| --- | --- |
| 1 — Aufenthalt und Erreichbarkeit | **Trägt.** Eine SAF-Quelle kann einen stabilen Tree-Token liefern; `reachability` bleibt ein Vergleich opaker Werte. Nicht getragen hat die fremde Vorbedingung des Aufrufers: `scan_folder_inner` verlangte mit `is_absolute`, was nur der Unix-Vorfahrenlauf braucht. Die Zusicherung sitzt jetzt bei `UnixLibrarySource`. |
| 2 — Traversierung | **Trägt.** Der Rust-Adapter leitet den stromorientierten Walk aus wiederholten `listChildren`-Aufrufen ab, trägt Fehler in Reihenfolge weiter und stoppt ohne den Baum zu materialisieren. Der Gerätelauf bestätigte außerdem den unbekannten Nenner beim ersten Scan; ein Vorzähl-Lauf wäre unter Binder genau die falsche Optimierung gewesen. |
| 3 — Präsenz und Metadaten | **Trägt nicht in der ursprünglichen Signatur.** `Option<LibraryPathMetadata>` musste gleichzeitig „bestätigt nicht vorhanden" und „Binder-Aufruf gescheitert" ausdrücken. Der erste Adapter verrenkte die Fehlerseite deshalb zu einer faktenfreien Präsenz. Erst `LibraryPathPresence::{Present, Absent, Unknown}` bildet die Quelle ab; nur `Absent` darf einen Missing-Schreibzugriff lizenzieren. Das ist die Signatur, die der MVP korrigiert hat. |
| 4 — Lese-Griff | **Trägt für den geprüften lokalen Provider.** Der echte Deskriptor liest und sucht, Rust übernimmt ihn ohne Cache-Kopie, und der Gerätelauf parst darüber Audio. Das Urteil gilt nicht pauschal für Netzwerk-Provider, die eine nicht seekbare Pipe liefern dürfen. |
| 5 — Tag-Lesung | **Trägt.** Lofty liest Tags und Eigenschaften über denselben Griff; `sine.flac` und `broken-tags.mp3` erreichten auf dem Gerät dieselben Datenbank- und Fehlerverdikte wie im Kern. Die explizite Endungs-Vorsaat bleibt nötig, weil ein Griff allein den Container-Typ nicht kennt. |

Die Paketgrenzen für Traversierung, Metadaten, Griff und Parser waren damit
brauchbar. Der Fehler lag in der Beweisreihenfolge: Der Vertrag wurde gegen
Unix und gegen Testdoppelgänger verbreitert, bevor ein zweiter Quelltyp ihn
unter seinen eigenen Fehlern und Bezeichnern benutzen musste.

### Fünf Befunde, die kein selbstgeschriebener Doppelgänger gezeigt hat

1. `is_absolute` stand am Scanner-Eingang, obwohl nur der Unix-Adapter eine
   absolute Wurzel für seinen Vorfahrenlauf braucht. Eine Content-URI ist für
   `Path` nicht absolut.
2. `probe` hatte mit `Option` nur zwei Ergebnisse. Ein echter Binder-Aufruf
   hat mindestens drei: vorhanden, bestätigt abwesend und nicht feststellbar.
3. Der Scanner trug einen Plattform-Booleschen Wert, um
   `mount_point_of` zu überspringen. Die eigentliche Frage lautet, ob diese
   Quelle für dieses Objekt eine gemeinsame Ausfallgrenze benennen kann.
4. Der Titelfallback nahm `file_stem` aus dem Bezeichner. Bei SAF ist das die
   kodierte Dokument-ID, nicht `COLUMN_DISPLAY_NAME`.
5. Der Albumfallback nahm `parent().file_name()` aus demselben Bezeichner.
   Für `content://…/document/…/broken-tags.mp3` ergibt das wörtlich
   `document`, nicht den Anzeigenamen des übergeordneten Dokuments.

Diese fünf Fehler waren gegen die Doppelgänger unsichtbar. **Ein
Doppelgänger, den wir selbst schreiben, scheitert nicht; er antwortet nur.**
Er liefert auf Nachfrage genau den Token, die Metadaten und den Baum, die sein
Test erwartet. Er hat keinen abgebrochenen Binder-Rundlauf, keinen
DocumentsProvider ohne Mount-Begriff und keinen opaken Bezeichner, solange wir
ihm diese Eigenschaften nicht vorher einbauen. Damit kann er bekannte
Zusicherungen festhalten, aber nicht belegen, dass die Fragen vollständig oder
am richtigen Besitzer liegen.

### Wie der Schnitt beim nächsten Mal anders beginnt

Die zweite Quelle kommt künftig **vor** der gemeinsamen Abstraktion. Zuerst
wird der schmalste echte vertikale Lauf in beiden Quellen gebaut; danach wird
nur das gemeinsam benannte Verhalten herausgezogen. Für die Leseseite hätte
das bedeutet: ein realer SAF-Walk mit Providerfehler, Metadaten, Griff und
Anzeigenamen, bevor `LibrarySource` seine endgültigen Methoden erhält. Dann
wären `Unknown`, `mount_point`, `display_name` und `container_name` aus zwei
Implementierungen entstanden statt nachträglich aus vier Reparaturen.

Das ist auch die Reihenfolge für die noch offenen Pakete:

- **Schreibseite:** Erst auf SAF konkret beweisen, wie Namensreservierung,
  Kollision, temporäre Veröffentlichung, Austausch und Aufräumen sicher
  funktionieren. Danach den gemeinsamen Vertrag mit Unix schneiden.
  `create_new` und atomarer `rename` dürfen nicht als Methoden vorgegeben
  werden, wenn der zweite Provider diese Zusicherungen nicht besitzt.
- **Watcher:** Erst das Verhalten einer zweiten Quelle bauen. SAF hat kein
  allgemeines `notify`-Gegenstück; möglich sind Provider-Beobachter,
  periodischer Abgleich oder gar keine Push-Fähigkeit. Erst aus diesem
  konkreten Ergebnis darf eine optionale Watcher-Fähigkeit oder ein
  Scan-Fallback abstrahiert werden.

Für beide gilt daher: **zweite Quelle zuerst, dann abstrahieren.** Ein weiterer
Vertrag gegen einen Doppelgänger würde erneut nur zeigen, dass unser eigener
Antwortgeber die Fragen erfüllt, die wir ihm vorher gegeben haben.

## Paket 2 — Bilanz von `PlaybackBackend` am zweiten Backend (2026-08-03)

Der Android-Player läuft nun über eine echte zweite Implementierung von
`PlaybackBackend`: Media3 führt die Befehle aus, der Ereignisadapter meldet
Zustand, Position, Dauer und Abschluss zurück, und eine Rust-eigene Sitzung
bindet beides an `reprise_core::queue::Queue`. Die Compose-Seite enthält keine
Warteschlangenentscheidung. Sie übergibt beim Antippen die eingefrorene
aktuelle Liste samt Cursor und rendert danach nur Snapshots.

Das Urteil ist zweigeteilt. **Als Plattformvertrag war `PlaybackBackend`
deutlich besser vorbereitet als `LibrarySource`.** Keine Core-Signatur musste
für Media3 geändert werden, GStreamer und der Desktop blieben unberührt. Als
vollständige Anwendungsnaht war es dagegen nicht fertig: Der Vertrag besitzt
den Player, aber nicht die Bindung zwischen Playerereignissen und der
Core-Warteschlange. Diese Bindung musste für Android als schmale Rust-Sitzung
neben dem Trait gebaut werden.

### Was am zweiten Backend getragen hat

| Teil des Vertrags | Befund mit Media3 |
| --- | --- |
| Start und Grundtransport | **Trägt.** `play_uri`, `seek_to`, `set_volume` und `stop` bilden Media3 direkt ab. Eine `content://`-Adresse erreicht unverändert `MediaItem.fromUri`; die String-Signatur konnte sie tragen, obwohl der Doc-Kommentar bisher nur `http`, `https` und `file` nennt. |
| Zustands- und Positionsereignisse | **Trägt.** `StateChanged` und `Position { position_ms, duration_ms }` beantworten genau die Fragen der mobilen Oberfläche. Die Dauer kommt aus demselben Ereignis; Kotlin fragt den Player dafür nicht ab. |
| Abschluss und lückenloser Übergang | **Trägt mit einer Signaturspannung.** `TrackFinished` und `AdvancedToNext` trennen den gewöhnlichen Core-Vorlauf vom bereits hörbaren Media3-Handoff richtig. `set_next` funktioniert technisch, aber sein Parameter und Kommentar heißen „Pfad“, während Android eine Content-URI vorfüttert. |
| Stream-Generationen | **Trägt besonders gut.** Media3s Application Looper serialisiert Start, Generationswechsel und Listener. Dadurch kann jedes Ereignis die Generation vom Produktionszeitpunkt tragen, und verspätete Ereignisse werden mit derselben strikten Kleiner-als-Regel verworfen wie beim Linux-Pfad. |
| Übergänge und fehlende Fähigkeiten | **Trägt.** Das Trait erlaubt ausdrücklich, `Crossfade` als `Gapless` zu behandeln. Audioeffekte und Spektrum melden Nichtunterstützung, statt erfundene Media3-Funktionen vorzutäuschen. |
| Ereignisübergabe | **Trägt hinter einem Adapter.** Der Rust-Closure-Konstruktor ist nicht UniFFI-fähig. Ein benanntes, Rust-eigenes `PlaybackEventBridge` nimmt die flachen Kotlin-Ereignisse an und ruft den unveränderten Closure intern auf; Reihenfolge und Generationspaar bleiben erhalten. |

### Welche Signaturen sich verrenkt haben

Zwei Stellen sind nicht so geschnitten, wie man sie nach der zweiten
Implementierung neu schneiden würde:

1. **`set_next(Option<&str>)` meint einen lokalen Pfad.** Der Aufrufer braucht
   aber dieselbe Ortswahl wie beim Start: lokaler Pfad oder URI. Android musste
   den String als URI interpretieren, während Linux ihn weiterhin als Pfad
   liest. Das Verhalten ist eindeutig implementiert, aber der Typ sagt es
   nicht. Ein gemeinsames `PlaybackLocation::{Path, Uri}` für Start und
   Vorfütterung würde die Zusicherung beim Besitzer benennen.
2. **`toggle_pause` ist ein Oberflächenbefehl, kein vollständiger
   Transportvertrag.** Der Compose-Knopf passt natürlich darauf. Eine
   MediaSession erhält dagegen getrennte, idempotente Befehle „Play“ und
   „Pause“. Der Kotlin-Adapter muss deshalb zuerst Media3s `playWhenReady`
   ansehen und nur bei einer wirklichen Zustandsänderung `toggle_pause`
   aufrufen. `set_playing(bool)` oder getrennte `play`-/`pause`-Methoden wären
   die bessere Plattformsignatur.

Die Event-Closure ist ebenfalls nicht sprachübergreifend, aber sie zwang keine
falsche Semantik in das Trait: Der benannte FFI-Adapter übersetzt nur den
Transport. Würde Android von Anfang an als gleichrangige Plattform geplant,
wäre ein benanntes `PlayerEventSink`-Objekt die einfachere Konstruktornaht;
die `PlayerEvent`- und `StreamEvent`-Typen selbst würden unverändert bleiben.

### Was außerhalb des Traits fehlte

`PlaybackBackend` entscheidet absichtlich nicht, welcher Track folgt. Diese
Entscheidung liegt in `reprise-core::queue::Queue`, während die allgemeine
Bindung von Queue, Backend und Ereignissen heute in `reprise-runtime` und in
der GNOME-Steuerung lebt. `reprise-android-ffi` darf laut Architektur nur von
Core abhängen und konnte diesen Runtime-Transport daher nicht wiederverwenden.

Android besitzt nun eine schmale Rust-Sitzung, die ausschließlich Core-
Entscheidungen ausführt:

- Antippen: `Queue::set_tracks` mit der ganzen aktuellen Liste und dem Cursor;
- natürliches Ende und gapless Handoff: `Queue::advance_auto`;
- Next: `Queue::next_manual`;
- Previous: `playback_history::resolve_previous` über den tatsächlich
  gehörten Verlauf, mit `Queue::jump_to_order_position` nur für noch gültige
  Kontextpositionen.

Damit liegt keine Reihenfolge- oder Fortschaltlogik in Kotlin. Trotzdem ist
die Sitzung ein Hinweis auf den nächsten besseren Schnitt: Eine
frontend-neutrale Playback-Sitzung sollte neben der Queue im gemeinsamen
Rust-Layer liegen und `PlaybackBackend` plus `StreamEvent` besitzen. Dann
würden GNOME, Android und ein späteres KDE-Frontend dieselbe Bindung benutzen,
nicht nur dieselben Einzelteile.

### Expliziter Vergleich mit den fünf Storage-Paketen

Die Storage-Reihe hatte fünf Pakete gegen selbstgeschriebene Doppelgänger
bewiesen. An der ersten realen SAF-Quelle fielen fünf vorher unsichtbare
Annahmen: `is_absolute` am falschen Besitzer, ein zweistufiges `probe` ohne
„unbekannt“, ein Plattform-Bool statt einer Mount-Fähigkeit, `file_stem` als
Anzeigename und der Pfad-Elternname als Containername.

Beim Playback war das Ergebnis besser:

- **keine** Core-Signatur musste für das zweite Backend geändert werden;
- die Befehlsseite, Ereignisvarianten, Dauer und Generationen passten direkt;
- zwei Signaturen spannten (`set_next` als Pfad und `toggle_pause` als einziger
  Play/Pause-Befehl), ohne eine falsche Core-Aussage zu erzwingen;
- eine fehlende gemeinsame Schicht wurde sichtbar: die Playback-Sitzung über
  Queue, Backend und Ereignissen.

`PlaybackBackend` war also **besser vorbereitet, aber nicht vollständig**.
Der wichtigste Grund ist nicht, dass seine Tests bessere Doppelgänger hatten.
Der Vertrag entstand bereits aus einem echten, asynchronen GStreamer-Backend
und seinem produktiven GTK-Verbraucher. Seine Kommentare kodierten konkrete
Fehlerfälle — späte Ereignisse, gapless Handoff und erlaubte Degradation —
statt nur Antworten eines selbst gebauten Gegenübers. Außerdem ähneln sich
GStreamer und Media3 in dieser Domäne stärker als Unix-Dateisystem und SAF:
beide spielen eine Adresse ab, melden Zustand und Position und besitzen einen
seriellen Ereignisstrom.

Die Storage-Lehre bleibt trotzdem bestehen: Erst das zweite reale Gegenüber
zeigt, ob die Fragen am richtigen Besitzer liegen. Hier zeigte es keinen
kaputten Plattformvertrag, sondern zwei zu enge Signaturen und die fehlende
gemeinsame Orchestrierungsschicht. Genau das ist der Ertrag dieses Pakets.

Diese Bilanz beruht auf den Rust- und Kotlin-Seams, der Android-
Cross-Kompilation und den generierten Bindings. Der getrennte Gerätelauf wurde
für dieses Paket ausdrücklich nicht ausgeführt und wird hier nicht behauptet.

## Paket 3 — Browse-Oberfläche als Messung von `reprise-view` (2026-08-03)

Die Android-Oberfläche besitzt nun drei Reiter für Titel, Alben und
Interpreten. Jede Änderung des Suchfelds — einschliesslich des leeren Texts —
geht unverändert an `reprise-core`. Ein Album wird mit Titel und Albuminterpret
geöffnet; Core bestimmt Zugehörigkeit und kanonische Disc-/Track-Reihenfolge.
Beim Antippen friert Android genau die sichtbare Titel- beziehungsweise
Albumliste samt Cursor für die Core-Warteschlange ein. In Kotlin gibt es keine
Abfrage, Sortierung, Filterung, Gruppierung oder Albumregel.

Das ist eine schmalere Aussage als „die Browse-Präsentation ist teilbar“. Der
Vergleich trennt deshalb Entscheidungsregeln von Transport- und Widgetarbeit.

### Reibungen im direkten Vergleich

| Reibung | Was `reprise-gnome` besitzt | Was Compose stattdessen tat | Urteil |
| --- | --- | --- | --- |
| Core-Typen an einer Sprachgrenze | GNOME kann `Track`, `AlbumSummary` und `ArtistSummary` als Rust-Typen direkt verwenden. Die aktuellen drei mobilen Listen haben dort kein gleiches Presenter-Modul; `query_albums` und `query_artists` haben zurzeit keinen GNOME-Aufrufer. | UniFFI braucht flache `TrackRow`, `AlbumRow` und `ArtistRow`; der Adapter benennt den gespeicherten SAF-Ort ehrlich als `representative_uri` und projiziert nicht benötigte Desktop-Statistiken weg. Kotlin bildet diese Records nochmals auf unveränderliche Oberflächenwerte ab. | **Keine geteilte Präsentationsschicht.** Die Form ist eine notwendige FFI-/Oberflächengrenze. Ein gemeinsamer Presenter würde Transportunterschiede nur verstecken. |
| Album-/Interpreten-Gruppierung und Listenordnung | GNOME hat diese Zusammenfassungslisten nach der kanonischen Trackoberfläche nicht mehr; seine Album- und Interpretensichten sind Core-Scopes eines `BrowserPlace`. Die weiterhin vorhandenen `query_albums` und `query_artists` besitzen Gruppenschlüssel, Ausschluss fehlender Tracks und stabile Ordnung, werden aber nicht von GNOME präsentiert. | `listAlbums` und `listArtists` reichen genau diese Core-Reihenfolge durch UniFFI und Kotlin bis `LazyColumn`; Compose gruppiert und sortiert nichts nach. | **Echte gemeinsame Entscheidungsregeln in Core, kein Beleg für gemeinsame Präsentation.** Dass nur Android die zwei Listen zeigt, ändert nicht den Besitzer ihrer Gruppierung und Ordnung. |
| Albumidentität, Inhalt und Reihenfolge | `BrowserPlace::fresh_album` trägt dieselbe Identität aus Album und Albuminterpret. `TrackListModel` fragt die so eingeschränkte Core-Quelle mit der aktuellen View-Sortierung ab; Core entscheidet die Mitgliedschaft und führt die gewählte Ordnung aus. GNOMEs Zeilenaktivierung friert danach diese sichtbare Ordnung ein. | `listAlbumTracks(album, albumArtist)` liefert fertige Zeilen in kanonischer Disc-/Track-Reihenfolge, weil die kleine Oberfläche keine wählbare Sortierung besitzt. Compose zeigt sie in genau dieser Folge. Der zuerst erwogene Tracknummern-Weg hätte mehrere Discs vermischt und fiel im Core-Test rot. | **Echte gemeinsame Entscheidungsregel, aber bereits richtig in Core.** Identität und Mitgliedschaft sind gemeinsam; auch die gewählte Ordnung wird in Core ausgeführt. Ob eine Oberfläche die kanonische oder eine vom Benutzer gewählte Ordnung verlangt, ist ihr Eingabevertrag. Ein zusätzlicher `reprise-view`-Typ ist dafür nicht belegt. |
| Suche, Sortierung und leerer Suchtext | `TrackViewState` hält Suchtext und Sortierung; `TrackListModel` gibt sie an die Core-Abfragen. `view_session::wire_search` verzögert nur die teure Neuladung um 200 ms und hält den sichtbaren Text sofort fest. Core entscheidet Treffer und Reihenfolge. | `OutlinedTextField` reicht jeden literalen Wert sofort an `searchTracks` weiter. `""` geht ebenfalls durch Core und bedeutet dort die gesamte vorhandene Bibliothek in Titelreihenfolge. Der Kotlin-Test lässt das Port-Doppel absichtlich eine nicht passende Zeile liefern und beweist so, dass Kotlin nicht nachfiltert. | **Treffer, Ordnung und Bedeutung von leer gehören gemeinsam in Core; Eingabeverzögerung gehört zur Oberfläche.** Der Core-Façade ist belegt, ein geteilter Debounce nicht. |
| Ausführbare Query statt SQL-Baustein | GNOME besitzt mit `TrackListModel::set_query_browsed` einen GTK-nahen Aufrufer für Quelle, Spaltensortierung, Richtung, Text, Facetten, Queue und AI-Ausschluss. Er kann `build_track_query` nicht als vollständige Anwendungsschnittstelle behandeln. | Android hätte dieselben GTK-geprägten Parameter erfinden müssen. Stattdessen kamen `query_library_text_search` und `query_album_tracks` als enge Core-Façaden hinzu. | **Gemeinsame Anwendungsentscheidung in Core.** Wiederverwendet werden soll die benannte Abfrage, nicht GNOMEs umfassender Tabellenzustand und nicht dessen Parameterliste. |
| Besitz und Fehlerübersetzung | Im selben Prozess leiht GNOME `&str` und behandelt `rusqlite::Error` direkt, meist durch Loggen und einen leeren beziehungsweise unveränderten Modellzustand. | UniFFI liefert besessene Strings; Rust boxt den Bibliothekszustand, leiht die Werte intern und hüllt SQLite-Fehler in `LibraryError::Query`. Compose wandelt Fehler in sichtbare Aktionsmeldungen um. | **Oberflächen- und transportspezifisch.** Besitz, Fehlerhülle und sichtbare Meldung belegen keine gemeinsame View-Schicht. |
| Mehrere Abfragen statt eines Browse-Snapshots | GNOME rendert jeweils einen `BrowserPlace` über ein langlebiges `TrackListModel`; es braucht keinen atomaren Snapshot dreier paralleler Reiter. Album- und Interpretenzusammenfassungen bilden dort aktuell keine solche Dreieroberfläche. | Beim Laden koordiniert Android vier Aufrufe: leere Titelsuche, Alben, Interpreten und später die Tracks eines geöffneten Albums. Die Records werden synchron zu einem `LibraryScreenState.Browse` zusammengesetzt. | **Noch kein Beleg für eine gemeinsame Schicht.** Die Dreiteilung ist ein mobiles Produkt-/Layoutdetail. Ein gebündelter Core-Snapshot wäre erst gerechtfertigt, wenn getrennte Aufrufe nachweislich inkonsistente Zustände oder relevante FFI-Kosten erzeugen. |
| Welcher Tap welche Warteschlange startet | GNOME baut für eine Zeilenaktivierung die IDs der aktuell sichtbaren Sortier-/Filteransicht und den Cursor und übergibt beides an `PlayerController::play_from_view`. Der Player friert diesen Kontext ein. | `PlaybackSelection` trägt die gerade gerenderte Liste samt `startIndex`. In der Titelsuche ist das die Trefferliste; im Albumdetail ausschliesslich `selectedAlbum.tracks`. Die Rust-Sitzung setzt daraus die Core-Queue. | **Echte gemeinsame Entscheidungsregel.** „Gerenderter Kontext plus Cursor“ sollte frontend-neutral benannt bleiben. Die Kotlin-Datenkopie ist jedoch nur der Adapter zur bereits erkannten gemeinsamen Playback-Sitzung, kein neuer Browse-Presenter. |
| Navigation und Wiederherstellung | GNOME besitzt `BrowserPlace`, `TrackViewState`, `nav_history`, `view_session` und `view_state_memory`: Sammlung, Suche, Facetten, Sortierung, stabiler Anker, Auswahl und Fokus werden als Ort erfasst und wiederhergestellt. GTK projiziert daraus Scrollwert und Widgetfokus. | Android hält aktiven Reiter, Suchtext und geöffnetes Album mit `remember(state)` und setzt sie nach einem neuen Bibliothekszustand zurück. Es gibt in diesem Paket noch keine Prozess- oder Navigationswiederherstellung. | **Beleg für einen gemeinsamen Orts-/View-State, nicht für gemeinsame Widgets.** Sammlung, Verfeinerungen und Wiederherstellungssemantik sind teilbar; Tab, Back-Schaltfläche, Scrollanker und Fokusadapter bleiben oberflächenspezifisch. |
| Anzeigetexte und Dauerformat | GNOME verwendet seine katalogisierten, reicheren Zeilenprojektionen. | Compose formatiert Dauer, „N tracks“, unbekannte Interpreten und Leerzustände lokal. | **Kein Fall für eine gemeinsame Schicht.** Das sind kleine Darstellung und spätere Lokalisierungsarbeit, keine Browse-Regeln. |

### Die 500-Zeilen-Grenze ist ein Produktbefund

`query_library_text_search` und `query_album_tracks` benutzen
`query_track_window` mit `MAX_WINDOW_LIMIT = 500`. Ihre Android-Schnittstellen
geben weder Gesamtzahl noch Offset, Cursor oder Folgeseite zurück. Eine Suche
mit mehr als 500 Treffern und ein Album mit mehr als 500 Tracks werden daher
still abgeschnitten. `query_albums` und `query_artists` haben umgekehrt gar
keine Seitengrenze und materialisieren die vollständige Ergebnisliste vor dem
FFI-Übergang.

GNOME besitzt für seine Trackansicht bereits den anderen Vertrag:
`TrackListModel` kennt die Gesamtzahl, lädt 200-Zeilen-Fenster nach Bedarf und
hält höchstens acht Fenster im Cache. GTK virtualisiert die Widgets, das
Modell die Daten; der Benutzer kann die ganze Bibliothek durchlaufen. Compose
virtualisiert mit `LazyColumn` nur die bereits materialisierten Zeilen und
kann die fehlenden Daten nicht anfordern.

Das ist **keine Kotlin-Aufgabe**. Die nächste grosse Bibliotheksoberfläche
braucht eine Core-Abfrage mit stabilem Request aus Scope, Suche und Ordnung
sowie Response aus Gesamtzahl und Fenster beziehungsweise Cursor. Erst danach
darf jede Oberfläche ihr eigenes Vorladen und ihren eigenen Cache wählen. Das
vorliegende Paket hat genau den Risikofall — eine grosse reale Bibliothek —
nicht ausführen können. Deshalb bleiben sowohl das Abschneiden bei 500 als
auch die fehlende Pagination ausdrückliche, ungemessene Produktrisiken; sie
sind nicht durch den kleinen Testbestand entkräftet.

### Urteil über die vier verbliebenen P1a-Cluster

Die mechanische Zeilenzählung liefert nach dieser zweiten Oberfläche nur für
einen der vier Cluster positive Evidenz:

| Cluster | Evidenz aus diesem Paket |
| --- | --- |
| `tag_edit_flow` | **Nicht gestützt.** Das Paket ist rein lesend und hat keinen zweiten Verbraucher für Feldmischung, Validierung oder Schreibentscheidungen erzeugt. Die mechanische Grösse darf hier keine mobile Teilbarkeit behaupten. |
| `session_restore` + `view_session` | **Gestützt, aber enger als der Dateiumfang.** Android braucht bereits denselben Begriff eines Browserorts und seiner Verfeinerungen; derzeit besitzt es nur flüchtigen Widgetzustand. `BrowserPlace` und `TrackViewState` zeigen die tragende gemeinsame Semantik. GTK-Scrollanker, Widgetfokus und 200-ms-Debounce sowie Compose-Reiter und `remember` bleiben Adapter. |
| `column_layout` + `keyboard_reorder` | **Nicht gestützt.** Die mobile Ansicht hat weder Spalten noch Tastaturreihenfolge. Das Paket liefert im Gegenteil erste Evidenz, dass diese Dateien Desktop-Interaktion beschreiben. `ColumnId` kann aus internen Gründen rein sein; diese Oberfläche rechtfertigt aber keinen plattformübergreifenden View-Vertrag dafür. |
| `missing_view` + `import_errors_view` | **Nicht gestützt.** Weder fehlende Dateien noch Importfehler liegen im Umfang dieser Oberfläche; es entstand kein zweiter Verbraucher und damit keine empirische Teilbarkeit. |

Damit überstimmt die Messung die Schätzung: **Von den vier verbleibenden
Clustern ist nur `session_restore` + `view_session` durch Paket 3 als nächster
Kandidat belegt.** Die anderen drei dürfen nicht wegen ihres mechanisch
gezählten reinen Anteils vorgezogen werden. Die wichtigste neu entdeckte
gemeinsame Naht — eine paginierbare Core-Trackabfrage für grosse Bibliotheken
— steht nicht in diesen vier Clustern. Sie ist neue Evidenz für die Abfrage-
und Tracklistenplanung und sollte deren Reihenfolge eher korrigieren als in
einen unpassenden Presenter-Umzug hineingedeutet werden.

Diese Bilanz beruht auf den Core-, GNOME-, UniFFI- und Compose-Seams sowie den
automatisierten Rust- und Kotlin-Prüfungen. Der getrennte Gerätelauf wurde wie
vorgegeben nicht ausgeführt; insbesondere Laufzeitkosten und Scrollverhalten
einer grossen Android-Bibliothek werden hier nicht behauptet.

## Nachtrag — Fenstervertrag und grosser Emulator-Scan (2026-08-03)

Die vier Browse-Fassaden liefern jetzt gezählte, begrenzte Fenster statt
nackter Vektoren. Der Vertrag ist automatisiert über mehr Zeilen als ein
Fenster geprüft: Gesamtzahl und Fensterinhalt stimmen, das nächste Fenster
schliesst ohne Lücke oder Dopplung an, und `has_more` macht jede unvollständige
Antwort sichtbar. Compose fordert an seinem geladenen Listenende selbst das
nächste 200-Zeilen-Fenster an; Ausrichtung, Vorladen und Duplikatschutz bleiben
damit Oberflächenpolitik.

Das beweist den Vertrag, nicht sein Laufzeitverhalten an einer grossen realen
Bibliothek. Der grosse Emulatorlauf verwendete 1.824 Dateien in 562
Verzeichnissen, aber weder ein echtes Telefon noch die Bibliothek eines
Benutzers. Er vermass den Storage-Scan, nicht langes Scrollen durch alle
Browse-Fenster. Für diese Bilanz gilt daher ausdrücklich: **Der Emulator misst
Anzahlen und Verlaufsgestalt, nie Zeit.** Aus ihm folgt keine belastbare Dauer,
kein Geräte-Durchsatz und kein Leistungsversprechen.

### Was der instrumentierte Lauf tatsächlich zählt

- Der Scan verursachte **3.520 Provider-Rundläufe für 1.824 Dateien in 562
  Verzeichnissen**. Das sind rechnerisch **1,93 Rundläufe je Datei**. Die **42
  Rundläufe des Ordner-Pickers** wurden separat gezählt und sind in den 3.520
  nicht enthalten.
- Die Instrumentierung trennt Verzeichnisauflistung und Tag-Lesung noch nicht.
  Die 3.520 belegen deshalb Last und Form des Gesamtlaufs, aber nicht, welcher
  Anteil auf Listing und welcher auf Öffnen beziehungsweise Tag-Parsing
  entfällt. Ohne diese Trennung wäre jede gezielte Optimierungsbehauptung
  geraten.
- Die beobachtete Fortschrittsrate blieb über den Lauf bei **3–5/s** und zeigte
  keinen Abwärtstrend. Das ist nur die Form der aufgezeichneten Folge, kein
  Emulator-Benchmark und insbesondere keine gemessene Gesamtdauer für ein
  reales Gerät.
- `scan_folder_inner` eröffnet vor dem Walk eine
  `unchecked_transaction()` und committet erst nach Walk, Vanish-Abgleich und
  Change-Log-Eintrag. Eine Transaktion umfasst damit den vollständigen Scan.
  Das ist ein Befund für die spätere Scanner-Planung; dieses Paket ändert ihn
  bewusst nicht.

### Lehren für den nächsten Messschnitt

1. Ein gezähltes Fenster verhindert stilles Abschneiden; seine Korrektheit
   sagt noch nichts über Scrollkosten oder Gerätegeschwindigkeit.
2. Emulatorzahlen dürfen Lastform und fehlenden Abfall zeigen, aber keine Zeit
   eines realen Geräts ersetzen.
3. Der nächste Storage-Zähler muss Listing und Tag-Lesung getrennt benennen,
   bevor eine der beiden Seiten optimiert wird.
4. Die scanweite Transaktion ist als möglicher Skalierungsfaktor sichtbar,
   aber ohne isolierende Messung weder Ursache noch Reparaturauftrag.
5. Android-Testquellen sind noch keine ausgeführten Tests. Die beiden Dateien
   unter `android/app/src/test` waren `fun main()`-Skripte ohne JUnit-Laufzeit;
   `:app:testDebugUnitTest` entdeckte deshalb **null Tests**. Erst nach der
   Umstellung auf echte JUnit-Tests entdeckte derselbe Task 18: Eine absichtliche
   Mutation machte genau einen rot, nach ihrer Rücknahme liefen 18 von 18 grün.
   Ein Assemble- oder Compilerfolg darf nie wieder als Testausführung gelten.

Die reale grosse Bibliothek, ein physisches Android-Gerät, die Scrollkosten
über viele Fenster, die Listing-/Tag-Read-Aufteilung und die Wirkung der
scanweiten Transaktion bleiben damit ausdrücklich ungemessen. Der Vertrag ist
korrekt geprüft; unter dieser Last ist er noch nicht bewährt.
