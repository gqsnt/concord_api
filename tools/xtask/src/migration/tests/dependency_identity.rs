//! Dependency identity (bounded-return review §5.3): the crate names
//! `concord`, `concord_core`, `concord_extras` and `concord_macros` are
//! treated as Concord's only when the package manifest establishes it. A
//! dependency under one of those names that is another package (a
//! `package = ".."` rename, a `path` to another package, a `workspace = true`
//! entry the root declares as another package or does not declare), and a
//! Concord package under a name the tool does not recognize, leave every
//! file with something to migrate unchanged with one
//! `C3-MIGRATE-DEPENDENCY-IDENTITY` diagnostic.

use super::super::Migration;
use super::super::report::Diagnostic;
use super::*;

/// A v2.1 library with one declaration and one genuine optional setter call.
const LIB: &str = r#"use concord::{api, Empty};
api! {
    client Api {
        base "https://review.example.test/";
        timeout 5s;
        GET item(x?: u32) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; }
        }
    }
}
pub fn call(client: &Api) {
    let _ = client.item().set_x(None);
}
"#;

/// A file of the same crate with nothing to migrate.
const PLAIN: &str = "pub fn plain() -> u32 { 1 }\n";

/// The refusal code (spelled out, so the test compiles without the code).
const IDENTITY: &str = "C3-MIGRATE-DEPENDENCY-IDENTITY";

const MIGRATED_CALL: &str = "let _ = client.item().replace_x(None);";

/// A `Cargo.toml` of package `app` whose `[dependencies]` are `dependencies`.
fn app_manifest(dependencies: &str) -> String {
    format!(
        "[package]\nname = \"app\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\n{dependencies}"
    )
}

/// A package manifest named `name`.
fn named_manifest(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n")
}

/// Migrates the package `app/` of a temporary tree holding `files`
/// (`(relative path, contents)`, `app/src/lib.rs` = [`LIB`] and
/// `app/src/plain.rs` = [`PLAIN`] added) and returns the migration.
fn migrate_app(files: &[(&str, &str)]) -> Migration {
    let tree = TempDir::new("dependency-identity");
    for (relative, contents) in files {
        tree.write(relative, contents);
    }
    tree.write("app/src/lib.rs", &format!("{LIB}mod plain;\n"));
    tree.write("app/src/plain.rs", PLAIN);
    migrate(&tree.0.join("app"), &Settings::default()).expect("migration")
}

fn identity_diagnostics(migration: &Migration) -> Vec<&Diagnostic> {
    migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == IDENTITY)
        .collect()
}

/// The genuine packages migrate: a registry dependency, the workspace's own
/// `package = ".."` renames between matching names (as in the Concord
/// repository), a `path` dependency whose manifest is the Concord package,
/// `workspace = true` through the root's `[workspace.dependencies]` (paths
/// relative to the root), and a target-specific dev-dependency.
#[test]
fn genuine_concord_dependencies_migrate() {
    let concord = named_manifest("concord");
    let core = named_manifest("concord-core");
    let cases: Vec<Vec<(&str, String)>> = vec![
        vec![("app/Cargo.toml", app_manifest("concord = \"3.0.0\"\n"))],
        vec![(
            "app/Cargo.toml",
            app_manifest(
                "concord = { version = \"3\" }\nconcord_core = { package = \"concord-core\", version = \"3\" }\nconcord_extras = { package = \"concord_extras\", version = \"3\" }\nconcord-macros = \"3\"\nserde = \"1\"\n",
            ),
        )],
        vec![
            ("app/Cargo.toml", app_manifest("concord = { path = \"../concord\" }\nconcord_core = { package = \"concord-core\", path = \"../core/\" }\n")),
            ("concord/Cargo.toml", concord.clone()),
            ("core/Cargo.toml", core.clone()),
        ],
        vec![
            ("Cargo.toml", "[workspace]\nmembers = [\"app\", \"crates/concord\"]\n\n[workspace.dependencies]\nconcord = { path = \"crates/concord\", version = \"3\" }\n".to_owned()),
            ("app/Cargo.toml", app_manifest("concord = { workspace = true }\n")),
            ("crates/concord/Cargo.toml", concord.clone()),
        ],
        vec![(
            "app/Cargo.toml",
            format!(
                "{}\n[target.'cfg(unix)'.dev-dependencies]\nconcord_core = {{ package = \"concord-core\", version = \"3\" }}\n",
                app_manifest("concord = \"3\"\n")
            ),
        )],
    ];
    for case in cases {
        let files = case
            .iter()
            .map(|(path, contents)| (*path, contents.as_str()))
            .collect::<Vec<_>>();
        let migration = migrate_app(&files);
        let converted = migration.converted("src/lib.rs").expect("library");
        assert!(converted.contains(MIGRATED_CALL), "{case:#?}\n{converted}");
        assert!(converted.contains("GET item(query x?: u32)"), "{converted}");
        assert!(identity_diagnostics(&migration).is_empty(), "{case:#?}");
        assert!(
            migration
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == Code::SetterReplace),
            "{case:#?}"
        );
    }
}

