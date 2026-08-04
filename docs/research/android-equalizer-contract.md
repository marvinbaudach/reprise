# Android equalizer contract — M6 measurement and proposal

Measured on 2026-08-04 from the checked-out Reprise sources, the public Android
16 API, and AndroidX Media3. This was a documentation/source measurement only:
no code was changed, compiled, or run, and no device, emulator, or `adb` run was
used. Consequently, this report deliberately does **not** claim that a
particular phone has five bands, particular centre frequencies, or a particular
level range.

## Result

The stored ten-value array is not a cross-backend equalizer contract. It is a
sampled representation of GStreamer's `equalizer-10bands` topology. Android's
contract is the opposite: ask the live audio-session effect for its topology and
limits. Keeping ten anonymous values as the shared truth would therefore require
an approximation on Android and an inverse approximation after phone edits.
Either operation changes the meaning of the curve if it is written back.

**Recommendation:** store one backend-independent curve as ordered
`(frequency_hz, gain_db)` control points, keep each backend's sampled band
levels as an ephemeral projection, and never write a projection back over the
curve. Applying a non-exact or clipped projection on a new capability shape must
be an explicit user decision. This preserves the authored curve; it does not
pretend that different equalizers can produce identical sound.

That recommendation still needs the owner's decision. If an explicit
approximation workflow is unacceptable, the honest fallback is
capability-local curves that do not travel, not hidden resampling.

## Reprise as it is today

### Stored types and semantics

`AudioEffects` is exactly:

```rust
pub struct AudioEffects {
    pub equalizer_enabled: bool,
    pub equalizer_bands: [f64; 10],
    pub replay_gain: ReplayGainMode,
}
```

Its defaults are `false`, `[0.0; 10]`, and `ReplayGainMode::Off`
(`crates/reprise-core/src/playback.rs:23-37`). `ReplayGainMode` is the enum
`Off | Track | Album`; `TrackTransition` is the separate enum
`Off | Gapless | Crossfade` (`crates/reprise-core/src/library/settings.rs:287-305`).
The playback port receives the whole `AudioEffects` value and receives
`TrackTransition` plus `crossfade_seconds: u8` separately
(`crates/reprise-core/src/playback.rs:222-261`).

The ten `f64` values are **decibel gains**, not linear gain factors. The current
runtime protocol says “gain per band in dB”, in ascending centre-frequency
order, and rejects any length other than ten instead of interpolating
(`crates/reprise-runtime-protocol/src/effects.rs:12-26`,
`crates/reprise-runtime-protocol/src/effects.rs:40-54`). Core clamps every stored
value to `-12.0..=12.0`; the GNOME controls expose the same range in 1 dB steps
(`crates/reprise-core/src/library/settings.rs:493-517`,
`crates/reprise-gnome/src/ui/preferences/preference_playback.rs:14-55`).

