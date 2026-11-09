//! The workspace walk includes production files and leaves out the files that
//! test-only `mod` declarations pull in.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::field_sweep::{distinct_keys, workspace_sites};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A scratch `crates/` directory that removes itself, also when an assertion
/// fails.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "reprise-field-sweep-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("the scratch directory can be created");
        Self { root }
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("the parent can be created");
        fs::write(path, text).expect("the file can be written");
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn test_only_modules_are_excluded_and_production_files_are_included() {
    let tree = Tree::new();
    tree.write(
        "demo/src/lib.rs",
        r#"
        #[cfg(test)]
        mod hidden;
        #[cfg(test)]
        #[path = "moved/elsewhere.rs"]
        mod relocated;
        #[path = "moved/production.rs"]
        mod promoted;
        mod shown;
        mod gated;
        pub fn f() { info!(lib_key = 1); }
        "#,
    );
    tree.write("demo/src/hidden.rs", "fn f() { info!(hidden_key = 1); }");
    tree.write(
        "demo/src/hidden/deeper.rs",
        "fn f() { info!(hidden_deeper_key = 1); }",
    );
    tree.write(
        "demo/src/moved/elsewhere.rs",
        "fn f() { info!(moved_key = 1); }",
    );
    tree.write(
        "demo/src/moved/production.rs",
        "fn f() { info!(promoted_key = 1); }",
    );
    tree.write("demo/src/shown.rs", "fn f() { info!(shown_key = 1); }");
    tree.write(
        "demo/src/gated.rs",
        "#![cfg(test)]\nmod gated_child;\nfn f() { info!(gated_key = 1); }",
    );
    tree.write(
        "demo/src/gated/gated_child.rs",
        "fn f() { info!(gated_child_key = 1); }",
    );
    tree.write(
        "demo/src/bin/tool.rs",
        "#[cfg(test)]\nmod bin_helper;\nfn main() { info!(tool_key = 1); }",
    );
    tree.write(
        "demo/src/bin/bin_helper.rs",
        "fn f() { info!(bin_hidden_key = 1); }",
    );

    let sweep = workspace_sites(tree.path()).expect("the scratch tree is readable");

    let keys: Vec<&str> = distinct_keys(&sweep.sites).into_iter().collect();
    assert_eq!(keys, ["lib_key", "promoted_key", "shown_key", "tool_key"]);
    assert_eq!(sweep.files, 4);
}

#[test]
fn an_aliased_tracing_import_fails_the_walk_with_its_file_and_line() {
    let tree = Tree::new();
    tree.write("demo/src/lib.rs", "fn f() {}\nuse tracing::warn as w;\n");

    let error = workspace_sites(tree.path()).expect_err("an alias is refused");

    assert!(error.contains("demo/src/lib.rs:2"), "{error}");
}

#[test]
fn a_test_only_module_inside_an_inline_module_resolves_in_that_modules_directory() {
    let tree = Tree::new();
    tree.write(
        "demo/src/lib.rs",
        r#"
        mod outer {
            pub mod inner {
                #[cfg(test)]
                mod hidden;
                #[cfg(test)]
                #[path = "moved.rs"]
                mod relocated;
                mod shown;
            }
            #[cfg(test)]
            mod sibling;
        }
        #[cfg(test)]
        mod top_hidden;
        "#,
    );
    tree.write(
        "demo/src/outer/inner/hidden.rs",
        "fn f() { info!(hidden_key = 1); }",
    );
    tree.write(
        "demo/src/outer/inner/moved.rs",
        "fn f() { info!(moved_key = 1); }",
    );
    tree.write(
        "demo/src/outer/inner/shown.rs",
        "fn f() { info!(shown_key = 1); }",
    );
    tree.write(
        "demo/src/outer/sibling.rs",
        "fn f() { info!(sibling_key = 1); }",
    );
    tree.write(
        "demo/src/top_hidden.rs",
        "fn f() { info!(top_hidden_key = 1); }",
    );
    // The module a stale reading would have excluded instead.
    tree.write(
        "demo/src/hidden.rs",
        "fn f() { info!(wrongly_hidden_key = 1); }",
    );

    let sweep = workspace_sites(tree.path()).expect("the scratch tree is readable");

    let keys: Vec<&str> = distinct_keys(&sweep.sites).into_iter().collect();
    assert_eq!(keys, ["shown_key", "wrongly_hidden_key"]);
}
