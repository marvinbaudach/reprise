use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::mix_planner::{
    approve_mix_draft, plan_mix_draft, profile_target_for_available_tracks,
    profile_target_for_tracks, CriteriaMode, EnergyCurve, Familiarity, MixDiagnostic, MixDraft,
    MixIntent, MixSource, ProfileTarget, SelectionReason, Variety,
};
use reprise_core::models::Track;

use super::{show_toast, Shared};
use crate::ui::{dialogs, strings};

#[derive(Default)]
pub(in crate::ui) struct MixBuilderState {
    preview: Option<MixDraft>,
}

impl MixBuilderState {
    pub(in crate::ui) fn set_preview(&mut self, draft: MixDraft) {
        self.preview = Some(draft);
    }

    pub(in crate::ui) fn controls_changed(&mut self) {
        self.preview = None;
    }

    pub(in crate::ui) fn visible_track_ids(&self) -> Vec<i64> {
        self.preview
            .as_ref()
            .map(|draft| draft.tracks.iter().map(|track| track.track_id).collect())
            .unwrap_or_default()
    }

    pub(in crate::ui) fn applicable_draft_id(&self) -> Option<&str> {
        self.preview.as_ref().map(|draft| draft.draft_id.as_str())
    }
}

struct Controls {
    criteria: adw::ComboRow,
    duration: adw::ComboRow,
    familiarity: adw::ComboRow,
    variety: adw::ComboRow,
    energy: adw::ComboRow,
}

pub(in crate::ui) fn present(shared: &Rc<Shared>, seeds: &[Track]) {
    present_with_target(shared, seeds, None);
}

pub(in crate::ui) fn present_target(shared: &Rc<Shared>, target: ProfileTarget) {
    present_with_target(shared, &[], Some(target));
}

fn present_with_target(shared: &Rc<Shared>, seeds: &[Track], target: Option<ProfileTarget>) {
    if seeds.is_empty() && target.is_none() {
        return;
    }
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("mix builder: main window unavailable");
        return;
    };

    let state = Rc::new(RefCell::new(MixBuilderState::default()));
    let page = adw::PreferencesPage::new();
    page.set_margin_top(12);
    page.set_margin_bottom(12);
    page.set_margin_start(12);
    page.set_margin_end(12);

    let seed_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::MIX_BUILDER_SEEDS))
        .build();
    for track in seeds {
        seed_group.add(
            &adw::ActionRow::builder()
                .title(&track.title)
                .subtitle(&track.artist)
                .use_markup(false)
                .build(),
        );
    }
    if target.is_some() {
        seed_group.add(
            &adw::ActionRow::builder()
                .title(strings::text(strings::MIX_BUILDER_STATS_TARGET))
                .subtitle(strings::text(strings::MIX_BUILDER_STATS_TARGET_DESCRIPTION))
                .use_markup(false)
                .build(),
        );
    }
    page.add(&seed_group);

    let options = adw::PreferencesGroup::builder()
        .title(strings::text(strings::MIX_BUILDER_OPTIONS))
        .build();
    let controls = Rc::new(Controls {
        criteria: combo(
            strings::MIX_BUILDER_CRITERION,
            &[
                strings::MIX_BUILDER_CRITERION_BALANCED,
                strings::MIX_BUILDER_CRITERION_AUDIO,
                strings::MIX_BUILDER_CRITERION_GENRE,
                strings::MIX_BUILDER_CRITERION_RELATED,
            ],
            u32::from(target.is_some()),
        ),
        duration: combo(
            strings::MIX_BUILDER_DURATION,
            &[
                strings::MIX_BUILDER_DURATION_30,
                strings::MIX_BUILDER_DURATION_60,
                strings::MIX_BUILDER_DURATION_90,
            ],
            1,
        ),
        familiarity: combo(
            strings::MIX_BUILDER_FAMILIARITY,
            &[
                strings::MIX_BUILDER_FAMILIAR,
                strings::MIX_BUILDER_CRITERION_BALANCED,
                strings::MIX_BUILDER_DISCOVER,
            ],
            1,
        ),
        variety: combo(
            strings::MIX_BUILDER_VARIETY,
            &[
                strings::MIX_BUILDER_COHESIVE,
                strings::MIX_BUILDER_CRITERION_BALANCED,
                strings::MIX_BUILDER_WIDE,
            ],
            1,
        ),
        energy: combo(
            strings::MIX_BUILDER_ENERGY,
            &[
                strings::MIX_BUILDER_FLAT,
                strings::MIX_BUILDER_RISE,
                strings::MIX_BUILDER_FALL,
                strings::MIX_BUILDER_ARC,
            ],
            0,
        ),
    });
    for row in [
        &controls.criteria,
        &controls.duration,
        &controls.familiarity,
        &controls.variety,
        &controls.energy,
    ] {
        options.add(row);
    }
    page.add(&options);

    let preview_group = adw::PreferencesGroup::builder()
        .title(strings::text(strings::MIX_BUILDER_PREVIEW_HEADING))
        .description(strings::text(strings::MIX_BUILDER_PREVIEW_EMPTY))
        .build();
    let preview_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    preview_group.add(&preview_list);
    page.add(&preview_group);
    if !seeds.is_empty() {
        super::mix_builder_discovery::append(
            &page,
            shared,
            seeds.iter().map(|track| track.id).collect(),
        );
    }

    let preview_button = gtk4::Button::with_label(&strings::text(strings::MIX_BUILDER_PREVIEW));
    preview_button.add_css_class("suggested-action");
    let play_button = gtk4::Button::with_label(&strings::text(strings::MIX_BUILDER_PLAY));
    let queue_button = gtk4::Button::with_label(&strings::text(strings::MIX_BUILDER_QUEUE));
    let save_button = gtk4::Button::with_label(&strings::text(strings::MIX_BUILDER_SAVE));
    for button in [&play_button, &queue_button, &save_button] {
        button.set_sensitive(false);
    }
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    actions.set_margin_top(12);
    actions.set_margin_bottom(12);
    actions.set_margin_start(12);
    actions.set_margin_end(12);
    actions.append(&preview_button);
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    actions.append(&spacer);
    actions.append(&play_button);
    actions.append(&queue_button);
    actions.append(&save_button);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&page);
    content.append(&actions);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .title(strings::text(strings::MIX_BUILDER_TITLE))
        .child(&toolbar)
        .content_width(680)
        .content_height(720)
        .build();

    wire_invalidation(
        &controls,
        &state,
        &preview_group,
        &preview_list,
        &[&play_button, &queue_button, &save_button],
    );
    wire_preview(
        shared,
        seeds,
        target,
        &controls,
        &state,
        &preview_group,
        &preview_list,
        &preview_button,
        &[&play_button, &queue_button, &save_button],
    );
    wire_apply_actions(
        shared,
        &window,
        &state,
        &play_button,
        &queue_button,
        &save_button,
    );
    dialog.present(Some(&window));
}

