//! Parser trees, diagnostic catalogue, documentation, and malformed input checks.
use std::{
    fs,
    path::{Path, PathBuf},
};

use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;

use crate::{ast::*, parse};

fn parsed(source: &str) -> parse::Parsed {
    parse::parse(source.parse().unwrap())
}

fn root(source: &str) -> Node {
    let result = parsed(source);
    assert!(
        result.diagnostics.is_empty(),
        "{}",
        result.diagnostics.into_tokens()
    );
    result.client.unwrap().node
}

fn errors(source: &str) -> String {
    parsed(source).diagnostics.into_tokens().to_string()
}

fn package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[path = "message_catalogue.rs"]
mod messages_page;

#[test]
fn c1_7_optional_mark_is_retained_without_reading_the_type() {
    let node = root("client C(count?: u32, required: Option<u32>) { GET g(tags?: Vec<String>, milestone?: Option<u64>) { body json { tags; milestone; } } }");
    let inputs = &node.signature.unwrap().entries;
    let Parameter::Input(optional) = &inputs[0] else {
        panic!()
    };
    assert!(optional.optional.is_some());
    assert_eq!(optional.ty.to_token_stream().to_string(), "u32");
    let Parameter::Input(required) = &inputs[1] else {
        panic!()
    };
    assert!(required.optional.is_none());
    assert!(matches!(&required.ty, syn::Type::Path(_)));
    assert!(node.children[0]
        .signature
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .all(|entry| matches!(entry, Parameter::Input(input) if input.optional.is_some())));
}

#[test]
fn c1_3_all_twelve_statements_and_part_base_keep_the_written_order() {
    let node = root(
        r#"client C {
        base "https://api.example.com"; path "v1/";
        query { a; } headers { "accept" = "text/plain"; }
        read Json; status { 404 => None } problem E read Other;
        middleware A; middleware B; idle_timeout Duration::from_secs(30);
        part regional(route: Region) { base "https://{route}.api.example.com"; }
        GET g(..regional) { body json { a; "k" = v; } paginate pager; idempotent; }
    }"#,
    );
    let words: Vec<_> = node
        .statements
        .iter()
        .map(|statement| statement.keyword.to_string())
        .collect();
    assert_eq!(
        words,
        [
            "base",
            "path",
            "query",
            "headers",
            "read",
            "status",
            "problem",
            "middleware",
            "middleware",
            "idle_timeout"
        ]
    );
    assert!(matches!(
        &node.parts[0].statements[0].kind,
        StatementKind::Base(_)
    ));
    assert!(matches!(
        &node.children[0].statements[2].kind,
        StatementKind::Idempotent(None)
    ));
}

#[test]
fn c1_21_body_entries_and_expression_are_distinct() {
    let node = root(
        r#"client C {
        part p(a: A) { body crate::make::<T> { a; "k" = "{a}"; } }
        POST entries(..p) { body json { a; "k" = f(v)?; } }
        POST expression() { body json(x); }
        POST block() { body { let x = f(); json(x) }; }
    }"#,
    );
    let StatementKind::Body(Body::Entries { function, entries }) =
        &node.parts[0].statements[0].kind
    else {
        panic!()
    };
    assert_eq!(
        function.to_token_stream().to_string(),
        "crate :: make :: < T >"
    );
    assert_eq!(entries.entries.len(), 2);
    assert!(matches!(
        &entries.entries[1].kind,
        EntryKind::Value {
            value: Value::Template(_),
            ..
        }
    ));
    assert!(matches!(
        &node.children[1].statements[0].kind,
        StatementKind::Body(Body::Expression(syn::Expr::Call(_)))
    ));
    assert!(matches!(
        &node.children[2].statements[0].kind,
        StatementKind::Body(Body::Expression(syn::Expr::Block(_)))
    ));
    assert!(errors("client C { POST g() { body client.f { a; } } }").contains("expected `;`"));
}

