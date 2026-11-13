//! Regression cases from both accepted readings of P3.1.
use super::*;
use crate::diag::{self, Written};
use proc_macro2::Span;

fn line(source: &str) -> Written {
    Written::new(source.parse().unwrap())
}
fn messages(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    diagnostic_spans(parsed(source).diagnostics.into_tokens(), &mut found);
    found.into_iter().map(|(text, _, _)| text).collect()
}
fn only_error(source: &str) -> String {
    let errors = messages(source);
    assert_eq!(errors.len(), 1, "{source}: {errors:?}");
    errors.into_iter().next().unwrap()
}

#[test]
fn c1_18_a1_depth_is_bounded_before_syn_recursion() {
    for depth in [65, 2_048] {
        let mut source = "client C {".to_owned();
        for _ in 0..depth {
            source.push_str("scope nested {");
        }
        source.push_str("GET leaf();");
        for _ in 0..=depth {
            source.push('}');
        }
        assert!(only_error(&source).contains("limit of 64 levels"));
    }
    for depth in [65, 96, 128, 192, 256, 2_048, 4_000] {
        for expression in [
            format!("{}x{}", "(".repeat(depth), ")".repeat(depth)),
            format!("{}x", "!".repeat(depth)),
        ] {
            assert!(
                only_error(&format!("client C {{ POST g() {{ body {expression}; }} }}"))
                    .contains("limit of 64 levels")
            );
        }
    }
    for depth in [65, 128, 2_048] {
        let expression = format!("{}value", "move |x: X, y: Y| ".repeat(depth));
        assert!(
            only_error(&format!("client C {{ POST g() {{ body {expression}; }} }}"))
                .contains("limit of 64 levels")
        );
    }
    let pattern = format!("{}last", "binding @ ".repeat(2_048));
    assert!(only_error(&format!(
        "client C {{ POST g() {{ body move |{pattern}| value; }} }}"
    ))
    .contains("limit of 64 levels"));
    let mut source = "client C {".to_owned();
    for _ in 0..64 {
        source.push_str("scope nested {");
    }
    source.push_str("GET leaf();");
    for _ in 0..=64 {
        source.push('}');
    }
    root(&source);
    root(&format!(
        "client C {{ POST g() {{ body {}x{}; }} }}",
        "(".repeat(64),
        ")".repeat(64)
    ));
    root(&format!(
        "client C {{ POST g() {{ body {}x; }} }}",
        "!".repeat(64)
    ));
}

#[test]
fn c1_18_a2_recovery_never_reinterprets_consumed_prefixes() {
    for source in [
        "client C { METHOD read(x); GET ok(); }",
        r#"client C { METHOD "PURGE"suffix read(); GET ok(); }"#,
        "client C { GET bad(x:) GET ok(); }",
    ] {
        let result = parsed(source);
        let node = result.client.unwrap().node;
        assert!(node.statements.is_empty());
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].name, "ok");
        assert_eq!(messages(source).len(), 1, "{}", errors(source));
    }

    let result = parsed("client C { override read ; }");
    assert!(result.client.unwrap().node.statements.is_empty());
    let errors = messages("client C { override read ; }");
    assert_eq!(errors.len(), 1);
    assert!(!errors[0].contains("duplicate"));
    let result = parsed("client C { GET read(x); GET ok(); }");
    let node = result.client.unwrap().node;
    assert!(node.statements.is_empty());
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].name, "ok");
    let result = parsed("client C { read ; read Json; }");
    assert_eq!(result.client.unwrap().node.statements.len(), 1);
    assert_eq!(messages("client C { read ; read Json; }").len(), 1);
}

#[test]
fn c1_18_a3_failed_endpoint_carries_a_mark() {
    let result = parsed("client C { POST broken() { body ; } GET good(); }");
    let node = result.client.unwrap().node;
    assert!(!node.parse_failed);
    assert!(node.children[0].parse_failed);
    assert!(!node.children[1].parse_failed);
}

