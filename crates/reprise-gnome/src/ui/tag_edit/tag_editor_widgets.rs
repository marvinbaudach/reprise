//! Widget builders and mixed-value presentation helpers for the tag editor.
//!
//! Layout note (3a rework, TAG-2/TAG-3): fields stay on their existing
//! concrete types (`adw::EntryRow` for Title/Year/Track-number,
//! `AutocompleteEntry`-wrapped `adw::EntryRow` for Artist/Album/Album
//! artist/Genre) — `tag_editor_dirty.rs` and `tag_editor_save.rs` (both
//! outside this package's ownership this wave) are pinned to those concrete
//! types (`wire_entry(row: &adw::EntryRow, ..)`, `SaveWidgets { title: &'a
//! adw::EntryRow, .. }`), so swapping to a plain `gtk4::Entry` here would
//! break their compilation. This module instead reworks *layout and styling*
//! around the existing widgets: no boxed-list/`PreferencesGroup` chrome, a
//! label-bearing column per field, and a reserved (always-present, P-4)
//! "was: …" line underneath.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::cover::{self, ThumbnailSize};
use reprise_core::library::tag_edit::{EditableTagSummary, MixedValue};

use crate::ui::autocomplete_entry::AutocompleteEntry;
use crate::ui::strings;
use crate::ui::tag_editor::{STAR_FILLED, STAR_OUTLINE};
use crate::ui::tag_editor_dirty::UpdateCallback;
use crate::ui::tag_editor_state::{
    FIELD_ALBUM, FIELD_ALBUM_ARTIST, FIELD_ARTIST, FIELD_GENRE, FIELD_YEAR, RATING_MAX,
};

/// Builds the cover art area. For single track, shows a thumbnail. For
/// multi-track, shows a stacked representation with a count badge. No
/// "Change cover…" affordance (Beschluss #1: v1 never writes covers) — the
/// old disabled link is gone, not just greyed out.
pub(in crate::ui) fn build_cover_area(tracks: &[(i64, PathBuf)], is_multi: bool) -> gtk4::Box {
    const COVER_SIDE: i32 = 120;

    let outer = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    outer.set_valign(gtk4::Align::Start);
    outer.add_css_class("reprise-tag-cover-area");

    if is_multi {
        let overlay = gtk4::Overlay::new();
        overlay.add_css_class("reprise-tag-cover-stack");

        let cover = load_cover_picture(tracks.first().map(|(_, p)| p.as_path()));
        cover.set_size_request(COVER_SIDE, COVER_SIDE);
        overlay.set_child(Some(&cover));

        let badge = gtk4::Label::new(Some(&strings::tag_cover_count(tracks.len())));
        badge.add_css_class("reprise-tag-cover-badge");
        badge.set_halign(gtk4::Align::End);
        badge.set_valign(gtk4::Align::End);
        badge.set_margin_end(6);
        badge.set_margin_bottom(6);
        overlay.add_overlay(&badge);

        outer.append(&overlay);
    } else {
        let cover = load_cover_picture(tracks.first().map(|(_, p)| p.as_path()));
        cover.set_size_request(COVER_SIDE, COVER_SIDE);
        outer.append(&cover);
    }

    outer
}

/// Loads a cover thumbnail for a track path, returning a `gtk4::Picture`
/// wrapped in a frame box. Falls back to a placeholder if no cover is found.
pub(in crate::ui) fn load_cover_picture(track_path: Option<&Path>) -> gtk4::Box {
    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.add_css_class("reprise-tag-cover");
    frame.set_overflow(gtk4::Overflow::Hidden);

    let picture = gtk4::Picture::new();
    picture.set_content_fit(gtk4::ContentFit::Cover);
    picture.set_can_shrink(true);

    let loaded = track_path
        .and_then(cover::resolve_source)
        .and_then(|source| cover::thumbnail(&source, ThumbnailSize::Grid).ok());

    if let Some(thumb_path) = loaded {
        let texture = gdk::Texture::from_filename(&thumb_path).ok();
        if let Some(texture) = texture {
            picture.set_paintable(Some(&texture));
        }
    }

    frame.append(&picture);
    frame
}