#[test]
fn c1_21_body_function_paths_include_qualified_rust_paths() {
    let node = root("client C { POST g() { body <Codec as crate::Encode>::make::<T> { a; } } }");
    let StatementKind::Body(Body::Entries { function, entries }) =
        &node.children[0].statements[0].kind
    else {
        panic!()
    };
    assert!(function.qself.is_some());
    assert_eq!(entries.entries.len(), 1);
}

#[test]
fn c1_20_entry_value_classifies_only_a_whole_bare_string_as_template() {
    let node = root(
        r#"client C { query { "a" = "{x}"; "b" = "{".to_owned(); "c" = ("{"); "d" = f::<A, B>(|a, b| (a, b)); } }"#,
    );
    let StatementKind::Query(entries) = &node.statements[0].kind else {
        panic!()
    };
    for (index, entry) in entries.entries.iter().enumerate() {
        let EntryKind::Value { value, .. } = &entry.kind else {
            panic!()
        };
        assert_eq!(matches!(value, Value::Template(_)), index == 0);
    }
}

#[test]
fn c1_20_templates_decode_rust_escapes_raw_strings_and_escaped_braces() {
    let node = root(
        r##"client C { path r#"{{literal}}/{x}/{*owner.path}"#; base "https://\u{7b}region\u{7d}.example.com"; }"##,
    );
    let StatementKind::Path(template) = &node.statements[0].kind else {
        panic!()
    };
    assert!(
        matches!(&template.pieces[0], TemplatePiece::Text { text, .. } if text == "{literal}/")
    );
    assert!(
        matches!(&template.pieces[1], TemplatePiece::Hole { reference, star: false, .. } if reference.name == "x")
    );
    assert!(
        matches!(&template.pieces[3], TemplatePiece::Hole { reference, star: true, .. } if reference.owner.as_ref().unwrap() == "owner" && reference.name == "path")
    );
    let StatementKind::Base(Base::Template(base)) = &node.statements[1].kind else {
        panic!()
    };
    assert!(
        matches!(&base.pieces[1], TemplatePiece::Hole { reference, .. } if reference.name == "region")
    );
    for hole in [
        "{ x }",
        "{x.y.z}",
        "{/*x*/y}",
        "{x//comment\n}",
        "{",
        "}",
        "{}",
        "{*}",
        "{x..y}",
    ] {
        let source = format!("client C {{ path {:?}; }}", hole);
        assert!(!parsed(&source).diagnostics.is_empty(), "accepted {hole}");
    }
}

#[test]
fn c1_20_star_hole_placement_and_other_checks_await_step_4() {
    // Parsing preserves these forms for messages 29, 34, 45 and 14, rather than
    // performing semantic validation in the parser.
    let node = root(
        r#"client C { path "{*x}.json"; query { "k" = "{*x}"; } #[cfg(no)] GET g(#[cfg(no)] x: u32) { status { 600 { bodi, body: x, headers: x } => x } } }"#,
    );
    assert_eq!(node.children.len(), 1);
}

#[test]
fn c1_18_forms_named_by_later_diagnostics_remain_in_the_written_tree() {
    // These are deliberately invalid descriptions. Parsing must preserve the
    // offending tokens so resolve/check can give their assigned messages,
    // rather than replacing those messages with an early generic parse error.
    let node = root(
        r#"client C(..p) {
        override path "v1";
        part p(..q) { GET child(); }
        POST g() {
            body json { override "k" = v; remove "gone"; }
            status { nope => (), 600 => () }
        }
    }"#,
    );
    assert!(matches!(
        &node.signature.as_ref().unwrap().entries[0],
        Parameter::Spread(_)
    ));
    assert_eq!(node.statements[0].edit.as_ref().unwrap(), "override");
    assert!(matches!(
        &node.parts[0].signature.as_ref().unwrap().entries[0],
        Parameter::Spread(_)
    ));
    assert_eq!(node.parts[0].children.len(), 1);
    let StatementKind::Body(Body::Entries { entries, .. }) = &node.children[0].statements[0].kind
    else {
        panic!()
    };
    assert_eq!(entries.entries[0].edit.as_ref().unwrap(), "override");
    assert!(matches!(&entries.entries[1].kind, EntryKind::Remove(_)));
    let StatementKind::Status(arms) = &node.children[0].statements[1].kind else {
        panic!()
    };
    assert_eq!(arms.arms[0].key.to_string(), "nope");
}

