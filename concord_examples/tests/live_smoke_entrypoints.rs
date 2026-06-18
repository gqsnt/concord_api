use std::path::PathBuf;

fn workspace_file(path: &str) -> String {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();

    std::fs::read_to_string(workspace.join(path))
        .unwrap_or_else(|err| panic!("read workspace file {path}: {err}"))
}

fn source_contains_in_order(source: &str, snippets: &[&str]) -> bool {
    let mut search_from = 0;

    for snippet in snippets {
        let Some(relative) = source[search_from..].find(snippet) else {
            return false;
        };
        search_from += relative + snippet.len();
    }

    true
}

fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("missing function `{signature}`"));
    let rest = &source[start..];
    let end = rest
        .find("\npub async fn ")
        .filter(|idx| *idx > 0)
        .unwrap_or(rest.len());
    &rest[..end]
}

fn occurrences(source: &str, needle: &str) -> usize {
    source.match_indices(needle).count()
}

#[test]
fn riot_manual_smoke_entrypoint_is_opt_in_and_bounded() {
    let riot = workspace_file("concord_examples/src/riot.rs");
    let riot_test = function_body(&riot, "pub async fn riot_test()");

    for snippet in [
        "pub async fn riot_test()",
        "DISC OF THE SUN",
        "\"EUW\"",
        "RIOT_API_KEY",
        ".by_riot_id",
        ".by_puuid",
        ".by_game_and_puuid",
        ".summoner_v4()",
        ".league_v4()",
        ".champion_mastery_v4()",
        ".champion_v3()",
        ".status_v4()",
        ".challenges_v1()",
        ".clash_v1()",
        ".ids_by_puuid",
        ".paginate()",
        ".max_items(60)",
        ".count(20)",
        ".by_id",
        ".timeline",
        ".replays_by_puuid",
        ".active_game_by_puuid",
    ] {
        assert!(
            riot_test.contains(snippet),
            "riot_test should contain `{snippet}`"
        );
    }

    assert!(
        !riot.contains("RGAPI-"),
        "riot.rs must not contain a literal Riot API key"
    );
    for forbidden in [
        ".max_items(10_000)",
        ".create_codes",
        ".register_provider",
        ".register_tournament",
        "tournament_stub_v5().create",
    ] {
        assert!(
            !riot_test.contains(forbidden),
            "riot_test must not contain `{forbidden}`"
        );
    }
}

#[test]
fn ddragon_manual_smoke_entrypoint_covers_core_metadata_paths() {
    let ddragon = workspace_file("concord_examples/src/ddragon.rs");

    for snippet in [
        "pub async fn ddragon_test()",
        ".api().versions()",
        ".languages()",
        "realm(\"euw\".to_string())",
        ".champion_list()",
        ".champion_detail(\"Aatrox\".to_string())",
    ] {
        assert!(
            ddragon.contains(snippet),
            "ddragon.rs should contain `{snippet}`"
        );
    }
}

#[test]
fn examples_main_gates_live_smoke_calls_by_environment() {
    let main = workspace_file("concord_examples/src/main.rs");

    for snippet in ["CONCORD_RUN_RIOT_TEST", "CONCORD_RUN_DDRAGON_TEST"] {
        assert!(main.contains(snippet), "main.rs should contain `{snippet}`");
    }

    assert!(source_contains_in_order(
        &main,
        &[
            "std::env::var_os(\"CONCORD_RUN_RIOT_TEST\")",
            "concord_examples::riot::riot_test().await?",
        ],
    ));
    assert!(source_contains_in_order(
        &main,
        &[
            "std::env::var_os(\"CONCORD_RUN_DDRAGON_TEST\")",
            "concord_examples::ddragon::ddragon_test().await?",
        ],
    ));

    assert_eq!(
        occurrences(&main, "concord_examples::riot::riot_test().await?"),
        1,
        "riot_test should only be called from the gated branch"
    );
    assert_eq!(
        occurrences(&main, "concord_examples::ddragon::ddragon_test().await?"),
        1,
        "ddragon_test should only be called from the gated branch"
    );

    for forbidden in [
        "let _ = concord_examples::riot::riot_test().await",
        "let _ = concord_examples::ddragon::ddragon_test().await",
        "concord_examples::riot::riot_test().await?;\n\n    if std::env::var_os",
        "concord_examples::ddragon::ddragon_test().await?;\n\n    if std::env::var_os",
    ] {
        assert!(
            !main.contains(forbidden),
            "main.rs must not contain an unconditional live smoke call matching `{forbidden}`"
        );
    }
}
