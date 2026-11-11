//! Which structured tracing fields may appear in a diagnostics line.
//!
//! A log line leaves the machine inside a Copy Diagnostics report, so a
//! structured field survives only when its key is in [`KEEP_FIELDS`]. Every
//! other key is replaced by `$REDACTED`, and so is any key nobody has looked
//! at. The same predicate, [`super::is_safe_structured_field`], decides for both
//! redaction passes: `EventFields::record_value` in
//! `reprise-gnome/src/ui/diagnostics.rs` for typed fields, and
//! `redact_sensitive_assignments` for `key=value` text.
//!
//! The lists are closed lists of exact key names. There are no suffix or
//! prefix rules (`*_ms`, `*_id`, `is_*`): a future `foo_id` that carries a
//! string would leak through a rule like that.
//!
//! # The rule for `KEEP_FIELDS`
//!
//! A key goes into `KEEP_FIELDS` only when **every** place in production code
//! that emits it carries one of: an integer, a float, a bool, a duration, the
//! `Debug` or `Display` of a fieldless enum (or of an enum whose payloads are
//! numbers) or of a closed set of static labels, a crate version, or an
//! internal database or sequence id. A string that could come from a file, a
//! tag, a URL, a network response, user input or a foreign library's message
//! is not such a value. When one site of a key fails the test, the key goes
//! into `REDACT_FIELDS`, or that one site is renamed to a key that does.
//!
//! # What the policy does not promise
//!
//! The predicate also governs the `key=value` tokens inside free-text
//! messages and the log tail (D2). There a kept key keeps its value only when
//! the value is a number (an integer, a float with one dot, an exponent form,
//! a duration such as `12.5ms`, or `Some(..)` of one; several dots only under
//! `version`), a plain identifier (`[A-Za-z_][A-Za-z0-9_]*`, and not eight
//! or more hex digits), or an option, tuple variant or list of those:
//! `Some(Track(5))`, `[1, 2, 3]`, `[NowPlaying, PlayNext(0)]` (integers and
//! identifiers only inside; at most three wrappers and 64 elements, see
//! `redact_composite`). Anything else, such as `mode=Compact,title=Foo` or a
//! struct body (`StaleFor { seconds: 60 }`), is
//! `key=$REDACTED`, and, like an unknown key, it takes every word after it up
//! to the end of the line. Only a token whose value is already exactly
//! `$REDACTED` ends that: it is what the typed path writes for a redacted field,
//! so it carries nothing, and the typed fields after it stay readable (a known
//! `key=` token ends the swallow after such a token, never after raw text, where
//! `count=3` may be forged). A secret-like `token:` takes the rest of the line
//! whatever follows. What remains is a plain identifier that is
//! itself foreign text (`state=Artist`): the policy cannot tell it from a
//! variant name, so a message that interpolates foreign text after a kept key
//! is a review concern. The typed `EventFields` path keeps typed values and is
//! unaffected, but the file mirror runs this pass over the finished line too,
//! so a kept key whose `Debug` value is not one of the shapes above (a
//! struct variant, a list holding a float) is redacted in the log file, and
//! the fields written after it on that line go with it.
//!
//! The classification test checks keys, not value types, and it does not see
//! fields that dependencies emit (zbus, for one). A new site that emits an
//! existing kept key with a string, or a `Debug` of a payload enum that gains
//! a `String` field, passes the test. A kept site that formats with `%` or
//! `?` therefore needs its value's type checked in review.
//!
//! # The check
//!
//! Every key emitted by a `tracing` macro in production code under `crates/`
//! (events and span fields, `span!`, `*_span!` and `#[instrument(fields(…))]`)
//! must be in exactly one of the two lists, and a listed key must still be
//! emitted (`field_policy_sweep_tests`). Adding a field to a log call is
//! therefore a decision the test suite forces: the failing test names the key
//! and the site, and says which list to put it in.

