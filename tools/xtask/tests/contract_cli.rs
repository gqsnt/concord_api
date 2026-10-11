//! End-to-end `concord-xtask contract` command tests against the registered
//! Catalog-like fixture target (V3-O02, V3-O03) and, with the `examples`
//! feature, the real `api!` example targets (EXPLAIN-03). No network work
//! occurs.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn xtask(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_concord-xtask"))
        .args(arguments)
        .output()
        .expect("run concord-xtask")
}

fn scratch(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("contract_cli");
    fs::create_dir_all(&directory).expect("scratch directory");
    directory.join(name)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn export_check_and_diff_round_trip_through_the_binary() {
    let snapshot = scratch("catalog.contract.json");
    let path = snapshot.to_str().expect("utf-8 path");

    let export = xtask(&[
        "contract",
        "export",
        "--target",
        "fixture-catalog",
        "--output",
        path,
    ]);
    assert!(export.status.success(), "{}", text(&export.stderr));
    let first = fs::read_to_string(&snapshot).expect("snapshot");
    assert!(first.contains("\"format\": \"concord.contract/3\""));

    let again = xtask(&[
        "contract",
        "export",
        "--target",
        "fixture-catalog",
        "--output",
        path,
    ]);
    assert!(again.status.success());
    assert_eq!(
        fs::read_to_string(&snapshot).expect("snapshot"),
        first,
        "byte-identical export"
    );

    let clean = xtask(&[
        "contract",
        "check",
        "--target",
        "fixture-catalog",
        "--snapshot",
        path,
    ]);
    assert_eq!(clean.status.code(), Some(0), "{}", text(&clean.stdout));
    assert!(text(&clean.stdout).contains("matches the snapshot"));

    let edited = scratch("catalog.edited.json");
    let edited_path = edited.to_str().expect("utf-8 path");
    // Edit the root `X-Client-Version` literal everywhere it is represented.
    let changed = first.replace("\"text\": \"3\"", "\"text\": \"9\"");
    assert_ne!(changed, first);
    fs::write(&edited, &changed).expect("edited snapshot");

    let dirty = xtask(&[
        "contract",
        "check",
        "--target",
        "fixture-catalog",
        "--snapshot",
        edited_path,
    ]);
    assert_eq!(
        dirty.status.code(),
        Some(1),
        "check must fail on a structural difference"
    );
    let stdout = text(&dirty.stdout);
    assert!(stdout.contains("differs from the snapshot"));
    assert!(stdout.contains("catalog.items.list (CatalogApi::regional::item_summaries)"));
    assert!(stdout.contains("[Provenance]") || stdout.contains("Provenance"));

    let json = xtask(&["contract", "diff", edited_path, path, "--format", "json"]);
    assert!(json.status.success(), "{}", text(&json.stderr));
    let report = text(&json.stdout);
    assert!(report.contains("\"format\": \"concord.contract-diff/3\""));
    assert!(report.contains("\"verdict\": \"different\""));
    assert!(report.contains("\"Wire\""));

    let plain = xtask(&["contract", "diff", path, path, "--format", "text"]);
    assert!(plain.status.success());
    assert!(text(&plain.stdout).contains("verdict structurally-equal"));
}

#[test]
fn unknown_targets_and_invalid_snapshots_exit_with_usage_status() {
    let unknown = xtask(&[
        "contract",
        "export",
        "--target",
        "no-such",
        "--output",
        "unused.json",
    ]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(text(&unknown.stderr).contains("fixture-catalog"));

    let invalid = scratch("invalid.json");
    fs::write(&invalid, "{\"format\": \"concord.contract/2\"}").expect("invalid snapshot");
    let check = xtask(&[
        "contract",
        "check",
        "--target",
        "fixture-catalog",
        "--snapshot",
        invalid.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(check.status.code(), Some(2));

    let list = xtask(&["contract", "list"]);
    assert!(list.status.success());
    assert!(text(&list.stdout).contains("fixture-catalog"));
}

/// EXPLAIN-03 (§18.3): the real `api!` registry targets (compiled with the
/// `examples` feature) export deterministic `concord.contract/3` snapshots of
/// every generated endpoint, `check` accepts their own snapshot and rejects an
/// edited one with exit status 1 naming the changed endpoint, and `diff`
/// compares two exports. No network work occurs: providers are static
/// `Client::contract()` functions.
#[cfg(feature = "examples")]
#[test]
fn example_registry_targets_export_and_check_through_the_binary() {
    let list = xtask(&["contract", "list"]);
    assert!(list.status.success());
    for target in ["catalog", "catalog-v3", "riot", "data-dragon"] {
        assert!(
            text(&list.stdout).contains(target),
            "{target} is registered"
        );
    }

    // (target, client, endpoint count, one expected endpoint ID)
    let targets = [
        ("catalog", "CatalogApi", 4, "catalog.items.list"),
        ("catalog-v3", "CatalogApi", 8, "catalog.item.find"),
        (
            "riot",
            "RiotApi",
            46,
            "riot_api.platform.champion_v3.champion_rotations",
        ),
        ("data-dragon", "DDragonApi", 6, "d_dragon_api.versions"),
    ];
    for (target, client, endpoints, sample) in targets {
        // Names distinct from the round-trip test, which runs in parallel
        // in the same scratch directory.
        let snapshot = scratch(&format!("registry-{target}.contract.json"));
        let path = snapshot.to_str().expect("utf-8 path");
        let export = xtask(&["contract", "export", "--target", target, "--output", path]);
        assert!(
            export.status.success(),
            "{target}: {}",
            text(&export.stderr)
        );
        let first = fs::read_to_string(&snapshot).expect("snapshot");
        let document: serde_json::Value = serde_json::from_str(&first).expect("json");
        assert_eq!(document["format"], "concord.contract/3", "{target}");
        assert_eq!(document["client"]["name"], client, "{target}");
        let ids = document["endpoints"]
            .as_array()
            .expect("endpoints")
            .iter()
            .map(|endpoint| endpoint["id"]["value"].as_str().expect("id").to_owned())
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), endpoints, "{target}: every generated endpoint");
        assert!(ids.iter().any(|id| id == sample), "{target}: {ids:?}");

        let again = xtask(&["contract", "export", "--target", target, "--output", path]);
        assert!(again.status.success());
        assert_eq!(
            fs::read_to_string(&snapshot).expect("snapshot"),
            first,
            "{target}: byte-identical export"
        );

        let clean = xtask(&["contract", "check", "--target", target, "--snapshot", path]);
        assert_eq!(
            clean.status.code(),
            Some(0),
            "{target}: {}",
            text(&clean.stdout)
        );
        assert!(text(&clean.stdout).contains("matches the snapshot"));

        // One endpoint's method changes in the snapshot: a wire difference.
        let edited = scratch(&format!("registry-{target}.edited.json"));
        let changed = first.replacen("\"method\": \"GET\"", "\"method\": \"HEAD\"", 1);
        assert_ne!(changed, first, "{target} has a GET endpoint");
        fs::write(&edited, &changed).expect("edited snapshot");
        let edited_path = edited.to_str().expect("utf-8 path");
        let dirty = xtask(&[
            "contract",
            "check",
            "--target",
            target,
            "--snapshot",
            edited_path,
        ]);
        assert_eq!(
            dirty.status.code(),
            Some(1),
            "{target}: check must fail on a structural difference"
        );
        let stdout = text(&dirty.stdout);
        assert!(stdout.contains("differs from the snapshot"), "{stdout}");
        assert!(stdout.contains("Wire"), "{stdout}");

        let json = xtask(&["contract", "diff", edited_path, path, "--format", "json"]);
        assert!(json.status.success(), "{}", text(&json.stderr));
        assert!(text(&json.stdout).contains("\"verdict\": \"different\""));
    }
}