#[test]
fn c1_20_signature_spreads_are_last_and_lists_have_no_stray_commas() {
    let node =
        root("client C { part p() {} part q() {} scope s(x: X, ..p, ..q,) { GET e(..p, ..q,); } }");
    let signature = node.children[0].signature.as_ref().unwrap();
    assert_eq!(signature.entries.len(), 3);
    let Parameter::Spread(spread) = &signature.entries[2] else {
        panic!()
    };
    assert_eq!(spread.name, "q");
    for source in [
        "client C(,) {}",
        "client C { GET g(,); }",
        "client C { GET g(..p, x: X); }",
    ] {
        assert!(!parsed(source).diagnostics.is_empty(), "accepted {source}");
    }
}

#[test]
fn c1_13_entries_and_arms_retain_edits_bindings_and_rust_expressions() {
    let node = root(
        r#"client C { GET g(remove: X, query: X) -> Reply { query { remove; query; override "k" = f(); remove "gone"; } status { override 200 { body: task, headers, status: code, } => make::<A, B>(task, |a, b| (a,b)), remove 4xx, 500 => { fail() }, } } }"#,
    );
    let endpoint = &node.children[0];
    let StatementKind::Query(entries) = &endpoint.statements[0].kind else {
        panic!()
    };
    assert!(
        matches!(&entries.entries[0].kind, EntryKind::Short(reference) if reference.name == "remove")
    );
    assert_eq!(entries.entries[2].edit.as_ref().unwrap(), "override");
    assert!(matches!(&entries.entries[3].kind, EntryKind::Remove(_)));
    let StatementKind::Status(arms) = &endpoint.statements[1].kind else {
        panic!()
    };
    assert_eq!(arms.arms.len(), 3);
    assert_eq!(
        arms.arms[0].fields.as_ref().unwrap().fields[0]
            .binding
            .as_ref()
            .unwrap(),
        "task"
    );
    assert!(arms.arms[1].expression.is_none());
    for source in [
        "client C { GET g() { status { 200 {} => () } } }",
        "client C { GET g() { status { 200 => {} 201 => () } } }",
    ] {
        assert!(!parsed(source).diagnostics.is_empty(), "accepted {source}");
    }
}

#[test]
fn c1_19_attributes_docs_and_all_rust_syntax_are_retained() {
    let node = root(
        r#"/// Client docs.
        #[cfg(feature = "x")] pub(crate) client C(/// Key docs.
        key: Secret) {
            problem E read Readers::<A, B>::new();
            /// Middleware docs.
            middleware Limit::new(key.clone());
            /// Part docs.
            part p(#[doc = "input"] a: T) { body make { a; } }
            #[deprecated(note = "old")] #[doc(hidden)] scope s {
                #[cfg(any())] GET g(x?: fn(A, B) -> C, ..p) -> Result<A, B> {
                    read (|a, b| f(a, b))(x, key);
                }
            }
        }"#,
    );
    assert!(matches!(node.kind, NodeKind::Client));
    assert_eq!(node.attrs.len(), 2);
    assert!(matches!(node.visibility, syn::Visibility::Restricted(_)));
    let Parameter::Input(input) = &node.signature.as_ref().unwrap().entries[0] else {
        panic!()
    };
    assert_eq!(input.attrs.len(), 1);
    assert_eq!(node.statements[1].attrs.len(), 1);
    assert_eq!(node.parts[0].attrs.len(), 1);
    assert_eq!(node.children[0].attrs.len(), 2);
    assert_eq!(node.children[0].children[0].attrs.len(), 1);
}