/// Builds the clickable star rating widget. Returns the container box and
/// a shared `Cell` holding the current rating value.
pub(in crate::ui) fn build_star_rating(value: &MixedValue<i32>) -> (gtk4::Box, Rc<Cell<i32>>) {
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    container.add_css_class("reprise-tag-stars");

    let current = match value {
        MixedValue::Uniform(v) => *v,
        MixedValue::Mixed => 0,
    };
    let rating_value = Rc::new(Cell::new(current));

    for i in 1..=RATING_MAX {
        let btn = gtk4::Button::new();
        btn.add_css_class("flat");
        let label = gtk4::Label::new(None);
        btn.set_child(Some(&label));

        if i <= current {
            label.set_label(STAR_FILLED);
            btn.add_css_class("star-filled");
        } else {
            label.set_label(STAR_OUTLINE);
            btn.add_css_class("star-outline");
        }
        container.append(&btn);
    }

    update_star_display(&container, current);
    (container, rating_value)
}

/// Updates the visual state of star buttons in a rating box.
pub(in crate::ui) fn update_star_display(container: &gtk4::Box, rating: i32) {
    let mut child = container.first_child();
    let mut idx = 1;
    while let Some(widget) = child {
        if let Some(btn) = widget.downcast_ref::<gtk4::Button>() {
            if let Some(label) = btn.child().and_then(|c| c.downcast::<gtk4::Label>().ok()) {
                if idx <= rating {
                    label.set_label(STAR_FILLED);
                    btn.remove_css_class("star-outline");
                    btn.add_css_class("star-filled");
                } else {
                    label.set_label(STAR_OUTLINE);
                    btn.remove_css_class("star-filled");
                    btn.add_css_class("star-outline");
                }
            }
        }
        child = widget.next_sibling();
        idx += 1;
    }
}

/// Wires click handlers on each star button. Clicking star N sets rating
/// to N; clicking the already-selected star clears to 0.
pub(in crate::ui) fn wire_star_clicks(
    container: &gtk4::Box,
    rating_value: &Rc<Cell<i32>>,
    on_changed: &UpdateCallback,
) {
    let mut child = container.first_child();
    let mut idx: i32 = 1;
    while let Some(widget) = child {
        if let Some(btn) = widget.downcast_ref::<gtk4::Button>() {
            let rating_val = rating_value.clone();
            let container_c = container.clone();
            let on_changed_c = on_changed.clone();
            let star_idx = idx;
            btn.connect_clicked(move |_| {
                let current = rating_val.get();
                let new_rating = if current == star_idx { 0 } else { star_idx };
                rating_val.set(new_rating);
                update_star_display(&container_c, new_rating);
                on_changed_c();
            });
        }
        child = widget.next_sibling();
        idx += 1;
    }
}

/// Sets an `EntryRow` text from a `MixedValue<String>`.
pub(in crate::ui) fn set_entry_from_mixed_string(row: &adw::EntryRow, value: &MixedValue<String>) {
    match value {
        MixedValue::Uniform(text) => row.set_text(text),
        MixedValue::Mixed => {
            // Leave empty; the mixed styling/annotation conveys the state.
        }
    }
}

/// Sets an `EntryRow` text from a `MixedValue<Option<u32>>`.
pub(in crate::ui) fn set_entry_from_mixed_number(
    row: &adw::EntryRow,
    value: &MixedValue<Option<u32>>,
) {
    match value {
        MixedValue::Uniform(Some(n)) => row.set_text(&n.to_string()),
        MixedValue::Uniform(None) | MixedValue::Mixed => {}
    }
}

/// Initialises an `AutocompleteEntry` from a `MixedValue`, adding
/// mixed-field annotations in multi-track mode. Returns the annotation label
/// when the field starts Mixed (needed by [`attach_type_to_arm`]).
///
/// TAG-2: a Mixed field stays editable from the start — no click-to-unlock.
/// The dashed/italic `reprise-tag-mixed` styling is purely visual; typing or
/// Backspace/Delete arms it (see [`attach_type_to_arm`]).
pub(in crate::ui) fn init_autocomplete_from_mixed(
    ac: &AutocompleteEntry,
    value: &MixedValue<String>,
    _track_count: usize,
    is_multi: bool,
) -> Option<gtk4::Label> {
    match value {
        MixedValue::Uniform(text) => {
            ac.set_text(text);
            if is_multi {
                add_annotation(ac.row(), &strings::text(strings::TAG_SAME_ON_ALL), false);
            }
            None
        }
        MixedValue::Mixed => {
            if is_multi {
                ac.row().add_css_class("reprise-tag-mixed");
                Some(add_annotation(
                    ac.row(),
                    &strings::text(strings::MULTIPLE_VALUES),
                    false,
                ))
            } else {
                None
            }
        }
    }
}

