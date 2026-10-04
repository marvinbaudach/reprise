use crate::ui::sidebar::sidebar_presentation::NavIcon;
use crate::ui::source_empty_state::SourceEmptyStateCopy;
use crate::ui::strings;

pub(super) fn empty_state() -> SourceEmptyStateCopy {
    SourceEmptyStateCopy {
        icon_name: NavIcon::Radio.icon_name(),
        title: strings::text(strings::RADIO_NO_STATIONS),
        body: strings::text(strings::RADIO_NO_STATIONS_DESCRIPTION),
        button_label: strings::text(strings::RADIO_ADD),
        button_icon_name: "list-add-symbolic",
        secondary_line: None,
    }
}

pub(super) fn module_off() -> SourceEmptyStateCopy {
    SourceEmptyStateCopy {
        icon_name: NavIcon::Radio.icon_name(),
        title: strings::podcast_source_off_title(&strings::text(strings::RADIO)),
        body: strings::text(strings::RADIO_SOURCE_OFF_DESCRIPTION),
        button_label: strings::text(strings::PODCAST_ENABLE_IN_PREFERENCES),
        button_icon_name: "network-server-symbolic",
        secondary_line: None,
    }
}
