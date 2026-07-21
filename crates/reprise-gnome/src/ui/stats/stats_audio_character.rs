use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::listened_audio_character::{ListenedAudioCharacter, ProfileDirection};
use reprise_core::mix_planner::ProfileTarget;

use crate::ui::strings;

#[derive(Debug, Eq, PartialEq)]
pub(in crate::ui) struct AudioCharacterCopy {
    pub(in crate::ui) title: String,
    pub(in crate::ui) subtitle: String,
}

pub(in crate::ui) fn presentation(insight: &ListenedAudioCharacter) -> AudioCharacterCopy {
    let title = match insight.direction {
        ProfileDirection::Intensity => strings::STATS_AUDIO_INTENSITY,
        ProfileDirection::Brightness => strings::STATS_AUDIO_BRIGHTNESS,
        ProfileDirection::Dynamicity => strings::STATS_AUDIO_DYNAMICITY,
        ProfileDirection::Rhythmicity => strings::STATS_AUDIO_RHYTHMICITY,
    };
    let coverage = if insight.total_plays == 0 {
        0
    } else {
        insight.analyzed_plays * 100 / insight.total_plays
    };
    AudioCharacterCopy {
        title: strings::text(title),
        subtitle: strings::stats_audio_evidence(insight.analyzed_plays, coverage),
    }
}

type TargetCallback = Rc<dyn Fn(ProfileTarget)>;

#[derive(Clone)]
pub(in crate::ui) struct StatsAudioCharacter {
    root: gtk4::ListBox,
    row: adw::ActionRow,
    target: Rc<Cell<Option<ProfileTarget>>>,
    callback: Rc<RefCell<Option<TargetCallback>>>,
}

impl StatsAudioCharacter {
    pub(in crate::ui) fn new() -> Self {
        let row = adw::ActionRow::builder().use_markup(false).build();
        let button =
            gtk4::Button::with_label(&strings::text(strings::CONTEXT_MENU_CREATE_SIMILAR_MIX));
        button.set_valign(gtk4::Align::Center);
        row.add_suffix(&button);
        row.set_activatable_widget(Some(&button));
        let root = gtk4::ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        root.append(&row);
        root.set_visible(false);
        let target = Rc::new(Cell::new(None::<ProfileTarget>));
        let callback = Rc::new(RefCell::new(None::<TargetCallback>));
        button.connect_clicked({
            let target = Rc::clone(&target);
            let callback = Rc::clone(&callback);
            move |_| {
                let callback = callback.borrow().clone();
                if let (Some(target), Some(callback)) = (target.get(), callback) {
                    callback(target);
                }
            }
        });
        Self {
            root,
            row,
            target,
            callback,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ListBox {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, insight: Option<&ListenedAudioCharacter>) {
        let Some(insight) = insight else {
            self.target.set(None);
            self.root.set_visible(false);
            return;
        };
        let copy = presentation(insight);
        self.row.set_title(&copy.title);
        self.row.set_subtitle(&copy.subtitle);
        self.target.set(Some(insight.target));
        self.root.set_visible(true);
    }

    pub(in crate::ui) fn set_on_create_mix(&self, callback: impl Fn(ProfileTarget) + 'static) {
        *self.callback.borrow_mut() = Some(Rc::new(callback));
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::library::listened_audio_character::{
        ListenedAudioCharacter, ProfileDirection,
    };
    use reprise_core::mix_planner::ProfileTarget;

    use super::presentation;

    #[test]
    fn listened_profile_copy_is_qualified_and_names_its_evidence() {
        let insight = ListenedAudioCharacter {
            target: ProfileTarget::new(0.8, 0.2, 0.3, 0.4).unwrap(),
            direction: ProfileDirection::Intensity,
            analyzed_plays: 21,
            total_plays: 30,
        };
        let copy = presentation(&insight);
        assert_eq!(copy.title, "Your listening leaned toward higher intensity");
        assert_eq!(copy.subtitle, "Based on 21 analyzed plays · 70% coverage");
        assert!(!copy.title.contains("mood"));
    }
}
