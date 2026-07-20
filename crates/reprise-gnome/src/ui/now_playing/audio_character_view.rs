//! Audio Character projection and widgets for the loaded track.

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::{PlaybackState, SpectrumFrame};
use reprise_core::sound_profile::{
    self, AnalysisState, AnalysisVersions, ReadyAnalysis, TrackAnalysis,
};
use rusqlite::Connection;

use crate::ui::strings;

use super::song_visualizer::SongVisualizer;

const READY_PAGE: &str = "ready";
const STATUS_PAGE: &str = "status";
const DIMENSION_NAMES: [&str; 4] = [
    strings::AUDIO_CHARACTER_INTENSITY,
    strings::AUDIO_CHARACTER_BRIGHTNESS,
    strings::AUDIO_CHARACTER_DYNAMICITY,
    strings::AUDIO_CHARACTER_RHYTHMICITY,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct DimensionPresentation {
    pub name: &'static str,
    pub value_percent: u8,
    pub confidence_percent: u8,
    pub accessible_label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct TempoPresentation {
    pub bpm: u16,
    pub confidence_percent: u8,
    pub accessible_label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) enum AudioCharacterPresentation {
    Empty,
    Disabled,
    Pending,
    Failed,
    Stale,
    Ready {
        dimensions: Box<[DimensionPresentation; 4]>,
        tempo: Option<TempoPresentation>,
    },
}

fn percent(value: f64) -> u8 {
    (value * 100.0).round().clamp(0.0, 100.0) as u8
}

pub(in crate::ui) fn dimensions(analysis: &ReadyAnalysis) -> [DimensionPresentation; 4] {
    let profile = analysis.profile;
    let values = [
        profile.intensity,
        profile.brightness,
        profile.dynamicity,
        profile.rhythmicity,
    ];
    std::array::from_fn(|index| {
        let name = DIMENSION_NAMES[index];
        let value_percent = percent(values[index].value().get());
        let confidence_percent = percent(values[index].confidence().get());
        DimensionPresentation {
            name,
            value_percent,
            confidence_percent,
            accessible_label: strings::formatted(
                strings::AUDIO_CHARACTER_DIMENSION_ACCESSIBLE,
                &[
                    ("dimension", &strings::text(name)),
                    ("value", &value_percent.to_string()),
                    ("confidence", &confidence_percent.to_string()),
                ],
            ),
        }
    })
}

pub(in crate::ui) fn presentation(
    enabled: bool,
    state: Option<AnalysisState>,
    analysis: Option<&ReadyAnalysis>,
) -> AudioCharacterPresentation {
    let Some(state) = state else {
        return AudioCharacterPresentation::Empty;
    };
    if !enabled {
        return AudioCharacterPresentation::Disabled;
    }
    match state {
        AnalysisState::Ineligible | AnalysisState::Pending => AudioCharacterPresentation::Pending,
        AnalysisState::Failed => AudioCharacterPresentation::Failed,
        AnalysisState::Stale => AudioCharacterPresentation::Stale,
        AnalysisState::Ready => {
            let Some(analysis) = analysis else {
                return AudioCharacterPresentation::Failed;
            };
            let tempo = analysis.evidence.tempo().map(|tempo| {
                let bpm = tempo.bpm().round().clamp(1.0, f64::from(u16::MAX)) as u16;
                let confidence_percent = percent(tempo.confidence().get());
                TempoPresentation {
                    bpm,
                    confidence_percent,
                    accessible_label: strings::formatted(
                        strings::AUDIO_CHARACTER_TEMPO_ACCESSIBLE,
                        &[
                            ("bpm", &bpm.to_string()),
                            ("confidence", &confidence_percent.to_string()),
                        ],
                    ),
                }
            });
            AudioCharacterPresentation::Ready {
                dimensions: Box::new(dimensions(analysis)),
                tempo,
            }
        }
    }
}

pub(in crate::ui) fn load_presentation(
    conn: &Connection,
    track_id: Option<i64>,
) -> AudioCharacterPresentation {
    let Some(track_id) = track_id else {
        return AudioCharacterPresentation::Empty;
    };
    let enabled = reprise_core::library::settings::get_audio_analysis_enabled(conn);
    let versions = AnalysisVersions::new(
        reprise_core::audio_analysis::CURRENT_EXTRACTOR_VERSION,
        sound_profile::CURRENT_PROFILE_VERSION,
    )
    .expect("built-in Audio Character versions are nonzero");
    let state = match sound_profile::analysis_state(conn, track_id, versions) {
        Ok(state) => state,
        Err(error) => {
            tracing::warn!(%error, track_id, "could not load Audio Character state");
            return AudioCharacterPresentation::Failed;
        }
    };
    let analysis = if state == AnalysisState::Ready {
        match sound_profile::load_analysis(conn, track_id) {
            Ok(Some(TrackAnalysis::Ready(analysis))) => Some(analysis),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(%error, track_id, "could not load Audio Character profile");
                None
            }
        }
    } else {
        None
    };
    presentation(enabled, Some(state), analysis.as_ref())
}

pub(in crate::ui) fn result_is_current(
    requested_generation: u64,
    current_generation: u64,
    requested_track: Option<i64>,
    current_track: Option<i64>,
) -> bool {
    requested_generation == current_generation && requested_track == current_track
}

#[cfg(test)]
pub(in crate::ui) fn visible_text(presentation: &AudioCharacterPresentation) -> String {
    match presentation {
        AudioCharacterPresentation::Empty => strings::AUDIO_CHARACTER_EMPTY.to_owned(),
        AudioCharacterPresentation::Disabled => strings::AUDIO_CHARACTER_DISABLED.to_owned(),
        AudioCharacterPresentation::Pending => strings::AUDIO_CHARACTER_PENDING.to_owned(),
        AudioCharacterPresentation::Failed => strings::AUDIO_CHARACTER_FAILED.to_owned(),
        AudioCharacterPresentation::Stale => strings::AUDIO_CHARACTER_STALE.to_owned(),
        AudioCharacterPresentation::Ready { dimensions, tempo } => {
            let mut text = dimensions
                .iter()
                .map(|dimension| dimension.accessible_label.clone())
                .collect::<Vec<_>>();
            if let Some(tempo) = tempo {
                text.push(tempo.accessible_label.clone());
            }
            text.join(" · ")
        }
    }
}

#[derive(Clone)]
struct DimensionWidgets {
    value: gtk4::Label,
    confidence: gtk4::Label,
    bar: gtk4::ProgressBar,
}

#[derive(Clone)]
pub(in crate::ui) struct AudioCharacterView {
    root: gtk4::Box,
    stack: gtk4::Stack,
    status: adw::StatusPage,
    dimensions: Vec<DimensionWidgets>,
    tempo: gtk4::Box,
    tempo_value: gtk4::Label,
    tempo_confidence: gtk4::Label,
    visualizer: SongVisualizer,
}

impl AudioCharacterView {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let visualizer = SongVisualizer::new();
        visualizer.widget().set_visible(false);
        root.append(visualizer.widget());

        let stack = gtk4::Stack::new();
        stack.set_vexpand(true);
        root.append(&stack);
        let status = adw::StatusPage::new();
        status.set_icon_name(Some("audio-x-generic-symbolic"));
        stack.add_named(&status, Some(STATUS_PAGE));

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(18);
        content.set_margin_end(18);
        let mut dimension_widgets = Vec::with_capacity(DIMENSION_NAMES.len());
        for name in DIMENSION_NAMES {
            let block = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            let heading = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            let title = gtk4::Label::builder()
                .label(strings::text(name))
                .xalign(0.0)
                .hexpand(true)
                .build();
            let value = gtk4::Label::builder().xalign(1.0).build();
            heading.append(&title);
            heading.append(&value);
            let bar = gtk4::ProgressBar::new();
            let confidence = gtk4::Label::builder().xalign(0.0).build();
            confidence.add_css_class("dim-label");
            block.append(&heading);
            block.append(&bar);
            block.append(&confidence);
            content.append(&block);
            dimension_widgets.push(DimensionWidgets {
                value,
                confidence,
                bar,
            });
        }

        let tempo = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        tempo.add_css_class("reprise-audio-character-tempo");
        let tempo_title = gtk4::Label::builder()
            .label(strings::text(strings::AUDIO_CHARACTER_TEMPO))
            .xalign(0.0)
            .build();
        let tempo_value = gtk4::Label::builder().xalign(0.0).build();
        let tempo_confidence = gtk4::Label::builder().xalign(0.0).build();
        tempo_confidence.add_css_class("dim-label");
        tempo.append(&tempo_title);
        tempo.append(&tempo_value);
        tempo.append(&tempo_confidence);
        content.append(&tempo);

        let scroller = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&content)
            .build();
        stack.add_named(&scroller, Some(READY_PAGE));
        Self {
            root,
            stack,
            status,
            dimensions: dimension_widgets,
            tempo,
            tempo_value,
            tempo_confidence,
            visualizer,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_visuals_enabled(&self, enabled: bool) {
        self.visualizer.widget().set_visible(enabled);
        if !enabled {
            self.visualizer.set_active(false);
            self.visualizer.close_fullscreen();
        }
    }

    pub(in crate::ui) fn set_visual_active(&self, active: bool) {
        self.visualizer
            .set_active(active && self.visualizer.widget().is_visible());
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        self.visualizer.set_spectrum(frame);
    }

    pub(in crate::ui) fn set_playback_state(&self, state: PlaybackState) {
        self.visualizer.set_playback_state(state);
    }

    pub(in crate::ui) fn toggle_visual_fullscreen(&self, parent: &adw::ApplicationWindow) {
        self.visualizer.toggle_fullscreen(parent);
    }

    pub(in crate::ui) fn render(&self, presentation: &AudioCharacterPresentation) {
        if let AudioCharacterPresentation::Ready { dimensions, tempo } = presentation {
            self.visualizer.set_profile(
                &dimensions
                    .as_ref()
                    .each_ref()
                    .map(|dimension| dimension.value_percent),
            );
            for (widgets, dimension) in self.dimensions.iter().zip(dimensions.iter()) {
                widgets
                    .value
                    .set_label(&format!("{}%", dimension.value_percent));
                widgets.confidence.set_label(&strings::formatted(
                    strings::AUDIO_CHARACTER_CONFIDENCE,
                    &[("confidence", &dimension.confidence_percent.to_string())],
                ));
                widgets
                    .bar
                    .set_fraction(f64::from(dimension.value_percent) / 100.0);
                widgets.bar.update_property(&[
                    gtk4::accessible::Property::Label(&dimension.accessible_label),
                    gtk4::accessible::Property::ValueMin(0.0),
                    gtk4::accessible::Property::ValueMax(100.0),
                    gtk4::accessible::Property::ValueNow(f64::from(dimension.value_percent)),
                    gtk4::accessible::Property::ValueText(&format!("{}%", dimension.value_percent)),
                ]);
            }
            self.render_tempo(tempo.as_ref());
            self.stack.set_visible_child_name(READY_PAGE);
            return;
        }
        let (title, description, icon) = status_copy(presentation);
        self.status.set_title(&strings::text(title));
        self.status
            .set_description(Some(&strings::text(description)));
        self.status.set_icon_name(Some(icon));
        self.stack.set_visible_child_name(STATUS_PAGE);
    }

    fn render_tempo(&self, tempo: Option<&TempoPresentation>) {
        self.tempo.set_visible(tempo.is_some());
        let Some(tempo) = tempo else {
            return;
        };
        self.tempo_value.set_label(&strings::formatted(
            strings::AUDIO_CHARACTER_BPM,
            &[("bpm", &tempo.bpm.to_string())],
        ));
        self.tempo_confidence.set_label(&strings::formatted(
            strings::AUDIO_CHARACTER_CONFIDENCE,
            &[("confidence", &tempo.confidence_percent.to_string())],
        ));
        self.tempo
            .update_property(&[gtk4::accessible::Property::Label(&tempo.accessible_label)]);
    }
}

fn status_copy(
    presentation: &AudioCharacterPresentation,
) -> (&'static str, &'static str, &'static str) {
    match presentation {
        AudioCharacterPresentation::Empty => (
            strings::AUDIO_CHARACTER_EMPTY,
            strings::AUDIO_CHARACTER_EMPTY_DESCRIPTION,
            "audio-x-generic-symbolic",
        ),
        AudioCharacterPresentation::Disabled => (
            strings::AUDIO_CHARACTER_DISABLED,
            strings::AUDIO_CHARACTER_DISABLED_DESCRIPTION,
            "changes-prevent-symbolic",
        ),
        AudioCharacterPresentation::Pending => (
            strings::AUDIO_CHARACTER_PENDING,
            strings::AUDIO_CHARACTER_PENDING_DESCRIPTION,
            "content-loading-symbolic",
        ),
        AudioCharacterPresentation::Failed => (
            strings::AUDIO_CHARACTER_FAILED,
            strings::AUDIO_CHARACTER_FAILED_DESCRIPTION,
            "dialog-warning-symbolic",
        ),
        AudioCharacterPresentation::Stale => (
            strings::AUDIO_CHARACTER_STALE,
            strings::AUDIO_CHARACTER_STALE_DESCRIPTION,
            "view-refresh-symbolic",
        ),
        AudioCharacterPresentation::Ready { .. } => unreachable!("ready uses profile widgets"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::sound_profile::{
        AnalysisState, AnalysisVersions, AudioEvidence, ProfileDimension, ReadyAnalysis,
        SoundProfile, SourceFingerprint, TempoEstimate,
    };

    fn ready(tempo: Option<TempoEstimate>) -> ReadyAnalysis {
        ReadyAnalysis::new(
            SourceFingerprint::new(10, 20).unwrap(),
            AnalysisVersions::new(1, 1).unwrap(),
            100,
            AudioEvidence::new(0.4, 0.2, 2_000.0, 4_000.0, 0.3, 2.0, tempo).unwrap(),
            SoundProfile::new(
                ProfileDimension::new(0.72, 0.91).unwrap(),
                ProfileDimension::new(0.43, 0.82).unwrap(),
                ProfileDimension::new(0.61, 0.73).unwrap(),
                ProfileDimension::new(0.88, 0.64).unwrap(),
            ),
        )
        .unwrap()
    }

    #[test]
    fn ac_5_ready_has_four_named_numeric_dimensions_and_optional_tempo() {
        let analysis = ready(Some(TempoEstimate::new(128.4, 0.76).unwrap()));
        let presentation = presentation(true, Some(AnalysisState::Ready), Some(&analysis));
        let AudioCharacterPresentation::Ready { dimensions, tempo } = presentation else {
            panic!("ready analysis did not project to ready UI");
        };

        assert_eq!(
            dimensions
                .as_ref()
                .each_ref()
                .map(|dimension| dimension.name),
            ["Intensity", "Brightness", "Dynamicity", "Rhythmicity"]
        );
        assert_eq!(
            dimensions
                .as_ref()
                .each_ref()
                .map(|dimension| dimension.value_percent),
            [72, 43, 61, 88]
        );
        assert!(dimensions.iter().all(|dimension| {
            dimension.accessible_label.contains(dimension.name)
                && dimension.accessible_label.contains('%')
        }));
        let tempo = tempo.expect("tempo estimate");
        assert_eq!(tempo.bpm, 128);
        assert_eq!(tempo.confidence_percent, 76);
    }

    #[test]
    fn ac_5_missing_tempo_never_invents_zero_bpm() {
        let analysis = ready(None);
        let presentation = presentation(true, Some(AnalysisState::Ready), Some(&analysis));
        let AudioCharacterPresentation::Ready { tempo, .. } = presentation else {
            panic!("ready analysis did not project to ready UI");
        };
        assert!(tempo.is_none());
        assert!(!visible_text(&AudioCharacterPresentation::Ready {
            dimensions: Box::new(dimensions(&analysis)),
            tempo,
        })
        .contains("0 BPM"));
    }

    #[test]
    fn ac_4_disabled_pending_failed_stale_and_empty_are_distinct() {
        let cases = [
            (
                presentation(false, Some(AnalysisState::Ready), None),
                "disabled",
            ),
            (
                presentation(true, Some(AnalysisState::Pending), None),
                "pending",
            ),
            (
                presentation(true, Some(AnalysisState::Failed), None),
                "failed",
            ),
            (
                presentation(true, Some(AnalysisState::Stale), None),
                "stale",
            ),
            (presentation(true, None, None), "play a track"),
        ];
        let texts = cases
            .iter()
            .map(|(state, marker)| {
                let text = visible_text(state).to_lowercase();
                assert!(text.contains(marker));
                text
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(texts.len(), cases.len());
    }

    #[test]
    fn ac_4_generation_rejects_a_previous_track_result() {
        assert!(result_is_current(4, 4, Some(7), Some(7)));
        assert!(!result_is_current(3, 4, Some(7), Some(7)));
        assert!(!result_is_current(4, 4, Some(7), Some(8)));
    }

    #[test]
    fn ac_5_visible_copy_never_exposes_paths_versions_or_mood_claims() {
        let analysis = ready(Some(TempoEstimate::new(120.0, 0.5).unwrap()));
        let text = visible_text(&presentation(
            true,
            Some(AnalysisState::Ready),
            Some(&analysis),
        ));
        for forbidden in ["/home/", "extractor", "profile version", "happy", "sad"] {
            assert!(!text.to_lowercase().contains(forbidden));
        }
    }

    #[test]
    fn loaded_track_projection_follows_opt_in_and_current_storage_state() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        conn.execute(
            "INSERT INTO tracks
               (id, path, title, artist, added_at, file_mtime, file_size)
             VALUES (7, '/not-visible.flac', 'Fixture', 'Artist', 1, 10, 20)",
            [],
        )
        .unwrap();
        assert_eq!(
            load_presentation(&conn, Some(7)),
            AudioCharacterPresentation::Disabled
        );
        reprise_core::library::settings::set_audio_analysis_enabled(&conn, true).unwrap();
        assert_eq!(
            load_presentation(&conn, Some(7)),
            AudioCharacterPresentation::Pending
        );
        sound_profile::save_ready_analysis(&conn, 7, &ready(None)).unwrap();
        assert!(matches!(
            load_presentation(&conn, Some(7)),
            AudioCharacterPresentation::Ready { .. }
        ));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ac_5_ready_widgets_repeat_values_for_sight_and_screenreaders() {
        gtk4::init().unwrap();
        let view = AudioCharacterView::new();
        let presentation = presentation(
            true,
            Some(AnalysisState::Ready),
            Some(&ready(Some(TempoEstimate::new(128.0, 0.76).unwrap()))),
        );

        view.render(&presentation);

        assert_eq!(view.stack.visible_child_name().as_deref(), Some(READY_PAGE));
        assert_eq!(view.dimensions.len(), 4);
        for dimension in &view.dimensions {
            assert!(dimension.value.text().ends_with('%'));
            assert!(dimension.confidence.text().contains("Confidence"));
            assert!(dimension.bar.fraction() > 0.0);
            assert!(gtk4::test_accessible_has_property(
                &dimension.bar,
                gtk4::AccessibleProperty::Label
            ));
            assert!(gtk4::test_accessible_has_property(
                &dimension.bar,
                gtk4::AccessibleProperty::ValueNow
            ));
        }
        assert!(view.tempo.is_visible());
        assert_eq!(view.tempo_value.text(), "128 BPM");
    }
}
