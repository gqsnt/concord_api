//! Name resolution and validation, exercised independently of code generation.
use crate::{ast::*, check, flat, name, parse, resolve};
use std::{fs, path::PathBuf};

fn parse_source(source: &str) -> parse::Parsed {
    let tokens: proc_macro2::TokenStream = source.parse().unwrap();
    if source.trim_start().starts_with("concord") {
        let group = tokens.clone().into_iter().last().unwrap();
        if let proc_macro2::TokenTree::Group(group) = group {
            return parse::parse(group.stream());
        }
    }
    parse::parse(tokens)
}
fn run(source: &str) -> (String, String) {
    let mut parsed = parse_source(source);
    let mut printed = String::new();
    if let Some(client) = &parsed.client {
        name::check(client, &mut parsed.diagnostics);
        let model = resolve::resolve(client, &mut parsed.diagnostics);
        check::check(client, &model, &mut parsed.diagnostics);
        printed = model.print();
    }
    (printed, parsed.diagnostics.into_tokens().to_string())
}
fn accepted(source: &str) -> String {
    let (printed, errors) = run(source);
    assert!(errors.is_empty(), "{source}\n{errors}");
    printed
}
fn refuses(source: &str, text: &str, cell: &str) {
    let (_, errors) = run(source);
    assert!(
        errors.contains(text),
        "{cell}\n{source}\nexpected {text}\n{errors}"
    );
}
fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn c10_3_real_description_flat_lists() {
    let mut files: Vec<_> = fs::read_dir(path().join("tests/snapshots/resolved"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "api"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 16); // Eleven API description copies and all five tour clients.
    for file in files {
        let source = fs::read_to_string(&file).unwrap();
        let (printed, errors) = run(&source);
        assert!(errors.is_empty(), "{}: {errors}", file.display());
        let expected = file.with_extension("flat");
        if std::env::var_os("CONCORD_REVIEW_GOLDENS").is_some() {
            let output = path().join("../target/resolved-snapshots");
            fs::create_dir_all(&output).unwrap();
            fs::write(output.join(expected.file_name().unwrap()), &printed).unwrap();
        }
        if !expected.exists() && std::env::var_os("CONCORD_CREATE_GOLDENS").is_some() {
            fs::write(&expected, &printed).unwrap();
        }
        assert_eq!(
            printed,
            fs::read_to_string(&expected).unwrap(),
            "{}",
            file.display()
        );
    }
}

#[test]
fn c10_3_two_orders_replacements_and_removal_keep_first_wire_position() {
    let text = accepted(
        r#"client Orders { base "https://orders.test"; query { "a" = first(); "b" = second(); }
        scope middle { query { override "a" = third(); "c" = fourth(); }
            GET go() { query { override "b" = fifth(); } }
        }
    }"#,
    );
    let query: Vec<_> = text.lines().filter(|l| l.starts_with("query")).collect();
    assert!(query[0].contains("wire=0") && query[0].contains("third"));
    assert!(query[1].contains("wire=2") && query[1].contains("fourth"));
    assert!(query[2].contains("wire=1") && query[2].contains("fifth"));
    accepted(
        r#"client Orders { base "https://orders.test"; query { "a" = 1; } scope s { query { remove "a"; } GET go() { query { "a" = 3; } } } }"#,
    );
}

#[test]
fn c1_5_names_acronyms_digits_and_receivers() {
    for (client, module) in [
        ("GitHub", "git_hub"),
        ("Github", "github"),
        ("XMLHttpClient", "xml_http_client"),
        ("V2API", "v2_api"),
    ] {
        assert_eq!(name::module_name(client), module);
    }
    assert_eq!(name::type_name("get_sha256sum"), "GetSha256sum");
    assert_eq!(name::type_name("one_2"), "One2");
    for receiver in [
        NodeKind::Client,
        NodeKind::Scope,
        NodeKind::Endpoint(Method {
            keyword: syn::parse_str("GET").unwrap(),
            custom: None,
        }),
    ] {
        assert!(name::reserved("clone", &receiver));
        assert!(name::reserved("with_custom", &receiver));
        assert!(!name::reserved("timeout", &receiver));
        assert!(!name::reserved("retry", &receiver));
    }
    accepted(
        r#"client Naming { base "https://names.test"; GET send(new: u32) { query { new; } } scope pages { GET request(); } }"#,
    );
}