#[test]
fn c1_20_a4_suffixes_are_refused_in_every_language_string_position() {
    for source in [
        r#"client C { path "items"suffix; }"#,
        r##"client C { path r#"items"#suffix; }"##,
        r#"client C { base "https://x"suffix; }"#,
        r#"client C { METHOD "PURGE"suffix e(); GET good(); }"#,
        r#"client C { query { "key"suffix = value; } }"#,
        r#"client C { headers { remove "key"suffix; } }"#,
        r#"client C { query { "key" = "value"suffix; } }"#,
        r#"client C { POST g() { body json { "key"suffix = value; } } }"#,
    ] {
        assert!(
            only_error(source).contains("cannot have a suffix"),
            "{source}"
        );
    }
}

#[test]
fn c1_18_a5_part_problem_and_middleware_await_37() {
    let node =
        root("client C { part p() { problem ApiProblem read MyReader; middleware MyLayer; } }");
    assert_eq!(node.parts[0].statements.len(), 2);
    assert!(!node.parts[0].parse_failed);
}

#[test]
fn c1_18_a6_substitution_is_one_pass() {
    let message = diag::m23_remove_query("{kind}", &line(r#"remove "{kind}";"#)).render();
    assert!(message.contains("query pair `{kind}`"));
    assert!(message.contains(r#"delete `remove "{kind}";`"#));
    let authored = line(r#"body format!("{node} {statement} {line}");"#);
    let message = diag::m18_requires_endpoint(&authored, "body", "client").render();
    assert!(message.contains(&authored.text()));
}

#[test]
fn c1_18_a7_edited_statements_use_their_actual_keyword_and_line() {
    for keyword in [
        "base",
        "read",
        "idle_timeout",
        "body",
        "paginate",
        "idempotent",
        "problem",
        "middleware",
        "headers",
        "query",
        "status",
    ] {
        let message =
            diag::m24_edited_statement(keyword, &line(&format!("{keyword} custom;"))).render();
        assert!(message.starts_with(&format!("`{keyword}` cannot be")));
        assert!(!message.contains("paths cannot"));
        assert!(message.contains(&format!("{keyword} custom;")));
    }
}

#[test]
fn c1_18_o1_contextual_endpoint_names_get_message_15() {
    for word in [
        "base",
        "path",
        "query",
        "headers",
        "body",
        "read",
        "status",
        "problem",
        "middleware",
        "paginate",
        "idempotent",
        "idle_timeout",
        "scope",
        "part",
        "remove",
        "client",
    ] {
        for tail in ["();", "() -> Health;", "(id: Id) -> Health;"] {
            let source = format!("client C {{ {word}{tail} GET good(); }}");
            if word == "problem" && tail == "();" {
                root(&source); // The complete unit-valued statement wins (grammar / Fable 15).
                continue;
            }
            let error = only_error(&source);
            assert!(
                error.starts_with("expected an HTTP method"),
                "{word}: {error}"
            );
            assert!(error.contains(&format!("GET {word}{tail}")));
            assert_eq!(
                parsed(&source).client.unwrap().node.children[0].name,
                "good"
            );
        }
    }
    // The endpoint lookahead must leave a parenthesized Rust RHS intact.
    root("client C { read (|a, b| f(a, b))(x, key); }");
}

#[test]
fn c1_18_o2_help_contains_the_authors_tokens() {
    for (source, expected) in [
        (
            r#"client C { GET g(); path "tasks/{id}"; }"#,
            r#"move `path "tasks/{id}";`"#,
        ),
        (
            "client C { POST g() { body form(payload); body form(second); } }",
            "keep one declaration of this behaviour",
        ),
        (
            "client C { scope s { idempotent retries_allowed; } }",
            "place this behaviour on an endpoint",
        ),
        (
            "client C { scope s { problem MyFault read FaultCodec; } }",
            "consolidate it with an existing problem declaration",
        ),
        (
            r#"client C { METHOD "LOCK/2" acquire(lock_id: LockId); }"#,
            r#"write `METHOD "LOCK2" acquire"#,
        ),
    ] {
        assert!(
            messages(source)
                .iter()
                .any(|message| message.contains(expected)),
            "{}",
            errors(source)
        );
    }
    let text = line("path r#\"own\"#;").text();
    assert_eq!(text, "path r#\"own\"#;");
    let fallback = Written::new(quote::quote!(body custom(payload);));
    // A39 requires conventional Rust spacing even without joined spans.
    assert_eq!(fallback.text(), "body custom(payload);");
}

#[test]
fn c1_18_o3_all_forbidden_part_contents_are_retained() {
    let node =
        root(r#"client C { part p() { path "v1"; problem E; middleware M; GET child(); } }"#);
    assert_eq!(node.parts[0].statements.len(), 3);
    assert_eq!(node.parts[0].children.len(), 1);
    assert!(!node.parts[0].parse_failed);
}

#[test]
fn c1_18_o4_after_part_has_its_own_label_and_help() {
    let error = only_error("client C { part p() {} idle_timeout custom_clock; }");
    assert!(error.contains("label: statement appears after a part"));
    assert!(error.contains("move `idle_timeout custom_clock;` before the first part"));
    assert!(!error.contains("child"));
}

#[test]
fn c1_18_o5_catalogue_variants_and_second_places_are_contextual() {
    let first = Span::call_site();
    let variants = [
        diag::m01_duplicate_part("p", "q", first),
        diag::m01_duplicate_input("x", "y", first),
        diag::m03_reserved_child("clone", "copy_item"),
        diag::m04_keyword_input("kind", "OwnType", "match"),
        diag::m04_keyword_scope(&line("scope r#match {}"), "matching"),
        diag::m04_keyword_endpoint(&line("GET r#match();"), "matching"),
        diag::m07_owner_input("client", "client_id", first),
        diag::m12_short_entry("client", "x"),
        diag::m12_ancestor_base("client", "base_url"),
        diag::m14_part_attribute(&line("#[cfg(x)]"), "the client"),
        diag::m14_client_attribute(&line("#[deprecated]"), "a scope or endpoint"),
        diag::m14_scope_attribute(&line("#[allow(x)]")),
        diag::m14_endpoint_attribute(&line("#[allow(x)]")),
        diag::m16_part_after_child(&line("part own() {}")),
        diag::m21_inherited_query("q", &line(r#""q" = own;"#), first),
        diag::m21_inherited_arm("404", &line("404 => Missing"), first),
        diag::m22_override_query("q", &line(r#""q" = own;"#)),
        diag::m22_override_arm("404", &line("404 => Missing")),
        diag::m22_override_body("q", &line(r#""q" = own;"#)),
        diag::m23_remove_header("accept", &line(r#"remove "accept";"#)),
        diag::m23_remove_query("q", &line(r#"remove "q";"#)),
        diag::m23_remove_body("q", &line(r#"remove "q";"#)),
        diag::m23_remove_class_member("404", "4xx", &line("404 => Missing")),
        diag::m38_two_parts("base", "a", "b", first),
        diag::m38_part_key("header", "accept", "p", first),
        diag::m38_duplicate_key("query pair", "q", &line(r#""q" = own;"#), first),
        diag::m38_part_input("x", "p", first),
        diag::m38_duplicate_spread("p", first),
        diag::m38_body_conflict("p", &line("body make();"), first),
        diag::m46_result_arm("200", &line("200 { body } => Ok(unpack(body))"), first),
        diag::m46_option_exact_arm("201", &line("201 { body } => Some(unpack(body))"), first),
    ];
    for message in &variants {
        assert!(!message.render().contains("{replacement}"));
    }
    assert!(variants[3].render().contains("OwnType"));
    assert!(variants[6]
        .render()
        .contains(r#"query { "client" = client_id; }"#));
    assert!(variants[7].render().contains(r#"write `"x" = client.x;`"#));
    assert!(variants[15].render().contains("override 404 => Missing"));
    assert!(!variants[15].render().contains("\"404\""));
    assert!(variants[23].render().contains("parts `a` and `b`"));
    assert!(variants[29].render().contains("a `Result`"));
    assert!(!variants[29].render().contains("Some("));
    let mut sink = diag::Diagnostics::default();
    sink.raise(
        first,
        diag::m17_duplicate_statement(&line("idle_timeout own;"), "idle_timeout", first),
    );
    let text = sink.into_tokens().to_string();
    assert!(text.contains("the first `idle_timeout` statement is here"));
    assert!(!text.contains("the first one is here"));
}

#[test]
fn c1_18_o7_damaged_nodes_entries_and_arms_are_marked() {
    let result = parsed(
        r#"client C {
        GET entries() { query { "bad" = ; "good" = value; } }
        GET arms() { status { 200 =>, 201 => value } }
        GET clean();
    }"#,
    );
    let node = result.client.unwrap().node;
    assert!(node.children[0].parse_failed);
    assert!(node.children[1].parse_failed);
    assert!(!node.children[2].parse_failed);
    let StatementKind::Query(entries) = &node.children[0].statements[0].kind else {
        panic!()
    };
    assert!(entries.parse_failed);
    let StatementKind::Status(arms) = &node.children[1].statements[0].kind else {
        panic!()
    };
    assert!(arms.parse_failed);
}

#[test]
fn c1_18_o8_recovery_eats_a_failed_lines_semicolon() {
    for source in [
        "client C { unknown { tokens }; GET good(); }",
        "client C { read { let = 0; }; GET good(); }",
    ] {
        assert_eq!(messages(source).len(), 1, "{}", errors(source));
        assert_eq!(parsed(source).client.unwrap().node.children[0].name, "good");
    }
}

#[test]
fn c1_18_o9_keywords_are_names_never_verbs_or_indefinite_articles() {
    for keyword in ["body", "paginate", "idempotent", "idle_timeout"] {
        let text = diag::m17_duplicate_statement(
            &line(&format!("{keyword} own;")),
            keyword,
            Span::call_site(),
        )
        .render();
        assert!(text.contains(&format!("declares `{keyword}`")));
        assert!(!text.contains(&format!("a {keyword}")));
        for node in ["client", "scope"] {
            let text =
                diag::m18_requires_endpoint(&line(&format!("{keyword} own;")), keyword, node)
                    .render();
            assert!(text.contains(&format!(
                "`{keyword}` stands on an endpoint, not on a {node}"
            )));
        }
    }
}

#[test]
fn c1_18_o10_lock_and_workspace_lint_policy_are_adopted() {
    let manifest = fs::read_to_string(package().join("Cargo.toml")).unwrap();
    assert!(manifest.contains("[lints]\nworkspace = true"));
    let lock = fs::read_to_string(package().join("../Cargo.lock")).unwrap();
    assert!(lock.contains("name = \"trybuild\""));
}

#[test]
fn c1_20_depth_guard_handles_flat_rust_chains_and_opaque_macros() {
    let conditional = format!("{}{{ value }}", "if flag { value } else ".repeat(2_048));
    assert!(only_error(&format!(
        "client C {{ POST g() {{ body {conditional}; }} }}"
    ))
    .contains("limit of 64 levels"));
    for operator in ["/", "%", "^", ">"] {
        let expression = format!("{}value", format!("value {operator} ").repeat(2_048));
        assert!(
            only_error(&format!("client C {{ POST g() {{ body {expression}; }} }}"))
                .contains("too long")
        );
    }
    let macro_tokens = "value + ".repeat(256) + "value";
    root(&format!(
        "client C {{ POST g() {{ body custom!({macro_tokens}); }} }}"
    ));
}

#[test]
fn c1_20_a71_postfix_chains_are_flat_and_closures_still_nest() {
    for depth in [64, 65] {
        root(&format!(
            "client C {{ POST g() {{body value{};}} }}",
            ".next()".repeat(depth)
        ));
        {
            let expr = format!("{}value", "move || ".repeat(depth));
            let source = format!("client C {{ POST g() {{ body {expr}; }} }}");
            if depth == 64 {
                root(&source);
            } else {
                assert!(only_error(&source).contains("limit of 64 levels"));
            }
        }
    }
    let source = format!(
        "client C {{ {}POST g() {{ body {}value{}; }}{} }}",
        "scope nested {".repeat(64),
        "(".repeat(64),
        ")".repeat(64),
        "}".repeat(64)
    );
    root(&source);
}

#[test]
fn c1_18_catalogue_additional_contexts_are_fully_bound() {
    let span = Span::call_site();
    for (message, expected) in [
        (
            diag::m11_scope_input_in_part("route", &line("route: Region")),
            "route: Region",
        ),
        (
            diag::m13_optional_base(&line("server: Base"), span),
            "server: Base",
        ),
        (
            diag::m14_statement_attribute(&line("#[cfg(x)]"), "read"),
            "unsupported attribute on `read`",
        ),
        (
            diag::m18_part_requires_endpoint("body", &line("..payload")),
            "..payload",
        ),
        (
            diag::m29_star_outside_path("route", &line(r#"query { "k" = "{route}"; }"#)),
            "{route}",
        ),
        (
            diag::m31_head_part_body("head_fields", &line("2xx => true"), span),
            "head_fields",
        ),
        (
            diag::m33_problem_reader(&line("problem Fault;"), "FaultReader"),
            "read FaultReader",
        ),
        (
            diag::m34_duplicate_field("body", &line("200 { body } => value"), span),
            "field `body` is written twice",
        ),
        (
            diag::m34_duplicate_binding("b", &line("200 { body: b, headers: h } => value"), span),
            "name `b` is bound twice",
        ),
        (
            diag::m37_part_statement("problem", &line("problem Fault;")),
            "move `problem Fault;`",
        ),
        (diag::m37_part_child(&line("GET own();")), "GET own();"),
        (
            diag::m37_part_spread(&line("..own")),
            "a part names no other part",
        ),
        (
            diag::m37_client_spread(&line("..own")),
            "the client cannot name a part",
        ),
        (
            diag::m38_two_part_inputs("value", "a", "b", span),
            "input `value` is declared in both parts `a` and `b`",
        ),
    ] {
        assert!(message.render().contains(expected), "{}", message.render());
    }
}

#[test]
fn c1_18_a39_a70_m6_unknown_settings_explain_their_actual_home() {
    for (line, help) in [
        (
            "timeout 5s;",
            "configure the timeout with `with_timeout` on the client or the call",
        ),
        (
            "retry Retry::standard();",
            "configure retries with `with_retry` on the client or the call",
        ),
        (
            "tries 3;",
            "configure retries with `with_retry` on the client or the call",
        ),
        (
            "base_url \"https://other.test\";",
            "set the request URL with the `base` statement",
        ),
        (
            "auth key;",
            "place credentials in the appropriate `headers` entries",
        ),
    ] {
        for body in [
            line.to_owned(),
            format!("scope items {{ {line} GET get(); }}"),
            format!("GET get() {{ {line} }}"),
            format!("part options() {{ {line} }}"),
        ] {
            let source = format!("client C {{ {body} }}");
            assert!(parsed(&source).diagnostics.numbers().is_empty());
            assert_eq!(
                only_error(&source),
                format!("expected a statement, part, scope or HTTP method\nhelp: {help}"),
                "{source}"
            );
        }
    }
    assert_eq!(
        only_error("client C { mystery value; }"),
        "expected a statement, part, scope or HTTP method"
    );
    // These names remain ordinary child names when the HTTP method is present.
    root("client C { GET timeout(); GET retry(); GET tries(); GET base_url(); GET auth(); }");
}

#[test]
fn c1_16_m6_bare_constructor_spelling_alone_does_not_identify_a_rust_item() {
    for name in ["Some", "Ok", "Err"] {
        let source=format!("client C {{ base \"https://types.test\"; HEAD get() -> u32 {{ status {{ 2xx => {name} }} }} }}");
        let output = crate::expand(source.parse().unwrap()).to_string();
        assert!(!output.contains("compile_error"), "{output}");
    }
}