/// A foreign or unknown package under a Concord crate name, and a Concord
/// package under another name, are refused with the precise reason: the
/// file is unchanged, its only diagnostic is the refusal, and a file with
/// nothing to migrate is not reported.
#[test]
fn foreign_or_unknown_concord_dependency_is_refused() {
    let cases: Vec<(Vec<(&str, String)>, &str)> = vec![
        (
            vec![("app/Cargo.toml", app_manifest("concord = { package = \"not-concord\", version = \"1\" }\n"))],
            "the dependency `concord` in `[dependencies]` is the package `not-concord`, not the Concord package the crate name `concord` denotes",
        ),
        (
            vec![
                ("app/Cargo.toml", app_manifest("concord = { path = \"../fake\" }\n")),
                ("fake/Cargo.toml", named_manifest("fake")),
            ],
            "the `path` dependency `concord` in `[dependencies]` leads to the package `fake`, not the Concord package `concord`",
        ),
        (
            vec![("app/Cargo.toml", app_manifest("concord = { path = \"../missing\" }\n"))],
            "the `path` dependency `concord` in `[dependencies]` cannot be identified: the manifest `../missing/Cargo.toml` could not be read",
        ),
        (
            vec![("app/Cargo.toml", app_manifest("concord = \"3\"\ncc = { package = \"concord\", version = \"3\" }\n"))],
            "the dependency `cc` in `[dependencies]` is the Concord package `concord` under the crate name `cc`, which the tool does not recognize as Concord (only `concord` names that package)",
        ),
        (
            vec![("app/Cargo.toml", app_manifest("concord = \"3\"\nconcord_core = { package = \"concord\", version = \"3\" }\n"))],
            "the dependency `concord_core` in `[dependencies]` is the Concord package `concord` under the crate name `concord_core`, which the tool recognizes as another Concord package (only `concord` names `concord`)",
        ),
        (
            vec![
                ("Cargo.toml", "[workspace]\nmembers = [\"app\"]\n\n[workspace.dependencies]\nconcord = { package = \"other\", version = \"1\" }\n".to_owned()),
                ("app/Cargo.toml", app_manifest("concord.workspace = true\n")),
            ],
            "the dependency `concord` in `[dependencies]` is the package `other`, not the Concord package the crate name `concord` denotes",
        ),
        (
            vec![("app/Cargo.toml", app_manifest("concord = { workspace = true }\n"))],
            "the dependency `concord` in `[dependencies]` inherits `workspace = true`, but no workspace root with a `[workspace]` table is above the package, so its package is unknown",
        ),
        (
            vec![
                ("Cargo.toml", "[workspace]\nmembers = [\"app\"]\n".to_owned()),
                ("app/Cargo.toml", app_manifest("concord = { workspace = true }\n")),
            ],
            "the dependency `concord` in `[dependencies]` inherits `workspace = true`, but the workspace root declares no such `[workspace.dependencies]` entry, so its package is unknown",
        ),
        (
            vec![(
                "app/Cargo.toml",
                format!(
                    "{}\n[target.'cfg(windows)'.dev-dependencies]\nconcord_extras = {{ package = \"shim\", version = \"1\" }}\n",
                    app_manifest("concord = \"3\"\n")
                ),
            )],
            "the dependency `concord_extras` in `[target.'cfg(windows)'.dev-dependencies]` is the package `shim`, not the Concord package the crate name `concord_extras` denotes",
        ),
        (
            vec![(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\nname = \"concord\"\n".to_owned(),
            )],
            "the package's own library is the crate `concord`, but the package is `app`, not the Concord package of that name",
        ),
    ];
    for (case, reason) in cases {
        let files = case
            .iter()
            .map(|(path, contents)| (*path, contents.as_str()))
            .collect::<Vec<_>>();
        let migration = migrate_app(&files);
        let converted = migration.converted("src/lib.rs").expect("library");
        assert_eq!(
            converted,
            format!("{LIB}mod plain;\n"),
            "{case:#?}: the file must be unchanged"
        );
        assert_eq!(migration.converted("src/plain.rs"), Some(PLAIN));
        assert_eq!(
            migration.diagnostics.len(),
            1,
            "{case:#?}: {:#?}",
            migration.diagnostics
        );
        let refusal = &migration.diagnostics[0];
        assert_eq!(refusal.code.as_str(), IDENTITY);
        assert_eq!(refusal.severity(), Severity::Error);
        assert_eq!(
            (refusal.file.as_str(), refusal.line, refusal.column),
            ("src/lib.rs", 1, 1)
        );
        assert!(
            refusal
                .message
                .contains(&format!("genuine Concord packages: {reason}. ")),
            "{case:#?}: {}",
            refusal.message
        );
        assert!(
            refusal.message.contains(
                "(4 edit(s), 1 declaration(s) and 1 diagnostic(s) it would have produced)"
            ),
            "{}",
            refusal.message
        );
        assert!(migration.identities.is_empty(), "{case:#?}");
        assert_eq!(migration.declarations, 0, "{case:#?}");
    }
}

