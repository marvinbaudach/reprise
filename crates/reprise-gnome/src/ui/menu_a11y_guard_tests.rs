//! Source scans that keep every popover menu behind `menu_a11y`.
//!
//! They read the production sources under `src/ui` and fail when a new site
//! builds a popover menu or a menu button's model without the naming helper.
//! The scans are textual, so each rule is a heuristic and is spelled out where
//! it is applied.

use std::path::{Path, PathBuf};

/// The production code of `source`: everything before a trailing
/// `#[cfg(test)] mod …` (inline or `#[path]`-wired test module), without
/// comment lines. Test modules sit at the end of a file here, and a
/// `#[cfg(test)]` on a field or function in the middle of a file must not hide
/// what follows it, so only a `mod` item ends the production code.
fn production_code(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let next_item = lines[index + 1..]
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.starts_with("#["));
        if next_item.is_some_and(is_module_item) {
            end = index;
            break;
        }
    }
    lines[..end]
        .iter()
        .filter(|line| !line.trim_start().starts_with("//"))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_module_item(line: &str) -> bool {
    let line = line
        .strip_prefix("pub")
        .map_or(line, |rest| match rest.find(')') {
            Some(close) if rest.starts_with('(') => rest[close + 1..].trim_start(),
            _ => rest.trim_start(),
        });
    line.starts_with("mod ")
}

/// A `PopoverMenu` constructor: any associated function called on the type.
/// `downcast::<PopoverMenu>()` and type annotations have no `::` after the name.
fn builds_a_popover_menu(code: &str) -> bool {
    code.match_indices("PopoverMenu::").any(|(at, name)| {
        let rest = &code[at + name.len()..];
        // `PopoverMenuBar::` and friends are other types.
        rest.starts_with(|c: char| c.is_ascii_lowercase())
    })
}

/// How many menu models `code` gives to a `MenuButton`, each of which needs its
/// own `name_menu_button_items` call.
///
/// A site is `.menu_model(<argument>)` on a builder, or `.set_menu_model(` on a
/// receiver whose last name does not mention a popover. The argument may sit on
/// the next line; `.menu_model()` with no argument is the getter. Whitespace and
/// line breaks between the tokens are ignored.
fn menu_button_model_sites(code: &str) -> usize {
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let builder = compact
        .match_indices(".menu_model(")
        .filter(|(at, name)| !compact[at + name.len()..].starts_with(')'))
        .count();
    let setter = compact
        .match_indices(".set_menu_model(")
        .filter(|(at, _)| !receiver_is_a_popover(&compact[..*at]))
        .count();
    builder + setter
}

/// Whether the expression ending at `before_dot` names a popover, by its last
/// identifier (`self.popover`, `menu_popover`, `popover`).
fn receiver_is_a_popover(before_dot: &str) -> bool {
    let end = before_dot.trim_end_matches(')');
    let start = end
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(0, |at| at + 1);
    end[start..].to_ascii_lowercase().contains("popover")
}

fn named_menu_button_calls(code: &str) -> usize {
    code.matches("menu_a11y::name_menu_button_items(").count()
}

fn ui_sources(directory: &Path, found: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            ui_sources(&path, found);
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let is_production_source =
            name.ends_with(".rs") && !name.ends_with("_tests.rs") && !name.starts_with("menu_a11y");
        if is_production_source {
            if let Ok(source) = std::fs::read_to_string(&path) {
                found.push((path, production_code(&source)));
            }
        }
    }
}

fn production_sources() -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    ui_sources(&root, &mut found);
    assert!(found.len() > 100, "the scan reads the ui source tree");
    found
}

#[test]
fn gp_10_no_popover_menu_is_built_without_the_naming_helper() {
    let bypasses: Vec<String> = production_sources()
        .into_iter()
        .filter(|(_, code)| builds_a_popover_menu(code))
        .map(|(path, _)| path.display().to_string())
        .collect();

    assert!(
        bypasses.is_empty(),
        "build popover menus with menu_a11y::popover_menu_from_model so their items have \
         accessible names (GTK fb6f2118); bypassed in {bypasses:?}"
    );
}

