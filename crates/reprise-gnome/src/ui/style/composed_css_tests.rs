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

#[test]
#[ignore = "probe: prints the composed stylesheet's parse errors with context"]
fn probe_composed_css_errors() {
    gtk4::init().unwrap();
    let css = super::app_css();
    let lines: Vec<&str> = css.lines().collect();
    let errors = super::css_parse_errors(&css);
    println!(
        "composed stylesheet: {} lines, {} errors",
        lines.len(),
        errors.len()
    );
    for error in &errors {
        println!("  {error}");
    }
    for number in [515usize, 550] {
        if let Some(line) = lines.get(number - 1) {
            let shown: String = line.chars().take(240).collect();
            println!("line {number}: {shown}");
        }
    }
}
