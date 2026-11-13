//! A39/A40: regressions accepted from both independent P3.2 readings.
use super::*;
use crate::diag::Written;

fn diagnostics(source: &str) -> crate::diag::Diagnostics {
    let mut parsed = parse_source(source);
    if let Some(client) = &parsed.client {
        name::check(client, &mut parsed.diagnostics);
        let flat = resolve::resolve(client, &mut parsed.diagnostics);
        check::check(client, &flat, &mut parsed.diagnostics);
    }
    parsed.diagnostics
}

#[test]
fn c1_18_a40_damage_never_proves_an_absence() {
    for damage in [
        "read First; read Second;",
        "query { \"k\" = ; }",
        "headers { \"k\" = ; }",
        "status { 600 => () }",
    ] {
        let source = format!("client Audit {{ {damage} GET child() {{ query {{ override \"k\" = 1; remove \"q\"; }} headers {{ override \"k\" = 1; remove \"q\"; }} status {{ override 200 => (), remove 404 }} }} }}");
        let d = diagnostics(&source);
        for n in [22, 23, 43, 33] {
            assert!(!d.numbers().contains(&n), "{source}\n{:?}", d.messages());
        }
    }
    let d = diagnostics(
        "client Audit { base \"https://audit.test\"; GET f(x:u8,x:u8) { query { x; } } }",
    );
    assert_eq!(d.numbers(), [1]);
    let d = diagnostics(
        "client Audit { base \"https://audit.test\"; part p(x:u8) {} GET f(..p) { body ; } }",
    );
    assert!(!d.numbers().contains(&9));
}

#[test]
fn c1_18_astra_unused_parts_depend_on_signatures_not_unrelated_bodies() {
    let d = diagnostics("client Audit { part unused() {} GET f() { query { \"k\" = ; } } }");
    assert!(d.numbers().contains(&36), "{:?}", d.messages());
    let d = diagnostics("client Audit { part perhaps_used() {} GET f(..) {} }");
    assert!(!d.numbers().contains(&36));
}

#[test]
fn c1_18_astra_unit_problem_is_a_complete_statement() {
    let source = "client Audit { base \"https://audit.test\"; problem () read Reader; GET f(); }";
    assert!(accepted(source).contains("problem ()"));
}

#[test]
fn c5_9_astra_arm_edits_use_the_original_inherited_state_in_either_order() {
    for arms in [
        "override 2xx => (), override 200 => ()",
        "override 200 => (), override 2xx => ()",
    ] {
        let source = format!("client Audit {{ base \"https://audit.test\"; status {{ 2xx => () }} GET f() {{ status {{ {arms} }} }} }}");
        let flat = accepted(&source);
        assert_eq!(
            flat.lines().filter(|line| line.starts_with("arm")).count(),
            2
        );
    }
    let d = diagnostics("client Audit { base \"https://audit.test\"; status { 2xx => () } GET f() { status { override 2xx => (), 200 => () } } }");
    assert_eq!(d.numbers(), [21]);
}

#[test]
fn c5_9_mutation_r12_head_drops_only_inherited_body_arms() {
    let source = "client Audit { base \"https://audit.test\"; read Reader; status { 200 { body } => body, 404 => false } HEAD f(); }";
    let flat = accepted(source);
    assert!(!flat.lines().any(|line| line.starts_with("arm 200")));
    assert!(flat.lines().any(|line| line.starts_with("arm 404")));
}

#[test]
fn c1_18_mutation_r18_unknown_part_preserves_uncertainty() {
    let d = diagnostics("client Audit { GET f(..missing) -> Record; }");
    assert_eq!(d.numbers(), [35]);
}

#[test]
fn c1_8_mutation_c12_nested_borrows_are_all_diagnosed() {
    let d = diagnostics(
        "client Audit { base \"https://audit.test\"; GET f(x: &&str) { query { x; } } }",
    );
    assert_eq!(d.numbers(), [10, 10]);
    for message in d.messages() {
        assert!(message.contains("`x: String`"), "{message}");
    }
}

#[test]
fn c1_4_mutation_n08_raw_non_keyword_still_gets_message_four() {
    let d = diagnostics("client Audit { base \"https://audit.test\"; GET r#ordinary(); }");
    assert_eq!(d.numbers(), [4]);
}