#[test]
fn c1_7_closed_arrow_spellings_and_parentheses() {
    for spelling in [
        "Option<u8>",
        "::std::option::Option<u8>",
        "((core::option::Option<u8>))",
        "Result<u8,E>",
        "::core::result::Result<u8,E>",
    ] {
        let ty: syn::Type = syn::parse_str(spelling).unwrap();
        let (wrapper, inner) = flat::arrow(&ty);
        assert!(wrapper.is_some(), "{spelling}");
        assert_eq!(flat::tokens(inner), "u8");
    }
    for spelling in ["Maybe<u8>", "model::Option<u8>", "model::Result<u8,E>"] {
        assert!(flat::arrow(&syn::parse_str(spelling).unwrap()).0.is_none());
    }
}

#[test]
fn c1_9_token_use_is_conservative_but_strings_need_a_macro() {
    for expression in [
        "format!(\"{amount}\")",
        "object.amount",
        "|amount| 1",
        "opaque!(amount)",
    ] {
        accepted(&format!(
            r#"client Usage {{ base "https://uses.test"; GET run(amount: u32) {{ query {{ "x" = {expression}; }} }} }}"#
        ));
    }
    refuses(
        r#"client Usage { base "https://uses.test"; GET run(amount: u32) { query { "x" = ("amount"); } } }"#,
        "never used",
        "C1.9 ordinary literal",
    );
}

#[test]
fn c1_18_recovered_nodes_and_blocks_produce_no_dependent_semantic_messages() {
    let (_, text) = run(
        r#"client Recovery { GET broken(x: &str) { query { "broken" = ; } } GET other() { path "/bad"; } }"#,
    );
    assert!(!text.contains("borrowed input"));
    assert!(!text.contains("input `x`"));
    assert!(text.contains("path segments must be nonempty"));
    let (_, text) = run(r#"client Recovery { status { 200 => ; } GET sibling() -> Thing; }"#);
    assert!(!text.contains("no `base`"));
    assert!(!text.contains("no `read`"));
}

#[test]
fn c1_20_every_malformed_input_completes_resolution_and_validation() {
    for file in fs::read_dir(path().join("tests/parser/malformed")).unwrap() {
        let file = file.unwrap().path();
        if file.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let source = fs::read_to_string(&file).unwrap();
        if source.parse::<proc_macro2::TokenStream>().is_err() {
            continue;
        }
        let result = std::panic::catch_unwind(|| run(&source));
        assert!(result.is_ok(), "{}", file.display());
    }
}

macro_rules! placement_row {
    ($name:ident, $statement:expr, [$($expected:expr),+]) => {
        #[test] fn $name() {
            let statement = $statement;
            let expectations: [Option<&str>; 4] = [$($expected),+];
            for (column, expected) in expectations.into_iter().enumerate() {
                let root = r#"base "https://matrix.test"; read Reader;"#;
                let source = match column {
                    0 => format!("client Matrix {{ {statement} }}"),
                    1 => format!("client Matrix {{ {root} scope branch {{ {statement} }} }}"),
                    2 => format!("client Matrix {{ {root} GET leaf() {{ {statement} }} }}"),
                    _ => format!("client Matrix {{ {root} part fragment() {{ {statement} }} GET leaf(..fragment); }}"),
                };
                let (_, errors) = run(&source);
                if let Some(expected) = expected { assert!(errors.contains(expected), "table 1 row {} column {column}: {errors}", stringify!($name)); }
                else { assert!(errors.is_empty(), "table 1 row {} column {column}: {errors}", stringify!($name)); }
            }
        }
    }
}
placement_row!(
    c1_3_table1_base,
    r#"base "https://other.test";"#,
    [None, None, None, None]
);
placement_row!(
    c1_3_table1_path,
    "path \"records\";",
    [None, None, None, Some("a part cannot hold `path`")]
);
placement_row!(
    c1_3_table1_query,
    "query { \"q\" = 1; }",
    [None, None, None, None]
);
placement_row!(
    c1_3_table1_headers,
    "headers { \"x-q\" = 1; }",
    [None, None, None, None]
);
placement_row!(
    c1_3_table1_body_expression,
    "body make();",
    [
        Some("requires an endpoint"),
        Some("requires an endpoint"),
        None,
        None
    ]
);
placement_row!(
    c1_3_table1_body_entries,
    "body make { \"x\" = 1; }",
    [
        Some("requires an endpoint"),
        Some("requires an endpoint"),
        None,
        None
    ]
);
placement_row!(c1_3_table1_read, "read Reader;", [None, None, None, None]);
placement_row!(
    c1_3_table1_status,
    "status { 410 => () }",
    [None, None, None, None]
);
placement_row!(
    c1_3_table1_problem,
    "problem Issue read Reader;",
    [
        None,
        Some("requires the client"),
        Some("requires the client"),
        Some("a part cannot hold `problem`")
    ]
);
placement_row!(
    c1_3_table1_paginate,
    "paginate pager;",
    [
        Some("requires an endpoint"),
        Some("requires an endpoint"),
        None,
        None
    ]
);
placement_row!(
    c1_3_table1_idempotent,
    "idempotent;",
    [
        Some("requires an endpoint"),
        Some("requires an endpoint"),
        None,
        None
    ]
);
placement_row!(
    c1_3_table1_idle_timeout,
    "idle_timeout clock();",
    [None, None, None, None]
);
placement_row!(
    c1_3_table1_middleware,
    "middleware Layer;",
    [
        None,
        Some("requires the client"),
        Some("requires the client"),
        Some("a part cannot hold `middleware`")
    ]
);

