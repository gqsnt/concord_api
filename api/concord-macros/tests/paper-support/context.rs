//! The surrounding declarations assumed by standalone guide fragments.
// Keep the quoted fragment itself byte-for-byte intact.
pub fn source(stem: &str, fragment: &str) -> String {
    let stripped = fragment.trim_start();
    if stripped.starts_with("concord::api!") {
        return fragment.into();
    }
    if stripped.starts_with("pub client") || stripped.starts_with("client") {
        return format!("concord::api! {{\n{fragment}\n}}");
    }
    let (parameters, headers, parts, tail) = match stem {
        "tour_011" => (
            "(key: Secret)",
            "headers { \"authorization\" = \"Bearer {key}\"; }",
            "",
            "",
        ),
        "3-language_003" => (
            "(token: Secret)",
            "headers { \"authorization\" = \"Bearer {token}\"; }",
            "",
            "",
        ),
        "5-response_001" => (
            "",
            "headers { \"accept\" = \"application/vnd.github+json\"; }",
            "",
            "",
        ),
        "3-language_004" | "5-response_009" => ("", "", "", "POST fixture(..formatted);"),
        "3-language_005" | "5-response_010" => {
            ("", "", include_str!("../paper/3-language_004.rs"), "")
        }
        "3-language_009" => ("", "", "", "scope matches(..regional) { GET fixture(); }"),
        "6-behaviour_004" => ("", "", "", "POST fixture(..keyed);"),
        "6-behaviour_006" => ("", "", "", "GET fixture(..page) -> List<Customer>;"),
        "6-behaviour_007" => ("", "", include_str!("../paper/6-behaviour_006.rs"), ""),
        "apis_riot_002" => ("", "", "", "scope matches(..platform) { GET fixture(); }"),
        "apis_again_kubernetes_003" => ("", "", include_str!("kubernetes_parts.api"), ""),
        _ => ("", "", "", ""),
    };
    format!("concord::api! {{ pub client Fixture{parameters} {{\nbase \"https://fixture.example\"; read Json;\n{headers}\n{parts}\n{fragment}\n{tail}\n}} }}")
}