fn combo(title: &str, labels: &[&str], selected: u32) -> adw::ComboRow {
    let translated = labels
        .iter()
        .map(|label| strings::text(label))
        .collect::<Vec<_>>();
    let refs = translated.iter().map(String::as_str).collect::<Vec<_>>();
    adw::ComboRow::builder()
        .title(strings::text(title))
        .model(&gtk4::StringList::new(&refs))
        .selected(selected)
        .build()
}

fn wire_invalidation(
    controls: &Controls,
    state: &Rc<RefCell<MixBuilderState>>,
    group: &adw::PreferencesGroup,
    list: &gtk4::ListBox,
    buttons: &[&gtk4::Button],
) {
    for row in [
        &controls.criteria,
        &controls.duration,
        &controls.familiarity,
        &controls.variety,
        &controls.energy,
    ] {
        let state = Rc::clone(state);
        let group = group.clone();
        let list = list.clone();
        let buttons = buttons
            .iter()
            .map(|button| (*button).clone())
            .collect::<Vec<_>>();
        row.connect_selected_notify(move |_| {
            state.borrow_mut().controls_changed();
            clear_list(&list);
            group.set_description(Some(&strings::text(strings::MIX_BUILDER_PREVIEW_EMPTY)));
            for button in &buttons {
                button.set_sensitive(false);
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn wire_preview(
    shared: &Rc<Shared>,
    seeds: &[Track],
    target: Option<ProfileTarget>,
    controls: &Rc<Controls>,
    state: &Rc<RefCell<MixBuilderState>>,
    group: &adw::PreferencesGroup,
    list: &gtk4::ListBox,
    preview_button: &gtk4::Button,
    apply_buttons: &[&gtk4::Button],
) {
    let shared = Rc::clone(shared);
    let seed_ids = seeds.iter().map(|track| track.id).collect::<Vec<_>>();
    let controls = Rc::clone(controls);
    let state = Rc::clone(state);
    let group = group.clone();
    let list = list.clone();
    let apply_buttons = apply_buttons
        .iter()
        .map(|button| (*button).clone())
        .collect::<Vec<_>>();
    preview_button.connect_clicked(move |_| {
        let result = build_intent(&shared, &seed_ids, target, &controls)
            .and_then(|intent| plan_mix_draft(&shared.conn.borrow(), &intent, unix_now(), 30 * 60));
        clear_list(&list);
        match result {
            Ok(draft) => {
                render_preview(&list, &group, &draft);
                state.borrow_mut().set_preview(draft);
                for button in &apply_buttons {
                    button.set_sensitive(true);
                }
            }
            Err(error) => {
                state.borrow_mut().controls_changed();
                group.set_description(Some(&format!(
                    "{}: {error}",
                    strings::text(strings::MIX_BUILDER_FAILED)
                )));
                for button in &apply_buttons {
                    button.set_sensitive(false);
                }
            }
        }
    });
}

fn build_intent(
    shared: &Shared,
    seed_ids: &[i64],
    target_override: Option<ProfileTarget>,
    controls: &Controls,
) -> Result<MixIntent, reprise_core::mix_planner::MixPlannerError> {
    let criteria = match controls.criteria.selected() {
        1 => CriteriaMode::AudioCharacter,
        2 => CriteriaMode::Genre,
        3 => CriteriaMode::RelatedArtists,
        _ => CriteriaMode::Balanced,
    };
    let target = if matches!(
        criteria,
        CriteriaMode::AudioCharacter | CriteriaMode::Balanced
    ) {
        match target_override {
            Some(target) => target,
            None if criteria == CriteriaMode::Balanced => {
                profile_target_for_available_tracks(&shared.conn.borrow(), seed_ids)?
                    .unwrap_or_else(ProfileTarget::neutral)
            }
            None => profile_target_for_tracks(&shared.conn.borrow(), seed_ids)?,
        }
    } else {
        ProfileTarget::neutral()
    };
    let duration_ms = [30, 60, 90][controls.duration.selected() as usize] * 60_000;
    let familiarity = match controls.familiarity.selected() {
        0 => Familiarity::Familiar,
        2 => Familiarity::Discover,
        _ => Familiarity::Balanced,
    };
    let variety = match controls.variety.selected() {
        0 => Variety::Cohesive,
        2 => Variety::Wide,
        _ => Variety::Balanced,
    };
    let energy = match controls.energy.selected() {
        1 => EnergyCurve::Rise,
        2 => EnergyCurve::Fall,
        3 => EnergyCurve::Arc,
        _ => EnergyCurve::Flat,
    };
    if seed_ids.is_empty() && criteria == CriteriaMode::AudioCharacter {
        return MixIntent::from_target(
            MixSource::Library,
            target,
            duration_ms,
            familiarity,
            variety,
            energy,
        );
    }
    MixIntent::new(
        MixSource::Library,
        seed_ids.to_vec(),
        criteria,
        target,
        duration_ms,
        familiarity,
        variety,
        energy,
    )
}

fn render_preview(list: &gtk4::ListBox, group: &adw::PreferencesGroup, draft: &MixDraft) {
    let minutes = (draft.total_duration_ms + 30_000) / 60_000;
    let count = draft.tracks.len().to_string();
    let minutes = minutes.to_string();
    let analyzed = draft.analyzed_candidates.to_string();
    let total = draft.total_candidates.to_string();
    let diagnostics = diagnostic_suffix(&draft.diagnostics);
    group.set_description(Some(&strings::formatted(
        strings::MIX_BUILDER_SUMMARY,
        &[
            ("count", &count),
            ("minutes", &minutes),
            ("analyzed", &analyzed),
            ("total", &total),
            ("diagnostics", &diagnostics),
        ],
    )));
    for (index, track) in draft.tracks.iter().enumerate() {
        let row = adw::ActionRow::builder()
            .title(format!("{}. {}", index + 1, track.title))
            .subtitle(format!(
                "{} · {}",
                track.artist,
                reason_text(&track.reasons)
            ))
            .use_markup(false)
            .build();
        list.append(&row);
    }
}

fn reason_text(reasons: &[SelectionReason]) -> String {
    reasons
        .iter()
        .map(|reason| match reason {
            SelectionReason::IntensityMatch => strings::MIX_REASON_INTENSITY,
            SelectionReason::BrightnessMatch => strings::MIX_REASON_BRIGHTNESS,
            SelectionReason::DynamicityMatch => strings::MIX_REASON_DYNAMICITY,
            SelectionReason::RhythmicityMatch => strings::MIX_REASON_RHYTHMICITY,
            SelectionReason::GenreMatch => strings::MIX_REASON_GENRE,
            SelectionReason::RelatedArtist => strings::MIX_REASON_RELATED_ARTIST,
            SelectionReason::FamiliarityMatch => strings::MIX_REASON_FAMILIARITY,
            SelectionReason::ArtistDiversity => strings::MIX_REASON_DIVERSITY,
            SelectionReason::DurationFit => strings::MIX_REASON_DURATION,
        })
        .map(strings::text)
        .collect::<Vec<_>>()
        .join(", ")
}

fn diagnostic_suffix(diagnostics: &[MixDiagnostic]) -> String {
    if diagnostics.is_empty() {
        return String::new();
    }
    let details = diagnostics
        .iter()
        .map(|diagnostic| match diagnostic {
            MixDiagnostic::ArtistGapRelaxed => strings::MIX_DIAGNOSTIC_ARTIST_GAP,
            MixDiagnostic::DurationUnderfilled => strings::MIX_DIAGNOSTIC_DURATION,
            MixDiagnostic::MissingAudioEvidence => strings::MIX_DIAGNOSTIC_AUDIO,
            MixDiagnostic::MissingGenreEvidence => strings::MIX_DIAGNOSTIC_GENRE,
            MixDiagnostic::MissingRelatedArtistEvidence => strings::MIX_DIAGNOSTIC_RELATED,
        })
        .map(strings::text)
        .collect::<Vec<_>>()
        .join(", ");
    let count = diagnostics.len().to_string();
    strings::formatted(
        strings::MIX_BUILDER_DIAGNOSTICS,
        &[("count", &count), ("details", &details)],
    )
}

fn wire_apply_actions(
    shared: &Rc<Shared>,
    window: &adw::ApplicationWindow,
    state: &Rc<RefCell<MixBuilderState>>,
    play: &gtk4::Button,
    queue: &gtk4::Button,
    save: &gtk4::Button,
) {
    {
        let shared = Rc::clone(shared);
        let state = Rc::clone(state);
        play.connect_clicked(move |_| {
            let ids = state.borrow().visible_track_ids();
            let callback = shared.on_play_mix.borrow().clone();
            if let Some(callback) = callback {
                callback(ids);
            }
        });
    }
    {
        let shared = Rc::clone(shared);
        let state = Rc::clone(state);
        queue.connect_clicked(move |_| {
            let ids = state.borrow().visible_track_ids();
            let callback = shared.on_queue_selected.borrow().clone();
            if let Some(callback) = callback {
                callback(ids);
            }
        });
    }
    {
        let shared = Rc::clone(shared);
        let state = Rc::clone(state);
        let window = window.clone();
        save.connect_clicked(move |_| {
            let Some(draft_id) = state.borrow().applicable_draft_id().map(str::to_owned) else {
                return;
            };
            let shared = Rc::clone(&shared);
            dialogs::prompt_name(
                &window,
                &strings::text(strings::MIX_BUILDER_SAVE_TITLE),
                &strings::text(strings::MIX_BUILDER_SAVE_PLACEHOLDER),
                &strings::text(strings::MIX_BUILDER_SAVE_ACTION),
                move |name| {
                    let request_id = format!("gtk:{draft_id}");
                    let result = approve_mix_draft(
                        &mut shared.conn.borrow_mut(),
                        &draft_id,
                        &name,
                        &request_id,
                        unix_now(),
                    );
                    match result {
                        Ok(_) => {
                            let callback = shared.on_playlist_mutated.borrow().clone();
                            if let Some(callback) = callback {
                                callback();
                            }
                            show_toast(&shared, &strings::text(strings::MIX_BUILDER_SAVED));
                        }
                        Err(error) => {
                            tracing::error!(%error, "mix builder: playlist approval failed");
                            show_toast(&shared, &strings::text(strings::MIX_BUILDER_SAVE_FAILED));
                        }
                    }
                },
            );
        });
    }
}

fn clear_list(list: &gtk4::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use reprise_core::mix_planner::{
        CriteriaMode, EnergyCurve, Familiarity, MixDraft, MixDraftTrack, MixIntent, MixSource,
        ProfileTarget, Variety,
    };

    use super::MixBuilderState;

    fn draft() -> MixDraft {
        MixDraft {
            draft_id: "draft-1".into(),
            intent: MixIntent::new(
                MixSource::Library,
                vec![9],
                CriteriaMode::Balanced,
                ProfileTarget::neutral(),
                3_600_000,
                Familiarity::Balanced,
                Variety::Balanced,
                EnergyCurve::Flat,
            )
            .unwrap(),
            tracks: [3, 1, 2]
                .into_iter()
                .map(|track_id| MixDraftTrack {
                    track_id,
                    title: format!("Track {track_id}"),
                    artist: "Artist".into(),
                    album: "Album".into(),
                    duration_ms: 180_000,
                    score: 0.1,
                    profile_intensity: 0.5,
                    reasons: Vec::new(),
                })
                .collect(),
            total_duration_ms: 540_000,
            analyzed_candidates: 8,
            total_candidates: 10,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn ac_14_controls_invalidate_the_exact_visible_draft() {
        let mut state = MixBuilderState::default();
        state.set_preview(draft());
        assert_eq!(state.visible_track_ids(), vec![3, 1, 2]);
        assert_eq!(state.applicable_draft_id(), Some("draft-1"));

        state.controls_changed();
        assert!(state.visible_track_ids().is_empty());
        assert_eq!(state.applicable_draft_id(), None);
    }
}