fn diagnostic_spans(tokens: TokenStream, found: &mut Vec<(String, usize, usize)>) {
    let mut start = None;
    for token in tokens {
        match token {
            TokenTree::Ident(ident) if ident == "compile_error" => {
                start = Some(ident.span().start())
            }
            TokenTree::Group(group) => {
                let before = found.len();
                diagnostic_spans(group.stream(), found);
                if let Some(start) = start.take() {
                    for (_, line, column) in &mut found[before..] {
                        *line = start.line;
                        *column = start.column;
                    }
                }
            }
            TokenTree::Literal(literal) => {
                let start = literal.span().start();
                let text = syn::parse2::<syn::LitStr>(TokenTree::Literal(literal).into())
                    .unwrap()
                    .value();
                found.push((text, start.line, start.column));
            }
            _ => {}
        }
    }
}

#[test]
fn c1_18_messages_point_at_the_specified_tokens() {
    for (source, message, line, column) in [
        ("client C {\n    list();\n}", "method before", 2, 4),
        (
            "client C {\n    GET g();\n    read Json;\n}",
            "precede children",
            3,
            4,
        ),
        (
            "client C {\n    POST g() {\n        body f();\n        body f();\n    }\n}",
            "duplicate",
            4,
            8,
        ),
        (
            "client C {\n    scope s {\n        paginate p;\n    }\n}",
            "requires an endpoint",
            3,
            8,
        ),
        (
            "client C {\n    METHOD \"PROP FIND\" find();\n}",
            "invalid method",
            2,
            11,
        ),
        (
            "client C {\n    scope s {\n        problem E;\n    }\n}",
            "requires the client",
            3,
            8,
        ),
    ] {
        let mut found = Vec::new();
        diagnostic_spans(parsed(source).diagnostics.into_tokens(), &mut found);
        assert!(
            found
                .iter()
                .any(|(text, actual_line, actual_column)| text.contains(message)
                    && *actual_line == line
                    && *actual_column == column),
            "{found:?}"
        );
    }
    let mut found = Vec::new();
    diagnostic_spans(
        parsed("client C {\n    POST g() {\n        body f();\n        body g();\n    }\n}")
            .diagnostics
            .into_tokens(),
        &mut found,
    );
    assert!(found.iter().any(
        |(text, line, column)| text == "the first `body` statement is here"
            && *line == 3
            && *column == 8
    ));
}

#[test]
fn c1_2_custom_methods_are_http_tokens_including_punctuation() {
    for method in ["PURGE", "VERSION-CONTROL", "a!#$%&'*+-.^_`|~09"] {
        let node = root(&format!("client C {{ METHOD {:?} purge(); }}", method));
        let NodeKind::Endpoint(method) = &node.children[0].kind else {
            panic!()
        };
        assert!(method.custom.is_some());
    }
    for method in ["", "PROP FIND", "M/GET", "é", "GET\n", "G:ET"] {
        assert!(errors(&format!("client C {{ METHOD {:?} g(); }}", method))
            .contains("invalid method name"));
    }
}

#[test]
fn c1_18_structure_messages_are_raised_together_and_sound_siblings_survive() {
    let result = parsed(
        r#"client C {
        scope s {
            paginate p;
            problem E;
            list();
            GET broken(x:) { body f(); }
            GET good();
            read Json;
            read Text;
            METHOD "PROP FIND" find();
        }
        GET final_one();
    }"#,
    );
    let client = result.client.unwrap();
    assert_eq!(client.node.children.len(), 2);
    let retained: Vec<_> = client.node.children[0]
        .children
        .iter()
        .map(|child| child.name.to_string())
        .collect();
    assert_eq!(retained, ["good", "find"]);
    let text = result.diagnostics.into_tokens().to_string();
    for phrase in [
        "requires an endpoint",
        "requires the client",
        "method before",
        "expected",
        "precede children",
        "duplicate",
        "invalid method name",
        "the first `read` statement is here",
    ] {
        assert!(text.contains(phrase), "missing {phrase}: {text}");
    }
    assert!(!text.contains("later compiler passes"));
}

