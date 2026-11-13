//! Regression probes from Fable's complete reading of M4.
use super::*;

fn diagnostics(source: &str) -> crate::diag::Diagnostics {
    let mut p = parse_source(source);
    if let Some(c) = &p.client {
        name::check(c, &mut p.diagnostics);
        let flat = resolve::resolve(c, &mut p.diagnostics);
        check::check(c, &flat, &mut p.diagnostics);
    }
    p.diagnostics
}
fn help(d: &crate::diag::Diagnostics, number: u8) -> &str {
    d.numbers()
        .iter()
        .zip(d.messages())
        .find(|(n, _)| **n == number)
        .unwrap()
        .1
        .split_once("\nhelp: ")
        .unwrap()
        .1
}
fn replace(source: &str, bad: &str, good: &str) -> String {
    let norm = |s: &str| s.parse::<proc_macro2::TokenStream>().unwrap().to_string();
    let s = norm(source);
    let bad = norm(bad);
    assert!(s.contains(&bad));
    s.replacen(&bad, &norm(good), 1)
}

#[test]
fn c1_18_fable_2_remove_idempotent_help_preserves_nonrepeatability() {
    let source = "client C { base \"https://a.test\"; POST create() { remove idempotent; } }";
    let d = diagnostics(source);
    assert_eq!(help(&d, 24), "write `idempotent false;`");
    let fixed = replace(source, "remove idempotent;", "idempotent false;");
    let flat = accepted(&fixed);
    assert!(flat.contains("idempotent false"));
}
#[test]
fn c1_18_a70_fable_4_helps_check_their_context() {
    for (source,number,forbidden) in [
        ("client C { base \"https://a.test\"; GET get() { path \"items?tag=a&tag=b\"; } }",28,"write `path"),
        ("client C { base \"https://a.test\"; GET get() -> u64 { status {2xx { bodi } => bodi} } }",34,"write `2xx"),
        ("client C { base \"https://a.test\"; GET get(a:String) { query {\"k\"=a; \"k\"=\"2\";} } }",38,"keep one `\"k\" = \"2\";`"),
    ] {
        let d=diagnostics(source);assert!(!help(&d,number).contains(forbidden),"{}",help(&d,number));
    }
    let source="client C { base \"https://a.test\"; headers { \"user-agent\"=\"c\"; } GET get(user_agent:String) {headers {user_agent;} } }";
    let d = diagnostics(source);
    let h = help(&d, 41);
    let replacement = h
        .strip_prefix("write `")
        .unwrap()
        .strip_suffix('`')
        .unwrap();
    accepted(&replace(source, "user_agent;", replacement));
    for (bad, number, old) in [
        ("GET list(Send?:bool) {query{\"s\"=Send;}}", 5, "Send"),
        (
            "GET list(Client:String) {query{\"c\"=Client;}}",
            5,
            "Client",
        ),
        ("scope things {GET Clone();}", 5, "Clone"),
        (
            "scope things(get?:u32) {query{get;} GET get(); GET fetch_get();}",
            2,
            "get",
        ),
        ("GET one2(); GET one_2(); GET one_2_other();", 8, "one_2"),
    ] {
        let source = format!("client C {{base \"https://a.test\"; {bad}}}");
        let d = diagnostics(&source);
        let h = help(&d, number);
        if let Some((_, tail)) = h.split_once('`') {
            let suggestion = tail.split('`').next().unwrap();
            if h.starts_with("rename") {
                let replacement = if number == 8 {
                    h.split('`').nth(3).unwrap()
                } else {
                    suggestion
                };
                let fixed = if number == 2 {
                    source.replacen("GET get();", &format!("GET {replacement}();"), 1)
                } else {
                    source.replace(old, replacement)
                };
                accepted(&fixed);
            }
        }
    }
}
#[test]
fn c1_16_fable_11_self_is_refused_in_every_authored_expression() {
    for line in [
        "query {\"e\"=Self::__ENDPOINT;}",
        "headers {\"e\"=stringify!(Self);}",
        "body Self::make();",
        "read Self::reader();",
        "idempotent Self::safe();",
        "idle_timeout Self::clock();",
        "paginate Self::pager();",
        "status { 404 => Self::value() }",
    ] {
        let source = format!("client C {{base \"https://a.test\"; POST get() {{{line}}} }}");
        let d = diagnostics(&source);
        assert_eq!(
            d.numbers().iter().filter(|n| **n == 10).count(),
            1,
            "{line}: {:?}",
            d.messages()
        );
    }
}
#[test]
fn c1_20_fable_15_unit_problem_is_a_complete_statement() {
    accepted("client C {base \"https://a.test\"; read Json; problem (); GET get();}");
}
#[test]
fn c1_18_fable_16_a_bad_reference_does_not_prove_an_unused_input_or_bad_header() {
    let d =
        diagnostics("client C {base \"https://a.test\"; GET get(id:String) {path \"{idd}\";} }");
    assert_eq!(d.numbers(), [11]);
    let d = diagnostics(
        "client C(key:String) {base \"https://a.test\"; GET get() {path \"{client.ky}\";} }",
    );
    assert_eq!(d.numbers(), [11]);
    let d=diagnostics("client C(user_agent:String) {base \"https://a.test\"; GET get() {headers {client.user_agent;}} }");
    assert_eq!(d.numbers(), [12]);
}
#[test]
fn c1_20_a71_fable_12_flat_chains_are_not_nesting() {
    for expr in [
        format!("s{}", ".replace(\"a\",\"b\")".repeat(100)),
        vec!["true"; 70].join(" || "),
        vec!["1u32"; 257].join(" + "),
    ] {
        let s=format!("client C {{base \"https://a.test\"; GET get(s:String) {{query {{\"k\"={expr}; \"s\"=s;}} }} }}");
        accepted(&s);
    }
    let expr = vec!["1u32"; 258].join(" + ");
    let d = diagnostics(&format!(
        "client C {{base \"https://a.test\"; GET get() {{query {{\"k\"={expr};}} }} }}"
    ));
    assert!(d.messages().is_empty());
    let text = d.into_tokens().to_string();
    assert!(text.contains("too long"), "{text}");
    assert!(!text.contains("nesting"));
}