The Linux backend instantiates GStreamer's `equalizer-10bands` and assigns the
ten values to `band0` through `band9`, substituting `0.0` while disabled
(`crates/reprise-platform-linux/src/player_effects.rs:10-22`,
`crates/reprise-platform-linux/src/player_effects.rs:148-156`). The element's
actual centres are **29, 59, 119, 237, 474, 947, 1,889, 3,770, 7,523, and
15,011 Hz**, while the GTK labels round these to 31, 62, 125, 250, 500, 1k,
2k, 4k, 8k, and 16k
([GStreamer `equalizer-10bands`](https://gstreamer.freedesktop.org/documentation/equalizer/equalizer-10bands.html),
`crates/reprise-gnome/src/ui/preferences/preference_playback.rs:5-7`).
GStreamer itself accepts `-24..=12` dB, but Reprise deliberately narrows that to
`-12..=12` dB in persistence, UI, and backend application
([GStreamer `equalizer-10bands`](https://gstreamer.freedesktop.org/documentation/equalizer/equalizer-10bands.html),
`crates/reprise-core/src/library/settings.rs:493-517`,
`crates/reprise-platform-linux/src/player_effects.rs:148-156`).

`crossfade_seconds` is a whole-second `u8`, clamped to `0..=10`; zero means no
crossfade (`crates/reprise-core/src/library/settings.rs:251-258`,
`crates/reprise-core/src/library/settings.rs:570-580`). `TrackTransition` is
**derived, not persisted**: a positive crossfade selects `Crossfade`; otherwise
the persisted gapless boolean selects `Gapless` or `Off`
(`crates/reprise-core/src/library/settings.rs:544-580`).

### Persistence and complete production reader/writer inventory

All five settings are string values in the generic SQLite `settings` table:
`playback.equalizer_enabled`, `playback.equalizer_bands`,
`playback.replay_gain_mode`, `playback.gapless_enabled`, and
`playback.crossfade_seconds` (`crates/reprise-core/src/db.rs:186-195`,
`crates/reprise-core/src/library/settings.rs:244-258`). Writes are upserts that
deduplicate identical values and append a settings change event
(`crates/reprise-core/src/library/settings.rs:27-80`). The ten gains are one
comma-separated string; ReplayGain is `off`, `track`, or `album`; the other
stored forms are a boolean and the decimal seconds value
(`crates/reprise-core/src/library/settings.rs:479-580`).

The production setting readers and writers are:

- **Core persistence API:** individual equalizer, ReplayGain, gapless, and
  crossfade getters/setters, plus the derived transition reader, are exposed at
  `crates/reprise-core/src/library/settings_api.rs:230-283`. The atomic
  `AudioEffects` facade reads the three effect keys and writes them in one
  transaction at `crates/reprise-core/src/library/audio_effect_settings.rs:8-24`.
- **GNOME startup/backend synchronization:** startup loads and applies the
  complete effects value; on backend refusal it disables equalizer and
  ReplayGain but deliberately preserves the authored band values
  (`crates/reprise-gnome/src/ui/playback/audio_effects.rs:11-39`). The player
  controller also reads and applies the derived transition and crossfade at
  startup (`crates/reprise-gnome/src/ui/playback/player_controller.rs:436-446`).
- **GNOME settings surface:** it reads equalizer enabled/bands, ReplayGain mode,
  gapless, and crossfade, and writes switch, band, preset, mode, and transition
  changes at `crates/reprise-gnome/src/ui/preferences/preferences.rs:509-632`
  and `crates/reprise-gnome/src/ui/preferences/preferences.rs:654-764`. The
  shared effect handler persists equalizer/ReplayGain changes and reapplies the
  full effects value at
  `crates/reprise-gnome/src/ui/preferences/preference_effects.rs:13-79`. The
  isolated preferences smoke hook is also a production writer of a preset,
  enabled equalizer, and track ReplayGain
  (`crates/reprise-gnome/src/ui/preferences/preferences.rs:381-408`).
- **GNOME transition refresh:** every preference/queue refresh reads the
  derived transition and crossfade again, applies them to the backend, and
  decides whether to feed a next item
  (`crates/reprise-gnome/src/ui/playback/up_next_transport.rs:341-369`).
- **Headless runtime:** construction loads/applies effects and reads/applies the
  transition (`crates/reprise-runtime/src/runtime.rs:196-230`). A runtime effect
  command validates ten bands, asks the backend first, persists the accepted
  value, and exposes the active snapshot
  (`crates/reprise-runtime/src/effects.rs:25-135`,
  `crates/reprise-runtime/src/runtime.rs:300-310`,
  `crates/reprise-runtime/src/runtime.rs:483-495`). The Linux D-Bus service is
  the writer entry point (`crates/reprise-platform-linux/src/runtime_service/interface.rs:310-321`);
  the runtime client exposes the corresponding request/event shapes at
  `crates/reprise-runtime-client/src/events.rs:25-37` and
  `crates/reprise-runtime-client/src/events.rs:136-178`. Runtime clients are
  readers of the applied snapshot
  (`crates/reprise-runtime-client/src/mirror.rs:25-35`,
  `crates/reprise-runtime-client/src/mirror.rs:72-80`,
  `crates/reprise-runtime-client/src/mirror.rs:113-116`,
  `crates/reprise-runtime-client/src/mirror.rs:141-145`,
  `crates/reprise-runtime-client/src/mirror.rs:177-186`). This is another
  boundary that currently embeds the fixed GStreamer-shaped array.
- **Linux playback backend:** `set_audio_effects` owns and updates the active
  filter (`crates/reprise-platform-linux/src/player.rs:464-486`), with the
  equalizer and ReplayGain values consumed in
  `crates/reprise-platform-linux/src/player_effects.rs:10-64` and
  `crates/reprise-platform-linux/src/player_effects.rs:148-200`. Transition mode
  and seconds are retained for the gapless/crossfade machinery at
  `crates/reprise-platform-linux/src/player.rs:538-550`.
- **Android playback backend today:** the Rust adapter discards the supplied
  `AudioEffects` value and calls a parameterless port method
  (`crates/reprise-android-ffi/src/playback.rs:121-123`); Kotlin rejects that
  method as unsupported
  (`android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt:116-120`).
  Android therefore has no effects setting reader or writer yet.

Tests call these APIs too, but they are verification fixtures rather than
additional production settings surfaces. Their concentrated coverage is in
`crates/reprise-core/src/library/settings_tests.rs:275-339`,
`crates/reprise-gnome/src/ui/playback/audio_effects.rs:82-137`,
`crates/reprise-runtime/src/runtime_effects_tests.rs:1-241`, and
`crates/reprise-platform-linux/tests/runtime_service/effects_surface.rs:1-80`.

## Android platform equalizer

### What the platform guarantees

An Android 16 device implementation that declares `android.hardware.audio.output` must provide `EFFECT_TYPE_EQUALIZER` through `android.media.audiofx.Equalizer`. The compatibility requirement mandates the effect, but does not mandate a band count, frequency layout, or level range ([Android 16 CDD, section 5.5.2](https://source.android.com/docs/compatibility/16/android-16-cdd#552_audio_effects)).

`Equalizer` is a controller for the equalizer engine associated with an audio session. Android standardizes how an application discovers and controls that engine; it does not standardize a five-band or ten-band curve ([Equalizer overview](https://developer.android.com/reference/android/media/audiofx/Equalizer)). The exact runtime surface is:

| Property | Runtime query | Unit and meaning | Fixed by Android? |
| --- | --- | --- | --- |
| Band count | `getNumberOfBands(): short` | Number of bands supported by this equalizer engine | **No.** The API returns what the engine supports; it defines no numeric constant ([API](https://developer.android.com/reference/android/media/audiofx/Equalizer#getNumberOfBands())). |
| Band indices | `0 .. getNumberOfBands() - 1` | Index passed to all per-band calls | The indexing rule is fixed; the count is not ([API](https://developer.android.com/reference/android/media/audiofx/Equalizer#getBandFreqRange(short))). |
| Centre frequency | `getCenterFreq(band): int` | milliHertz; divide by 1,000 for Hz | **No fixed centres.** Query every band from the live engine ([API](https://developer.android.com/reference/android/media/audiofx/Equalizer#getCenterFreq(short))). |
| Effective frequency interval | `getBandFreqRange(band): int[2]` | lower and upper bounds in milliHertz | **No fixed intervals.** Query them from the live engine ([API](https://developer.android.com/reference/android/media/audiofx/Equalizer#getBandFreqRange(short))). |
| Gain limits | `getBandLevelRange(): short[2]` | lower and upper band level in milliBel | **No fixed range.** Query it from the live engine ([API](https://developer.android.com/reference/android/media/audiofx/Equalizer#getBandLevelRange())). |
| Band gain | `getBandLevel(band)` / `setBandLevel(band, level)` | milliBel; 100 mB = 1 dB, so divide by 100 for dB | The unit is fixed; the permitted minimum and maximum are engine-defined ([getter](https://developer.android.com/reference/android/media/audiofx/Equalizer#getBandLevel(short)), [setter](https://developer.android.com/reference/android/media/audiofx/Equalizer#setBandLevel(short,%20short))). |

The platform documentation also lets an app query the band having the greatest effect on an arbitrary frequency (`getBand`) and each band's effective range, reinforcing that applications are expected to discover the engine's topology rather than assume one ([`getBand`](https://developer.android.com/reference/android/media/audiofx/Equalizer#getBand(int)), [`getBandFreqRange`](https://developer.android.com/reference/android/media/audiofx/Equalizer#getBandFreqRange(short))). Preset count and preset names are likewise queried from the implementation, not portable identifiers ([`getNumberOfPresets`](https://developer.android.com/reference/android/media/audiofx/Equalizer#getNumberOfPresets())).

Therefore “commonly five bands” may be a useful empirical warning, but it is not a platform contract. The only portable answer to “how many bands, at which centres, and over what range?” is: construct the effect for the playback audio session and query `getNumberOfBands()`, `getCenterFreq()`/`getBandFreqRange()`, and `getBandLevelRange()` on that instance. Any UI or stored model that assumes five or ten is relying on a backend detail the Android API explicitly makes discoverable.

Creating an effect does not enable it; `setEnabled(true)` is required for processing. The framework can also transfer control of a session's effect engine according to the controller priority, so production code must handle construction/control failures instead of treating the object as an infallible value ([AudioEffect ownership and enablement](https://developer.android.com/reference/android/media/audiofx/AudioEffect)).

### Audio-session attachment and player rebuilds

The `Equalizer(priority, audioSession)` constructor attaches the effect to the `MediaPlayer` or `AudioTrack` using that system-wide audio-session identifier. Attaching an insert effect to session `0` (the global mix) is deprecated ([Equalizer constructor](https://developer.android.com/reference/android/media/audiofx/Equalizer#Equalizer(int,%20int)), [AudioEffect session guidance](https://developer.android.com/reference/android/media/audiofx/AudioEffect)). An audio session may contain one or more mixed streams, and effects sharing its ID apply to those streams ([`AudioManager.generateAudioSessionId`](https://developer.android.com/reference/android/media/AudioManager#generateAudioSessionId())).

Media3 exposes the current ID as `Player.getAudioSessionId()` and reports changes through `Player.Listener.onAudioSessionIdChanged(int)` ([getter](https://developer.android.com/reference/androidx/media3/common/Player#getAudioSessionId()), [listener](https://developer.android.com/reference/androidx/media3/common/Player.Listener#onAudioSessionIdChanged(int))). `ExoPlayer.setAudioSessionId(int)` can instead attach its underlying `AudioTrack` to an ID generated in advance by `AudioManager.generateAudioSessionId()`, although Media3 marks this method `@UnstableApi` ([ExoPlayer API](https://developer.android.com/reference/androidx/media3/exoplayer/ExoPlayer#setAudioSessionId(int))).

Implications for an ExoPlayer instance that is rebuilt:

1. An `Equalizer` is attached to an **audio session**, not to a persisted application setting or abstract player identity. A freshly built player must not be assumed to reuse the old session ID. Either observe the new ID and create/rebind the effect, or deliberately allocate and set a session ID on the new player before preparing it ([Player listener](https://developer.android.com/reference/androidx/media3/common/Player.Listener#onAudioSessionIdChanged(int)), [`setAudioSessionId`](https://developer.android.com/reference/androidx/media3/exoplayer/ExoPlayer#setAudioSessionId(int))).
2. Session changes are not limited to rebuilding the whole `ExoPlayer`: Media3 documents that recreating the underlying audio track after an audio-attribute change generates a new audio-session ID. The listener therefore remains necessary even if the app normally keeps one player instance ([renderer audio-attribute contract](https://developer.android.com/reference/androidx/media3/exoplayer/Renderer#MSG_SET_AUDIO_ATTRIBUTES)).
3. Release the old `Equalizer` when its session is no longer used. `AudioEffect.release()` frees native resources and returns control/resources to other applications; `Player.release()` is independently mandatory when the player is no longer needed ([AudioEffect release](https://developer.android.com/reference/android/media/audiofx/AudioEffect#release()), [Player release](https://developer.android.com/reference/androidx/media3/common/Player#release())).
4. Persist the user's requested curve in application storage, not in the `Equalizer` object. Recreate the effect, re-query its runtime topology and limits, and then decide explicitly whether that stored curve is representable. The platform effect is a session-scoped rendering resource, not the settings contract.

## Other transition/effect verdicts on Android

### Gapless: native when playback is one Media3 playlist

The platform-side verdict is **confirmed with an integration condition**.
Media3's playlist documentation says transitions between playlist items are
seamless ([Media3 playlists](https://developer.android.com/media/media3/exoplayer/playlists)).
Thus gapless playback does not require a second decoder orchestration project
when consecutive tracks are supplied to one continuing player playlist.

The plan's stronger statement that the setting “maps straight through” is
**false today**. Reprise's Android port has only `Off | Gapless`, and maps both
Core `Gapless` and `Crossfade` to Android `Gapless`
(`crates/reprise-android-ffi/src/playback.rs:19-24`,
`crates/reprise-android-ffi/src/playback.rs:139-148`). More importantly, the
Android session never reads the persisted transition. Its constructor
unconditionally selects `TrackTransition::Gapless`
(`crates/reprise-android-ffi/src/playback_session.rs:305-349`). Kotlin then keeps
the current and one pre-fed next item in the same ExoPlayer playlist when that
hard-coded mode is active
(`android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt:134-175`,
`crates/reprise-android-ffi/src/playback_session.rs:176-200`). M6 must wire the
derived Core setting; the playback capability already exists, but the setting
does not map at all yet.

Audio offload adds a device capability caveat. Media3 exposes `OFFLOAD_MODE_ENABLED_GAPLESS_REQUIRED`, which prefers offload except when the track needs gapless playback and the device cannot provide gapless offload; its stated purpose is to prioritize uninterrupted consecutive tracks over power savings ([AudioSink offload mode](https://developer.android.com/reference/androidx/media3/exoplayer/audio/AudioSink#OFFLOAD_MODE_ENABLED_GAPLESS_REQUIRED)). “ExoPlayer supports gapless” is therefore accurate, while “every device can keep compressed offload enabled and still be gapless” is not.

### Crossfade: no turnkey ExoPlayer transition

The platform-side verdict is **confirmed**. Ordinary ExoPlayer playlists provide seamless hard transitions, not overlapping crossfades. Even Media3's newer multi-source `Composition`/`CompositionPlayer` stack lists “Crossfading video or audio tracks” as a current unsupported operation ([Media3 composition limitations](https://developer.android.com/media/media3/transformer/composition#current-limitations)).

Media3 now has useful low-level ingredients: `GainProcessor` applies sample-level gain automation to one audio stream, `DefaultGainProvider` supplies linear and equal-power fade envelopes, and `AudioMixer` can align and mix concurrent sources ([GainProcessor](https://developer.android.com/reference/androidx/media3/common/audio/GainProcessor), [DefaultGainProvider](https://developer.android.com/reference/androidx/media3/common/audio/DefaultGainProvider), [AudioMixer](https://developer.android.com/reference/androidx/media3/transformer/AudioMixer)). Those APIs do not turn the normal ExoPlayer playlist transition into a crossfade, and the relevant APIs are unstable/experimental.

The following is an engineering inference from those API boundaries: a production crossfade needs overlapping decode/preload of the outgoing and incoming items, a common PCM timeline and mixer, complementary sample-accurate gain envelopes, and explicit behavior for seek, skip, repeat, queue edits, track failure, duration changes, audio focus, audio-session/equalizer ownership, MediaSession current-item/position reporting, and teardown. It must also decide when PCM processing disables or replaces passthrough/offload paths. Two independently clocked ExoPlayer instances with UI-thread volume ramps can prototype the sound, but do not by themselves provide a deterministic playback contract. Crossfade is therefore a playback subsystem project, not a direct mapping of `crossfade_seconds` to an ExoPlayer setting.

Reprise currently does not attempt that project. The Core port explicitly
permits a backend to degrade `Crossfade` to `Gapless`
(`crates/reprise-core/src/playback.rs:249-261`), and the Android adapter does
exactly that while discarding `crossfade_seconds`
(`crates/reprise-android-ffi/src/playback.rs:139-148`). Crossfade must remain
absent from Android settings until an overlapping playback design exists.

### ReplayGain: metadata is not automatic normalization

No public `Player` or `ExoPlayer` setting represents ReplayGain track/album mode. Current Media3 source does parse ReplayGain carried in a LAME MP3 Xing/Info frame into an `Mp3InfoReplayGain` metadata entry, whose gain field is expressed in decibels ([Media3 release note](https://github.com/androidx/media/blob/5fb306449733dd71595700c1227ad6087578c559/RELEASENOTES.md#L344-L346), [`Mp3InfoReplayGain`](https://github.com/androidx/media/blob/5fb306449733dd71595700c1227ad6087578c559/libraries/extractor/src/main/java/androidx/media3/extractor/mp3/Mp3InfoReplayGain.java#L80-L105), [`XingFrame.getMetadata`](https://github.com/androidx/media/blob/5fb306449733dd71595700c1227ad6087578c559/libraries/extractor/src/main/java/androidx/media3/extractor/mp3/XingFrame.java#L158-L165)). In that pinned official source snapshot, the entry is extractor metadata; it is not a renderer setting or an automatic gain-application path.

Media3's `GainProcessor` could apply an application-supplied gain envelope to decoded PCM, but the application would still have to obtain and select valid per-track/per-album gain and peak data, convert the dB adjustment to a gain factor, define clipping prevention, and integrate that processing with all playback formats and output modes ([GainProcessor](https://developer.android.com/reference/androidx/media3/common/audio/GainProcessor)). A stored **mode alone** is therefore insufficient to produce ReplayGain on Android; the platform does not synthesize gain values from that mode.

The repository measurement confirms that Core stores only the mode. `TrackMeta`
contains no gain or peak fields, and its tag extraction reads only ordinary
identity plus duration/bitrate
(`crates/reprise-core/src/library/scanner_meta.rs:70-83`,
`crates/reprise-core/src/library/scanner_meta.rs:93-124`). The scanner's track
upsert has no ReplayGain values (`crates/reprise-core/src/library/scanner.rs:547-595`),
the `Track` model has none (`crates/reprise-core/src/models.rs:53-119`). The
initial track schema has none, and every later track-column migration adds only
file identity, waveform, missing-state, artist-MBID, or disc-number fields
(`crates/reprise-core/src/db.rs:109-128`,
`crates/reprise-core/src/db.rs:147-149`,
`crates/reprise-core/src/db.rs:247-247`,
`crates/reprise-core/src/db.rs:319-323`,
`crates/reprise-core/src/db.rs:361-362`,
`crates/reprise-core/src/db.rs:427-427`).

The plan is nevertheless wrong if read as “no backend applies ReplayGain.” The
Linux backend inserts GStreamer's `rgvolume` whenever the mode is not `Off` and
sets its album/track preference from that mode
(`crates/reprise-platform-linux/src/player_effects.rs:10-64`,
`crates/reprise-platform-linux/src/player_effects.rs:195-200`). `rgvolume` reads
ReplayGain tags from the stream, using track gain by default or preferring album
gain in album mode
([GStreamer `rgvolume`](https://gstreamer.freedesktop.org/documentation/replaygain/rgvolume.html)).
Android applies none because its entire audio-effects call is rejected
(`android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt:116-120`).

The Android verdict remains **blocked**, but “make the scanner store gains” is
only one possible data path, not a measured necessity. Android needs both a
source for format-appropriate track/album gain and peak metadata and a defined
PCM gain/clipping stage. Whether to persist scanner-extracted values or obtain
them in the playback source is an owner/architecture decision.

## Candidate equalizer contracts

The relevant distinction is between the **authored curve** and a backend's
**projection** of it. A projection may be approximate; it must never replace the
authored curve without an explicit conversion action.

### A. Keep the ten-band array as stored truth; map at the edge

The current `[f64; 10]` remains canonical. Desktop keeps its current exact
mapping. Android queries its live centres/range, interpolates the ten desktop
samples onto those bands, and clamps as needed.

- **Cost:** small storage migration cost, but every non-desktop backend needs
  mapping, clipping diagnostics, and approval UI. Phone editing additionally
  needs an inverse mapping into ten desktop values.
- **Makes impossible:** faithfully representing a device-native curve whose
  controls are not the ten GStreamer centres. It also cannot promise the same
  audible response across effect implementations.
- **Desktop curve opened on phone:** unless the Android topology happens to
  match, it is an approximation. Applying it silently violates the requirement;
  the phone must preview/label it and obtain explicit approval.
- **Phone curve opened on desktop:** writing phone bands back requires an
  inverse interpolation and changes the curve. If that happens automatically,
  this contract silently destroys the user's phone-authored shape. Avoiding
  that leaves the phone unable to author the shared value.

**Verdict: reject as the shared contract.** It preserves today's schema by
making GStreamer the definition of every future frontend. The fixed current
runtime protocol demonstrates the same leakage
(`crates/reprise-runtime-protocol/src/effects.rs:12-26`,
`crates/reprise-runtime/src/effects.rs:110-135`).

### B. Store a backend-independent frequency curve; sample per backend

The stored truth becomes explicit ordered control points, for example:

```text
EqualizerCurve {
  enabled: bool,
  points: [{ frequency_hz, gain_db }, ...]
}

EqualizerCapabilities {
  bands: [{ centre_hz, min_db, max_db, step_db }, ...]
}

EqualizerProjection {
  capability_signature,
  band_levels_db,
  exact,
  clipped_points
}
```

The contract must specify interpolation in log-frequency space, endpoint
behavior, legal frequency/gain bounds, and rounding. A backend samples the
canonical curve at its queried centres, reports whether that projection is
exact or clipped, and never writes `band_levels_db` back as the curve.

- **Cost:** a Core/settings and runtime-protocol migration, projection math and
  tests, capability reporting, and UI that distinguishes the authored curve
  from the device rendering. Desktop's ten sliders become an editor/projection
  of a curve rather than the curve's anonymous storage slots.
- **Makes impossible:** guaranteeing an identical transfer function or sound on
  different equalizer engines. An Android engine with five broad bands cannot
  render all detail in a ten-point curve.
- **Desktop curve opened on phone:** the authored points remain byte-for-byte
  unchanged. The phone shows the projected result and, when it is non-exact or
  clipped, requires explicit acceptance before enabling it for that capability
  signature.
- **Phone curve opened on desktop:** phone sliders create/update canonical
  points at their real queried centre frequencies. Desktop samples those points
  for GStreamer and likewise asks for approval when the result is non-exact. It
  never stores the ten sampled outputs over the phone-authored curve.

**Verdict: recommended.** It is the only candidate that shares semantic intent
without promoting either backend's topology to the domain model. Existing ten
band settings can migrate losslessly for current desktop behavior by becoming
control points at GStreamer's **actual** centres (29 through 15,011 Hz), not the
rounded UI labels. That migration still must not auto-enable an approximate
Android projection.

### C. Store capability-local curves that do not travel

Store separate native curves keyed by a capability signature containing at
least band centres and level range. “Desktop” versus “phone” is too coarse:
Android phones may expose different topologies.

- **Cost:** multiple profiles, capability matching, profile selection, and
  device-fragmentation UX; there is no single shared equalizer setting.
- **Makes impossible:** editing one curve and carrying it between desktop and
  phone, or even necessarily between two phones.
- **Desktop curve opened on phone:** it is shown as incompatible and is not
  applied. The phone starts neutral or uses an existing exact-match profile.
- **Phone curve opened on desktop:** it is likewise preserved but unavailable;
  desktop uses its own profile.

**Verdict: acceptable only if the owner rejects approximation entirely.** It is
honest and exact on the originating engine, but it gives up the M6 goal of a
shared playback setting.

### D. Refuse to share or offer equalizer on Android

Keep the current desktop-only setting and show no Android equalizer.

- **Cost:** almost none beyond documenting the unsupported capability.
- **Makes impossible:** phone equalizer control and phone-authored curves.
- **Desktop curve opened on phone:** preserved in storage, ignored, and not
  presented as active.
- **Phone curve opened on desktop:** impossible because Android offers no editor.

**Verdict: safe but incomplete.** This is preferable to hidden resampling, but
it does not deliver the owner's requested Android playback setting.

## Proposed M6 contract and owner decisions

Proceed with candidate B if the owner accepts these rules:

1. The persisted value is the authored frequency/gain curve. Backend band
   levels are cacheable projections only and can never overwrite it.
2. A backend publishes its queried capability shape. Projection is deterministic
   and reports `exact`, rounding, and clipping rather than concealing them.
3. A non-exact or clipped projection is disabled until the user explicitly
   accepts it for that capability signature. A changed signature requires a new
   decision.
4. Editing on any surface edits explicit canonical control points. Opening the
   curve elsewhere never rewrites those points merely by viewing or applying it.
5. The existing ten values migrate to points at the actual GStreamer centres so
   the current desktop rendering does not change.

The owner still needs to decide:

- whether cross-device approximation with explicit per-capability acceptance is
  acceptable at all; otherwise choose candidate C;
- the curve's frequency bounds, log-frequency interpolation, endpoint behavior,
  precision, maximum point count, and whether clipped projections may be
  accepted or must remain unavailable;
- whether an edit made through native band sliders moves canonical points or
  creates a new named curve/version, and how the UI makes that distinction clear;
- whether capability approval is local device state or synced user state; and
- separately from equalizer, where Android ReplayGain obtains gain/peak metadata
  and what clipping policy its gain stage uses.