#[test]
fn c1_3_every_singleton_statement_uses_its_own_name_in_message_17() {
    for (word, statement) in [
        ("base", "base b;"),
        ("path", "path \"x\";"),
        ("query", "query {}"),
        ("headers", "headers {}"),
        ("body", "body make();"),
        ("read", "read R;"),
        ("status", "status {}"),
        ("problem", "problem E;"),
        ("paginate", "paginate p;"),
        ("idempotent", "idempotent;"),
        ("idle_timeout", "idle_timeout clock;"),
    ] {
        let body = format!("{statement} {statement}");
        let source = if matches!(word, "body" | "paginate" | "idempotent") {
            format!("client C {{ POST g() {{ {body} }} }}")
        } else {
            format!("client C {{ {body} }}")
        };
        let text = errors(&source);
        assert!(
            text.contains(&format!("duplicate `{word}` statement")),
            "{text}"
        );
        assert!(text.contains(&format!("the first `{word}` statement is here")));
    }
    root("client C { middleware A; middleware B; }");
    assert!(
        errors("client C { part p() { read A; read B; } }").contains("duplicate `read` statement")
    );
}

#[test]
fn c1_18_structure_messages_name_actual_words_and_node_kinds() {
    for (word, statement) in [
        ("body", "body f();"),
        ("paginate", "paginate p;"),
        ("idempotent", "idempotent;"),
    ] {
        for (kind, source) in [
            ("client", format!("client C {{ {statement} }}")),
            ("scope", format!("client C {{ scope s {{ {statement} }} }}")),
        ] {
            let text = errors(&source);
            assert!(
                text.contains(&format!("`{word}` requires an endpoint")),
                "{text}"
            );
            assert!(text.contains(&format!("a {kind}")), "{text}");
        }
    }
    for (word, statement) in [
        ("problem", "problem E;"),
        ("middleware", "middleware m;"),
        ("part", "part p() {}"),
    ] {
        for node in ["scope s", "GET g()"] {
            let text = errors(&format!("client C {{ {node} {{ {statement} }} }}"));
            assert!(
                text.contains(&format!("`{word}` requires the client")),
                "{text}"
            );
        }
    }
    assert!(
        errors("client C { GET g(); part p() {} }").contains("statements must precede children")
    );
}

#[test]
fn c1_18_entry_and_arm_errors_preserve_other_entries_arms_and_endpoints() {
    let result = parsed(
        r#"client C {
        GET broken() {
            query { "bad" = ; "good" = 42; }
            status { 200 =>, 201 => () }
        }
        GET good();
    }"#,
    );
    let node = result.client.unwrap().node;
    assert_eq!(node.children.len(), 2);
    let StatementKind::Query(entries) = &node.children[0].statements[0].kind else {
        panic!()
    };
    assert_eq!(entries.entries.len(), 1);
    let StatementKind::Status(arms) = &node.children[0].statements[1].kind else {
        panic!()
    };
    assert_eq!(arms.arms.len(), 1);
    assert_eq!(arms.arms[0].key.to_token_stream().to_string(), "201");
    assert_eq!(
        result
            .diagnostics
            .into_tokens()
            .to_string()
            .matches("compile_error")
            .count(),
        2
    );
}

#[test]
fn c1_18_an_endpoint_without_a_base_reports_message_43() {
    let text = crate::expand("client C { GET g(); }".parse().unwrap()).to_string();
    assert_eq!(text.matches("compile_error").count(), 1);
    assert!(text.contains("no `base` in reach of this endpoint"));
    assert!(!text.contains("step 5"));
    let text = crate::expand("client C { list(); }".parse().unwrap()).to_string();
    assert!(!text.contains("later compiler passes"));
}