/// A standalone file (no `Cargo.toml` above it) keeps the documented
/// assumption: its Concord crate names are Concord's.
#[test]
fn standalone_file_keeps_the_documented_assumption() {
    let outcome = convert(LIB);
    assert!(outcome.converted.contains(MIGRATED_CALL));
    assert!(
        outcome
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.as_str() != IDENTITY)
    );
}

/// Focused review of R3 B01 (dependency notes): [`migrate_app`] with
/// `prefix` written before [`LIB`] in `app/src/lib.rs` (the crate root).
fn migrate_app_with(prefix: &str, files: &[(&str, &str)]) -> Migration {
    let tree = TempDir::new("dependency-identity");
    for (relative, contents) in files {
        tree.write(relative, contents);
    }
    tree.write("app/src/lib.rs", &format!("{prefix}{LIB}mod plain;\n"));
    tree.write("app/src/plain.rs", PLAIN);
    migrate(&tree.0.join("app"), &Settings::default()).expect("migration")
}

/// Asserts that the library of `migration` was refused for `reason` and left
/// unchanged (its text is `prefix` + [`LIB`]).
fn assert_refused(migration: &Migration, prefix: &str, reason: &str, case: &str) {
    let converted = migration.converted("src/lib.rs").expect("library");
    assert_eq!(
        converted,
        format!("{prefix}{LIB}mod plain;\n"),
        "{case}: the file must be unchanged"
    );
    let refusals = identity_diagnostics(migration);
    assert_eq!(refusals.len(), 1, "{case}: {:#?}", migration.diagnostics);
    assert_eq!(migration.diagnostics.len(), 1, "{case}");
    assert_eq!(refusals[0].severity(), Severity::Error);
    assert!(
        refusals[0].message.contains(reason),
        "{case}: `{reason}` in: {}",
        refusals[0].message
    );
    assert_eq!(migration.declarations, 0, "{case}");
}