#[test]
fn c1_4_mutation_n09_trailing_underscore_is_refused() {
    let d = diagnostics("client Audit { base \"https://audit.test\"; GET trailing_(); }");
    assert_eq!(d.numbers(), [5]);
}

#[test]
fn c1_18_a39_prints_rust_punctuation_without_joined_spans() {
    for source in [
        "url: Base",
        "Duration::from_secs(60)",
        "..ladder",
        "types?: Vec<String>",
        "path \"contents/{path}.md\";",
    ] {
        let tokens = source.parse().unwrap();
        assert_eq!(Written::new(tokens).text(), source);
    }
}

#[test]
fn c1_18_a39_no_arrow_reports_once_and_head_reports_only_body_error() {
    let d = diagnostics("client Audit { base \"https://audit.test\"; read Reader; status { 200 { body } => body, 201 { body } => body } GET f(); }");
    assert_eq!(d.numbers(), [39]);
    let d = diagnostics("client Audit { base \"https://audit.test\"; HEAD f() { status { 200 { body } => body } } }");
    assert_eq!(d.numbers(), [31]);
}

#[test]
fn c1_18_a39_spelling_suggestions_are_unique_and_close() {
    let d = diagnostics("client Audit { base \"https://audit.test\"; part alphabet() {} part lol_filter() {} GET f(..lol_filters); }");
    assert!(d.messages().iter().any(|m| m.contains("`..lol_filter`")));
    let d = diagnostics(
        "client Audit { base \"https://audit.test\"; part alphabet() {} GET f(..unrelated); }",
    );
    let message = d
        .messages()
        .iter()
        .find(|m| m.starts_with("no part"))
        .unwrap();
    assert!(!message.contains("`..alphabet`"));
}

#[test]
fn c2_8_indexed_entries_preserve_both_orders_at_scale() {
    let mut source = "client Audit { base \"https://audit.test\"; query {".to_owned();
    for i in 0..16_000 {
        source.push_str(&format!("\"k{i}\" = {i};"));
    }
    source.push_str("} scope s { query { remove \"k2\"; override \"k0\" = 77; } GET f() { query { \"k2\" = 99; } } } }");
    let flat = accepted(&source);
    let entries: Vec<_> = flat
        .lines()
        .filter(|line| line.starts_with("query"))
        .collect();
    assert_eq!(entries.len(), 16_000);
    assert!(
        entries[entries.len() - 2].contains("wire=0") && entries[entries.len() - 2].contains("77")
    );
    assert!(entries.last().unwrap().contains("wire=2") && entries.last().unwrap().contains("99"));
}

#[path = "real_cases.rs"]
mod real_cases;

fn normalized(source: &str) -> String {
    source
        .parse::<proc_macro2::TokenStream>()
        .unwrap()
        .to_string()
}
fn replace(source: &str, bad: &str, good: &str) -> String {
    let source = normalized(source);
    let bad = normalized(bad);
    assert!(source.contains(&bad), "missing repair target {bad}");
    source.replacen(&bad, &normalized(good), 1)
}
fn code_after(message: &str, prefix: &str) -> String {
    message
        .split_once(prefix)
        .unwrap()
        .1
        .split('`')
        .next()
        .unwrap()
        .into()
}
fn real_source(file: &str) -> String {
    fs::read_to_string(path().join("tests/real").join(file)).unwrap()
}

#[test]
fn c1_18_a39_all_97_real_mistakes_and_the_intended_description_after_repair() {
    let mut covered = std::collections::BTreeSet::new();
    for &(fixture, base, number, edits) in real_cases::CASES {
        let base = fs::read_to_string(path().join("tests/flat").join(base)).unwrap();
        // The corrected descriptions are read afresh from the API-page copies;
        // the provenance test separately compares those copies with docs/apis.
        accepted(&base);
        let mut broken = base.clone();
        for &(old, new, occurrence) in edits {
            let offset = broken
                .match_indices(old)
                .nth(occurrence)
                .unwrap_or_else(|| panic!("{fixture}: missing mutation {old:?}"))
                .0;
            broken.replace_range(offset..offset + old.len(), new);
        }
        let checked_in = real_source(fixture);
        let body = checked_in.rsplit_once("fn main()").unwrap().0;
        assert_eq!(
            normalized(&broken),
            normalized(body),
            "{fixture}: compiler fixture provenance"
        );
        let d = diagnostics(&broken);
        // `type` as a Rust expression is already refused by the parser. It has
        // no step-2 help to apply; the raw spelling tests the name diagnostic.
        if fixture != "m04_plain.rs" {
            assert!(
                d.numbers().contains(&number),
                "{fixture}: {:?}",
                d.messages()
            );
            covered.insert(number);
        }
        let _ = repairs(fixture, &broken, &base, &d);
    }
    assert_eq!(covered, (1..=46).collect());
}

