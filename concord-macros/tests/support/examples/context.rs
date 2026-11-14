//! The surrounding declarations assumed by standalone regression fragments.
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
        "description_055" => (
            "(key: Secret)",
            "headers { \"authorization\" = \"Bearer {key}\"; }",
            "",
            "",
        ),
        "description_011" => (
            "(token: Secret)",
            "headers { \"authorization\" = \"Bearer {token}\"; }",
            "",
            "",
        ),
        "description_034" => (
            "",
            "headers { \"accept\" = \"application/vnd.github+json\"; }",
            "",
            "",
        ),
        "description_012" | "description_042" => ("", "", "", "POST fixture(..formatted);"),
        "description_013" | "description_043" => (
            "",
            "",
            include_str!("../../parser/regressions/description_012.rs"),
            "",
        ),
        "description_017" => ("", "", "", "scope matches(..regional) { GET fixture(); }"),
        "description_004" => ("", "", "", "POST fixture(..keyed);"),
        "description_006" => ("", "", "", "GET fixture(..page) -> List<Customer>;"),
        "description_007" => (
            "",
            "",
            include_str!("../../parser/regressions/description_006.rs"),
            "",
        ),
        "description_027" => ("", "", "", "scope matches(..platform) { GET fixture(); }"),
        "description_024" => ("", "", include_str!("kubernetes_parts.api"), ""),
        _ => ("", "", "", ""),
    };
    format!("concord::api! {{ pub client Fixture{parameters} {{\nbase \"https://fixture.example\"; read Json;\n{headers}\n{parts}\n{fragment}\n{tail}\n}} }}")
}
