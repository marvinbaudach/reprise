use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use super::preferences::PreferencesContext;
use super::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceSection {
    WindowDecorations,
}

fn appearance_sections() -> [AppearanceSection; 1] {
    [AppearanceSection::WindowDecorations]
}

pub(super) fn build(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::PREFERENCES_APPEARANCE))
        .icon_name("applications-graphics-symbolic")
        .build();
    for section in appearance_sections() {
        match section {
            AppearanceSection::WindowDecorations => {
                let decorations = adw::PreferencesGroup::builder()
                    .title(super::window_decoration_strings::text(
                        super::window_decoration_strings::WINDOW_DECORATIONS,
                    ))
                    .build();
                decorations.add(&super::preference_window_decorations::row(context));
                page.add(&decorations);
            }
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_page_contains_only_system_integrated_controls() {
        assert_eq!(
            appearance_sections(),
            [AppearanceSection::WindowDecorations]
        );
    }
}