/// Apply the code actually printed by the macro; worded repairs restore the
/// page author's intended spelling/placement rather than inventing new data.
fn repairs(fixture: &str, broken: &str, intended: &str, d: &crate::diag::Diagnostics) -> String {
    let number = fixture[1..3].parse::<u8>().unwrap();
    let messages: Vec<_> = d
        .numbers()
        .iter()
        .zip(d.messages())
        .filter(|(n, _)| **n == number)
        .map(|(_, m)| m.as_str())
        .collect();
    let first = messages.first().copied().unwrap_or("");
    let written = || code_after(first, "write `");
    let corrected = match fixture {
        "m02.rs" => rename_mutations(fixture, broken, "account", &code_after(first, "to `")),
        "m03_input.rs" => rename_mutations(fixture, broken, "request", &code_after(first, "to `")),
        "m03_part.rs" => rename_mutations(fixture, broken, "pages", &code_after(first, "to `")),
        "m03_with.rs" => {
            rename_mutations(fixture, broken, "with_forks", &code_after(first, "to `"))
        }
        "m04_raw.rs" => rename_mutations(fixture, broken, "r#type", &code_after(first, "to `")),
        "m05_input.rs" => {
            rename_mutations(fixture, broken, "paymentIntent", &code_after(first, "to `"))
        }
        "m05_endpoint.rs" => {
            rename_mutations(fixture, broken, "byRiotId", &code_after(first, "to `"))
        }
        "m05_double.rs" => rename_mutations(
            fixture,
            broken,
            "mark__uncollectible",
            &code_after(first, "to `"),
        ),
        "m06.rs" => rename_mutations(
            fixture,
            broken,
            "riot",
            code_after(first, "write `client ").as_str(),
        ),
        "m06_snake.rs" => rename_mutations(
            fixture,
            broken,
            "GitHub_Api",
            code_after(first, "write `client ").as_str(),
        ),
        "m07_input.rs" => {
            rename_mutations(fixture, broken, "tournament", &code_after(first, "to `"))
        }
        "m08.rs" => rename_mutations(fixture, broken, "top_3", &code_after(first, "to `")),
        "m16_child.rs" | "m16_part.rs" => move_to_client(broken, &code_after(first, "move `")),
        "m27_hash.rs" => path_hole_repair(
            broken,
            "\"accounts/by-riot-id/{game_name}#{tag_line}\"",
            "\"accounts/by-riot-id/{game_name}{separator}{tag_line}\"",
        ),
        "m27_space.rs" => path_hole_repair(
            broken,
            "\"lol/status/v4/platform data\"",
            "\"lol/status/v4/platform{separator}data\"",
        ),
        "m27_percent.rs" => path_hole_repair(
            broken,
            "\"stats%2Fcontributors\"",
            "\"stats{separator}contributors\"",
        ),
        "m24_block.rs" => replace(
            broken,
            "override headers { \"accept\" = \"application/vnd.github.diff\"; }",
            &written(),
        ),
        "m24_base.rs" => replace(
            broken,
            "override base \"https://uploads.github.com\";",
            &written(),
        ),
        "m24_read.rs" => replace(broken, "override read Binary;", &written()),
        "m24_path.rs" => replace(broken, "override path \"assets\";", &written()),
        "m28.rs" if first.contains("write `") => replace(
            broken,
            "path \"by-puuid/{puuid}/ids?start=0&count=100\";",
            &written(),
        ),
        "m28_template.rs" => replace(
            broken,
            "path \"champion-masteries/by-puuid/{puuid}/top?count={count}\";",
            &written(),
        ),
        "m29.rs" => replace(broken, "path \"contents/{*path}.md\";", &written()),
        "m29_query.rs" => replace(broken, "\"{*prefix}\"", &written()),
        "m10_vec.rs" => replace(broken, "types?: Vec<&str>", &written()),
        "m10_str.rs" => replace(broken, "username: &str", &written()),
        "m13_path.rs" => replace(broken, "tag_line?: String", &code_after(first, "declare `")),
        "m13_host.rs" => replace(broken, "route?: Platform", &code_after(first, "declare `")),
        "m19.rs" => replace(broken, "METHOD \"LIST VERSIONS\" versions() -> VersionsPage { query { \"versions\" = \"\"; } }", &written()),
        "m20_middleware.rs" => move_to_client(broken, &code_after(first, "move `")),
        "m23_arm.rs" | "m23_header.rs" | "m23_query.rs" => replace(broken, &code_after(first, "delete `"), ""),
        "m38_body.rs" => intended.to_owned(),
        "m18_spread.rs" => {
            let spread = code_after(first, "move `");
            let fixed = replace(broken, "scope league(route: Platform, ..ladder)", "scope league(route: Platform)");
            replace(&fixed, "GET entries(queue: Queue, tier: Tier, division: Division)", &format!("GET entries(queue: Queue, tier: Tier, division: Division, {spread})"))
        }
        "m13_base.rs" => replace(broken, "url?: Base", &code_after(first, "declare `")),
        "m34.rs" => replace(
            broken,
            "2xx { header } => header.require(\"etag\")?.to_owned()",
            &written(),
        ),
        "m34_bodi.rs" => replace(
            broken,
            "404 { bodi } => document_404::<GetReply>(bodi)?",
            &written(),
        ),
        "m22_arm.rs" => replace(
            broken,
            "override 404 { body } => document_404::<GetReply>(body)?",
            &written(),
        ),
        "m22_body.rs" => replace(broken, "override \"message_id\" = message_id;", &written()),
        "m22_header.rs" => replace(
            broken,
            "override \"accept-encoding\" = \"identity\";",
            &written(),
        ),
        "m22_removed.rs" => replace(
            broken,
            "override \"x-riot-token\" = client.key;",
            &written(),
        ),
        "m21_header.rs" => replace(
            broken,
            "\"accept\" = \"application/vnd.github.diff\";",
            &written(),
        ),
        "m21_arm.rs" => replace(
            broken,
            "2xx { body } => Identity::from_reply(body)?",
            &written(),
        ),
        "m21_class.rs" => replace(broken, "200 { body } => body", &written()),
        "m21_query.rs" => {
            let source = normalized(broken);
            let target = normalized("\"encoding-type\" = \"url\";");
            let offset = source.find(&target).unwrap() + target.len();
            source[..offset].to_owned()
                + &source[offset..].replace(&target, &normalized(&written()))
        }
        "m12_short.rs" => replace(broken, "upload.upload_id;", &written()),
        "m12_hole.rs" => broken.replace(
            "\"Bearer {token}\"",
            &format!("\"Bearer {}\"", code_after(first, "write `")),
        ),
        "m12_base.rs" => replace(broken, "base bulk_url;", &written()),
        "m40.rs" => replace(broken, "region?: String", &code_after(first, "declare `")),
        "m41.rs" => replace(broken, "if_match;", &written()),
        "m35.rs" => replace(
            broken,
            "..lol_filters",
            &code_after(first, "did you mean `"),
        ),
        "m44.rs" => append_arm(broken, "remove 200", false),
        "m46_option.rs" => append_arm(broken, &code_after(first, "block, add `"), false),
        "m46_result.rs" => append_arm(broken, &code_after(first, "add `"), true),
        "m23_class.rs" => replace(
            broken,
            "remove 200",
            "override 200 { body } => Reply::into_result(body)?",
        ),
        _ => intended.to_owned(),
    };
    if fixture == "m15.rs" {
        assert!(normalized(&corrected).contains(&normalized(&written())));
    }
    if fixture == "m44.rs" {
        assert!(first.contains("remove 200"));
    }
    let (_, errors) = run(&corrected);
    if !errors.is_empty() {
        fs::write(path().join("../target/p3-2-literal-failed.rs"), &corrected).unwrap();
    }
    assert!(
        errors.is_empty(),
        "{fixture}: literal repair\n{first}\n{errors}"
    );
    corrected
}

