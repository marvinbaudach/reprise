//! Redesign context/popup menu chrome. The palette already themes the menu
//! background (`popover_bg_color`); this adds the redesign's rounding and an
//! accent hover highlight on items. Installed app-wide by [`super`].

pub(super) fn css() -> String {
    use super::tokens::RADIUS_SURFACE;
    format!(
        "popover.menu > contents {{ border-radius: {RADIUS_SURFACE}; padding: 6px; }}\n\
         popover.menu modelbutton {{ border-radius: 8px; padding: 6px 10px; }}\n\
         /* The accent hover is this family's designed language and stays. \
            `style::buttons` is composed earlier in `app_css`, so this rule \
            wins the hover while the central set supplies the press and focus \
            states menus were missing entirely (BTN-1). */\n\
         popover.menu modelbutton:hover {{ \
           background-color: alpha(@accent_bg_color, 0.18); }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn css_rounds_menus_and_accents_item_hover() {
        let css = super::css();
        assert!(css.contains("popover.menu > contents"));
        assert!(css.contains("border-radius"));
        assert!(css.contains("modelbutton:hover"));
        assert!(css.contains("@accent_bg_color"));
    }
}