// Fixtures include complete client roots and standalone whole-node fragments.
// Only tests supply an enclosing client; the production entry accepts one client.
fn fixture(source: &str) -> parse::Parsed {
    let tokens: TokenStream = source.parse().unwrap();
    let trees: Vec<_> = tokens.clone().into_iter().collect();
    if trees
        .first()
        .is_some_and(|token| matches!(token, TokenTree::Ident(id) if id == "concord"))
    {
        let TokenTree::Group(group) = trees.last().unwrap() else {
            panic!()
        };
        return parse::parse(group.stream());
    }
    // Skip metadata and visibility to identify the declaration's kind.
    let word = trees
        .iter()
        .filter_map(|token| {
            if let TokenTree::Ident(id) = token {
                Some(id.to_string())
            } else {
                None
            }
        })
        .find(|word| word != "pub")
        .unwrap_or_default();
    if word == "client" {
        parse::parse(tokens)
    } else {
        parsed(&format!("client Fixture {{\n{source}\n}}"))
    }
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    files.sort();
    files
}

#[test]
fn c1_20_complete_parser_descriptions_parse() {
    let files = rs_files(&package().join("tests/parser/descriptions"));
    let mut count = 0;
    for path in files {
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("schematic_")
        {
            continue;
        }
        let result = fixture(&fs::read_to_string(&path).unwrap());
        assert!(
            result.diagnostics.is_empty(),
            "{}: {}",
            path.display(),
            result.diagnostics.into_tokens()
        );
        count += 1;
    }
    assert_eq!(count, 41);
}

#[path = "../support/examples/context.rs"]
mod example_context;

#[test]
fn c1_20_regression_descriptions_parse_and_validate() {
    let files = rs_files(&package().join("tests/parser/regressions"));
    assert_eq!(files.len(), 57);
    for path in files {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let source = fs::read_to_string(&path).unwrap();
        let contextual = example_context::source(stem, &source);
        let mut result = fixture(&contextual);
        if let Some(client) = &result.client {
            crate::name::check(client, &mut result.diagnostics);
            let flat = crate::resolve::resolve(client, &mut result.diagnostics);
            crate::check::check(client, &flat, &mut result.diagnostics);
        }
        assert!(
            result.diagnostics.is_empty(),
            "{}: {}",
            path.display(),
            result.diagnostics.into_tokens()
        );
    }
}

#[test]
fn c1_20_generic_scopes_are_rejected() {
    let source =
        fs::read_to_string(package().join("tests/parser/unsupported/generic_scope.rs")).unwrap();
    assert!(!fixture(&source).diagnostics.is_empty());
}

#[test]
fn c1_20_schematic_fragments_are_explicitly_not_descriptions() {
    let files = rs_files(&package().join("tests/parser/descriptions"));
    let mut count = 0;
    for path in files {
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("schematic_")
        {
            continue;
        }
        let source = fs::read_to_string(path).unwrap();
        let fails = match source.parse::<TokenStream>() {
            Err(_) => true,
            Ok(_) => !fixture(&source).diagnostics.is_empty(),
        };
        assert!(fails, "schematic fragment accepted: {source}");
        count += 1;
    }
    assert_eq!(count, 8);
}

#[test]
fn c1_20_all_180_malformed_inputs_return_a_tree_or_errors_without_panicking() {
    let files = rs_files(&package().join("tests/parser/malformed"));
    assert_eq!(files.len(), 180);
    for path in files {
        let source = fs::read_to_string(&path).unwrap();
        let result = std::panic::catch_unwind(|| {
            if source.parse::<TokenStream>().is_ok() {
                let result = fixture(&source);
                assert!(result.client.is_some() || !result.diagnostics.is_empty());
                let _ = result.diagnostics.into_tokens();
            }
        });
        assert!(result.is_ok(), "panic on {}", path.display());
    }
}

mod parser_recovery;