/// Adds a mixed-field annotation for number fields in multi-track mode.
/// Returns the annotation label (needed by [`attach_type_to_arm`]). Like
/// [`init_autocomplete_from_mixed`], a Mixed number field stays editable.
pub(in crate::ui) fn apply_mixed_annotation_number(
    row: &adw::EntryRow,
    value: &MixedValue<Option<u32>>,
    _track_count: usize,
) -> Option<gtk4::Label> {
    match value {
        MixedValue::Uniform(_) => {
            add_annotation(row, &strings::text(strings::TAG_SAME_ON_ALL), false);
            None
        }
        MixedValue::Mixed => {
            row.add_css_class("reprise-tag-mixed");
            Some(add_annotation(
                row,
                &strings::text(strings::MULTIPLE_VALUES),
                false,
            ))
        }
    }
}

/// Adds a small annotation label as a suffix to an `EntryRow` and returns it.
pub(in crate::ui) fn add_annotation(row: &adw::EntryRow, text: &str, accent: bool) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("reprise-tag-field-annotation");
    if accent {
        label.add_css_class("accent");
    }
    row.add_suffix(&label);
    label
}

// ─────────────────────────── TAG-3: per-track fields ───────────────────────

/// Em dash shown in Title/Track-number when they're locked as per-track
/// (TAG-3) — a single positional glyph, kept at its use site rather than in
/// `strings.rs` (same carve-out as the rating stars).
const PER_TRACK_DASH: &str = "\u{2014}";

/// [`per_track_field_projection`]'s result: what Title/Track-number should
/// show — read-only "—" in Multi mode (TAG-3: "a mass title is always an
/// accident"), a normal editable empty field otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct PerTrackFieldProjection {
    pub(in crate::ui) text: &'static str,
    pub(in crate::ui) editable: bool,
    pub(in crate::ui) has_tooltip: bool,
}

/// TAG-3: Title and Track-number are per-track fields — a mass edit across
/// tracks-with-different-titles/positions is always an accident, so Multi
/// mode locks them to a read-only "—" with an explanatory tooltip rather
/// than letting a batch save clobber every track to the same value.
pub(in crate::ui) fn per_track_field_projection(is_multi: bool) -> PerTrackFieldProjection {
    if is_multi {
        PerTrackFieldProjection {
            text: PER_TRACK_DASH,
            editable: false,
            has_tooltip: true,
        }
    } else {
        PerTrackFieldProjection {
            text: "",
            editable: true,
            has_tooltip: false,
        }
    }
}

/// Applies [`per_track_field_projection`] to a Title/Track-number row: in
/// Multi mode, locks it to "—" with the per-track tooltip and annotation; in
/// Single mode, leaves it alone (caller sets the real value).
pub(in crate::ui) fn apply_per_track_field(row: &adw::EntryRow, is_multi: bool) {
    let projection = per_track_field_projection(is_multi);
    row.set_editable(projection.editable);
    if !projection.editable {
        row.set_text(projection.text);
        row.add_css_class("reprise-tag-per-track");
        if projection.has_tooltip {
            row.set_tooltip_text(Some(&strings::text(strings::TAG_PER_TRACK_TOOLTIP)));
        }
        add_annotation(row, &strings::text(strings::TAG_PER_TRACK), false);
    }
}

// ───────────────────── TAG-2: direct-typable mixed fields ──────────────────

/// TAG-2: the first real keystroke into an unarmed mixed field arms it —
/// only the empty→non-empty transition matters (callers only invoke this
/// while the field is still in its unarmed placeholder state).
pub(in crate::ui) fn mixed_field_arms_on_change(new_text: &str) -> bool {
    !new_text.is_empty()
}

/// TAG-2: Backspace/Delete on an unarmed, still-empty mixed field also arms
/// it, as an explicit "clear for all" — nothing about a mixed field's blank
/// state is a silently swallowed no-op keypress.
pub(in crate::ui) fn mixed_field_key_arms_as_clear(keyval: gdk::Key, text_is_empty: bool) -> bool {
    text_is_empty && matches!(keyval, gdk::Key::BackSpace | gdk::Key::Delete)
}