fn assert_migrated(migration: &Migration, case: &str) {
    let converted = migration.converted("src/lib.rs").expect("library");
    assert!(converted.contains(MIGRATED_CALL), "{case}\n{converted}");
    assert!(
        identity_diagnostics(migration).is_empty(),
        "{case}: {:#?}",
        migration.diagnostics
    );
}

/// `extern crate <other> as concord;` makes `concord` name another crate
/// whatever the manifest says; `extern crate concord as x;` is fine only when
/// the manifest proves `concord` is the Concord package.
#[test]
fn extern_crate_alias_of_a_concord_name_is_refused() {
    let genuine = app_manifest("concord = \"3\"\nevil = \"1\"\n");
    let refused: &[(&str, &str)] = &[
        (
            "extern crate evil as concord;\n",
            "`extern crate evil as concord;` (src/lib.rs:1) makes the crate name `concord` denote the crate `evil`, not the Concord package `concord`",
        ),
        (
            "extern crate concord_core as concord;\n",
            "`extern crate concord_core as concord;` (src/lib.rs:1) makes the crate name `concord` denote the crate `concord_core`, not the Concord package `concord`",
        ),
    ];
    for (prefix, reason) in refused {
        let migration = migrate_app_with(prefix, &[("app/Cargo.toml", genuine.as_str())]);
        assert_refused(&migration, prefix, reason, prefix);
    }
    let undeclared = app_manifest("evil = \"1\"\n");
    let migration = migrate_app_with(
        "extern crate concord as cc;\n",
        &[("app/Cargo.toml", undeclared.as_str())],
    );
    assert_refused(
        &migration,
        "extern crate concord as cc;\n",
        "`extern crate concord as cc;` (src/lib.rs:1) names the crate `concord`, which the manifest does not declare, so it is not proved to be the Concord package",
        "undeclared",
    );
    for prefix in ["extern crate concord as cc;\n", "extern crate concord;\n"] {
        let migration = migrate_app_with(prefix, &[("app/Cargo.toml", genuine.as_str())]);
        assert_migrated(&migration, prefix);
    }
    // `extern crate self as concord;` is the package's own library: genuine
    // only in the Concord package itself.
    let own = "extern crate self as concord;\n";
    let concord_package = named_manifest("concord");
    let migration = migrate_app_with(own, &[("app/Cargo.toml", concord_package.as_str())]);
    assert_migrated(&migration, "the Concord package");
    let migration = migrate_app_with(own, &[("app/Cargo.toml", genuine.as_str())]);
    assert_refused(
        &migration,
        own,
        "`extern crate self as concord;` (src/lib.rs:1) makes the crate name `concord` denote this package's own library, not the Concord package `concord`",
        "own library",
    );
}