fn offset(source: &str, at: proc_macro2::LineColumn) -> usize {
    let start = source
        .split_inclusive('\n')
        .take(at.line - 1)
        .map(str::len)
        .sum::<usize>();
    start
        + source[start..]
            .char_indices()
            .nth(at.column)
            .map_or(source.len() - start, |(byte, _)| byte)
}
fn append_arm(source: &str, arm: &str, result: bool) -> String {
    let parsed = parse_source(source);
    let mut pending = vec![&parsed.client.as_ref().unwrap().node];
    while let Some(node) = pending.pop() {
        if matches!(node.kind, NodeKind::Endpoint(_)) {
            let target = node.output.as_ref().is_some_and(|o| {
                flat::arrow(&o.ty).0
                    == Some(if result {
                        flat::Wrapper::Result
                    } else {
                        flat::Wrapper::Option
                    })
            });
            let statement = node.statements.iter().find_map(|s| {
                if let StatementKind::Status(arms) = &s.kind {
                    Some(arms)
                } else {
                    None
                }
            });
            if result && target {
                let at = offset(source, node.braces.as_ref().unwrap().span.open().end());
                let mut fixed = source.to_owned();
                fixed.insert_str(at, arm);
                return fixed;
            }
            if let Some(arms) = statement {
                let relevant = target
                    || arms
                        .arms
                        .iter()
                        .any(|a| a.edit.is_some() && a.key.to_string() == "2xx");
                if !result && relevant {
                    let at = offset(source, arms.braces.span.close().start());
                    let mut fixed = source.to_owned();
                    fixed.insert_str(at, &format!(", {arm}"));
                    return fixed;
                }
            }
        }
        pending.extend(&node.children);
    }
    panic!("no endpoint for literal arm repair")
}