/// TAG-2: arms a Mixed field on the user's first real interaction — typing a
/// character (`changed` sees non-empty text) or pressing Backspace/Delete
/// while still empty (forced through as a real text round-trip so
/// `tag_editor_dirty`'s own, separately-connected `changed` listener —
/// connected later, in `tag_editor_dirty::wire` — actually observes a
/// change instead of a silently swallowed keypress). Once armed, swaps the
/// dashed "mixed" styling for the accent "armed" look and updates
/// `annotation` to "will be applied to all N" (click-to-unlock's old job,
/// now keystroke-triggered — TAG-2 removes click-to-unlock entirely).
pub(in crate::ui) fn attach_type_to_arm(
    row: &adw::EntryRow,
    annotation: Option<&gtk4::Label>,
    track_count: usize,
) {
    let armed = Rc::new(Cell::new(false));
    let will_apply = strings::tag_will_apply(track_count);

    {
        let row_c = row.clone();
        let annotation_c = annotation.cloned();
        let armed_c = armed.clone();
        let will_apply_c = will_apply.clone();
        row.connect_changed(move |entry| {
            if armed_c.get() {
                return;
            }
            if mixed_field_arms_on_change(&entry.text()) {
                armed_c.set(true);
                arm_mixed_field(&row_c, annotation_c.as_ref(), &will_apply_c);
            }
        });
    }

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let row_c = row.clone();
        let annotation_c = annotation.cloned();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if armed.get() {
                return glib::Propagation::Proceed;
            }
            let text_is_empty = row_c.text().is_empty();
            if mixed_field_key_arms_as_clear(keyval, text_is_empty) {
                armed.set(true);
                // Force a real changed-signal round-trip (empty text alone
                // has nothing to delete, so a plain Backspace never fires
                // `changed`) — this also lets `tag_editor_dirty`'s own
                // listener see it as a genuine pending change.
                row_c.set_text(" ");
                row_c.set_text("");
                arm_mixed_field(&row_c, annotation_c.as_ref(), &will_apply);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    row.add_controller(key_controller);
}

fn arm_mixed_field(row: &adw::EntryRow, annotation: Option<&gtk4::Label>, will_apply: &str) {
    row.remove_css_class("reprise-tag-mixed");
    row.add_css_class("reprise-tag-field-armed");
    if let Some(label) = annotation {
        label.set_text(will_apply);
        label.add_css_class("accent");
    }
}

// ───────────────────────── Header subtitle (Beschluss #2) ──────────────────

/// Single-track header subtitle: "FLAC · 987 kbit/s" — format from the file
/// extension, bitrate from `Track::bitrate_kbps`; either half is omitted if
/// unavailable (never a stray "·" or a fabricated "unknown"). The "Track N
/// of M" position prefix is Package G's job (TAG-4 browse snapshot) — this
/// function only ever renders format/bitrate, by design, until that lands.
pub(in crate::ui) fn format_track_subtitle(
    extension: Option<&str>,
    bitrate_kbps: Option<u32>,
) -> Option<String> {
    let format = extension.map(str::to_uppercase);
    let bitrate = bitrate_kbps.map(|kbps| format!("{kbps} kbit/s"));
    match (format, bitrate) {
        (Some(format), Some(bitrate)) => Some(format!("{format} \u{00B7} {bitrate}")),
        (Some(format), None) => Some(format),
        (None, Some(bitrate)) => Some(bitrate),
        (None, None) => None,
    }
}

// ───────────────────────────── Layout helpers ───────────────────────────────

/// Wraps a field widget in a vertical column: an optional external label on
/// top (used for the star rating, which has no built-in title the way
/// `adw::EntryRow` does), the field itself, then a reserved "was: …" line
/// (TAG-5, P-4) — present and space-allocated even while empty, styled with
/// a permanent strikethrough attribute so later callers only ever need to
/// set its text.
pub(in crate::ui) fn build_field_column(
    field_widget: &gtk4::Widget,
    external_label: Option<&str>,
) -> (gtk4::Box, gtk4::Label) {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    column.add_css_class("reprise-tag-field");

    if let Some(text) = external_label {
        let label = gtk4::Label::builder().label(text).xalign(0.0).build();
        label.add_css_class("reprise-tag-field-label");
        column.append(&label);
    }
    column.append(field_widget);

    let old_value = gtk4::Label::builder().label("").xalign(0.0).build();
    old_value.add_css_class("reprise-tag-old-value");
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrInt::new_strikethrough(true));
    old_value.set_attributes(Some(&attrs));
    column.append(&old_value);

    (column, old_value)
}