/// `[patch]` and `[replace]` entries (of the package or its workspace root)
/// that redirect a Concord package to a `path` whose package is not that
/// Concord package, or to another package, are refused; a `path` to the
/// genuine package, and a `git` patch (identified by its package name, like a
/// `git` dependency), are not.
#[test]
fn patch_or_replace_redirecting_a_concord_package_is_refused() {
    let dependency = "concord = \"3\"\n";
    let fork = named_manifest("fork");
    let genuine = named_manifest("concord");
    let refused: Vec<(Vec<(&str, String)>, &str)> = vec![
        (
            vec![
                (
                    "app/Cargo.toml",
                    format!(
                        "{}\n[patch.crates-io]\nconcord = {{ path = \"../fork\" }}\n",
                        app_manifest(dependency)
                    ),
                ),
                ("fork/Cargo.toml", fork.clone()),
            ],
            "the entry `concord` in `[patch.crates-io]` of `Cargo.toml` redirects the Concord package `concord` to the `path` `../fork`, whose package is `fork`",
        ),
        (
            vec![(
                "app/Cargo.toml",
                format!(
                    "{}\n[patch.crates-io]\nconcord = {{ package = \"evil\", version = \"1\" }}\n",
                    app_manifest(dependency)
                ),
            )],
            "the entry `concord` in `[patch.crates-io]` of `Cargo.toml` redirects the Concord package `concord` to the package `evil`",
        ),
        (
            vec![
                (
                    "app/Cargo.toml",
                    format!(
                        "{}\n[replace]\n\"concord:3.0.0\" = {{ path = \"../fork\" }}\n",
                        app_manifest(dependency)
                    ),
                ),
                ("fork/Cargo.toml", fork.clone()),
            ],
            "the entry `concord:3.0.0` in `[replace]` of `Cargo.toml` redirects the Concord package `concord` to the `path` `../fork`, whose package is `fork`",
        ),
        (
            vec![
                (
                    "Cargo.toml",
                    "[workspace]\nmembers = [\"app\"]\n\n[patch.crates-io]\nconcord = { path = \"fork\" }\n"
                        .to_owned(),
                ),
                ("app/Cargo.toml", app_manifest(dependency)),
                ("fork/Cargo.toml", fork.clone()),
            ],
            "the entry `concord` in `[patch.crates-io]` of `../Cargo.toml` redirects the Concord package `concord` to the `path` `fork`, whose package is `fork`",
        ),
    ];
    for (case, reason) in refused {
        let files = case
            .iter()
            .map(|(path, contents)| (*path, contents.as_str()))
            .collect::<Vec<_>>();
        let migration = migrate_app(&files);
        assert_refused(&migration, "", reason, &format!("{case:#?}"));
    }
    let accepted: Vec<Vec<(&str, String)>> = vec![
        vec![
            (
                "app/Cargo.toml",
                format!(
                    "{}\n[patch.crates-io]\nconcord = {{ path = \"../concord\" }}\n",
                    app_manifest(dependency)
                ),
            ),
            ("concord/Cargo.toml", genuine.clone()),
        ],
        vec![(
            "app/Cargo.toml",
            format!(
                "{}\n[patch.crates-io]\nconcord = {{ git = \"https://example.invalid/concord\" }}\nserde = {{ path = \"../serde\" }}\n",
                app_manifest(dependency)
            ),
        )],
    ];
    for case in accepted {
        let files = case
            .iter()
            .map(|(path, contents)| (*path, contents.as_str()))
            .collect::<Vec<_>>();
        let migration = migrate_app(&files);
        assert_migrated(&migration, &format!("{case:#?}"));
    }
}

/// A manifest without a `[package]` table (a virtual workspace manifest)
/// declares no dependencies of the file, which belongs to no package Cargo
/// builds: the documented behavior (docs/migration_v3.md, "Dependency
/// identity") is that the edition is unknown (edition-dependent paths are
/// unknown, see `round5_f03_n3_unknown_edition_makes_edition_dependent_paths_unknown`)
/// and the Concord crate names are assumed, as for a file outside any
/// package; its `[workspace.dependencies]` are not read for such a file.
/// Its members are packages: a `[patch]` of that root still applies to them
/// (see [`patch_or_replace_redirecting_a_concord_package_is_refused`]).
#[test]
fn virtual_manifest_keeps_the_documented_assumption() {
    let tree = TempDir::new("dependency-identity");
    tree.write(
        "ws/Cargo.toml",
        "[workspace]\nmembers = []\n\n[workspace.dependencies]\nconcord = { package = \"evil\", version = \"1\" }\n",
    );
    tree.write("ws/src/lib.rs", LIB);
    let migration = migrate(&tree.0.join("ws"), &Settings::default()).expect("migration");
    assert!(identity_diagnostics(&migration).is_empty());
    let converted = migration.converted("src/lib.rs").expect("library");
    assert!(converted.contains("GET item(query x?: u32)"), "{converted}");
}