/// Keys whose values are harmless in every place they are emitted. Sorted.
pub(super) const KEEP_FIELDS: &[&str] = &[
    "above",
    "acknowledged",
    "active_import_errors",
    "added",
    "advance_ms",
    "after_len",
    "after_ms",
    "after_secs",
    "age_seconds",
    "analyzed",
    "apply_ms",
    "attempt",
    "attempts",
    "auto_cleaned",
    "badge_updated",
    "bar_count",
    "before_len",
    "boundary_ms",
    "browse_bar_ms",
    "browse_visible",
    // `Debug` of an optional millisecond count.
    "buffered_ms",
    "build_ms",
    "bytes_per_second",
    "cached",
    "can_go_next",
    "can_go_previous",
    "cancelled",
    "candidate_count",
    "candidates",
    "change_id",
    "changes",
    // `Debug` of the fieldless `ClockConvention`.
    "clock",
    // `std::any::type_name::<K>()`: a compile-time constant.
    "column_type",
    "completed",
    "completed_at",
    "conflicts",
    // Counts of candidates, not identities.
    "consecutive_failures",
    // Every call site takes a `&'static str` label.
    "context",
    "continuation_len",
    // `Debug` of the fieldless `LibraryWindowControl`.
    "control",
    "corrections",
    "count",
    "cover_download",
    // The numeric GStreamer playbin flags.
    "current_flags",
    "current_track_ms",
    // `Debug` of `Option<QueueItem>`: a track or episode id.
    "current_up_next",
    "cycle",
    "dates",
    // `Debug` of the fieldless `FirstRunDecision`.
    "decision",
    "default_height",
    "default_width",
    "delay_secs",
    // A track id, or `Debug` of an `Option<i64>` track id.
    "deleted",
    "delta",
    "dismissed",
    "dismissed_import_errors",
    "done",
    "doubtful",
    "downloaded",
    "dropped",
    "duplicate_position",
    "duration_ms",
    "duration_us",
    "editable_columns",
    "elapsed_ms",
    "elapsed_us",
    // `ReloadEmit::as_str`: a static label.
    "emit",
    "end",
    "episode_id",
    // `Debug` of a `Vec<i64>` of episode ids.
    "episode_ids",
    "episodes",
    // `Debug` of `std::io::ErrorKind`: a fieldless enum, never the error text.
    "error_kind",
    "errors",
    "estimate_lease",
    "example_track_id",
    "exclude_ai",
    "exit_code",
    "expected",
    "expired_miss",
    "expired_retry",
    // `Debug` of the fieldless `BrowseFacet`.
    "facet",
    "failed",
    "failed_tracks",
    // Static labels chosen in `podcasts/ytdlp.rs`.
    "failure_kind",
    "failures",
    "feed_ms",
    "fetched",
    "fft_len",
    "first_mismatch",
    "first_position",
    "found",
    "frame",
    "fresh_miss",
    "fresh_portrait",
    "fresh_retry",
    "from_up_next",
    "gapless",
    // A counter.
    "generation",
    // `Debug` of an optional track id.
    "greeting_track_id",
    "had_ids",
    "has_place_pill",
    "has_pre_save_view",
    "head_frames",
    "healed",
    "height",
    // Internal database ids of tracks, episodes and playlists.
    "id",
    // A `usize` count.
    "id_count",
    "idle_wait_ms",
    "ids_ms",
    // `Result::is_ok`.
    "ids_ok",
    "ignored",
    "index",
    "info_visible",
    "inserted",
    "is_multi",
    "is_shuffled",
    // `Debug` of fieldless kind enums.
    "kind",
    "known_missing",
    // `PodcastsLane::name` returns a `&'static str`.
    "lane",
    "last_played",
    // `Debug` of `CompactLayout`. The serialized column layout string is logged as `layout_spec`.
    "layout",
    "leases",
    "len",
    "limit",
    "line",
    "line_count",
    // A `&'static str` listener name.
    "listener",
    "listeners_ms",
    "live",
    "live_ids_ms",
    "local_marker",
    "local_resolved",
    "longest_lease_ms",
    "main_thread_ms",
    "marked",
    "match_count",
    "matched",
    "max",
    "max_position_ms",
    "maximized",
    // A span field of `startup_report::measure(&'static str)`: every caller
    // passes a string literal such as `view.library.construct`.
    "measurement",
    "mirror_ms",
    "missing",
    // `Debug` of fieldless mode enums and a `u8` selection mode.
    "mode",
    // `ModuleDescriptor::id` is a `&'static str`.
    "module",
    "mounted",
    "moved",
    // `SourceError::technical_cause()` as logged by New Releases
    // (`artist_news_pipeline.rs::record_failed_attempt`): its values
    // are the constant `FetchError` `Display` strings from
    // `musicbrainz.rs` plus the fixed literal in
    // `invalid_response_source_error`, never user-entered or fetched
    // content. Named for MusicBrainz specifically, not the generic
    // `fetch_error`: this list is global over both redaction paths
    // (here and `EventFields::record_value` in
    // `reprise-gnome/src/ui/diagnostics.rs`), and a source whose
    // technical cause can carry a URL or a foreign library's own
    // message (podcasts, concerts) must not reuse this key.
    "musicbrainz_error",
    "mutated_ms",
    "n_items",
    "needs_fetch",
    "needs_request",
    "never_fetched",
    "new_import_errors",
    "new_missing",
    "new_rating",
    // The numeric GStreamer playbin flags.
    "next_flags",
    "next_frame_us",
    "next_sequence",
    "now_playing_deferred_ms",
    "now_playing_enqueue_ms",
    "offset",
    // Static labels, and `Debug` of `PodcastsOperation`, whose variants carry only integers and fieldless enums. The column editor's non-static `operation` is logged as `editor_operation`.
    "operation",
    "other_ms",
    // Static labels, and `Debug` of fieldless outcome enums.
    "outcome",
    // A count, or `Debug` of a GStreamer state.
    "pending",
    // `Debug` of an optional track id.
    "pending_random_start_track_id",
    "percent",
    // An image distance (`f64`).
    "placeholder_distance",
    "play_counts",
    // A static playback status label.
    "playback",
    "player_load_ms",
    "playlist_id",
    "playlists",
    // `Debug` of `RefreshPolicy`: `StaleFor` carries only a number of seconds.
    "policy",
    // Integers. The smoke hook's raw environment value is logged as `smoke_position`.
    "position",
    "position_ms",
    "position_us",
    "present_ms",
    "presentations",
    "previous",
    "processed",
    "query_ms",
    "query_us",
    "queue_has_tracks",
    "queue_id",
    "queue_len",
    "queue_notify_ms",
    "queued",
    "queued_ms",
    "ratings",
    // A byte count.
    "received_bytes",
    "recovered",
    // `RecoveryOutcome::as_str`: a static label.
    "recovery",
    "refill_len",
    "refresh_number",
    "reload_id",
    "reload_ms",
    "reload_work_ms",
    "removed",
    "renumbered",
    // `Debug` of an `Option` list position.
    "reorder_position",
    // `Debug` of the fieldless `Repeat`.
    "repeat",
    "replace",
    // A count. The MPRIS object path a client sent is logged as `requested_object_path`.
    "requested",
    "requested_edge",
    "resizable",
    // `Debug` of the fieldless `CompletionResponse`.
    "response",
    "restore_ms",
    "restored",
    "restricted",
    // `Debug` of an optional result count.
    "result_count",
    "retrying",
    "rhythmbox_found",
    "rhythmbox_import",
    "rhythmbox_import_default",
    "row_widgets_allocated",
    "row_widgets_present",
    // Counts, and `Debug` of a `Vec<QueueRow>`, whose variants carry only list indices.
    "rows",
    // `Debug` of `Option<QueueItem>`: a track or episode id.
    "runtime_current_up_next",
    "scan_id",
    "select_ms",
    "selected_tracks",
    "sequence",
    // Every value traces back to a `&'static str` label ("Last.fm", "ListenBrainz", "lastfm", "listenbrainz").
    "service",
    "sidebar_visible",
    // A signal number.
    "signal",
    // A `u64` library signature.
    "signature",
    // Counts of candidates, not identities (`remaining` is a `usize`).
    "skipped",
    "skips",
    "snapshot_len",
    // String literals at the call sites.
    "stage",
    "stale_portrait",
    "start",
    "start_index",
    "start_ms",
    // A `u64` library signature.
    "started_signature",
    // `Debug` of fieldless state enums. The `SessionState` struct, which carries the search text and browse filter, is logged as `session`; a `DownloadState` is logged as `state_kind`.
    "state",
    // `DownloadState::kind`: a `&'static str` that never carries the failure message.
    "state_kind",
    "station_id",
    // An HTTP status code.
    "status",
    "stored_points",
    "subscription_id",
    "subscriptions",
    "synchronous_listeners_ms",
    "tail_len",
    "target_ms",
    "target_view_position",
    // `log_name()` returns a `&'static str`.
    "task",
    "timeout_ms",
    "tombstoned",
    "total",
    "total_albums",
    "total_artists",
    // A byte count.
    "total_bytes",
    "touched",
    "track_id",
    "tracks",
    "trashed",
    "unanalysed",
    "unavailable",
    "unmatched",
    "up_next_len",
    "updated",
    "upper",
    "vanished",
    // A `&'static str` verdict label in `scanner_reconcile.rs`.
    "verdict",
    // The crate version and the numeric journal and session format versions. A journal version read from a file is logged as `foreign_version`.
    "version",
    "view_len",
    "view_position",
    "visible",
    "was_missing",
    "was_removed",
    "width",
    "window_end",
    "window_start",
    "work_done_us",
    "worker_ms",
    "write_ms",
];