#[test]
fn c1_17_table1_part_declarations() {
    accepted(r#"client Matrix { base "https://matrix.test"; part named() {} GET leaf(..named); }"#);
    refuses(
        "client Matrix { scope s { part inner() {} } }",
        "requires the client",
        "table 1 part / scope",
    );
    refuses(
        "client Matrix { GET leaf() { part inner() {} } }",
        "requires the client",
        "table 1 part / endpoint",
    );
    refuses(
        "client Matrix { part outer() { part inner() {} } GET leaf(..outer); }",
        "requires the client",
        "table 1 part / part",
    );
}
#[test]
fn c1_17_table1_spreads() {
    refuses(
        "client Matrix(..absent) {}",
        "client cannot name a part",
        "table 1 spread / client",
    );
    accepted(
        r#"client Matrix { base "https://matrix.test"; part named() {} scope branch(..named) { GET leaf(); } }"#,
    );
    accepted(r#"client Matrix { base "https://matrix.test"; part named() {} GET leaf(..named); }"#);
    refuses(
        "client Matrix { part outer(..inner) {} part inner() {} GET leaf(..outer); }",
        "part names no other part",
        "table 1 spread / part",
    );
}

fn table2_single(row: &str, first: &str, second: &str, endpoint_only: bool, allowed_part: bool) {
    let setup = r#"base "https://matrix.test"; read Reader;"#;
    accepted(&format!(
        "client Matrix {{ {setup} GET leaf() {{ {first} }} }}"
    )); // not set above
    if !endpoint_only {
        let base = if row == "base" {
            ""
        } else {
            r#"base "https://matrix.test";"#
        };
        let source =
            format!("client Matrix {{ {base} {first} scope branch {{ {second} GET leaf(); }} }}");
        accepted(&source);
        let parsed = parse_source(&source);
        let client = parsed.client.as_ref().unwrap();
        let model = resolve::resolve(client, &mut crate::diag::Diagnostics::default());
        let endpoint = model.endpoints.first().unwrap();
        let actual = match row {
            "base" => endpoint.base.as_ref().unwrap().origin.written,
            "read" => endpoint.reader.as_ref().unwrap().origin.written,
            "idle_timeout" => endpoint.idle_timeout.as_ref().unwrap().origin.written,
            _ => panic!("unexpected nearest row {row}"),
        };
        assert_eq!(actual.name, "branch", "table 2 {row} / nearest");
    } else {
        refuses(
            &format!("client Matrix {{ {first} GET leaf(); }}"),
            "requires an endpoint",
            &format!("table 2 {row} / above does not arise"),
        );
    }
    for edit in ["override", "remove"] {
        refuses(
            &format!("client Matrix {{ {setup} GET leaf() {{ {edit} {first} }} }}"),
            "cannot be overridden or removed",
            &format!("table 2 {row} / {edit}"),
        );
    }
    for (column, parts, signature, own, expected) in [
        (
            "part",
            format!("part one() {{ {first} }}"),
            "..one",
            "",
            None,
        ),
        (
            "part and node",
            format!("part one() {{ {first} }}"),
            "..one",
            second,
            Some("written in the part"),
        ),
        (
            "two parts",
            format!("part one() {{ {first} }} part two() {{ {second} }}"),
            "..one, ..two",
            "",
            Some("both parts"),
        ),
    ] {
        let (_, errors) = run(&format!(
            "client Matrix {{ {setup} {parts} GET leaf({signature}) {{ {own} }} }}"
        ));
        if !allowed_part {
            assert!(
                errors.contains("a part cannot hold"),
                "table 2 {row} / {column}: {errors}"
            );
        } else if let Some(expected) = expected {
            assert!(
                errors.contains(expected),
                "table 2 {row} / {column}: {errors}"
            );
        } else {
            assert!(errors.is_empty(), "table 2 {row} / {column}: {errors}");
        }
    }
}
macro_rules! single_row {
    ($name:ident,$row:expr,$a:expr,$b:expr,$endpoint:expr,$part:expr) => {
        #[test]
        fn $name() {
            table2_single($row, $a, $b, $endpoint, $part);
        }
    };
}
single_row!(
    c1_13_table2_base,
    "base",
    r#"base "https://one.test";"#,
    r#"base "https://two.test";"#,
    false,
    true
);
single_row!(
    c1_13_table2_read,
    "read",
    "read First;",
    "read Second;",
    false,
    true
);
single_row!(
    c1_13_table2_idle_timeout,
    "idle_timeout",
    "idle_timeout first();",
    "idle_timeout second();",
    false,
    true
);
single_row!(
    c1_13_table2_body_expression,
    "body expression",
    "body first();",
    "body second();",
    true,
    true
);
single_row!(
    c1_13_table2_paginate,
    "paginate",
    "paginate first;",
    "paginate second;",
    true,
    true
);
single_row!(
    c1_13_table2_idempotent,
    "idempotent",
    "idempotent;",
    "idempotent false;",
    true,
    true
);

#[test]
fn c1_13_table2_path() {
    let text = accepted(
        r#"client Matrix { base "https://matrix.test"; path "one"; scope branch { path "two/"; GET leaf() { path "three"; } } }"#,
    );
    let paths: Vec<_> = text.lines().filter(|l| l.starts_with("path")).collect();
    assert_eq!(paths.len(), 3);
    assert!(paths[0].contains("one"));
    assert!(paths[1].contains("two/"));
    assert!(paths[2].contains("three"));
    for edit in ["override", "remove"] {
        refuses(
            &format!(
                r#"client Matrix {{ base "https://matrix.test"; GET leaf() {{ {edit} path "x"; }} }}"#
            ),
            "paths cannot",
            &format!("table 2 path / {edit}"),
        );
    }
    for signature in ["..one", "..one, ..two"] {
        refuses(
            &format!(
                r#"client Matrix {{ part one() {{ path "a"; }} part two() {{ path "b"; }} GET leaf({signature}) {{ path "c"; }} }}"#
            ),
            "a part cannot hold `path`",
            "table 2 path / parts",
        );
    }
}
#[test]
fn c1_21_table2_body_entries() {
    let text = accepted(
        r#"client Matrix { base "https://matrix.test"; part one(a: A) { body make { a; } } part two(b: B) { body make { b; } } POST leaf(c: C, ..one, ..two) { body make { c; } } }"#,
    );
    let entries: Vec<_> = text
        .lines()
        .filter(|l| l.starts_with("body_entry"))
        .collect();
    assert_eq!(entries.len(), 3);
    for (line, key) in entries.iter().zip(["a", "b", "c"]) {
        assert!(line.contains(key), "table 2 body entries / order {key}");
    }
    for edit in ["override \"q\" = 1;", "remove \"q\";"] {
        refuses(
            &format!(
                r#"client Matrix {{ base "https://matrix.test"; POST leaf() {{ body make {{ {edit} }} }} }}"#
            ),
            "body",
            "table 2 body entries / entry edit",
        );
    }
    refuses(
        r#"client Matrix { part one() { body make { "q" = 1; } } POST leaf(..one) { body other { "r" = 2; } } }"#,
        "entry bodies join only",
        "table 2 body entries / different functions",
    );
    refuses(
        r#"client Matrix { part one() { body make { "q" = 1; } } part two() { body other { "r" = 2; } } POST leaf(..one, ..two); }"#,
        "both parts",
        "table 2 body entries / two different functions",
    );
    refuses(
        r#"client Matrix { part one() { body make { "q" = 1; } } POST leaf(..one) { body make { "q" = 2; } } }"#,
        "written in the part",
        "table 2 body entries / duplicate key",
    );
    refuses(
        r#"client Matrix { scope s { body make { "q" = 1; } } }"#,
        "requires an endpoint",
        "table 2 body entries / cannot inherit",
    );
}

macro_rules! client_row { ($name:ident,$word:expr,$statement:expr) => { #[test] fn $name() {
    accepted(&format!("client Matrix {{ {} }}",$statement));
    refuses(&format!("client Matrix {{ scope s {{ {} }} }}",$statement),"requires the client",concat!("table 2 ",$word," / below client"));
    for edit in ["override","remove"] { refuses(&format!("client Matrix {{ {edit} {} }}",$statement),"cannot be overridden or removed",concat!("table 2 ",$word," / edit")); }
    for signature in ["..one","..one, ..two"] { refuses(&format!("client Matrix {{ part one() {{ {0} }} part two() {{ {0} }} GET leaf({signature}); }}",$statement),"a part cannot hold",concat!("table 2 ",$word," / parts")); }
} }; }
client_row!(
    c1_13_table2_problem,
    "problem",
    "problem Issue read Reader;"
);
client_row!(c1_19_table2_middleware, "middleware", "middleware Layer;");

#[test]
fn c1_13_table2_entries() {
    for (kind, key, repeat) in [("query", "q", "q"), ("headers", "X-Key", "x-KEY")] {
        let setup = r#"base "https://matrix.test";"#;
        accepted(&format!(
            r#"client Matrix {{ {setup} GET leaf() {{ {kind} {{ "{key}" = 1; }} }} }}"#
        ));
        refuses(
            &format!(
                r#"client Matrix {{ {setup} {kind} {{ "{key}" = 1; }} GET leaf() {{ {kind} {{ "{repeat}" = 2; }} }} }}"#
            ),
            "already set above",
            &format!("table 2 {kind} / plain"),
        );
        accepted(&format!(
            r#"client Matrix {{ {setup} {kind} {{ "{key}" = 1; }} GET leaf() {{ {kind} {{ override "{repeat}" = 2; }} }} }}"#
        ));
        accepted(&format!(
            r#"client Matrix {{ {setup} {kind} {{ "{key}" = 1; }} GET leaf() {{ {kind} {{ remove "{repeat}"; }} }} }}"#
        ));
        for edit in [
            format!("override \"{key}\" = 2;"),
            format!("remove \"{key}\";"),
        ] {
            refuses(
                &format!("client Matrix {{ {setup} GET leaf() {{ {kind} {{ {edit} }} }} }}"),
                if edit.starts_with("remove") {
                    "to remove"
                } else {
                    "above"
                },
                &format!("table 2 {kind} / absent edit"),
            );
        }
        accepted(&format!(
            r#"client Matrix {{ {setup} {kind} {{ "{key}" = 1; }} part one() {{ {kind} {{ override "{repeat}" = 2; }} }} GET leaf(..one); }}"#
        ));
        refuses(
            &format!(
                r#"client Matrix {{ {setup} part one() {{ {kind} {{ "{key}" = 1; }} }} GET leaf(..one) {{ {kind} {{ "{repeat}" = 2; }} }} }}"#
            ),
            "written in the part",
            &format!("table 2 {kind} / part and node"),
        );
        refuses(
            &format!(
                r#"client Matrix {{ {setup} part one() {{ {kind} {{ "{key}" = 1; }} }} part two() {{ {kind} {{ "{repeat}" = 2; }} }} GET leaf(..one,..two); }}"#
            ),
            "both parts",
            &format!("table 2 {kind} / two parts"),
        );
    }
}
#[test]
fn c5_3_table2_status_arms() {
    accepted(
        r#"client Matrix { base "https://matrix.test"; status { 4xx => () } GET leaf() { status { override 410 => (), 200 => (), 2xx => () } } }"#,
    );
    refuses(
        r#"client Matrix { status { 4xx => () } GET leaf() { status { 410 => () } } }"#,
        "already set above",
        "table 2 arms / exact below class",
    );
    refuses(
        r#"client Matrix { status { 4xx => () } GET leaf() { status { remove 410 } } }"#,
        "only the `4xx`",
        "table 2 arms / removal matches key only",
    );
    refuses(
        r#"client Matrix { status { 201 => () } GET leaf() { status { override 2xx => () } } }"#,
        "leaves the inherited arm",
        "table 2 arms / class over exact",
    );
    for edits in [
        "override 2xx => (), override 201 => ()",
        "remove 201, override 2xx => ()",
    ] {
        accepted(&format!(
            r#"client Matrix {{ base "https://matrix.test"; status {{ 201 => () }} GET leaf() {{ status {{ {edits} }} }} }}"#
        ));
    }
    accepted(
        r#"client Matrix { base "https://matrix.test"; status { 201 => () } GET leaf() { status { 2xx => () } } }"#,
    );
    accepted(
        r#"client Matrix { base "https://matrix.test"; status { 410 => () } part one() { status { override 410 => () } } GET leaf(..one); }"#,
    );
    refuses(
        r#"client Matrix { part one() { status { 410 => () } } GET leaf(..one) { status { 410 => () } } }"#,
        "written in the part",
        "table 2 arms / part and node",
    );
    refuses(
        r#"client Matrix { part one() { status { 410 => () } } part two() { status { 410 => () } } GET leaf(..one,..two); }"#,
        "both parts",
        "table 2 arms / two parts",
    );
}

fn table9(row: &str, setup: &str, own: &str, paths: bool) {
    for changed in [false, true] {
        let change = if changed {
            r#"base "https://changed.test";"#
        } else {
            ""
        };
        let source = format!("client Gate(token: Secret) {{ {setup} scope target {{ {own} GET leaf() {{ {change} }} }} }}");
        let mut parsed = parse_source(&source);
        let client = parsed.client.as_ref().unwrap();
        name::check(client, &mut parsed.diagnostics);
        let model = resolve::resolve(client, &mut parsed.diagnostics);
        check::check(client, &model, &mut parsed.diagnostics);
        assert!(
            parsed.diagnostics.is_empty(),
            "table 9 {row} / changed={changed}: {}",
            parsed.diagnostics.into_tokens()
        );
        let endpoint = &model.endpoints[0];
        let actual = if paths {
            endpoint.paths[0].crosses_base
        } else {
            endpoint.headers[0].crosses_base
        };
        assert_eq!(actual, changed, "table 9 {row} / column changed={changed}");
        // with_base is a caller operation: the list and its gate do not change.
        assert_eq!(model.endpoints.len(), 1, "table 9 {row} / caller with_base");
    }
}
#[test]
fn c8_9_table9_client_base() {
    table9(
        "client own base",
        r#"base "https://gate.test"; headers { "authorization" = token; }"#,
        "",
        false,
    );
}
#[test]
fn c8_9_table9_client_no_base() {
    table9(
        "client no base",
        r#"headers { "authorization" = token; }"#,
        r#"base "https://gate.test";"#,
        false,
    );
}
#[test]
fn c8_9_table9_scope_part_base() {
    for changed in [false, true] {
        let base = if changed {
            r#"base "https://changed.test";"#
        } else {
            ""
        };
        let text = accepted(&format!(
            r#"client Gate(token: Secret) {{ headers {{ "authorization" = token; }} part regional() {{ base "https://gate.test"; }} scope target(..regional) {{ GET leaf() {{ {base} }} }} }}"#
        ));
        assert!(
            text.contains(&format!("cross_base={changed}")),
            "table 9 scope part base / changed={changed}"
        );
    }
}
#[test]
fn c8_9_table9_scope_own_base() {
    table9(
        "scope own base",
        "",
        r#"base "https://gate.test"; headers { "authorization" = client.token; }"#,
        false,
    );
}
#[test]
fn c8_9_table9_scope_inherited_base() {
    table9(
        "scope inherited base",
        r#"base "https://gate.test";"#,
        r#"headers { "authorization" = client.token; }"#,
        false,
    );
}
#[test]
fn c8_9_table9_part_entry() {
    for changed in [false, true] {
        let base = if changed {
            r#"base "https://changed.test";"#
        } else {
            ""
        };
        let text = accepted(&format!(
            r#"client Gate(token: Secret) {{ base "https://gate.test"; part credential() {{ headers {{ "authorization" = client.token; }} }} scope target(..credential) {{ GET leaf() {{ {base} }} }} }}"#
        ));
        assert!(
            text.contains(&format!("cross_base={changed}")),
            "table 9 part entry / changed={changed}"
        );
    }
}
#[test]
fn c8_9_table9_path() {
    table9(
        "path",
        r#"base "https://gate.test"; path "{token}";"#,
        "",
        true,
    );
}
#[test]
fn c8_9_table9_body_entry() {
    let text = accepted(
        r#"client Gate { base "https://gate.test"; POST leaf(token: Secret) { body make { token; } } }"#,
    );
    assert!(text.contains("body_entry"), "table 9 body / sent");
    assert!(
        !text.contains("cross_base=true"),
        "table 9 body / no descendant base"
    );
}
#[test]
fn c8_9_table9_nonsecret() {
    let text = accepted(
        r#"client Gate { base "https://gate.test"; headers { "x-value" = "public"; } GET leaf() { base "https://changed.test"; } }"#,
    );
    assert!(
        text.contains("cross_base=true"),
        "table 9 nonsecret / adapter gate, no type inference"
    );
}
#[test]
fn c8_9_table9_middleware() {
    let text = accepted(
        r#"client Gate(token: Secret) { base "https://gate.test"; middleware Signed(token); GET leaf() { base "https://changed.test"; } }"#,
    );
    assert_eq!(
        text.lines().filter(|l| l.starts_with("middleware")).count(),
        1,
        "table 9 middleware / once in new, retained under every base"
    );
}

#[test]
fn c1_18_semantic_fixture_reaches_each_intended_catalogue_message() {
    let mut covered = std::collections::BTreeSet::new();
    for line in fs::read_to_string(path().join("tests/diagnostics/rules/coverage.tsv"))
        .unwrap()
        .lines()
    {
        let (file, number) = line.split_once('\t').unwrap();
        let number: u8 = number.parse().unwrap();
        let source = fs::read_to_string(path().join("tests/diagnostics/rules").join(file)).unwrap();
        let body = source
            .split("concord::api! {")
            .nth(1)
            .unwrap()
            .rsplit_once("}\nfn main")
            .unwrap()
            .0;
        let mut parsed = parse_source(body);
        if let Some(client) = &parsed.client {
            name::check(client, &mut parsed.diagnostics);
            let model = resolve::resolve(client, &mut parsed.diagnostics);
            check::check(client, &model, &mut parsed.diagnostics);
        }
        let numbers = parsed.diagnostics.numbers().to_vec();
        assert!(
            numbers.contains(&number),
            "{file}: wanted {number}, got {numbers:?}: {}",
            parsed.diagnostics.into_tokens()
        );
        covered.insert(number);
    }
    assert_eq!(covered, (1..=46).collect());
}

#[test]
fn c1_18_accepted_description_is_written_as_rust() {
    let tokens = crate::expand(
        r#"client Complete { base "https://complete.test"; GET fetch(); }"#
            .parse()
            .unwrap(),
    )
    .to_string();
    assert!(!tokens.contains("compile_error"));
    syn::parse_file(&tokens).unwrap();
}

#[test]
fn c1_9_unary_not_does_not_turn_a_string_into_a_macro_use() {
    refuses(
        r#"client Usage { base "https://uses.test"; GET fetch(amount: u8) { idempotent !("amount".is_empty()); } }"#,
        "never used",
        "C1.9 unary !",
    );
}

#[test]
fn c8_9_rewrite_and_remove_reset_the_gate_but_identical_bases_count() {
    for edit in [
        r#"override "authorization" = client.token;"#,
        r#"remove "authorization";"#,
    ] {
        let text = accepted(&format!(
            r#"client Gate(token: Secret) {{ base "https://gate.test"; headers {{ "authorization" = token; }} GET leaf() {{ base "https://gate.test"; headers {{ {edit} }} }} }}"#
        ));
        assert!(!text.contains("cross_base=true"), "C8.9 {edit}");
    }
    let text = accepted(
        r#"client Gate(token: Secret) { base "https://gate.test"; headers { "authorization" = token; } GET leaf() { base "https://gate.test"; } }"#,
    );
    assert!(text.contains("cross_base=true"));
}

#[test]
fn c1_20_resolution_inputs_match_regressions_and_tour_descriptions() {
    for entry in fs::read_dir(path().join("tests/snapshots/resolved")).unwrap() {
        let file = entry.unwrap().path();
        if file.extension().is_none_or(|x| x != "api") {
            continue;
        }
        let stem = file.file_stem().unwrap().to_str().unwrap();
        let expected = if let Some(client) = stem.strip_prefix("tour_") {
            tour_description(client)
        } else {
            let case = stem.strip_prefix("regression_").unwrap();
            fs::read_to_string(path().join(format!("tests/parser/regressions/{case}.rs"))).unwrap()
        };
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            expected,
            "copied description {}",
            file.display()
        );
    }
}

