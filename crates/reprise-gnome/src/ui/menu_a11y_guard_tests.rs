//! Source scans that keep every popover menu behind `menu_a11y`.
//!
//! They read the production sources under `src/ui` and fail when a new site
//! builds a popover menu or a menu button's model without the naming helper.
//! The scans are textual, so each rule is a heuristic; they live in
//! `menu_a11y_scan.rs`, where each is spelled out, and the fixture tests below
//! pin what they catch and what they leave alone.

use std::path::{Path, PathBuf};

use super::scan::{
    builds_a_popover_menu, menu_button_model_sites, named_menu_button_calls, production_code,
};

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

#[test]
fn the_popover_scan_follows_import_aliases() {
    for violation in [
        "use gtk4::PopoverMenu as Foo;\nlet p = Foo::from_model(None);",
        "use gtk4::{Button, PopoverMenu as Pm};\nlet p = Pm::builder().build();",
        "use gtk::PopoverMenu as Foo;\nfn f() -> Foo { Foo::new_from_model(None) }",
        "type Menu = gtk4::PopoverMenu;\nlet p = Menu::from_model(None);",
    ] {
        assert!(builds_a_popover_menu(violation), "{violation}");
    }
    for fine in [
        "use gtk4::PopoverMenuBar as Bar;\nlet b = Bar::from_model(None);",
        "use gtk4::PopoverMenu as Foo;\nfn f(p: &Foo) -> bool { p.is_visible() }",
        "use gtk4::Button as Foo;\nlet b = Foo::new();",
    ] {
        assert!(!builds_a_popover_menu(fine), "{fine}");
    }
}

#[test]
fn the_popover_scan_catches_generic_object_constructors() {
    for violation in [
        "let p = glib::Object::new::<gtk4::PopoverMenu>();",
        "let p = Object::new::<PopoverMenu>();",
        "let p = glib::Object::builder::<gtk4::PopoverMenu>().build();",
        "let p = glib::Object::builder::<\n    gtk4::PopoverMenu,\n>()\n.build();",
        "use gtk4::PopoverMenu as Foo;\nlet p = glib::Object::new::<Foo>();",
    ] {
        assert!(builds_a_popover_menu(violation), "{violation}");
    }
    for fine in [
        "let b = glib::Object::new::<gtk4::Button>();",
        "let b = glib::Object::new::<gtk4::PopoverMenuBar>();",
        "let b = glib::Object::builder::<gtk4::Popover>().build();",
    ] {
        assert!(!builds_a_popover_menu(fine), "{fine}");
    }
}

#[test]
fn the_menu_button_scan_does_not_mistake_a_button_named_popover_for_a_popover() {
    for (code, sites) in [
        ("popover_button.set_menu_model(Some(&m));", 1),
        ("self.popover_menu_button.set_menu_model(None);", 1),
        (
            "let popover_trigger = gtk4::MenuButton::new();\npopover_trigger.set_menu_model(Some(&m));",
            1,
        ),
        (
            "struct S { popover_trigger: gtk4::MenuButton }\nself.popover_trigger.set_menu_model(None);",
            1,
        ),
        ("self.popover.set_menu_model(Some(&m));", 0),
        ("popover_menu.set_menu_model(Some(&m));", 0),
        (
            "let popover = button.popover().unwrap();\npopover.set_menu_model(Some(&m));",
            0,
        ),
    ] {
        assert_eq!(menu_button_model_sites(code), sites, "{code}");
    }
}

#[test]
fn the_scans_keep_the_production_code_that_follows_a_mid_file_test_module() {
    let inline = "fn before() {}\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = gtk4::PopoverMenu::from_model(None); let brace = \"{\"; let close = '}'; let quote = '\\''; let tab = \"\\\"{\"; }\n}\nfn after() { let _ = gtk4::PopoverMenu::from_model(None); }\n";
    let code = production_code(inline);
    assert!(code.contains("fn before"));
    assert!(
        code.contains("fn after"),
        "production code after the module"
    );
    assert!(!code.contains("fn t()"), "the test module is skipped");
    assert_eq!(code.matches("PopoverMenu::from_model").count(), 1);

    let wired =
        "fn before() {}\n#[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod tests;\nfn after() {}\n";
    assert_eq!(production_code(wired), "fn before() {}\nfn after() {}");

    let two = "#[cfg(test)]\nmod a {\n    mod nested { fn x() {} }\n}\nfn middle() {}\n#[cfg(test)]\nmod b {}\nfn last() {}\n";
    assert_eq!(production_code(two), "fn middle() {}\nfn last() {}");

    let unclosed = "fn before() {}\n#[cfg(test)]\nmod tests {\n    fn t() {}\n";
    assert_eq!(production_code(unclosed), "fn before() {}");
}