#[test]
fn c1_18_a70_fable_4_other_text_repairs_preserve_lexical_reach_and_all_part_uses() {
    for (source, number, unsafe_text) in [
        (
            r#"client C {base "https://a.test"; scope s(key:u32) {middleware Layer::new(key); GET get();}}"#,
            20,
            "move `",
        ),
        (
            r#"client C(key:u32) {base "https://a.test"; read Json; query {key;}
            status {200 {body} => body + *key} GET get() -> Option<u32>; }"#,
            46,
            "add `",
        ),
        (
            r#"client C(key:u32) {base "https://a.test"; read Json; query {key;}
            status {200 {body} => body + *key} GET get() -> u32 {status {override 2xx => 1}} }"#,
            44,
            "also write `",
        ),
        (
            r#"client C {base "https://a.test"; GET get() {override headers {"x"="1"; "x"="2";}}}"#,
            24,
            "write `",
        ),
        (
            r#"client C {base "https://a.test"; read Json; GET get()->u32 {status {200 {body,bodi} => body + bodi}}}"#,
            34,
            "write `",
        ),
        (
            r#"client C {base "https://a.test"; part agent(user_agent:String) {headers {user_agent;}}
            scope one { headers {"user-agent"="a";} GET get(..agent); } GET other(..agent); }"#,
            41,
            "write `",
        ),
        (
            r#"client C {base "https://a.test"; GET get(send?:bool,send_value?:bool) {query {send;send_value;}}}"#,
            3,
            "rename this input to `",
        ),
    ] {
        let d = diagnostics(source);
        assert!(
            !help(&d, number).contains(unsafe_text),
            "{source}: {}",
            help(&d, number)
        );
    }
    let s = r#"client C {base "https://a.test"; GET get() {path "items?";} }"#;
    let d = diagnostics(s);
    assert_eq!(help(&d, 28), r#"write `path "items";`"#);
    accepted(&replace(s, r#"path "items?";"#, r#"path "items";"#));
}