#[test]
fn c1_18_damage_does_not_invent_an_unused_part_or_unknown_ancestor_input() {
    let (_, errors) = run(
        r#"client Recovered(region: String) { broken(); part routing() {} scope s { GET fetch() { path "{client.region}"; } } }"#,
    );
    assert!(!errors.contains("unknown input"));
    let (_, errors) =
        run(r#"client Recovered { part routing() {} scope s(..routing) { path ; GET fetch(); } }"#);
    assert!(!errors.contains("never used"));
}

/// Copy the leading client description, with grant's separate Files fragment.
fn tour_description(client: &str) -> String {
    fn comment(source: &str, marker: &str) -> String {
        let lines: Vec<_> = source.lines().collect();
        let start = lines.iter().position(|line| line.contains(marker)).unwrap();
        let text = lines[start..]
            .iter()
            .take_while(|line| line.starts_with("    // "))
            .filter_map(|line| line.strip_prefix("    // "))
            .collect::<Vec<_>>()
            .join("\n");
        text[..text.find("\n}").unwrap() + 2].to_owned() + "\n"
    }
    let source = fs::read_to_string(path().join(format!("../tests/tour/src/{client}.rs"))).unwrap();
    let mut description = comment(&source, "// pub client ");
    if client == "files" {
        let source = fs::read_to_string(path().join("../tests/tour/src/grant.rs")).unwrap();
        let grant = comment(&source, "// POST grant(");
        description.truncate(description.len() - 2);
        description.push_str(&grant);
        description.push_str("}\n");
    }
    description
}

#[path = "client_bindings.rs"]
mod tour;

#[path = "diagnostic_repairs.rs"]
mod diagnostic_repairs;

#[path = "expression_scopes.rs"]
mod expression_scopes;