fn rename_mutations(fixture: &str, broken: &str, old: &str, new: &str) -> String {
    use proc_macro2::{Group, TokenStream, TokenTree};
    fn rename(tokens: TokenStream, old: &str, new: &str, entries: bool) -> TokenStream {
        let tokens: Vec<_> = tokens.into_iter().collect();
        let mut output = TokenStream::new();
        for (i, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Ident(ident) if ident == old => {
                    if entries
                        && matches!(tokens.get(i+1), Some(TokenTree::Punct(p)) if p.as_char() == ';')
                    {
                        let key = syn::LitStr::new(old.trim_start_matches("r#"), ident.span());
                        output.extend(quote::quote!(#key =));
                    }
                    output.extend([TokenTree::Ident(syn::Ident::new(new, ident.span()))]);
                }
                TokenTree::Literal(lit) => {
                    if let Ok(literal) =
                        syn::parse2::<syn::LitStr>(TokenStream::from(token.clone()))
                    {
                        let value = literal
                            .value()
                            .replace(&format!("{{{old}}}"), &format!("{{{new}}}"));
                        let literal = syn::LitStr::new(&value, lit.span());
                        output.extend(quote::quote!(#literal));
                    } else {
                        output.extend([token.clone()]);
                    }
                }
                TokenTree::Group(group) => {
                    let entries = i.checked_sub(1).and_then(|i| tokens.get(i)).is_some_and(
                        |t| matches!(t, TokenTree::Ident(i) if i == "query" || i == "headers"),
                    );
                    output.extend([TokenTree::Group(Group::new(
                        group.delimiter(),
                        rename(group.stream(), old, new, entries),
                    ))]);
                }
                _ => output.extend([token.clone()]),
            }
        }
        output
    }
    let parsed = parse_source(broken);
    let mut pending = vec![&parsed.client.as_ref().unwrap().node];
    while let Some(node) = pending.pop() {
        let input_case = matches!(&fixture[..3], "m03" | "m04")
            || fixture == "m05_input.rs"
            || fixture == "m07_input.rs";
        let target = if input_case {
            node.signature.as_ref().is_some_and(|s| s.entries.iter().any(|p| matches!(p, Parameter::Input(input) if input.name == old && (!fixture.starts_with("m03") || input.optional.is_some()))))
        } else {
            node.name == old
        };
        if target {
            let start = offset(broken, node.span.start());
            let end = if let Some(braces) = &node.braces {
                offset(broken, braces.span.close().end())
            } else {
                start + broken[start..].find(';').unwrap() + 1
            };
            let renamed = rename(broken[start..end].parse().unwrap(), old, new, false).to_string();
            let mut corrected = broken.to_owned();
            corrected.replace_range(start..end, &renamed);
            return corrected;
        }
        pending.extend(&node.children);
        pending.extend(&node.parts);
    }
    panic!("{fixture}: input to rename missing")
}
fn move_to_client(broken: &str, statement: &str) -> String {
    let source = normalized(broken).replacen(&normalized(statement), "", 1);
    let parsed = parse_source(&source);
    let at = offset(
        &source,
        parsed
            .client
            .unwrap()
            .node
            .braces
            .unwrap()
            .span
            .open()
            .end(),
    );
    let mut corrected = source;
    corrected.insert_str(at, statement);
    corrected
}

fn path_hole_repair(source: &str, bad: &str, replacement: &str) -> String {
    let mut fixed = replace(source, bad, replacement);
    let parsed = parse_source(&fixed);
    let mut pending = vec![&parsed.client.as_ref().unwrap().node];
    while let Some(node) = pending.pop() {
        if node.statements.iter().any(|s| matches!(&s.kind, StatementKind::Path(t) if t.literal.value().contains("{separator}"))) {
            if let Some(signature) = &node.signature {
                let at = offset(&fixed, signature.parens.span.close().start());
                let prefix = if signature.entries.is_empty() { "" } else { ", " };
                fixed.insert_str(at, &format!("{prefix}separator: String"));
            } else {
                let at = offset(&fixed, node.name.span().end());
                fixed.insert_str(at, "(separator: String)");
            }
            return fixed;
        }
        pending.extend(&node.children);
    }
    panic!("missing path for encoded input repair")
}

#[test]
fn c1_18_a40_an_invalid_arm_cannot_prove_its_input_unused() {
    let d = diagnostics("client Audit { base \"https://audit.test\"; GET f(code: u8) { status { 600 { bodi } => code } } }");
    assert_eq!(d.numbers(), [45]);
}

#[test]
fn c1_18_a39_wrappers_retain_the_real_expression_and_placement() {
    let source = real_source("m46_option.rs");
    let source = source.rsplit_once("fn main()").unwrap().0;
    let d = diagnostics(source);
    let help = d
        .messages()
        .iter()
        .find(|m| m.starts_with("an arm from above"))
        .unwrap();
    assert!(help.contains("in this `status` block"));
    assert!(help.contains("Some(telegram_result(body)?)"));
    assert!(!help.contains("unpack"));
    let source = real_source("m32.rs");
    let source = source.rsplit_once("fn main()").unwrap().0;
    let d = diagnostics(source);
    assert!(d
        .messages()
        .iter()
        .any(|m| m.contains("in this `status` block")));
}

#[test]
fn c1_18_a39_a_part_suggestion_must_not_duplicate_an_existing_spread() {
    let d = diagnostics(
        "client Audit { base \"https://audit.test\"; part page() {} GET f(..page, ..pages); }",
    );
    let help = d
        .messages()
        .iter()
        .find(|m| m.starts_with("no part"))
        .unwrap();
    assert!(!help.contains("`..page`"));
}

#[test]
fn c1_18_a39_query_repair_keeps_every_pair_and_value() {
    let source = "client Audit { base \"https://audit.test\"; GET f() { path \"things?start=0&count=100\"; } }";
    let d = diagnostics(source);
    assert_eq!(d.numbers(), [28]);
    let code = code_after(&d.messages()[0], "write `");
    let corrected = replace(source, "path \"things?start=0&count=100\";", &code);
    let flat = accepted(&corrected);
    let query: Vec<_> = flat
        .lines()
        .filter(|line| line.starts_with("query"))
        .collect();
    assert_eq!(query.len(), 2);
    assert!(query[0].contains("\"start\" = \"0\""));
    assert!(query[1].contains("\"count\" = \"100\""));
}

#[test]
fn c1_18_a39_a_part_cannot_capture_a_scope_input_by_declaring_it_again() {
    let source = real_source("m11_part.rs");
    let d = diagnostics(source.rsplit_once("fn main()").unwrap().0);
    let message = d
        .messages()
        .iter()
        .find(|m| m.starts_with("unknown input"))
        .unwrap();
    assert!(message.contains("a part sees only its own inputs and the client's"));
    assert!(!message.contains("declare the intended input in this signature"));
}
