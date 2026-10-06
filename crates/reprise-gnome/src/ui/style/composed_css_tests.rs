/// The stylesheet the app actually installs must parse without a single
/// error.
///
/// Every feature module has its own parse test, but each runs on that
/// module's section in isolation, and a section can be individually
/// well-formed while using a property or value GTK4 does not have. That is
/// not theoretical: two rules shipped inert for months — an `overflow`
/// clip (no such property in GTK4) and the mini player's whole transparency
/// fix, whose `!important` GTK4 rejects as junk, taking all five of its
/// declarations with it. Both looked fine in their own module's test and
/// only ever complained into the running app's log, where nobody reads.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_composed_stylesheet_parses_without_errors() {
    gtk4::init().unwrap();
    let errors = super::css_parse_errors(&super::app_css());
    assert!(
        errors.is_empty(),
        "the installed stylesheet has {} parser error(s):\n  {}",
        errors.len(),
        errors.join("\n  ")
    );
}