#[test]
fn gp_10_every_menu_button_model_is_matched_by_a_naming_call() {
    let bypasses: Vec<String> = production_sources()
        .into_iter()
        .filter(|(_, code)| menu_button_model_sites(code) > named_menu_button_calls(code))
        .map(|(path, _)| path.display().to_string())
        .collect();

    assert!(
        bypasses.is_empty(),
        "call menu_a11y::name_menu_button_items once for every MenuButton given a menu model, \
         so its items have accessible names (GTK fb6f2118); unmatched in {bypasses:?}"
    );
}

#[test]
fn the_popover_scan_catches_every_constructor_and_ignores_other_uses() {
    for violation in [
        "let p = gtk4::PopoverMenu::from_model(Some(&m));",
        "let p = gtk4::PopoverMenu::builder().build();",
        "let p = PopoverMenu::new();",
        "let p = gtk4::PopoverMenu::default();",
        "gtk4::PopoverMenu::new_from_model(&m)",
    ] {
        assert!(builds_a_popover_menu(violation), "{violation}");
    }
    for fine in [
        "popover.downcast::<gtk4::PopoverMenu>().ok()",
        "popover: gtk4::PopoverMenu,",
        "let bar = gtk4::PopoverMenuBar::from_model(None);",
    ] {
        assert!(!builds_a_popover_menu(fine), "{fine}");
    }
}

#[test]
fn the_menu_button_scan_counts_each_site_however_it_is_written() {
    let one_per_line = "
        let a = gtk4::MenuButton::builder()
            .menu_model(&first)
            .build();
        let b = gtk4::MenuButton::builder()
            .menu_model(&second)
            .build();";
    assert_eq!(menu_button_model_sites(one_per_line), 2);

    let broken_after_paren =
        "let a = gtk4::MenuButton::builder().menu_model(\n    &first,\n).build();";
    assert_eq!(menu_button_model_sites(broken_after_paren), 1);

    let by_value = "gtk4::MenuButton::builder().menu_model( Some_model() ).build();";
    assert_eq!(menu_button_model_sites(by_value), 1);

    assert_eq!(
        menu_button_model_sites("button.set_menu_model(Some(&m));"),
        1
    );
    assert_eq!(
        menu_button_model_sites("self.menu_button\n    .set_menu_model(None);"),
        1
    );

    assert_eq!(
        menu_button_model_sites("self.popover.set_menu_model(Some(&m));"),
        0
    );
    assert_eq!(
        menu_button_model_sites("let m = popover.menu_model().expect(\"model\");"),
        0
    );
    assert_eq!(
        menu_button_model_sites("let m = menu_model(&section, true);"),
        0
    );

    let matched = "name_menu_button_items(&a); menu_a11y::name_menu_button_items(&b);";
    assert_eq!(named_menu_button_calls(matched), 1);
}

#[test]
fn the_scans_skip_trailing_test_modules_and_comments_only() {
    let inline = "fn real() {}\n// PopoverMenu::from_model in a comment\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = gtk4::PopoverMenu::from_model(None); }\n}\n";
    let code = production_code(inline);
    assert!(code.contains("fn real"));
    assert!(!builds_a_popover_menu(&code));

    let wired = "fn real() {}\n#[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod tests;\n";
    assert_eq!(production_code(wired), "fn real() {}");

    let visible = "fn real() {}\n#[cfg(test)]\npub(crate) mod helpers {}\n";
    assert_eq!(production_code(visible), "fn real() {}");

    let on_a_field = "struct S {\n    #[cfg(test)]\n    field: u8,\n}\nfn later() { let _ = gtk4::PopoverMenu::from_model(None); }\n";
    assert!(
        builds_a_popover_menu(&production_code(on_a_field)),
        "a cfg(test) field must not hide the code after it"
    );
}