/// Keys that identify, or may identify, something and so stay redacted. Sorted.
///
/// This list exists so that no key is silently on either side: a key that is
/// emitted but listed nowhere fails the classification check. The free-text
/// pass also reads it, to tell a known `key=` that ends a swallowed value from
/// a fragment such as `abc==` inside it. The redaction itself never depends on
/// it: a key in neither list is redacted all the same.
pub(super) const REDACT_FIELDS: &[&str] = &[
    "action",
    // Smoke output of unverified type.
    "active_line",
    "album",
    "album_key",
    // Smoke callback result of unverified type.
    "appended",
    "artist",
    // A GIO attribute name passed as `&str`.
    "attribute",
    // A column id string.
    "before_column",
    "browse",
    // The text of an MCP playback bus error.
    "bus_message",
    // A D-Bus name, a `String`.
    "bus_name",
    "cached_path",
    "cause",
    "change",
    "channel_url",
    "chips",
    // A column key's string form.
    "column",
    // A raw column id read from the layout setting.
    "column_id",
    "command",
    "current",
    "data_root",
    // `Debug` of `DatePattern`, which holds the system's literal text.
    "date",
    "db",
    "db_path",
    "debug",
    "desired",
    "desired_path",
    "detail",
    // A filesystem device number, an identifier of the user's disk.
    "device",
    "device_id",
    "device_name",
    "device_path",
    "diagnostic",
    "dir",
    "directory",
    "e",
    // A non-static `&str` in the column editor.
    "editor_operation",
    // A foreign string from a feed.
    "entity",
    "error",
    // A string id from an update event.
    "event_id",
    "example_error",
    // A GStreamer error string, which can name a host, a stream or a file.
    "failure_message",
    // `Display` of a persisted sort field, a `String`.
    "field",
    "file",
    "filter",
    "first",
    "first_error",
    "first_spelling",
    // A smoke environment value.
    "fixture",
    // A version string read from a journal file.
    "foreign_version",
    "from",
    // A column id string.
    "hidden_column",
    // An MPRIS object path.
    "identity",
    "image_identifier",
    // A filesystem inode number, an identifier of the user's files.
    "inode",
    "item",
    "key",
    // Smoke output of unverified type.
    "latest",
    // The serialized column layout.
    "layout_spec",
    "linger_error",
    "locale_dir",
    // Only `reprise-android-ffi` emits it, and that crate never runs the redactor. A field
    // called `message` is the event's own text slot, which `EventFields` does not check
    // against the policy, so the classification test refuses it everywhere else.
    "message",
    "mount",
    "mount_point",
    // A column id string.
    "moved_column",
    "music",
    "name",
    // The text of a toast shown for a missing track.
    "notice_message",
    // A stream tag from the network.
    "organization",
    // `Debug` of a list of track ids paired with file paths.
    "orphaned_tracks",
    // A content-stack page name passed as `&str`.
    "page",
    "panic_location",
    "panic_payload",
    "path",
    "persist_error",
    "persistent_id",
    // `Debug` of `PlannedSyncPhase`, whose `Syncing` variant carries a step, and a smoke `&str`.
    "phase",
    "play",
    "playlist",
    "playlist_name",
    // A smoke environment value.
    "range",
    // `Debug` of a glib `Value`.
    "readback",
    "reason",
    "release_group_mbid",
    "report",
    // A D-Bus object path a client sent.
    "requested_object_path",
    "resident",
    "resident_path",
    // `Debug` of a `BrowseFilter`: genre, artist and album names.
    "restored.browse",
    // `Debug` of a `Result` whose error is a `String`.
    "result",
    // A non-static `&str` in the column editor.
    "role",
    "root",
    // `Debug` of `DoctorReviewRowId`, derived from track and album text.
    "row_id",
    // `ViewSource::label()` embeds album and artist names.
    "scope",
    "search",
    "second",
    "second_spelling",
    "seed",
    // `Debug` of `SessionState`, which carries the search text and the browse filter.
    "session",
    // `DeviceSessionState::Inert` carries a device name.
    "session_state",
    // A settings key passed as `&str`.
    "setting",
    "sheet",
    "sidecar",
    "sink",
    "smart_id",
    // A smoke environment value.
    "smoke",
    // The raw value of a smoke environment variable.
    "smoke_position",
    // A persisted `String`.
    "sort_dir",
    // A persisted `String`.
    "sort_field",
    "source",
    "source_path",
    "stable_id",
    // A page name `String`.
    "stack_page",
    "step",
    "storefront",
    "stream",
    "table",
    "target",
    "technical_error",
    "text",
    // A caller-supplied `&str` label.
    "timestamp",
    "title",
    "to",
    "track",
    "uri",
    "url",
    // A translated user-facing sentence.
    "user_notice",
    "value",
    // `ViewSource::label()` embeds album and artist names.
    "vanished_source",
    "volume",
    "welcome",
];