/// Returns the original text value for a given field index from a snapshot.
/// Returns `None` for fields that were Mixed (no single value).
pub(in crate::ui) fn field_snapshot_text(
    summary: &EditableTagSummary,
    field_idx: usize,
) -> Option<String> {
    match field_idx {
        FIELD_ARTIST => match &summary.artist {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_ALBUM => match &summary.album {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_ALBUM_ARTIST => match &summary.album_artist {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_GENRE => match &summary.genre {
            MixedValue::Uniform(v) => Some(v.clone()),
            MixedValue::Mixed => None,
        },
        FIELD_YEAR => match &summary.year {
            MixedValue::Uniform(Some(v)) => Some(v.to_string()),
            MixedValue::Uniform(None) | MixedValue::Mixed => None,
        },
        _ => None,
    }
}

/// Returns true if the given field was originally Mixed in the snapshot.
pub(in crate::ui) fn field_snapshot_is_mixed(
    summary: &EditableTagSummary,
    field_idx: usize,
) -> bool {
    match field_idx {
        FIELD_ARTIST => matches!(summary.artist, MixedValue::Mixed),
        FIELD_ALBUM => matches!(summary.album, MixedValue::Mixed),
        FIELD_ALBUM_ARTIST => matches!(summary.album_artist, MixedValue::Mixed),
        FIELD_GENRE => matches!(summary.genre, MixedValue::Mixed),
        FIELD_YEAR => matches!(summary.year, MixedValue::Mixed),
        _ => false,
    }
}

/// Builds a single pending-change item: "Field → Value" with a Revert button.
pub(in crate::ui) fn build_pending_item(
    field_name: &str,
    value: &str,
    on_revert: Box<dyn Fn()>,
) -> gtk4::Box {
    let item = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    item.add_css_class("reprise-tag-pending-item");

    let text = format!("{field_name} \u{2192} {value}");
    let label = gtk4::Label::builder()
        .label(&text)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    item.append(&label);

    let revert_btn = gtk4::Button::with_label(&strings::text(strings::TAG_REVERT));
    revert_btn.add_css_class("flat");
    revert_btn.connect_clicked(move |_| on_revert());
    item.append(&revert_btn);

    item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_3_per_track_fields_render_dash_readonly_in_multi() {
        let multi = per_track_field_projection(true);
        assert_eq!(multi.text, "\u{2014}");
        assert!(!multi.editable);
        assert!(multi.has_tooltip);

        let single = per_track_field_projection(false);
        assert!(single.editable);
        assert!(!single.has_tooltip);
        assert_eq!(single.text, "");
    }

    #[test]
    fn subtitle_omits_missing_bitrate() {
        assert_eq!(
            format_track_subtitle(Some("flac"), None),
            Some("FLAC".to_string())
        );
        assert_eq!(
            format_track_subtitle(Some("flac"), Some(987)),
            Some("FLAC \u{00B7} 987 kbit/s".to_string())
        );
        assert_eq!(
            format_track_subtitle(None, Some(987)),
            Some("987 kbit/s".to_string())
        );
        assert_eq!(format_track_subtitle(None, None), None);
    }

    #[test]
    fn tag_2_first_keystroke_arms_field() {
        assert!(!mixed_field_arms_on_change(""));
        assert!(mixed_field_arms_on_change("a"));
        assert!(mixed_field_arms_on_change("Suicide Silence"));
    }

    #[test]
    fn tag_2_backspace_in_placeholder_arms_as_clear_for_all() {
        assert!(mixed_field_key_arms_as_clear(gdk::Key::BackSpace, true));
        assert!(mixed_field_key_arms_as_clear(gdk::Key::Delete, true));
        assert!(!mixed_field_key_arms_as_clear(gdk::Key::BackSpace, false));
        assert!(!mixed_field_key_arms_as_clear(gdk::Key::a, true));
    }
}
