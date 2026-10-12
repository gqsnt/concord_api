//! Migration fixtures and differential checks.
//!
//! The Catalog/Riot checks compare the converted v3 text against the captured
//! v2.1 baseline recordings in `fixtures/migration/` (endpoint IDs, complete
//! rate plans with windows/costs/key labels, route segments and query key
//! order observed through the real generated v2.1 client). The v3 side is read
//! by an independent token-level extractor below, not by the converter.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use proc_macro2::{Delimiter, TokenStream, TokenTree};

use super::callers::DeclIndex;
use super::report::{Code, PolicyIdentity, Severity};
use super::{Settings, SourceOutcome, migrate, migrate_source, run_status};

fn convert(source: &str) -> SourceOutcome {
    migrate_source(
        "fixture.rs",
        source,
        &DeclIndex::default(),
        &Settings::default(),
    )
}

fn codes(outcome: &SourceOutcome) -> Vec<&'static str> {
    outcome
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn count(outcome: &SourceOutcome, code: Code) -> usize {
    outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .count()
}

fn errors(outcome: &SourceOutcome) -> Vec<String> {
    outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(super::report::Diagnostic::render)
        .collect()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn fixture(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/migrate_v3")
            .join(name),
    )
    .expect("migration fixture")
}

/// A self-deleting directory outside any Cargo workspace.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "concord-migrate-v3-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temporary directory");
        Self(path)
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("parent directory");
        fs::write(&path, contents).expect("write fixture");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// ----- independent v3 extractor ---------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
struct Policy {
    id: String,
    cost: u32,
    windows: Vec<(u64, u128)>,
    labels: Vec<String>,
}

#[derive(Clone, Debug)]
enum Attachment {
    Inline(Policy),
    Named { policy: String, cost: Option<u32> },
}

#[derive(Clone, Debug)]
enum PathOp {
    Append(String),
    Replace(String),
    Off,
}

#[derive(Clone, Debug, Default)]
struct Owner {
    kind: &'static str,
    name: String,
    method: String,
    path: Option<PathOp>,
    rates: Vec<Attachment>,
    id: Option<String>,
    direct_query: Vec<String>,
    block_query: Vec<(String, String)>,
    block_query_replace: bool,
    children: Vec<Owner>,
}

#[derive(Debug, Default)]
struct Client {
    name: String,
    base_path: Vec<String>,
    policies: HashMap<String, Policy>,
    root: Owner,
}

#[derive(Debug)]
struct EffectiveEndpoint {
    id: String,
    method: String,
    segments: Vec<String>,
    query: Vec<String>,
    policies: Vec<Policy>,
}

fn decode_string(literal: &proc_macro2::Literal) -> String {
    syn::parse_str::<syn::LitStr>(&literal.to_string())
        .expect("string literal")
        .value()
}

fn decode_int(tree: &TokenTree) -> u64 {
    let TokenTree::Literal(literal) = tree else {
        panic!("expected integer literal, found {tree}");
    };
    literal
        .to_string()
        .replace('_', "")
        .parse()
        .expect("integer literal")
}

fn decode_duration(tree: &TokenTree) -> u128 {
    let literal = syn::parse_str::<syn::LitInt>(&tree.to_string()).expect("duration");
    let value = u128::from(literal.base10_parse::<u64>().expect("duration value"));
    value
        * match literal.suffix() {
            "ms" => 1,
            "s" => 1_000,
            "m" => 60_000,
            "h" => 3_600_000,
            other => panic!("unexpected duration suffix {other}"),
        }
}

fn is_ident(tree: Option<&TokenTree>, text: &str) -> bool {
    matches!(tree, Some(TokenTree::Ident(ident)) if ident == text)
}

fn is_punct(tree: Option<&TokenTree>, character: char) -> bool {
    matches!(tree, Some(TokenTree::Punct(punct)) if punct.as_char() == character)
}

fn group(tree: Option<&TokenTree>, delimiter: Delimiter) -> Option<TokenStream> {
    match tree {
        Some(TokenTree::Group(group)) if group.delimiter() == delimiter => Some(group.stream()),
        _ => None,
    }
}

/// Splits a parameter or value list at top-level commas/semicolons, treating
/// `<...>` in Rust types as nesting.
fn split_items(stream: TokenStream, separator: char) -> Vec<Vec<TokenTree>> {
    let mut items = vec![Vec::new()];
    let mut depth = 0_i32;
    let mut previous_dash = false;
    for tree in stream {
        if let TokenTree::Punct(punct) = &tree {
            match punct.as_char() {
                '<' => depth += 1,
                '>' if !previous_dash => depth -= 1,
                character if character == separator && depth == 0 => {
                    items.push(Vec::new());
                    previous_dash = false;
                    continue;
                }
                _ => {}
            }
            previous_dash = punct.as_char() == '-';
        } else {
            previous_dash = false;
        }
        items.last_mut().expect("item").push(tree);
    }
    items.retain(|item| !item.is_empty());
    items
}

/// Reads direct query placements (`query x`, `query("k") x`) in order.
fn direct_query(items: Vec<Vec<TokenTree>>) -> Vec<String> {
    let mut keys = Vec::new();
    for item in items {
        let mut index = 0;
        while is_punct(item.get(index), '#') {
            index += 2;
        }
        if !is_ident(item.get(index), "query") {
            continue;
        }
        index += 1;
        let explicit = group(item.get(index), Delimiter::Parenthesis).map(|stream| {
            let Some(TokenTree::Literal(literal)) = stream.into_iter().next() else {
                panic!("query placement key");
            };
            decode_string(&literal)
        });
        if explicit.is_some() {
            index += 1;
        }
        if is_ident(item.get(index), "each") {
            index += 1;
        }
        let Some(TokenTree::Ident(name)) = item.get(index) else {
            panic!("query placement value name");
        };
        let name = name.to_string();
        keys.push(explicit.unwrap_or_else(|| name.trim_start_matches("r#").to_owned()));
    }
    keys
}

fn parse_policy_body(stream: TokenStream) -> Policy {
    let tokens = stream.into_iter().collect::<Vec<_>>();
    let mut policy = Policy {
        id: String::new(),
        cost: 1,
        windows: Vec::new(),
        labels: Vec::new(),
    };
    let mut order = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if is_ident(tokens.get(index), "id") {
            let Some(TokenTree::Literal(literal)) = tokens.get(index + 1) else {
                panic!("rate id literal");
            };
            policy.id = decode_string(literal);
            order.push("id");
            index += 3;
        } else if is_ident(tokens.get(index), "key") {
            let stream = group(tokens.get(index + 1), Delimiter::Bracket).expect("key list");
            for dimension in split_items(stream, ',') {
                match dimension.first() {
                    Some(TokenTree::Ident(ident)) if dimension.len() == 1 => {
                        policy.labels.push(ident.to_string());
                    }
                    Some(TokenTree::Literal(literal)) if is_punct(dimension.get(1), '=') => {
                        policy.labels.push(decode_string(literal));
                    }
                    _ => panic!("dimension is not canonical v3: {dimension:?}"),
                }
            }
            order.push("key");
            index += 3;
        } else if is_ident(tokens.get(index), "cost") {
            policy.cost = u32::try_from(decode_int(&tokens[index + 1])).expect("cost");
            order.push("cost");
            index += 3;
        } else if is_ident(tokens.get(index), "allow") {
            assert!(is_ident(tokens.get(index + 2), "per"));
            policy.windows.push((
                decode_int(&tokens[index + 1]),
                decode_duration(&tokens[index + 3]),
            ));
            index += 5;
        } else {
            panic!("unexpected rate token {}", tokens[index]);
        }
    }
    let mut canonical = order.clone();
    canonical.sort_by_key(|entry| ["id", "key", "cost"].iter().position(|name| name == entry));
    assert_eq!(
        order, canonical,
        "v3 fixes the rate header order to id; key; cost"
    );
    policy
}

fn parse_owner_body(stream: TokenStream, owner: &mut Owner, client: &mut Client) {
    let tokens = stream.into_iter().collect::<Vec<_>>();
    let mut index = 0;
    while index < tokens.len() {
        let current = &tokens[index];
        let TokenTree::Ident(ident) = current else {
            index += 1;
            continue;
        };
        let word = ident.to_string();
        match word.as_str() {
            "scope" => {
                let mut child = Owner {
                    kind: "scope",
                    name: tokens[index + 1].to_string(),
                    ..Owner::default()
                };
                let parameters =
                    group(tokens.get(index + 2), Delimiter::Parenthesis).expect("scope parameters");
                child.direct_query = direct_query(split_items(parameters, ','));
                let body = group(tokens.get(index + 3), Delimiter::Brace).expect("scope body");
                parse_owner_body(body, &mut child, client);
                owner.children.push(child);
                index += 4;
            }
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "METHOD" => {
                let mut offset = index + 1;
                if word == "METHOD" {
                    offset += 1;
                }
                let mut child = Owner {
                    kind: "endpoint",
                    name: tokens[offset].to_string(),
                    method: word.clone(),
                    ..Owner::default()
                };
                let parameters = group(tokens.get(offset + 1), Delimiter::Parenthesis)
                    .expect("endpoint parameters");
                child.direct_query = direct_query(split_items(parameters, ','));
                let mut end = offset + 2;
                while end < tokens.len()
                    && !is_punct(tokens.get(end), ';')
                    && group(tokens.get(end), Delimiter::Brace).is_none()
                {
                    end += 1;
                }
                if let Some(body) = group(tokens.get(end), Delimiter::Brace) {
                    parse_owner_body(body, &mut child, client);
                }
                owner.children.push(child);
                index = end + 1;
            }
            "base" => {
                let Some(TokenTree::Literal(literal)) = tokens.get(index + 1) else {
                    panic!("base literal");
                };
                let base = decode_string(literal);
                let after_scheme = base.split_once("://").expect("absolute base").1;
                let path = after_scheme.split_once('/').map_or("", |(_, path)| path);
                client.base_path = path
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_owned)
                    .collect();
                index += 2;
            }
            "values" => {
                let body = group(tokens.get(index + 1), Delimiter::Brace).expect("values");
                owner.direct_query = direct_query(split_items(body, ';'));
                index += 2;
            }
            "rate_limits" => {
                let body = group(tokens.get(index + 1), Delimiter::Brace).expect("rate_limits");
                let definitions = body.into_iter().collect::<Vec<_>>();
                let mut cursor = 0;
                while cursor < definitions.len() {
                    assert!(is_ident(definitions.get(cursor), "policy"));
                    let name = definitions[cursor + 1].to_string();
                    let body =
                        group(definitions.get(cursor + 3), Delimiter::Brace).expect("policy body");
                    client.policies.insert(name, parse_policy_body(body));
                    cursor += 4;
                }
                index += 2;
            }
            "rate_limit" => {
                if let Some(body) = group(tokens.get(index + 1), Delimiter::Brace) {
                    owner
                        .rates
                        .push(Attachment::Inline(parse_policy_body(body)));
                    index += 2;
                } else {
                    let policy = tokens[index + 1].to_string();
                    let mut cost = None;
                    index += 3;
                    if let Some(body) = group(tokens.get(index), Delimiter::Brace) {
                        let body = body.into_iter().collect::<Vec<_>>();
                        cost = Some(u32::try_from(decode_int(&body[1])).expect("cost"));
                    }
                    owner.rates.push(Attachment::Named { policy, cost });
                    index += 1;
                }
            }
            "path" => {
                owner.path = Some(match tokens.get(index + 1) {
                    Some(TokenTree::Literal(literal)) => PathOp::Append(decode_string(literal)),
                    Some(TokenTree::Ident(mode)) if mode == "replace" => {
                        let Some(TokenTree::Literal(literal)) = tokens.get(index + 2) else {
                            panic!("path replace literal");
                        };
                        PathOp::Replace(decode_string(literal))
                    }
                    Some(TokenTree::Ident(mode)) if mode == "off" => PathOp::Off,
                    other => panic!("path is not a v3 template: {other:?}"),
                });
                index += 2;
            }
            "id" if matches!(tokens.get(index + 1), Some(TokenTree::Literal(_)))
                && is_punct(tokens.get(index + 2), ';') =>
            {
                let Some(TokenTree::Literal(literal)) = tokens.get(index + 1) else {
                    unreachable!();
                };
                owner.id = Some(decode_string(literal));
                index += 3;
            }
            "query" => {
                let (replace, body) = if is_ident(tokens.get(index + 1), "replace") {
                    (true, group(tokens.get(index + 2), Delimiter::Brace))
                } else {
                    (false, group(tokens.get(index + 1), Delimiter::Brace))
                };
                if let Some(body) = body {
                    owner.block_query_replace = replace;
                    for statement in split_items(body, ';') {
                        let (operation, key) = match statement.first() {
                            Some(TokenTree::Literal(literal)) => ("=", decode_string(literal)),
                            Some(TokenTree::Ident(operation)) => {
                                let Some(TokenTree::Literal(literal)) = statement.get(1) else {
                                    panic!("query statement key");
                                };
                                (
                                    match operation.to_string().as_str() {
                                        "replace" => "replace",
                                        "when_present" => "when_present",
                                        "append" => "append",
                                        "remove" => "remove",
                                        other => panic!("unknown query operation {other}"),
                                    },
                                    decode_string(literal),
                                )
                            }
                            other => panic!("unexpected query statement {other:?}"),
                        };
                        owner.block_query.push((operation.to_owned(), key));
                    }
                }
                index += if replace { 3 } else { 2 };
            }
            _ => index += 1,
        }
    }
}

fn extract(converted: &str) -> Client {
    let file = syn::parse_file(converted).expect("converted file is valid Rust");
    let mut found = Vec::new();
    for item in &file.items {
        if let syn::Item::Macro(item) = item {
            if item.mac.path.is_ident("api") {
                found.push(item.mac.tokens.clone());
            }
        }
    }
    assert_eq!(found.len(), 1, "one top-level api! invocation");
    let tokens = found.remove(0).into_iter().collect::<Vec<_>>();
    let client_index = tokens
        .iter()
        .position(|tree| matches!(tree, TokenTree::Ident(ident) if ident == "client"))
        .expect("client");
    let mut client = Client {
        name: tokens[client_index + 1].to_string(),
        ..Client::default()
    };
    let body = group(tokens.get(client_index + 2), Delimiter::Brace).expect("client body");
    let mut root = Owner {
        kind: "client",
        ..Owner::default()
    };
    parse_owner_body(body, &mut root, &mut client);
    client.root = root;
    client
}

/// The IDENTITY-01 derivation, written independently of the converter.
fn derived_id(client: &str, path: &[String]) -> String {
    let characters = client.chars().collect::<Vec<_>>();
    let mut snake = String::new();
    for (index, character) in characters.iter().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            let previous = characters[index - 1];
            let next_lower = characters
                .get(index + 1)
                .is_some_and(char::is_ascii_lowercase);
            if previous.is_ascii_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_ascii_uppercase() && next_lower)
            {
                snake.push('_');
            }
        }
        snake.push(character.to_ascii_lowercase());
    }
    std::iter::once(snake)
        .chain(path.iter().cloned())
        .collect::<Vec<_>>()
        .join(".")
}

fn effective(client: &Client) -> Vec<EffectiveEndpoint> {
    struct State {
        path: Vec<String>,
        rates: Vec<Attachment>,
        query: Vec<String>,
        names: Vec<String>,
    }
    fn walk(
        client: &Client,
        owner: &Owner,
        state: &mut State,
        output: &mut Vec<EffectiveEndpoint>,
    ) {
        let saved = (
            state.path.clone(),
            state.rates.len(),
            state.query.clone(),
            state.names.len(),
        );
        match &owner.path {
            Some(PathOp::Append(template)) => {
                state.path.extend(template.split('/').map(str::to_owned));
            }
            Some(PathOp::Replace(template)) => {
                state.path = template.split('/').map(str::to_owned).collect();
            }
            Some(PathOp::Off) => state.path.clear(),
            None => {}
        }
        state.rates.extend(owner.rates.iter().cloned());
        if owner.block_query_replace {
            state.query.clear();
        }
        state.query.extend(owner.direct_query.iter().cloned());
        for (operation, key) in &owner.block_query {
            match operation.as_str() {
                "remove" => state.query.retain(|existing| existing != key),
                "replace" | "when_present" => {
                    state.query.retain(|existing| existing != key);
                    state.query.push(key.clone());
                }
                _ => state.query.push(key.clone()),
            }
        }
        if owner.kind != "client" {
            state.names.push(owner.name.clone());
        }
        if owner.kind == "endpoint" {
            let policies = state
                .rates
                .iter()
                .map(|attachment| match attachment {
                    Attachment::Inline(policy) => policy.clone(),
                    Attachment::Named { policy, cost } => {
                        let mut definition = client.policies[policy].clone();
                        if let Some(cost) = cost {
                            definition.cost = *cost;
                        }
                        definition
                    }
                })
                .collect();
            output.push(EffectiveEndpoint {
                id: owner
                    .id
                    .clone()
                    .unwrap_or_else(|| derived_id(&client.name, &state.names)),
                method: owner.method.clone(),
                segments: client
                    .base_path
                    .iter()
                    .cloned()
                    .chain(state.path.iter().cloned())
                    .collect(),
                query: state.query.clone(),
                policies,
            });
        }
        for child in &owner.children {
            walk(client, child, state, output);
        }
        state.path = saved.0;
        state.rates.truncate(saved.1);
        state.query = saved.2;
        state.names.truncate(saved.3);
    }
    let mut output = Vec::new();
    let mut state = State {
        path: Vec::new(),
        rates: Vec::new(),
        query: Vec::new(),
        names: Vec::new(),
    };
    walk(client, &client.root, &mut state, &mut output);
    output
}

#[derive(Debug)]
struct Recorded {
    method: String,
    segments: Vec<String>,
    query: Vec<String>,
    policies: Vec<Policy>,
}

fn recording(name: &str) -> BTreeMap<String, Vec<Recorded>> {
    let text = fs::read_to_string(repository_root().join("fixtures/migration").join(name))
        .expect("baseline recording");
    let entries: serde_json::Value = serde_json::from_str(&text).expect("recording JSON");
    let mut map: BTreeMap<String, Vec<Recorded>> = BTreeMap::new();
    for entry in entries.as_array().expect("recording array") {
        let url = entry["observed_url"].as_str().expect("url");
        let after_scheme = url.split_once("://").expect("absolute").1;
        let path_and_query = after_scheme.split_once('/').map_or("", |(_, rest)| rest);
        let (path, query) = path_and_query
            .split_once('?')
            .unwrap_or((path_and_query, ""));
        let mut keys: Vec<String> = Vec::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let key = pair.split_once('=').map_or(pair, |(key, _)| key).to_owned();
            if keys.last() != Some(&key) {
                keys.push(key);
            }
        }
        let policies = entry["limiter"]["policies"]
            .as_array()
            .expect("policies")
            .iter()
            .map(|policy| Policy {
                id: policy["id"].as_str().expect("id").to_owned(),
                cost: u32::try_from(policy["cost"].as_u64().expect("cost")).expect("cost"),
                windows: policy["windows"]
                    .as_array()
                    .expect("windows")
                    .iter()
                    .map(|window| {
                        (
                            window[0].as_u64().expect("capacity"),
                            u128::from(window[1].as_u64().expect("millis")),
                        )
                    })
                    .collect(),
                labels: policy["key"]
                    .as_array()
                    .expect("key")
                    .iter()
                    .map(|part| part[0].as_str().expect("label").to_owned())
                    .collect(),
            })
            .collect();
        map.entry(
            entry["endpoint_id"]
                .as_str()
                .expect("endpoint id")
                .to_owned(),
        )
        .or_default()
        .push(Recorded {
            method: entry["method"].as_str().expect("method").to_owned(),
            segments: path.split('/').map(str::to_owned).collect(),
            query: keys,
            policies,
        });
    }
    map
}

fn is_subsequence(needle: &[String], haystack: &[String]) -> bool {
    let mut remaining = haystack.iter();
    needle
        .iter()
        .all(|item| remaining.any(|candidate| candidate == item))
}

/// Asserts the converted v3 text reproduces every recorded v2.1 observation.
fn assert_matches_recording(converted: &str, recording_name: &str) -> Vec<EffectiveEndpoint> {
    let client = extract(converted);
    let endpoints = effective(&client);
    let recorded = recording(recording_name);
    for (id, observations) in &recorded {
        let endpoint = endpoints
            .iter()
            .find(|endpoint| &endpoint.id == id)
            .unwrap_or_else(|| panic!("converted declaration lost endpoint ID `{id}`"));
        for observation in observations {
            assert_eq!(endpoint.method, observation.method, "{id} method");
            assert_eq!(
                endpoint.policies, observation.policies,
                "{id}: complete rate plan (IDs, costs, windows, ordered key labels)"
            );
            assert_eq!(
                endpoint.segments.len(),
                observation.segments.len(),
                "{id}: route segment count {:?} vs {:?}",
                endpoint.segments,
                observation.segments
            );
            for (template, observed) in endpoint.segments.iter().zip(&observation.segments) {
                let hole = template.replace("{{", "").replace("}}", "").contains('{');
                if hole {
                    assert!(!observed.is_empty(), "{id}: empty hole segment");
                } else {
                    assert_eq!(
                        template.replace("{{", "{").replace("}}", "}"),
                        *observed,
                        "{id}: literal route segment"
                    );
                }
            }
            assert!(
                is_subsequence(&observation.query, &endpoint.query),
                "{id}: recorded query order {:?} is not preserved by v3 normalization {:?}",
                observation.query,
                endpoint.query
            );
        }
    }
    endpoints
}

fn identities_match_recording(outcome: &SourceOutcome, recording_name: &str) {
    let recorded = recording(recording_name);
    for (id, observations) in recorded {
        let identity = outcome
            .identities
            .iter()
            .find(|identity| identity.v3_id == id)
            .unwrap_or_else(|| panic!("captured identities lack `{id}`"));
        assert_eq!(identity.v2_id, id, "v2.1 ID is preserved");
        let captured = identity
            .policies
            .iter()
            .map(|policy: &PolicyIdentity| Policy {
                id: policy.id.clone(),
                cost: policy.cost,
                windows: policy.windows.clone(),
                labels: policy.labels.clone(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            captured, observations[0].policies,
            "{id}: captured v2.1 plan"
        );
    }
}

fn contains_owner_qualified_endpoint(stream: TokenStream) -> bool {
    let tokens = stream.into_iter().collect::<Vec<_>>();
    tokens.iter().enumerate().any(|(index, tree)| match tree {
        TokenTree::Ident(ident) if ident == "endpoint" => is_punct(tokens.get(index + 1), '.'),
        TokenTree::Group(group) => contains_owner_qualified_endpoint(group.stream()),
        _ => false,
    })
}

// ----- tests -----------------------------------------------------------------

/// V3-O04 (nested Option): `set_x(None)` on `x?: Option<U>` becomes
/// `replace_x(None)`; the decision uses the resolved declaration, so an
/// unrelated type with a same-named method is diagnosed, not rewritten.
#[test]
fn nested_optional_setters_become_exact_replace() {
    let source = r#"use concord::api;

pub struct Tracker;

impl Tracker {
    pub fn new() -> Self {
        Self
    }

    pub fn set_milestone(self, _: Option<u32>) -> Self {
        self
    }
}

api! {
    client Goals {
        base "https://goals.example.test/";
        timeout 5s;
        values {
            locale?: String;
        }
        scope project(key: String, #[default] depth: u32) {
            path ["projects", project.key];
            GET milestones(milestone?: Option<u32>, limit: u32 = 10) -> Vec<u32> {
                path ["milestones"];
                query {
                    "milestone" = endpoint.milestone;
                    "limit" = endpoint.limit;
                }
            }
        }
    }
}

pub fn calls(client: &Goals, maybe: Option<Option<u32>>, req: goals::project::MilestonesRequest) {
    let builder = Goals::builder().set_locale(None);
    let request = client
        .project("p".to_owned())
        .set_depth(3)
        .milestones()
        .set_milestone(None)
        .set_milestone(Some(None))
        .set_milestone(maybe)
        .set_limit(20);
    let typed = req.set_milestone(Some(Some(7)));
    let tracker = Tracker::new().set_milestone(None);
    let _ = (builder, request, typed, tracker);
}
"#;
    let outcome = convert(source);
    let converted = &outcome.converted;
    assert!(converted.contains("Goals::builder().replace_locale(None)"));
    assert!(
        converted.contains(".set_depth(3)"),
        "required initialized value keeps set_x"
    );
    assert!(converted.contains(".replace_milestone(None)\n"));
    assert!(converted.contains(".replace_milestone(Some(None))\n"));
    assert!(converted.contains(".replace_milestone(maybe)\n"));
    assert!(converted.contains(".set_limit(20)"));
    assert!(converted.contains("req.replace_milestone(Some(Some(7)))"));
    assert!(
        converted.contains("Tracker::new().set_milestone(None)"),
        "an unresolved receiver is never renamed"
    );
    assert_eq!(count(&outcome, Code::SetterReplace), 5);
    assert_eq!(count(&outcome, Code::SetterUnresolved), 1);
    let unresolved = outcome
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == Code::SetterUnresolved)
        .expect("unresolved diagnostic");
    assert_eq!(
        unresolved.line,
        source
            .lines()
            .position(|line| line.contains("Tracker::new().set_milestone"))
            .expect("line")
            + 1
    );
    assert_eq!(
        unresolved.suggestion.as_deref(),
        Some(".replace_milestone(None)")
    );
    // The declaration side: nested optional stays one Concord layer.
    assert!(converted.contains(
        "GET milestones(query milestone?: Option<u32>, query limit: u32 = 10) -> Vec<u32> {"
    ));
    assert!(converted.contains("path \"projects/{key}\";"));
}

/// V3-O04 (constant qualification): Rust constant paths keep their exact
/// qualification and are never mistaken for owner references; current-owner
/// references become bare and ancestor references keep exact qualification.
#[test]
fn constant_and_owner_qualification_are_resolved_exactly() {
    let source = r#"use concord::api;
api! {
    client Consts {
        base "https://consts.example.test/";
        timeout 5s;
        values {
            #[default]
            api_version: crate::defaults::ApiVersion;
            region: String = crate::defaults::REGION;
        }
        path [client.api_version];
        scope defaults(tier: u32 = defaults::TIER, zone?: String) {
            path ["tiers", defaults.tier];
            query {
                "zone" = defaults.zone;
                "region" = client.region;
            }
            GET items(offset: u64 = 0, count: u32 = crate::defaults::PAGE) -> Vec<u32> {
                path [client.api_version, "items", defaults.tier];
                query {
                    "offset" = endpoint.offset;
                    "count" = endpoint.count;
                    "tier" = defaults.tier;
                }
                paginate via concord::pagination::OffsetPagination {
                    offset -> endpoint.offset;
                    page_size = crate::defaults::PAGE_SIZE;
                }
            }
        }
    }
}
"#;
    let expected = r#"use concord::api;
api! {
    client Consts {
        base "https://consts.example.test/";
        timeout 5s;
        values {
            #[default]
            api_version: crate::defaults::ApiVersion;
            region: String = crate::defaults::REGION;
        }
        path "{api_version}";
        scope defaults(tier: u32 = defaults::TIER, query zone?: String) {
            path "tiers/{tier}";
            query {
                "region" = client.region;
            }
            GET items(query offset: u64 = 0, query count: u32 = crate::defaults::PAGE) -> Vec<u32> {
                path "{client.api_version}/items/{defaults.tier}";
                query {
                    "tier" = defaults.tier;
                }
                paginate via concord::pagination::OffsetPagination {
                    init concord::pagination::OffsetInit { offset: offset, page_size: crate::defaults::PAGE_SIZE };
                    cursor -> offset;
                }
            }
        }
    }
}
"#;
    let outcome = convert(source);
    assert_eq!(outcome.converted, expected);
    // v3 emits declaration paths inside its generated support modules, where
    // the relative `defaults::TIER` would name the generated module of scope
    // `defaults` (the v3 macro rejects it with [C3-GENERATED]). The tool
    // cannot know this module's absolute path, so it reports the site and
    // leaves the source unchanged; `crate::defaults::...` paths are accepted.
    let shadowed = errors(&outcome);
    assert_eq!(shadowed.len(), 1, "{shadowed:?}");
    let diagnostic = &outcome.diagnostics[outcome
        .diagnostics
        .iter()
        .position(|diagnostic| diagnostic.code == Code::GeneratedShadow)
        .expect("generated-shadow diagnostic")];
    assert_eq!(
        (diagnostic.line, diagnostic.column),
        (12, 36),
        "{}",
        diagnostic.render()
    );
    assert!(
        diagnostic.message.contains(
            "`defaults::TIER` would resolve through the support module of scope `defaults`"
        )
    );
    assert_eq!(
        diagnostic.suggestion.as_deref(),
        Some(
            "crate::<path of this module>::defaults::TIER  // or import it under a name no generated item uses"
        )
    );
    assert_eq!(outcome.identities[0].v3_id, "consts.defaults.items");
    assert!(!outcome.identities[0].pinned);
}

/// V3-O04 (direct-placement ordering): only a leading run of plain local
/// introductions in declaration order becomes direct placement, so the v3
/// normalized order (direct placements, then explicit statements) equals the
/// v2.1 statement order; values already placed stay explicit (LANG-01).
#[test]
fn direct_placement_never_reorders_mixed_operations() {
    let source = r#"use concord::api;
api! {
    client Order {
        base "https://order.example.test/";
        timeout 5s;
        GET mixed(a?: String, b?: String, c?: String) -> String {
            query {
                "b" = endpoint.b;
                "a" = endpoint.a;
                append "z" = "1";
                "c" = endpoint.c;
            }
        }
        GET appended(a?: String, b?: String) -> String {
            query { "a" = endpoint.a; append "a" = "again"; "b" = endpoint.b; }
        }
        GET adapted(tags: Vec<String>, #[sensitive] token?: String) -> String {
            query { "tag" = each endpoint.tags via crate::Tag; }
            headers { "X-Token" = endpoint.token; }
        }
        GET shared(locale?: String) -> String {
            query { "locale" = endpoint.locale; "lang" = endpoint.locale; }
            headers { "Accept-Language" = endpoint.locale; }
        }
    }
}
"#;
    let outcome = convert(source);
    let converted = &outcome.converted;
    assert!(errors(&outcome).is_empty(), "{:?}", errors(&outcome));
    assert!(converted.contains("GET mixed(a?: String, query b?: String, c?: String) -> String {"));
    assert!(converted.contains(
        "            query {\n                \"a\" = a;\n                append \"z\" = \"1\";\n                \"c\" = c;\n            }"
    ));
    assert!(converted.contains("GET appended(query a?: String, b?: String) -> String {"));
    assert!(converted.contains("query { append \"a\" = \"again\"; \"b\" = b; }"));
    assert!(converted.contains(
        "GET adapted(query(\"tag\") each tags: Vec<String> via crate::Tag, #[sensitive] header(\"X-Token\") token?: String) -> String {"
    ));
    assert!(converted.contains("GET shared(query locale?: String) -> String {"));
    assert!(converted.contains("query { \"lang\" = locale; }"));
    assert!(converted.contains("headers { \"Accept-Language\" = locale; }"));

    // Oracle: the independently extracted v3 normalization reproduces the
    // v2.1 statement order for every endpoint.
    let endpoints = effective(&extract(converted));
    let order = |name: &str| {
        endpoints
            .iter()
            .find(|endpoint| endpoint.id == format!("order.{name}"))
            .expect("endpoint")
            .query
            .clone()
    };
    assert_eq!(order("mixed"), ["b", "a", "z", "c"]);
    assert_eq!(order("appended"), ["a", "a", "b"]);
    assert_eq!(order("adapted"), ["tag"]);
    assert_eq!(order("shared"), ["locale", "lang"]);

    let explicit = migrate_source(
        "fixture.rs",
        source,
        &DeclIndex::default(),
        &Settings {
            direct_placement: false,
        },
    );
    assert!(
        explicit
            .converted
            .contains("GET mixed(a?: String, b?: String, c?: String) -> String {")
    );
    assert!(explicit.converted.contains("\"b\" = b;"));
    assert!(!explicit.converted.contains("query b?"));
}

/// v2.1 plain assignment over an earlier potential owner (inherited or
/// earlier in the same program) becomes exact `replace`; family fences clear
/// ownership; v3 precondition violations are diagnostics, not deletions.
#[test]
fn plain_assignment_over_an_earlier_owner_becomes_replace() {
    let source = r#"use concord::api;
api! {
    client Inherit {
        base "https://inherit.example.test/";
        timeout 5s;
        values {
            locale?: String;
            trace: String;
        }
        query { "locale" = client.locale; "a" = "1"; "a" = "2"; }
        headers { "X-Trace" = client.trace; }
        scope regional(locale?: String) {
            query { "locale" = regional.locale; }
            GET item(trace?: String) -> String {
                headers { "x-trace" = endpoint.trace; }
            }
            GET fenced(locale?: String) -> String {
                query replace { "locale" = endpoint.locale; }
            }
            GET cleanup() -> String {
                query { when_present "missing" = regional.locale; remove "absent"; remove "a"; }
            }
        }
    }
}
"#;
    let outcome = convert(source);
    let converted = &outcome.converted;
    assert!(converted.contains(
        "            query locale?: String;\n            header(\"X-Trace\") trace: String;"
    ));
    assert!(converted.contains("query { \"a\" = \"1\"; replace \"a\" = \"2\"; }"));
    assert!(!converted.contains("headers { \"X-Trace\""));
    assert!(converted.contains("query { replace \"locale\" = locale; }"));
    assert!(converted.contains("headers { replace \"x-trace\" = trace; }"));
    assert!(converted.contains("GET fenced(query locale?: String) -> String {"));
    assert!(converted.contains("query replace { }"));
    assert!(converted.contains(
        "query { when_present \"missing\" = regional.locale; remove \"absent\"; remove \"a\"; }"
    ));
    assert_eq!(count(&outcome, Code::KeyReplace), 3);
    assert_eq!(count(&outcome, Code::WhenPresentWithoutFallback), 1);
    assert_eq!(count(&outcome, Code::RemoveWithoutOwner), 1);
    let when_present = outcome
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == Code::WhenPresentWithoutFallback)
        .expect("diagnostic");
    assert!(
        when_present
            .suggestion
            .as_deref()
            .is_some_and(|suggestion| suggestion.starts_with("\"missing\" = regional.locale;")),
        "{when_present:?}"
    );
}

/// V3-R02 / MIGRATE-02: the Catalog baseline converts with endpoint IDs,
/// rate IDs/windows/costs and key labels (including the legacy
/// `"client.tenant"` label) byte-for-byte equal to the captured v2.1 plans.
#[test]
fn catalog_baseline_contract_preserves_labels_ids_and_order() {
    let source = fixture("catalog_contract_v2_1.rs.txt");
    let outcome = convert(&source);
    let converted = &outcome.converted;
    identities_match_recording(&outcome, "v2_1_catalog_recording.json");
    let endpoints = assert_matches_recording(converted, "v2_1_catalog_recording.json");
    assert_eq!(endpoints.len(), 4);
    // MIGRATE-04: the root `#[default] api_version` changes evaluation timing.
    assert_eq!(count(&outcome, Code::RootDefaultTiming), 1);
    assert!(converted.contains("key [origin, endpoint, \"client.tenant\" = client.tenant];"));
    assert!(converted.contains("key [origin, \"tenant\" = tenant];"));
    assert!(converted.contains("rate_limit tenant_reads(tenant = client.tenant);"));
    assert!(converted.contains("            #[sensitive]\n            query tenant: TenantId;"));
    assert!(converted.contains("header(\"Accept-Language\") language?: Language,"));
    assert!(converted.contains("query(\"tag\") each tags: Vec<Tag>,"));
    assert!(converted.contains("header(\"Idempotency-Key\") command_id: CommandId,"));
    assert!(converted.contains("headers {\n            \"X-Client-Version\" = \"2\";\n        }"));
    assert!(
        converted
            .contains("init concord::pagination::OffsetInit { offset: offset, page_size: count };")
    );
    assert!(converted.contains("timeout 20s;"));
    assert!(converted.contains(".replace_locale(Some(\"fr-FR\".to_owned()))"));
    assert!(converted.contains(".replace_language(Some(\"fr\".to_owned()))"));
    assert!(converted.contains(".replace_category(Some(\"featured\".to_owned()))"));
    assert!(converted.contains(
        ".response_body_limit(NonZeroUsize::new(8 * 1024 * 1024).expect(\"positive\").get())"
    ));
    assert!(converted.contains("request.collect_all(NonZeroU32::new(20).expect(\"positive\"))"));
    assert!(!converted.contains("PageLimit"));
    assert!(!converted.contains("\n\n\n"), "no double blank lines");
    assert_eq!(errors(&outcome).len(), 1, "{:?}", errors(&outcome));
    assert_eq!(count(&outcome, Code::StoreOutcome), 1);
    assert!(outcome.identities.iter().all(|identity| !identity.pinned));
}

/// V3-R01 / MIGRATE-01: all 46 Riot endpoints convert with no unautomatable
/// site, identical endpoint IDs, complete rate plans and key labels (no label
/// changes), route segments and query order equal to the v2.1 recording, and
/// the scope-shared groups still attached at their scopes.
#[test]
fn riot_declaration_converts_all_endpoints_with_identity_and_sharing() {
    let source = fixture("riot_v2_1.rs.txt");
    let outcome = convert_riot_with_original_context(&source);
    let converted = &outcome.converted;
    assert!(errors(&outcome).is_empty(), "{:?}", errors(&outcome));
    assert_eq!(outcome.identities.len(), 46);
    identities_match_recording(&outcome, "v2_1_riot_recording.json");
    let endpoints = assert_matches_recording(converted, "v2_1_riot_recording.json");
    assert_eq!(endpoints.len(), 46);
    let recorded = recording("v2_1_riot_recording.json");
    assert_eq!(recorded.len(), 46);
    assert!(
        endpoints
            .iter()
            .all(|endpoint| recorded.contains_key(&endpoint.id))
    );

    let client = extract(converted);
    for (scope, id) in [
        ("champion_mastery_v4", "concord.riot.2"),
        ("challenges_v1", "concord.riot.15"),
        ("tournament_stub_v5", "concord.riot.28"),
    ] {
        let owner = client
            .root
            .children
            .iter()
            .flat_map(|child| child.children.iter())
            .find(|owner| owner.name == scope)
            .expect("scope");
        assert!(
            owner.rates.iter().any(
                |attachment| matches!(attachment, Attachment::Inline(policy) if policy.id == id)
            ),
            "{scope} keeps its shared {id} attachment at scope level"
        );
    }
    let file = syn::parse_file(converted).expect("valid Rust");
    let syn::Item::Macro(item) = file
        .items
        .iter()
        .find(|item| matches!(item, syn::Item::Macro(_)))
        .expect("api!")
    else {
        unreachable!();
    };
    assert!(!contains_owner_qualified_endpoint(item.mac.tokens.clone()));
    assert!(!converted.contains("path ["));
    assert!(!converted.contains("platform.platform"));
    assert!(converted.contains("subdomain replace [platform, \"api\"];"));
    assert!(converted.contains("// ANCHOR: riot_client"));
    assert!(converted.contains("// ANCHOR_END: riot_match_ids_endpoint"));
    assert!(converted.contains("request explicit;\n            response explicit;"));
    assert!(converted.contains(
        "                query(\"startTime\") start_time?: i64,\n                query(\"endTime\") end_time?: i64,\n                query queue?: u32,\n                query(\"type\") match_type?: crate::riot::domain::MatchType,\n                query start: u64 = 0,\n                query count: std::num::NonZeroU32 = crate::riot::domain::DEFAULT_MATCH_PAGE_SIZE"
    ));
    assert!(converted.contains("path \"by-puuid/{puuid}/ids\";"));
    assert!(converted.contains("cursor -> start;"));
    assert!(
        converted
            .contains("rate_limit { id \"concord.riot.1\"; key [origin];\n            cost 1;")
    );
    assert!(
        outcome
            .identities
            .iter()
            .all(|identity| !identity.pinned && !identity.explicit)
    );
}

/// §25.1: apply writes only an isolated copy into an explicit, empty output
/// directory outside the input tree; the input is never modified.
#[test]
fn apply_refuses_to_write_into_the_input_tree() {
    let workspace = TempDir::new("apply");
    let input = workspace.0.join("crate");
    let catalog = fixture("catalog_contract_v2_1.rs.txt");
    workspace.write(
        "crate/Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    workspace.write("crate/src/lib.rs", &catalog);
    workspace.write("crate/notes.txt", "untouched\n");

    let migration = migrate(&input, &Settings::default()).expect("migration");
    for output in [
        input.clone(),
        input.join("src"),
        input.join("out/nested"),
        workspace.0.clone(),
    ] {
        let error = migration.apply(&output).expect_err("refused");
        assert!(error.contains("refusing"), "{error}");
    }
    let occupied = workspace.0.join("occupied");
    fs::create_dir_all(&occupied).expect("occupied");
    fs::write(occupied.join("keep"), "x").expect("occupied file");
    assert!(
        migration
            .apply(&occupied)
            .expect_err("non-empty output refused")
            .contains("not empty")
    );

    let arguments = |extra: &[&str]| {
        let mut arguments = vec!["--input".to_owned(), input.display().to_string()];
        arguments.extend(extra.iter().map(|argument| (*argument).to_owned()));
        arguments
    };
    assert_eq!(
        run_status(&arguments(&[
            "--apply",
            "--output",
            &input.join("v3").display().to_string()
        ])),
        2
    );
    assert_eq!(
        run_status(&arguments(&[
            "--check",
            "--report",
            &input.join("r.json").display().to_string()
        ])),
        2
    );
    assert_eq!(run_status(&arguments(&["--apply"])), 2);
    assert!(!input.join("v3").exists());
    assert!(!input.join("r.json").exists());

    let output = workspace.0.join("converted");
    let applied = migration.apply(&output).expect("apply to an isolated copy");
    assert_eq!(applied.written, 3);
    assert!(applied.skipped_links.is_empty());
    assert_eq!(
        fs::read_to_string(input.join("src/lib.rs")).expect("input"),
        catalog,
        "input tree unchanged"
    );
    let converted = fs::read_to_string(output.join("src/lib.rs")).expect("output");
    assert_eq!(Some(converted.as_str()), migration.converted("src/lib.rs"));
    assert!(converted.contains("path \"regions/{region}\";"));
    assert_eq!(
        fs::read_to_string(output.join("notes.txt")).expect("copied"),
        "untouched\n"
    );
    assert!(output.join("Cargo.toml").is_file());
    assert!(
        migration.apply(&output).is_err(),
        "second apply into a used directory is refused"
    );
}

/// Check mode exits nonzero only when unautomatable sites remain; the JSON
/// report carries stable codes and the captured identities.
#[test]
fn check_exit_status_and_json_report_are_stable() {
    let clean = TempDir::new("clean");
    clean.write(
        "Cargo.toml",
        "[package]\nname = \"clean\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    // The original module depends on crate::riot::domain::*; give namespace
    // proof its real original context instead of an unresolved root snippet.
    let riot_input = write_original_riot_context(&clean, &fixture("riot_v2_1.rs.txt"));
    let report = TempDir::new("report");
    let report_path = report.0.join("report.json");
    assert_eq!(
        run_status(&[
            "--input".to_owned(),
            riot_input.display().to_string(),
            "--check".to_owned(),
            "--report".to_owned(),
            report_path.display().to_string(),
        ]),
        0
    );
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("report")).expect("json");
    assert_eq!(json["summary"]["unautomatable"], 0);
    assert_eq!(
        json["captured_identities"]
            .as_array()
            .expect("identities")
            .len(),
        46
    );
    assert_eq!(json["mode"], "check");
    let codes = json["codes"].as_array().expect("codes");
    assert!(codes.iter().all(|code| {
        code["code"]
            .as_str()
            .is_some_and(|code| code.starts_with("C3-MIGRATE-"))
    }));
    assert_eq!(codes.len(), Code::ALL.len());

    let blocked = TempDir::new("blocked");
    blocked.write(
        "Cargo.toml",
        "[package]\nname = \"blocked\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    blocked.write("src/lib.rs", &fixture("catalog_contract_v2_1.rs.txt"));
    assert_eq!(
        run_status(&[
            "--input".to_owned(),
            blocked.0.display().to_string(),
            "--check".to_owned(),
        ]),
        1
    );
    let migration = migrate(&blocked.0, &Settings::default()).expect("migration");
    let diff = migration.diff();
    assert!(diff.starts_with("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ "));
    assert!(
        diff.contains("\n-        path [client.api_version];\n+        path \"{api_version}\";\n")
    );
}

/// Unautomatable declaration sites produce actionable diagnostics and leave
/// the original source in place; mechanical edge cases convert exactly.
#[test]
fn unautomatable_declaration_sites_are_diagnosed_not_guessed() {
    let source = r#"use concord::api;
api! {
    client Api2URL {
        base "https://edge.example.test/";
        timeout 5s;
        rate_limits {
            policy per_endpoint(tenant) {
                id "edge.per_endpoint";
                cost 2;
                key [origin, endpoint, tenant];
                allow 10 per 1s;
            }
            policy twin(tenant) {
                id "edge.twin";
                key [tenant];
                cost 1;
                allow 10 per 1s;
            }
        }
        scope braces(tenant: String) {
            rate_limit twin(tenant = braces.tenant);
            GET literal() -> String {
                path ["a{b}", "c}}d", concat ["v", 2, "-", true]];
            }
            GET fixed(id: String) -> String {
                path ["fixed", endpoint.id via crate::Fixed<{ 1 + 1 }>];
            }
            GET question() -> String {
                path ["what?"];
            }
            GET custom(cursor?: String) -> Vec<String> {
                paginate via crate::VendorController {
                    cursor -> endpoint.cursor;
                }
            }
            GET unbound(start: u64 = 0) -> Vec<String> {
                paginate via concord::pagination::OffsetPagination {
                    offset = 0;
                    page_size = crate::PAGE;
                }
            }
            GET keyed() -> String;
            GET placed(a?: String) -> String {
                query { "a" = endpoint.a; }
            }
            GET inline(b?: String) -> String { query { "b" = endpoint.b; } }
            GET windows() -> String {
                rate_limit { id "edge.dup"; key [origin]; cost 1; allow 5 per 1s; allow 7 per 1s; }
                rate_limit per_endpoint(tenant = braces.tenant) { cost 3; }
            }
        }
    }
}
"#;
    let outcome = convert(source);
    let converted = &outcome.converted;
    assert!(converted.contains("path \"a{{b}}/c}}}}d/v2-true\";"));
    assert!(converted.contains("path [\"fixed\", endpoint.id via crate::Fixed<{ 1 + 1 }>];"));
    assert!(converted.contains("path [\"what?\"];"));
    assert!(converted.contains("cursor -> endpoint.cursor;"));
    assert!(converted.contains("offset = 0;"));
    assert!(converted.contains("id \"edge.per_endpoint\";\n                key [origin, endpoint, \"tenant\" = tenant];\n                cost 2;"));
    assert!(converted.contains("key [\"tenant\" = tenant];\n                cost 1;"));
    assert!(converted.contains("rate_limit twin(tenant = tenant);"));
    assert!(converted.contains("rate_limit per_endpoint(tenant = braces.tenant) { cost 3; }"));
    assert!(converted.contains("allow 5 per 1s; allow 7 per 1s;"));
    assert_eq!(count(&outcome, Code::PathAdapterBraces), 1);
    let braces = outcome
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == Code::PathAdapterBraces)
        .expect("braces diagnostic");
    assert!(braces.suggestion.as_deref().is_some_and(|suggestion| {
        suggestion.starts_with("type PathAdapter = crate::Fixed<{ 1 + 1 }>;")
    }));
    assert_eq!(count(&outcome, Code::PathLiteral), 1);
    assert_eq!(count(&outcome, Code::PaginationCustomController), 1);
    assert_eq!(count(&outcome, Code::PaginationField), 1);
    assert_eq!(count(&outcome, Code::RateWindowDuplicate), 1);
    // v2.1 `api2url` versus v3 `api2_url`: every endpoint is pinned; the
    // endpoint-key policy alone would also require it.
    assert_eq!(count(&outcome, Code::IdPin), 9);
    assert!(
        errors(&outcome)
            .iter()
            .all(|error| !error.contains("C3-MIGRATE-INTERNAL"))
    );
    assert!(converted.contains(
        "GET placed(query a?: String) -> String {\n                id \"api2url.braces.placed\";\n            }"
    ));
    assert!(
        converted
            .contains("GET inline(query b?: String) -> String { id \"api2url.braces.inline\"; }")
    );
    assert!(converted.contains("GET literal() -> String {\n                id \"api2url.braces.literal\";\n                path"));
    assert!(converted.contains(
        "GET keyed() -> String {\n                id \"api2url.braces.keyed\";\n            }"
    ));
    let windows = outcome
        .identities
        .iter()
        .find(|identity| identity.v2_id == "api2url.braces.windows")
        .expect("identity");
    assert_eq!(windows.policies.len(), 3);
    assert_eq!(windows.policies[2].cost, 3, "named cost override is kept");
    assert_eq!(
        windows.policies[0].id, "edge.twin",
        "equal-number policies are not merged"
    );
    assert!(!converted.contains("__ConcordV"));
}

/// Caller terminals, runtime NonZero arguments and extension implementations
/// are rewritten only when exact, and otherwise diagnosed with a v3 spelling.
#[test]
fn caller_terminals_and_extension_contracts_are_migrated_or_diagnosed() {
    let source = r#"use std::num::{NonZeroU32, NonZeroUsize};
use concord::{PageLimit, api};
use concord::response::ResponseRepresentation;
use concord::rate_limit::{RateKeyPart, RateLimiter};

api! {
    client Pages {
        base "https://pages.example.test/";
        timeout 5s;
        credentials {
            credential session = endpoint login;
        }
        POST login(body input: String) -> String;
        GET list(offset: u64 = 0, count: NonZeroU32 = crate::PAGE) -> Vec<String> {
            query { "offset" = endpoint.offset; "count" = endpoint.count; }
            paginate via concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.count;
            }
        }
    }
}

pub struct Csv;

impl ResponseRepresentation<Vec<u8>> for Csv {
    fn accept() -> Option<&'static str> {
        Some("text/csv")
    }
}

pub struct Tenant;

impl RateKeyPart for Tenant {}

pub fn calls(client: Pages, n: u32, limit: PageLimit) {
    let first = client.list().paginate(PageLimit::Prefix(n));
    let dynamic = client.list().paginate(limit);
    let stored = client.login("user".to_owned()).store_as_session();
    let built = Pages::builder().configure_runtime(|runtime| {
        runtime
            .request_body_limit(NonZeroUsize::MIN.saturating_add(1024))
            .pagination_max_items(NonZeroUsize::new(4 + 4).unwrap())
    });
    let _ = (first, dynamic, stored, built);
}
"#;
    let outcome = convert(source);
    let converted = &outcome.converted;
    assert!(converted.contains("client.list().collect_prefix(n)"));
    assert!(converted.contains("client.list().paginate(limit)"));
    assert!(
        converted.contains(".request_body_limit(NonZeroUsize::MIN.saturating_add(1024).get())")
    );
    assert!(converted.contains(".pagination_max_items(NonZeroUsize::new(4 + 4).unwrap().get())"));
    assert!(
        converted.contains("use concord::{PageLimit, api};"),
        "PageLimit is still used"
    );
    assert_eq!(count(&outcome, Code::PaginatePrefix), 1);
    assert_eq!(count(&outcome, Code::PaginateDynamic), 1);
    assert_eq!(count(&outcome, Code::PageLimitReference), 1);
    assert_eq!(count(&outcome, Code::StoreOutcome), 1);
    assert_eq!(count(&outcome, Code::RuntimeNonZero), 2);
    assert_eq!(count(&outcome, Code::AcceptConst), 1);
    assert_eq!(count(&outcome, Code::RateKeyPart), 1);
    let accept = outcome
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == Code::AcceptConst)
        .expect("accept");
    assert_eq!(
        accept.suggestion.as_deref(),
        Some("const ACCEPT: Option<&'static str> = Some(\"text/csv\");")
    );
    assert!(codes(&outcome).contains(&"C3-MIGRATE-PAGINATION-INIT"));
    assert!(
        converted
            .contains("GET list(query offset: u64 = 0, query count: NonZeroU32 = crate::PAGE)")
    );

    let exact = source
        .replace("    let dynamic = client.list().paginate(limit);\n", "")
        .replace(
            "pub fn calls(client: Pages, n: u32, limit: PageLimit)",
            "pub fn calls(client: Pages, n: u32)",
        )
        .replace("(first, dynamic, stored, built)", "(first, stored, built)");
    let exact = convert(&exact);
    assert!(exact.converted.contains("use concord::{api};"));
    assert!(!exact.converted.contains("PageLimit"));
    assert_eq!(count(&exact, Code::PageLimitImport), 1);
}

/// Running the tool again on its own output changes nothing: converted
/// declarations are recognized as v3 and migrated caller code is stable.
#[test]
fn migration_output_is_a_fixed_point() {
    for name in ["catalog_contract_v2_1.rs.txt", "riot_v2_1.rs.txt"] {
        let contextual = |source: &str| {
            if name == "riot_v2_1.rs.txt" {
                convert_riot_with_original_context(source)
            } else {
                convert(source)
            }
        };
        let first = contextual(&fixture(name));
        let second = contextual(&first.converted);
        assert_eq!(second.converted, first.converted, "{name}");
        assert_eq!(second.edits, 0, "{name}");
        assert!(
            second
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != Code::ParseDeclaration),
            "{name}: {:?}",
            errors(&second)
        );
        assert_eq!(count(&second, Code::AlreadyV3), 1, "{name}");
    }
}

/// A declaration that already uses v3-only syntax is reported and left
/// unchanged instead of being re-parsed as v2.1.
#[test]
fn v3_declarations_are_left_unchanged() {
    let source = r#"use concord::api;
api! {
    client Catalog {
        base "https://catalog.example.test/v1/";
        values { query tenant: String; }
        GET item(id: String) -> String {
            path "items/{id}";
            absent on 404;
        }
    }
}
"#;
    let outcome = convert(source);
    assert_eq!(outcome.converted, source);
    assert_eq!(codes(&outcome), ["C3-MIGRATE-ALREADY-V3"]);
    assert!(errors(&outcome).is_empty());
}

/// §6.4 / MIGRATE-04: a key that carried a sensitive value stays redacted in
/// v3 after a non-sensitive override; the tool reports it as a behavior change.
#[test]
fn sticky_sensitivity_is_reported_as_a_behavior_change() {
    let source = r#"use concord::api;
api! {
    client Sticky {
        base "https://sticky.example.test/";
        timeout 5s;
        values {
            #[sensitive]
            tenant: String;
        }
        query { "tenant" = client.tenant; }
        scope open(label: String) {
            query replace { "tenant" = open.label; }
            GET read() -> String;
        }
    }
}
"#;
    let outcome = convert(source);
    assert_eq!(count(&outcome, Code::SensitivitySticky), 1);
    assert!(
        outcome
            .converted
            .contains("scope open(query(\"tenant\") label: String) {")
    );
    assert!(outcome.converted.contains("query replace { }"));
}

/// §4.2 / MIGRATE-04: only root `#[default]` values change evaluation timing
/// (v2.1 lazily in `build()`, v3 once at builder creation), so the tool
/// reports exactly those, as a behavior change, and leaves their source
/// alone. Root `= VALUE` initializers run no application code, and scope and
/// endpoint defaults already evaluated once at selection/construction in
/// v2.1. `tools/xtask/tests/migrated_v3.rs` observes the timing itself.
#[test]
fn root_default_timing_is_reported_as_a_behavior_change() {
    let source = r#"use concord::api;
api! {
    client Timing {
        base "https://timing.example.test/";
        timeout 5s;
        values {
            #[default]
            version: Version;
            trace: String = "root";
            #[default]
            revision: Revision;
        }
        headers {
            "X-Version" = client.version;
            "X-Trace" = client.trace;
            "X-Revision" = client.revision;
        }
        scope project(#[default] depth: u32) {
            query { "depth" = project.depth; }
            GET read(#[default] page: u32) -> String {
                query { "page" = endpoint.page; }
            }
        }
    }
}
"#;
    let outcome = convert(source);
    assert!(errors(&outcome).is_empty(), "{:?}", errors(&outcome));
    assert_eq!(count(&outcome, Code::RootDefaultTiming), 2);
    let timing = outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == Code::RootDefaultTiming)
        .collect::<Vec<_>>();
    for (diagnostic, name) in timing.iter().zip(["version", "revision"]) {
        assert_eq!(diagnostic.severity(), Severity::Warning);
        assert!(!diagnostic.code.automated());
        let line = source
            .lines()
            .position(|line| line.trim().starts_with(&format!("{name}: ")))
            .expect("value line")
            + 1;
        assert_eq!(diagnostic.line, line, "{name}");
        assert!(
            diagnostic
                .message
                .contains(&format!("root value `{name}` of client `Timing`"))
        );
        assert!(diagnostic.message.contains("inside `build()`"));
        assert!(
            diagnostic
                .message
                .contains("exactly once when `builder()`/`new()` creates the builder")
        );
        assert!(
            diagnostic
                .message
                .contains(&format!("`reset_{name}` restores that retained payload"))
        );
        assert!(
            diagnostic.suggestion.is_none(),
            "no source change is proposed"
        );
    }
    // The values keep `#[default]`; only the mechanical direct placement of
    // their introductions changes, and scope/endpoint defaults are unchanged.
    for kept in [
        "#[default]\n            header(\"X-Version\") version: Version;",
        "header(\"X-Trace\") trace: String = \"root\";",
        "#[default]\n            header(\"X-Revision\") revision: Revision;",
        "scope project(#[default] query depth: u32) {",
        "GET read(#[default] query page: u32) -> String {",
    ] {
        assert!(
            outcome.converted.contains(kept),
            "{kept}\n{}",
            outcome.converted
        );
    }
}

/// Setters inside expression-list macros and fully qualified setter calls are
/// migrated through the same resolution; a setter inside a macro body that is
/// not an expression list is reported instead of being silently missed.
#[test]
fn setters_inside_macros_and_qualified_calls_are_not_missed() {
    let source = r#"use concord::api;

api! {
    client Macros {
        base "https://macros.example.test/";
        timeout 5s;
        GET read(flag?: Option<bool>) -> String {
            query { "flag" = endpoint.flag; }
        }
    }
}

pub fn calls(client: Macros) {
    assert!(client.read().set_flag(None).plan().is_ok());
    let _ = macros::ReadRequest::set_flag(client.read(), Some(None));
    let _ = vec![client.read().set_flag(None); 1];
}
"#;
    let outcome = convert(source);
    let converted = &outcome.converted;
    assert!(converted.contains("assert!(client.read().replace_flag(None).plan().is_ok());"));
    assert!(converted.contains("macros::ReadRequest::replace_flag(client.read(), Some(None))"));
    assert!(converted.contains("vec![client.read().set_flag(None); 1]"));
    assert_eq!(count(&outcome, Code::SetterReplace), 2);
    assert_eq!(count(&outcome, Code::SetterUnresolved), 1);
}

/// Captured v2.1 fixtures and their crate-relative place in the compile-through
/// tree `tests/migrated_v3/`.
const COMPILED_FIXTURES: &[(&str, &str)] = &[
    ("riot_v2_1.rs.txt", "riot/api/riot.rs"),
    ("data_dragon_v2_1.rs.txt", "riot/api/data_dragon.rs"),
    ("riot_usage_v2_1.rs.txt", "riot/usage.rs"),
    ("representative_v2_1.rs.txt", "representative.rs"),
];

fn golden_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/migrated_v3")
}

fn golden_files(directory: &Path, output: &mut Vec<String>) {
    let mut entries = fs::read_dir(directory)
        .expect("golden directory")
        .map(|entry| entry.expect("golden entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            golden_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(
                path.strip_prefix(golden_root())
                    .expect("inside the golden root")
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}

/// V3-O04: `tests/migrated_v3/` is exactly the migrate-v3 output of one v2.1
/// crate made of the captured fixtures plus unchanged support modules, and
/// `tests/migrated_v3.rs` compiles that output through the real v3
/// `concord::api!`. This test pins the tool output to the compiled text, so
/// the compile-through evidence cannot drift from what the tool writes. The
/// crate root declares the modules as `tests/migrated_v3.rs` does (second
/// post-fix review of F03: a crate whose root is not visible has unknown
/// trait implementations, so navigation on client values would be unproved).
///
/// Regenerate after an intentional tool change with
/// `CONCORD_MIGRATE_BLESS=1 cargo test -p concord-xtask golden`, then review
/// the diff and rerun the compile-through test.
#[test]
fn golden_outputs_are_the_exact_tool_output() {
    let workspace = TempDir::new("golden");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"migrated\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    let mut expected = Vec::new();
    golden_files(&golden_root(), &mut expected);
    let targets = COMPILED_FIXTURES
        .iter()
        .map(|(_, target)| *target)
        .collect::<Vec<_>>();
    for target in &targets {
        assert!(
            expected.iter().any(|file| file == target),
            "missing golden {target}; bless it first"
        );
    }
    // Support modules are v2.1 input too: the migration must leave them as-is.
    for file in expected
        .iter()
        .filter(|file| !targets.contains(&file.as_str()))
    {
        let text = fs::read_to_string(golden_root().join(file)).expect("support module");
        workspace.write(&format!("src/{file}"), &text);
    }
    for (fixture_name, target) in COMPILED_FIXTURES {
        workspace.write(&format!("src/{target}"), &fixture(fixture_name));
    }
    let root = "pub mod representative;\npub mod riot;\n";
    workspace.write("src/lib.rs", root);
    let migration = migrate(&workspace.0.join("src"), &Settings::default()).expect("migration");
    assert_eq!(migration.converted("lib.rs"), Some(root));
    assert!(
        migration
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity() != Severity::Error),
        "the compiled crate has no unautomatable site: {:?}",
        migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity() == Severity::Error)
            .map(super::report::Diagnostic::render)
            .collect::<Vec<_>>()
    );
    // Every golden file, plus the crate root.
    assert_eq!(migration.files.len(), expected.len() + 1);
    let bless = std::env::var_os("CONCORD_MIGRATE_BLESS").is_some();
    for file in &expected {
        let converted = migration.converted(file).expect("migrated file");
        let path = golden_root().join(file);
        if bless {
            fs::write(&path, converted).expect("bless golden");
            continue;
        }
        let golden = fs::read_to_string(&path).expect("golden");
        assert!(
            golden == converted,
            "tests/migrated_v3/{file} differs from the migrate-v3 output:\n{}",
            super::edits::unified_diff(file, &golden, converted)
        );
        if !targets.contains(&file.as_str()) {
            let original = &migration
                .files
                .iter()
                .find(|outcome| outcome.relative == *file)
                .expect("outcome")
                .original;
            assert_eq!(original, converted, "{file}: support module unchanged");
        }
    }
}

/// V3-O04 / MIGRATE-01 (Riot usage): the same setter name is migrated per
/// resolved owner across files of one crate. `set_count(Some(5))` targets the
/// optional `count?: u32` of `top_masteries_by_puuid` and becomes
/// `replace_count`; `set_count(NonZeroU32..)` targets the required initialized
/// `count` of `match_ids_by_puuid` and is unchanged.
///
/// The crate includes `riot/api/mod.rs` (the v2.1 module that re-exports
/// `RiotApi` and `riot_api`), because `usage.rs` names the client through
/// `super::api::RiotApi` and the receiver is proved through that exact path.
/// It also has its crate root and the `riot` and `riot::domain` modules
/// (second post-fix review of F03): a navigation on a client value reaches
/// the generated `&self` method only after an auto-ref step, which is proved
/// only when the whole crate (its trait implementations, and the `domain`
/// import of `usage.rs`) is visible to the tool.
#[test]
fn riot_usage_setters_resolve_per_owner_across_the_crate() {
    let workspace = TempDir::new("riot-usage");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"riot\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    workspace.write("src/lib.rs", "pub mod riot;\n");
    for support in [
        "riot/mod.rs",
        "riot/domain/mod.rs",
        "riot/domain/account.rs",
        "riot/domain/data_dragon.rs",
        "riot/domain/league.rs",
        "riot/domain/match_api.rs",
        "riot/domain/routing.rs",
        "riot/domain/status.rs",
        "riot/domain/tournament.rs",
    ] {
        workspace.write(
            &format!("src/{support}"),
            &fs::read_to_string(golden_root().join(support)).expect("riot support module"),
        );
    }
    workspace.write(
        "src/riot/api/mod.rs",
        &fs::read_to_string(golden_root().join("riot/api/mod.rs")).expect("riot/api/mod.rs"),
    );
    workspace.write("src/riot/api/riot.rs", &fixture("riot_v2_1.rs.txt"));
    workspace.write(
        "src/riot/api/data_dragon.rs",
        &fixture("data_dragon_v2_1.rs.txt"),
    );
    workspace.write("src/riot/usage.rs", &fixture("riot_usage_v2_1.rs.txt"));
    let migration = migrate(&workspace.0.join("src/riot/usage.rs"), &Settings::default())
        .expect("single-file migration indexes the whole crate");
    let usage = migration.converted("usage.rs").expect("usage");
    assert!(usage.contains(".replace_count(Some(5))"));
    assert!(usage.contains(".set_count(NonZeroU32::new(10).expect(\"positive page size\"))"));
    assert!(usage.contains(".set_count(NonZeroU32::new(20).expect(\"20 is non-zero\"))"));
    assert_eq!(
        usage
            .matches(".replace_match_type(Some(MatchType::Ranked))")
            .count(),
        2
    );
    assert_eq!(usage.matches(".collect_prefix(2)").count(), 2);
    assert!(!usage.contains("PageLimit"));
    assert!(usage.contains("// ANCHOR: riot_paginated_request"));
    // The unawaited terminal keeps its type annotation, now the selected v3
    // terminal type; the awaited prefix keeps its v2.1 `Vec<String>`.
    assert!(usage.contains("let _: concord::pagination::CollectPrefix<_> = riot"));
    assert!(!usage.contains("PaginatedRequest"));
    assert!(usage.contains("        .collect_prefix(2)\n        .await?.into_items();"));
    assert!(
        migration
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity() != Severity::Error)
    );
    let count = |code| {
        migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == code)
            .count()
    };
    assert_eq!(count(Code::PaginatePrefix), 1, "the stored terminal");
    assert_eq!(count(Code::PaginatePrefixItems), 1, "the awaited prefix");
    assert_eq!(count(Code::PaginatedRequestType), 1);
}

// Independent review F03. Regression additions: NOT compiled by the reviewer.
// Parsing with syn is not proof of receiver identity after dropping qualifiers.
#[test]
fn review_f03_qualified_unrelated_request_type_is_not_renamed() {
    let source = r#"use concord::{api, Empty};
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
mod other {
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
}
pub fn unrelated(foreign: other::ItemRequest) -> u32 { foreign.set_x(None) }
pub fn genuine(local: api::ItemRequest) { let _ = local.set_x(None); }
"#;
    let outcome = convert(source);
    assert!(
        outcome.converted.contains("foreign.set_x(None)"),
        "a qualified unrelated receiver must not be rewritten: {}",
        outcome.converted
    );
    assert!(
        outcome.converted.contains("local.replace_x(None)"),
        "the genuine generated receiver should migrate"
    );
}

#[test]
fn review_f03_same_field_name_in_another_struct_is_not_receiver_proof() {
    let source = r#"use concord::{api, Empty};
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
struct Real { request: api::ItemRequest }
struct Other { request: Tracker }
struct Tracker;
impl Tracker {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
fn unrelated(other: Other) -> u32 { other.request.set_x(None) }
"#;
    let outcome = convert(source);
    assert!(
        outcome.converted.contains("other.request.set_x(None)"),
        "a field name found on an unrelated struct is not proof of ownership: {}",
        outcome.converted
    );
}

// ----- receiver identity (review F03, V3-O04) --------------------------------

/// The reviewer's declaration (`x` is Concord-optional on `api::ItemRequest`)
/// and an unrelated `other::ItemRequest` that offers both setter names, so a
/// wrong rename still compiles and changes the result (1 -> 2).
const IDENTITY_API: &str = r#"use concord::{api, Empty};
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
pub mod other {
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
}
pub trait Setter { fn set_x(self, value: Option<u32>) -> u32; }
// callers
"#;

/// Migrates `IDENTITY_API` followed by `callers` and checks the complete
/// resulting caller text: exactly the `(line, true)` sites are renamed to
/// `replace_x` (each with one `SETTER-REPLACE` note on its line), every
/// `(line, false)` site keeps its original `set_x` (each with one
/// `SETTER-UNRESOLVED` error on its line), and nothing else changes.
fn assert_receivers(callers: &str, sites: &[(&str, bool)]) {
    let source = format!("{IDENTITY_API}{callers}");
    let outcome = convert(&source);
    let (_, tail) = outcome
        .converted
        .split_once("// callers\n")
        .expect("caller marker");
    let mut expected = callers.to_owned();
    for (line, renamed) in sites {
        assert_eq!(callers.matches(line).count(), 1, "ambiguous site `{line}`");
        assert_eq!(
            line.matches("set_x(").count(),
            1,
            "one call per site `{line}`"
        );
        if *renamed {
            expected = expected.replace(line, &line.replace("set_x(", "replace_x("));
        }
    }
    assert!(
        tail == expected,
        "caller text differs from the expected identity decisions:\n{}",
        super::edits::unified_diff("callers", &expected, tail)
    );
    let line_of = |fragment: &str| {
        source
            .lines()
            .position(|line| line.contains(fragment))
            .expect("site line")
            + 1
    };
    let lines = |code: Code| {
        let mut lines = outcome
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == code)
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    let expected_lines = |renamed: bool| {
        let mut lines = sites
            .iter()
            .filter(|site| site.1 == renamed)
            .map(|site| line_of(site.0))
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    assert_eq!(
        lines(Code::SetterReplace),
        expected_lines(true),
        "renamed sites"
    );
    assert_eq!(
        lines(Code::SetterUnresolved),
        expected_lines(false),
        "unproved sites are reported: {:#?}",
        outcome.diagnostics
    );
    for diagnostic in &outcome.diagnostics {
        if diagnostic.code == Code::SetterUnresolved {
            assert!(diagnostic.message.contains("left unchanged"));
            assert!(
                diagnostic
                    .suggestion
                    .as_deref()
                    .is_some_and(|suggestion| suggestion.contains("replace_x(")),
                "an actionable suggestion: {diagnostic:?}"
            );
        }
    }
}

/// Review F03 / V3-O04: `use ... as` renames, module aliases, type aliases
/// and globs are followed to the item they name. They prove the generated
/// request when they point at it, and never when they point at another type
/// with the same leaf name (a local item also shadows a glob).
#[test]
fn receiver_identity_follows_imports_aliases_and_globs() {
    assert_receivers(
        r#"use self::api::ItemRequest as Req;
use crate::api as generated;
use self::other::ItemRequest as ForeignReq;
type Alias = api::ItemRequest;
type ForeignAlias = other::ItemRequest;
mod globbed {
    use super::api::*;
    pub fn glob(r: ItemRequest) { let _ = r.set_x(None); }
    pub fn glob_boxed(r: Box<ItemRequest>) { let _ = r.set_x(None); }
}
mod imported_foreign {
    use super::other::ItemRequest;
    pub fn foreign_import(r: ItemRequest) -> u32 { r.set_x(None) }
}
mod glob_shadowed {
    use super::api::*;
    pub struct ItemRequest;
    impl ItemRequest { pub fn set_x(self, _: Option<u32>) -> u32 { 1 } }
    pub fn local_over_glob(r: ItemRequest) -> u32 { r.set_x(None) }
}
pub fn renamed(r: Req) { let _ = r.set_x(None); }
pub fn module_alias(r: generated::ItemRequest) { let _ = r.set_x(None); }
pub fn type_alias(r: Alias) { let _ = r.set_x(None); }
pub fn renamed_foreign(r: ForeignReq) -> u32 { r.set_x(None) }
pub fn foreign_type_alias(r: ForeignAlias) -> u32 { r.set_x(None) }
"#,
        &[
            (
                "pub fn glob(r: ItemRequest) { let _ = r.set_x(None); }",
                true,
            ),
            // A glob of the generated module does not hide other names.
            (
                "pub fn glob_boxed(r: Box<ItemRequest>) { let _ = r.set_x(None); }",
                true,
            ),
            (
                "pub fn foreign_import(r: ItemRequest) -> u32 { r.set_x(None) }",
                false,
            ),
            (
                "pub fn local_over_glob(r: ItemRequest) -> u32 { r.set_x(None) }",
                false,
            ),
            ("pub fn renamed(r: Req) { let _ = r.set_x(None); }", true),
            (
                "pub fn module_alias(r: generated::ItemRequest) { let _ = r.set_x(None); }",
                true,
            ),
            (
                "pub fn type_alias(r: Alias) { let _ = r.set_x(None); }",
                true,
            ),
            (
                "pub fn renamed_foreign(r: ForeignReq) -> u32 { r.set_x(None) }",
                false,
            ),
            (
                "pub fn foreign_type_alias(r: ForeignAlias) -> u32 { r.set_x(None) }",
                false,
            ),
        ],
    );
}

/// Review F03 / V3-O04: same-leaf types are distinguished by their complete
/// path (`self::`, `super::`, `crate::`, another crate) and through standard
/// wrappers; only the generated `api::ItemRequest` is renamed.
#[test]
fn receiver_identity_distinguishes_same_leaf_qualified_types() {
    assert_receivers(
        r#"mod nested {
    pub fn parent_generated(r: super::api::ItemRequest) { let _ = r.set_x(None); }
    pub fn parent_foreign(r: super::other::ItemRequest) -> u32 { r.set_x(None) }
    pub fn crate_generated(r: crate::api::ItemRequest) { let _ = r.set_x(None); }
    pub fn crate_foreign(r: crate::other::ItemRequest) -> u32 { r.set_x(None) }
}
pub fn self_generated(r: self::api::ItemRequest) { let _ = r.set_x(None); }
pub fn self_foreign(r: self::other::ItemRequest) -> u32 { r.set_x(None) }
pub fn extern_foreign(r: other_crate::api::ItemRequest) -> u32 { r.set_x(None) }
pub fn boxed_generated(r: Box<api::ItemRequest>) { let _ = r.set_x(None); }
pub fn wrapped_foreign(r: Option<other::ItemRequest>) -> u32 { r.expect("present").set_x(None) }
"#,
        &[
            (
                "pub fn parent_generated(r: super::api::ItemRequest) { let _ = r.set_x(None); }",
                true,
            ),
            (
                "pub fn parent_foreign(r: super::other::ItemRequest) -> u32 { r.set_x(None) }",
                false,
            ),
            (
                "pub fn crate_generated(r: crate::api::ItemRequest) { let _ = r.set_x(None); }",
                true,
            ),
            (
                "pub fn crate_foreign(r: crate::other::ItemRequest) -> u32 { r.set_x(None) }",
                false,
            ),
            (
                "pub fn self_generated(r: self::api::ItemRequest) { let _ = r.set_x(None); }",
                true,
            ),
            (
                "pub fn self_foreign(r: self::other::ItemRequest) -> u32 { r.set_x(None) }",
                false,
            ),
            (
                "pub fn extern_foreign(r: other_crate::api::ItemRequest) -> u32 { r.set_x(None) }",
                false,
            ),
            (
                "pub fn boxed_generated(r: Box<api::ItemRequest>) { let _ = r.set_x(None); }",
                true,
            ),
            ("r.expect(\"present\").set_x(None)", false),
        ],
    );
}

/// Review F03 / V3-O04: a field is typed through the struct that owns it
/// (`self`, a typed binding, a struct literal, a tuple field), never by a
/// member-name union across structs. A generic field's substitution is not
/// modeled, so it is reported rather than guessed.
#[test]
fn receiver_identity_types_fields_through_their_owning_struct() {
    assert_receivers(
        r#"pub struct Real { pub request: api::ItemRequest }
pub struct Other { pub request: other::ItemRequest }
pub struct Wrapped(api::ItemRequest);
pub struct Generic<T> { pub request: T }
impl Real {
    pub fn by_self(self) { let _ = self.request.set_x(None); }
}
impl Other {
    pub fn foreign_by_self(self) -> u32 { self.request.set_x(None) }
}
pub fn real_field(real: Real) { let _ = real.request.set_x(None); }
pub fn other_field(other: Other) -> u32 { other.request.set_x(None) }
pub fn tuple_field(wrapped: Wrapped) { let _ = wrapped.0.set_x(None); }
pub fn literal_field(r: api::ItemRequest) { let _ = Real { request: r }.request.set_x(None); }
pub fn generic_field(g: Generic<api::ItemRequest>) { let _ = g.request.set_x(None); }
"#,
        &[
            (
                "pub fn by_self(self) { let _ = self.request.set_x(None); }",
                true,
            ),
            (
                "pub fn foreign_by_self(self) -> u32 { self.request.set_x(None) }",
                false,
            ),
            (
                "pub fn real_field(real: Real) { let _ = real.request.set_x(None); }",
                true,
            ),
            (
                "pub fn other_field(other: Other) -> u32 { other.request.set_x(None) }",
                false,
            ),
            (
                "pub fn tuple_field(wrapped: Wrapped) { let _ = wrapped.0.set_x(None); }",
                true,
            ),
            ("Real { request: r }.request.set_x(None)", true),
            (
                "pub fn generic_field(g: Generic<api::ItemRequest>) { let _ = g.request.set_x(None); }",
                false,
            ),
        ],
    );
}

/// Review F03 / V3-O04: a call is typed through the function its path
/// actually names. Block-local functions, block-local imports, local
/// closures, generic parameters, block-local items, pattern bindings and
/// binding-introducing macros shadow exactly as in Rust.
#[test]
fn receiver_identity_respects_shadowed_functions_and_bindings() {
    assert_receivers(
        r#"use self::api::ItemRequest;
macro_rules! rebind { ($name:ident) => { let $name = other::ItemRequest; }; }
pub fn make() -> api::ItemRequest { Api::new().expect("client").item() }
mod a { pub fn make() -> super::api::ItemRequest { super::make() } }
mod b { pub fn make() -> super::other::ItemRequest { super::other::ItemRequest } }
pub fn module_function() { let _ = make().set_x(None); }
pub fn qualified_function() { let _ = a::make().set_x(None); }
pub fn qualified_foreign_function() -> u32 { b::make().set_x(None) }
pub fn local_function_shadows() -> u32 {
    fn make() -> other::ItemRequest { other::ItemRequest }
    make().set_x(None) // local fn
}
pub fn local_import_shadows() -> u32 {
    use self::b::make;
    make().set_x(None) // local use
}
pub fn closure_shadows() -> u32 {
    let make = || other::ItemRequest;
    make().set_x(None) // closure
}
pub fn generic_parameter<ItemRequest: Setter>(r: ItemRequest) -> u32 { r.set_x(None) }
pub fn block_item_shadows() -> u32 {
    struct ItemRequest;
    impl ItemRequest { fn set_x(self, _: Option<u32>) -> u32 { 1 } }
    let local: ItemRequest = ItemRequest;
    local.set_x(None) // block item
}
pub fn pattern_shadows(request: ItemRequest, foreign: Option<other::ItemRequest>) -> u32 {
    if let Some(request) = foreign {
        return request.set_x(None); // if let
    }
    match Some(other::ItemRequest) {
        Some(request) => { let _ = request.set_x(None); } // match arm
        None => {}
    }
    let _ = request.set_x(None); // parameter
    0
}
pub fn macro_rebinds(request: ItemRequest) -> u32 {
    rebind!(request);
    request.set_x(None) // after a binding macro
}
pub fn expression_macro_keeps(request: ItemRequest) {
    println!("before");
    let _ = request.set_x(None); // after println
}
"#,
        &[
            (
                "pub fn module_function() { let _ = make().set_x(None); }",
                true,
            ),
            (
                "pub fn qualified_function() { let _ = a::make().set_x(None); }",
                true,
            ),
            (
                "pub fn qualified_foreign_function() -> u32 { b::make().set_x(None) }",
                false,
            ),
            ("make().set_x(None) // local fn", false),
            ("make().set_x(None) // local use", false),
            ("make().set_x(None) // closure", false),
            (
                "pub fn generic_parameter<ItemRequest: Setter>(r: ItemRequest) -> u32 { r.set_x(None) }",
                false,
            ),
            ("local.set_x(None) // block item", false),
            ("return request.set_x(None); // if let", false),
            (
                "Some(request) => { let _ = request.set_x(None); } // match arm",
                false,
            ),
            ("let _ = request.set_x(None); // parameter", true),
            ("request.set_x(None) // after a binding macro", false),
            ("let _ = request.set_x(None); // after println", true),
        ],
    );
}

/// Review F03 / V3-O04: fully qualified setter calls and constructor calls
/// resolve their type path like any other path. A trait-qualified call names
/// the trait's method and is never proved; an unrelated `Api` with the same
/// constructor and child names is not the generated client.
#[test]
fn receiver_identity_resolves_qualified_ufcs_and_constructors() {
    assert_receivers(
        r#"mod other_client {
    #[derive(Default)]
    pub struct Api;
    impl Api {
        pub fn new() -> Result<Self, ()> { Ok(Self) }
        pub fn item(&self) -> super::other::ItemRequest { super::other::ItemRequest }
    }
}
impl Setter for other::ItemRequest { fn set_x(self, _: Option<u32>) -> u32 { 3 } }
pub fn ufcs_generated(r: api::ItemRequest) { let _ = api::ItemRequest::set_x(r, None); }
pub fn ufcs_crate_generated(r: api::ItemRequest) { let _ = crate::api::ItemRequest::set_x(r, None); }
pub fn ufcs_qself_generated(r: api::ItemRequest) { let _ = <api::ItemRequest>::set_x(r, None); }
pub fn ufcs_foreign(r: other::ItemRequest) -> u32 { other::ItemRequest::set_x(r, None) }
pub fn ufcs_qself_foreign(r: other::ItemRequest) -> u32 { <other::ItemRequest>::set_x(r, None) }
pub fn ufcs_trait(r: other::ItemRequest) -> u32 { <other::ItemRequest as Setter>::set_x(r, None) }
pub fn constructed_generated() { let _ = Api::new().expect("client").item().set_x(None); }
pub fn constructed_foreign() -> u32 { other_client::Api::new().expect("client").item().set_x(None) }
pub fn unbound_generated() { let _ = Api::unbound().item().set_x(None); }
"#,
        &[
            ("let _ = api::ItemRequest::set_x(r, None);", true),
            ("let _ = crate::api::ItemRequest::set_x(r, None);", true),
            ("let _ = <api::ItemRequest>::set_x(r, None);", true),
            ("-> u32 { other::ItemRequest::set_x(r, None) }", false),
            ("-> u32 { <other::ItemRequest>::set_x(r, None) }", false),
            ("<other::ItemRequest as Setter>::set_x(r, None)", false),
            (
                "pub fn constructed_generated() { let _ = Api::new().expect(\"client\").item().set_x(None); }",
                true,
            ),
            (
                "other_client::Api::new().expect(\"client\").item().set_x(None)",
                false,
            ),
            (
                "pub fn unbound_generated() { let _ = Api::unbound().item().set_x(None); }",
                true,
            ),
        ],
    );
}

/// Review F03 / V3-O04 (crate-wide): module paths across files follow `mod`
/// declarations (including `#[path]`) and `use` imports; another crate of the
/// package (an integration test) reaches the library through its crate name.
/// An unrelated module that is also named `api` and also defines
/// `ItemRequest` is never taken for the generated support module.
#[test]
fn receiver_identity_follows_module_paths_across_the_package() {
    let workspace = TempDir::new("identity");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"identity-crate\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    workspace.write(
        "src/lib.rs",
        "pub mod declarations;\npub mod callers;\npub mod unrelated;\n#[path = \"elsewhere/renamed.rs\"]\npub mod renamed;\n",
    );
    let declarations = IDENTITY_API
        .replace("pub mod other {", "mod unused_other {")
        .replace("// callers\n", "");
    workspace.write("src/declarations.rs", &declarations);
    let unrelated = "pub mod api {\n    pub struct ItemRequest;\n    impl ItemRequest {\n        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }\n        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }\n    }\n}\n";
    workspace.write("src/unrelated/mod.rs", unrelated);
    let callers = "use crate::declarations::api;\nuse crate::unrelated::api as unrelated_api;\n\npub fn genuine(r: api::ItemRequest) { let _ = r.set_x(None); }\npub fn foreign(r: unrelated_api::ItemRequest) -> u32 { r.set_x(None) }\npub fn qualified(r: super::unrelated::api::ItemRequest) -> u32 { r.set_x(None) }\n";
    workspace.write("src/callers.rs", callers);
    let renamed = "use super::declarations::api;\n\npub fn via_path_attribute(r: api::ItemRequest) { let _ = r.set_x(None); }\n";
    workspace.write("src/elsewhere/renamed.rs", renamed);
    let integration = "use identity_crate::declarations::api;\n\npub fn external_genuine(r: api::ItemRequest) { let _ = r.set_x(None); }\npub fn external_foreign(r: identity_crate::unrelated::api::ItemRequest) -> u32 { r.set_x(None) }\n";
    workspace.write("tests/integration.rs", integration);

    let migration = migrate(&workspace.0, &Settings::default()).expect("migration");
    let renamed_sites = |text: &str| {
        text.replace(
            "{ let _ = r.set_x(None); }",
            "{ let _ = r.replace_x(None); }",
        )
    };
    assert_eq!(
        migration.converted("src/callers.rs"),
        Some(renamed_sites(callers).as_str())
    );
    assert_eq!(
        migration.converted("src/elsewhere/renamed.rs"),
        Some(renamed_sites(renamed).as_str())
    );
    assert_eq!(
        migration.converted("tests/integration.rs"),
        Some(renamed_sites(integration).as_str())
    );
    assert_eq!(migration.converted("src/unrelated/mod.rs"), Some(unrelated));
    let sites = |code: Code| {
        migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == code)
            .map(|diagnostic| (diagnostic.file.as_str(), diagnostic.line))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sites(Code::SetterReplace),
        [
            ("src/callers.rs", 4),
            ("src/elsewhere/renamed.rs", 3),
            ("tests/integration.rs", 3),
        ]
    );
    assert_eq!(
        sites(Code::SetterUnresolved),
        [
            ("src/callers.rs", 5),
            ("src/callers.rs", 6),
            ("tests/integration.rs", 4),
        ]
    );
}

/// The compile-through receiver-identity fixture and its golden output.
const IDENTITY_FIXTURE: &str = "receiver_identity_v2_1.rs.txt";
const IDENTITY_GOLDEN: &str = "tests/migrated_v3_identity/receiver_identity.rs";

/// Review F03 / V3-O04: `tests/migrated_v3_identity/receiver_identity.rs` is
/// exactly the migrate-v3 output of the captured fixture, migrated as a
/// module of a crate; `tests/migrated_v3.rs` compiles that output through the
/// real v3 `concord::api!` and checks that every unrelated receiver (each
/// offering both `set_x` and `replace_x`) keeps its original result while
/// the genuine generated calls keep the v2.1 exact-replacement meaning. The
/// unrelated sites are exactly the reported unautomatable ones.
///
/// Regenerate after an intentional tool change with
/// `CONCORD_MIGRATE_BLESS=1 cargo test -p concord-xtask receiver_identity`.
#[test]
fn receiver_identity_golden_output_is_the_exact_tool_output() {
    let workspace = TempDir::new("identity-golden");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"identity\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    workspace.write("src/lib.rs", "pub mod receiver_identity;\n");
    let source = fixture(IDENTITY_FIXTURE);
    workspace.write("src/receiver_identity.rs", &source);
    let migration = migrate(&workspace.0.join("src"), &Settings::default()).expect("migration");
    let converted = migration
        .converted("receiver_identity.rs")
        .expect("migrated fixture");
    assert_eq!(
        migration.converted("lib.rs"),
        Some("pub mod receiver_identity;\n")
    );
    let line_of = |marker: &str| {
        source
            .lines()
            .position(|line| line.contains(marker))
            .unwrap_or_else(|| panic!("marker {marker}"))
            + 1
    };
    let sites = |code: Code| {
        let mut lines = migration
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == code)
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    let unrelated = [
        "// unrelated: qualified",
        "// unrelated: alias",
        "// unrelated: field",
        "// unrelated: ufcs",
        "// unrelated: constructor",
        "// unrelated: imported function",
        "// unrelated: local function",
        // Post-fix review of F03.
        "// unrelated: option extension",
        "// unrelated: arc extension",
        "// unrelated: reference extension",
        "// unrelated: builder helper",
        "// unrelated: request helper",
        "// unrelated: pattern macro let",
        "// unrelated: pattern macro arm",
        "// unrelated: statement macro",
    ]
    .map(line_of);
    assert_eq!(
        sites(Code::SetterUnresolved),
        unrelated,
        "{:#?}",
        migration.diagnostics
    );
    let mut genuine = [
        "// genuine: chained",
        "// genuine: alias",
        "// genuine: field",
        "// genuine: function",
        "// genuine: ufcs",
        "// genuine: constructor",
        // Post-fix review of F03.
        "// genuine: builder",
        "// genuine: boxed",
        "// genuine: unwrap",
    ]
    .map(line_of)
    .to_vec();
    // The chained line carries two setters.
    genuine.push(line_of("// genuine: chained"));
    genuine.sort_unstable();
    assert_eq!(sites(Code::SetterReplace), genuine);
    // The v2.1 crate-level builder named by the user helper moves into the
    // generated support module (by code name: the code is new in the repair).
    let builder_paths = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == "C3-MIGRATE-BUILDER-PATH")
        .map(|diagnostic| diagnostic.line)
        .collect::<Vec<_>>();
    assert_eq!(builder_paths, [line_of("impl ApiBuilder {")]);
    let errors = migration
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(super::report::Diagnostic::render)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), unrelated.len(), "{errors:#?}");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(IDENTITY_GOLDEN);
    if std::env::var_os("CONCORD_MIGRATE_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("golden directory")).expect("golden directory");
        fs::write(&path, converted).expect("bless golden");
        return;
    }
    let golden = fs::read_to_string(&path).expect("golden");
    assert!(
        golden == converted,
        "{IDENTITY_GOLDEN} differs from the migrate-v3 output:\n{}",
        super::edits::unified_diff(IDENTITY_GOLDEN, &golden, converted)
    );
}

// ----- paginate receiver identity (review F03 follow-up, V3-O04) -------------

/// A paginated v2.1 declaration, an unrelated repository whose `paginate`
/// takes Concord's `PageLimit`, and a module with its own `PageLimit` and
/// `PaginatedRequest` types. Both unrelated types also offer
/// `collect_prefix`, so a wrong rewrite compiles.
const PAGINATE_API: &str = r#"use std::num::NonZeroU32;
use concord::{PageLimit, api};
use concord::pagination::PaginatedRequest;

api! {
    client Pages {
        base "https://pages.example.test/";
        timeout 5s;
        GET list(offset: u64 = 0, count: NonZeroU32 = crate::PAGE) -> Vec<String> {
            query { "offset" = endpoint.offset; "count" = endpoint.count; }
            paginate via concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.count;
            }
        }
    }
}

pub mod repo {
    pub struct MyRepo;
    impl MyRepo {
        pub fn paginate(&self, _: concord::PageLimit) -> u32 { 1 }
        pub fn collect_prefix(&self, _: u32) -> u32 { 2 }
    }
}

pub mod local {
    pub enum PageLimit { Prefix(u32), CompleteWithin(u32) }
    pub struct PaginatedRequest<T>(pub T);
    pub struct Store;
    impl Store {
        pub fn paginate(&self, _: PageLimit) -> PaginatedRequest<u32> { PaginatedRequest(1) }
        pub fn collect_prefix(&self, _: u32) -> PaginatedRequest<u32> { PaginatedRequest(2) }
    }
}
// callers
"#;

/// Lines of the diagnostics with one code, by the code's name (so these
/// tests also compile, and fail for the intended reason, on a tool without
/// that code).
fn diagnostic_lines(outcome: &SourceOutcome, code: &str) -> Vec<usize> {
    let mut lines = outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.as_str() == code)
        .map(|diagnostic| diagnostic.line)
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

/// Review F03 follow-up / V3-O04: `.paginate(PageLimit::..)` becomes a v3
/// terminal (and its `.await?`, annotation and imports follow) only when the
/// receiver is proved to be a generated request and `PageLimit` is Concord's.
/// An unrelated receiver whose `paginate` takes Concord's `PageLimit` is left
/// unchanged and reported (`C3-MIGRATE-PAGINATE-UNRESOLVED`, plus the
/// remaining `PageLimit` references); a local `PageLimit`/`PaginatedRequest`
/// is neither rewritten nor reported.
#[test]
fn paginate_rewrites_require_a_proved_receiver_and_concord_page_limit() {
    let callers = r#"pub async fn genuine(client: Pages, n: u32, bound: NonZeroU32) -> Result<(), concord::ApiError> {
    let stored = client.list().paginate(PageLimit::Prefix(n)); // genuine: stored
    let _: PaginatedRequest<_> = client.list().paginate(PageLimit::Prefix(n)); // genuine: annotated
    let items = client.list().paginate(PageLimit::Prefix(n)).await?; // genuine: awaited
    let all = Pages::new().expect("client").list().paginate(concord::PageLimit::CompleteWithin(bound)); // genuine: qualified
    let _ = (stored, items, all);
    Ok(())
}

pub fn unrelated(repo: repo::MyRepo) -> u32 {
    repo.paginate(PageLimit::Prefix(2)) // unrelated: concord page limit
}

pub fn unrelated_complete_within(repo: repo::MyRepo) -> u32 {
    repo.paginate(PageLimit::CompleteWithin(NonZeroU32::MIN)) // unrelated: complete within
}

pub fn local_types(store: local::Store) -> u32 {
    use self::local::{PageLimit, PaginatedRequest};
    let page: PaginatedRequest<_> = store.paginate(PageLimit::Prefix(2)); // local: types
    page.0
}
"#;
    let source = format!("{PAGINATE_API}{callers}");
    let outcome = convert(&source);
    let (head, tail) = outcome
        .converted
        .split_once("// callers\n")
        .expect("caller marker");
    let expected = callers
        .replace(
            "client.list().paginate(PageLimit::Prefix(n)); // genuine: stored",
            "client.list().collect_prefix(n); // genuine: stored",
        )
        .replace(
            "let _: PaginatedRequest<_> = client.list().paginate(PageLimit::Prefix(n)); // genuine: annotated",
            "let _: CollectPrefix<_> = client.list().collect_prefix(n); // genuine: annotated",
        )
        .replace(
            "client.list().paginate(PageLimit::Prefix(n)).await?; // genuine: awaited",
            "client.list().collect_prefix(n).await?.into_items(); // genuine: awaited",
        )
        .replace(
            ".list().paginate(concord::PageLimit::CompleteWithin(bound)); // genuine: qualified",
            ".list().collect_all(bound); // genuine: qualified",
        );
    assert!(
        tail == expected,
        "caller text:\n{}",
        super::edits::unified_diff("callers", &expected, tail)
    );
    // The unrelated calls still name Concord's `PageLimit`, so its import
    // stays (and they are reported); the annotation import follows the
    // rewritten annotation; the local module is untouched.
    assert!(head.contains("use concord::{PageLimit, api};\n"), "{head}");
    assert!(
        head.contains("use concord::pagination::CollectPrefix;\n"),
        "{head}"
    );
    let local_module = &PAGINATE_API[PAGINATE_API.find("pub mod local").expect("local")..];
    let local_module = &local_module[..local_module.find("// callers").expect("end")];
    assert!(head.contains(local_module), "{head}");

    let line_of = |fragment: &str| {
        source
            .lines()
            .position(|line| line.contains(fragment))
            .unwrap_or_else(|| panic!("line {fragment}"))
            + 1
    };
    let lines = |code: &str| diagnostic_lines(&outcome, code);
    assert_eq!(
        lines("C3-MIGRATE-PAGINATE-UNRESOLVED"),
        [
            line_of("// unrelated: concord page limit"),
            line_of("// unrelated: complete within"),
        ],
        "unproved receivers are reported: {:#?}",
        outcome.diagnostics
    );
    assert_eq!(
        lines("C3-MIGRATE-PAGELIMIT-REFERENCE"),
        [
            line_of("pub fn paginate(&self, _: concord::PageLimit) -> u32 { 1 }"),
            line_of("// unrelated: concord page limit"),
            line_of("// unrelated: complete within"),
        ],
        "only Concord's `PageLimit` is a remaining reference"
    );
    assert_eq!(
        lines("C3-MIGRATE-PAGINATED-REQUEST-REFERENCE"),
        Vec::<usize>::new(),
        "a local `PaginatedRequest` is not Concord's"
    );
    assert_eq!(
        lines("C3-MIGRATE-PAGINATE-PREFIX"),
        [
            line_of("// genuine: stored"),
            line_of("// genuine: annotated")
        ]
    );
    assert_eq!(
        lines("C3-MIGRATE-PAGINATE-PREFIX-ITEMS"),
        [line_of("// genuine: awaited")]
    );
    assert_eq!(
        lines("C3-MIGRATE-PAGINATE-COLLECT-ALL"),
        [line_of("// genuine: qualified")]
    );
    assert_eq!(
        lines("C3-MIGRATE-PAGINATED-REQUEST-TYPE"),
        [line_of("// genuine: annotated")]
    );
    assert_eq!(lines("C3-MIGRATE-PAGINATE-DYNAMIC"), Vec::<usize>::new());
    assert_eq!(lines("C3-MIGRATE-PAGELIMIT-IMPORT"), Vec::<usize>::new());
    for diagnostic in &outcome.diagnostics {
        if diagnostic.code.as_str() == "C3-MIGRATE-PAGINATE-UNRESOLVED" {
            assert!(
                diagnostic.message.contains("left unchanged"),
                "{diagnostic:?}"
            );
            assert!(
                diagnostic.suggestion.as_deref().is_some_and(|suggestion| {
                    suggestion.contains(".collect_prefix(") || suggestion.contains(".collect_all(")
                }),
                "{diagnostic:?}"
            );
        }
    }
}

/// Review F03 follow-up / V3-O04: when every Concord `PageLimit` use is a
/// proved rewrite, the import is removed; a local `PageLimit` elsewhere in
/// the file neither blocks that nor is reported.
#[test]
fn page_limit_import_removal_ignores_a_local_page_limit() {
    let source = r#"use std::num::NonZeroU32;
use concord::{PageLimit, api};

api! {
    client Pages {
        base "https://pages.example.test/";
        timeout 5s;
        GET list(offset: u64 = 0, count: NonZeroU32 = crate::PAGE) -> Vec<String> {
            query { "offset" = endpoint.offset; "count" = endpoint.count; }
            paginate via concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.count;
            }
        }
    }
}

pub mod local {
    pub struct PageLimit(pub u32);
    pub fn limit(value: PageLimit) -> u32 { value.0 }
}

pub fn genuine(client: Pages) {
    let _ = client.list().paginate(PageLimit::Prefix(2));
}
"#;
    let outcome = convert(source);
    assert!(
        outcome.converted.contains("use concord::{api};\n"),
        "{}",
        outcome.converted
    );
    assert!(
        outcome
            .converted
            .contains("let _ = client.list().collect_prefix(2);")
    );
    assert!(
        outcome
            .converted
            .contains("pub fn limit(value: PageLimit) -> u32 { value.0 }")
    );
    assert_eq!(
        diagnostic_lines(&outcome, "C3-MIGRATE-PAGELIMIT-IMPORT"),
        [2]
    );
    assert_eq!(
        diagnostic_lines(&outcome, "C3-MIGRATE-PAGELIMIT-REFERENCE"),
        Vec::<usize>::new()
    );
    assert_eq!(
        diagnostic_lines(&outcome, "C3-MIGRATE-PAGINATE-UNRESOLVED"),
        Vec::<usize>::new()
    );
}

// ----- post-fix review of F03 (V3-O04) ---------------------------------------
//
// The independent post-fix review (evidence `r2/postfix_review/f03/`) found
// rewrites that were still decided without proving that the call reaches the
// generated method: wrapper and reference receivers, chains through methods
// the generated surface does not define, names bound by pattern macros,
// crate-local statement macros with an allow-listed name, the same classes in
// the paginate follow-up, exponential glob resolution, and references to the
// v2.1 crate-level `<Client>Builder`. Each probe case below checks the
// complete resulting caller text and every diagnostic in it.

/// The reviewer's post-fix probe declaration (`x` is Concord-optional on the
/// generated request, `locale` on the generated builder), with a use of
/// `locale` so the declaration itself converts cleanly. `other` offers both
/// setter names of each value, so a wrong rename compiles and returns 2.
const POSTFIX_API: &str = r#"use concord::{api, Empty};
api! {
    client Api {
        base "https://review.example.test/";
        timeout 5s;
        values {
            locale?: String;
        }
        query {
            "locale" = client.locale;
        }
        GET item(x?: u32) -> () via Empty {
            path ["item"];
            query { "x" = endpoint.x; }
        }
    }
}
pub mod other {
    pub struct ItemRequest;
    impl ItemRequest {
        pub fn set_x(self, _: Option<u32>) -> u32 { 1 }
        pub fn replace_x(self, _: Option<u32>) -> u32 { 2 }
    }
    pub struct Locale;
    impl Locale {
        pub fn set_locale(self, _: Option<String>) -> u32 { 1 }
        pub fn replace_locale(self, _: Option<String>) -> u32 { 2 }
    }
}
// callers
"#;

/// The 1-based line of the one line of `source` containing `fragment`.
fn unique_line(source: &str, fragment: &str) -> usize {
    let hits = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(fragment))
        .map(|(index, _)| index + 1)
        .collect::<Vec<_>>();
    assert_eq!(
        hits.len(),
        1,
        "fragment `{fragment}` is not on exactly one line"
    );
    hits[0]
}

/// Migrates `api` followed by `callers` as a standalone crate and checks the
/// complete resulting caller text and every diagnostic located in it.
///
/// Each `// PROBE:<KIND> <name>` line holds one site. On a `GENUINE` line
/// every `set_x`/`set_locale` call (including a raw `r#set_x`) becomes
/// `replace_*` (`r#replace_x`), with one `C3-MIGRATE-SETTER-REPLACE` note per
/// call; an `UNPROVED` or `UNKNOWN` line keeps its text and carries exactly
/// one `C3-MIGRATE-SETTER-UNRESOLVED` error with an actionable `replace_*`
/// suggestion; an `UNTOUCHED` line keeps its text and carries no diagnostic.
/// `rewrites` are the further exact
/// `(from, to)` caller edits and `extra` the further `(code, line fragment)`
/// diagnostics; nothing else may change or be reported, and the declaration
/// itself has no unautomatable site.
fn assert_probe(
    api: &str,
    callers: &str,
    rewrites: &[(&str, &str)],
    extra: &[(&str, &str)],
) -> SourceOutcome {
    let source = format!("{api}{callers}");
    let outcome = convert(&source);
    let marker_line = api.lines().count();
    let (_, tail) = outcome
        .converted
        .split_once("// callers\n")
        .expect("caller marker");
    // Raw-identifier spellings (`r#set_x`) count as the same setter.
    let setter_calls = |line: &str| {
        line.matches(".set_x(").count()
            + line.matches(".set_locale(").count()
            + line.matches("::set_x(").count()
            + line.matches(".r#set_x(").count()
            + line.matches("::r#set_x(").count()
    };
    let mut expected_lines = Vec::new();
    let mut expected_diagnostics = Vec::new();
    for (index, line) in callers.lines().enumerate() {
        let number = marker_line + index + 1;
        let mut text = line.to_owned();
        if let Some((_, probe)) = line.split_once("// PROBE:") {
            let calls = setter_calls(line);
            assert!(calls >= 1, "a probe line holds a setter call: {line}");
            if probe.starts_with("GENUINE") {
                text = text
                    .replace(".set_x(", ".replace_x(")
                    .replace(".set_locale(", ".replace_locale(")
                    .replace("::set_x(", "::replace_x(")
                    .replace(".r#set_x(", ".r#replace_x(")
                    .replace("::r#set_x(", "::r#replace_x(");
                for _ in 0..calls {
                    expected_diagnostics.push(("C3-MIGRATE-SETTER-REPLACE".to_owned(), number));
                }
            } else if probe.starts_with("UNTOUCHED") {
                // Left unchanged without a diagnostic (a token-only standard
                // macro such as `stringify!`).
            } else {
                assert!(
                    probe.starts_with("UNPROVED") || probe.starts_with("UNKNOWN"),
                    "probe kind: {line}"
                );
                assert_eq!(calls, 1, "one unproved call per probe line: {line}");
                expected_diagnostics.push(("C3-MIGRATE-SETTER-UNRESOLVED".to_owned(), number));
            }
        }
        expected_lines.push(text);
    }
    let mut expected = expected_lines.join("\n");
    if callers.ends_with('\n') {
        expected.push('\n');
    }
    for (from, to) in rewrites {
        assert_eq!(expected.matches(from).count(), 1, "rewrite `{from}`");
        expected = expected.replace(from, to);
    }
    assert!(
        tail == expected,
        "caller text differs from the expected decisions:\n{}",
        super::edits::unified_diff("callers", &expected, tail)
    );
    for (code, fragment) in extra {
        expected_diagnostics.push(((*code).to_owned(), unique_line(&source, fragment)));
    }
    expected_diagnostics.sort();
    let mut actual = outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.line > marker_line)
        .map(|diagnostic| (diagnostic.code.as_str().to_owned(), diagnostic.line))
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        actual, expected_diagnostics,
        "caller diagnostics: {:#?}",
        outcome.diagnostics
    );
    let declaration_errors = outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.line <= marker_line && diagnostic.severity() == Severity::Error
        })
        .map(super::report::Diagnostic::render)
        .collect::<Vec<_>>();
    assert!(declaration_errors.is_empty(), "{declaration_errors:#?}");
    for diagnostic in &outcome.diagnostics {
        if diagnostic.code == Code::SetterUnresolved {
            assert!(
                diagnostic.message.contains("left unchanged"),
                "{diagnostic:?}"
            );
            assert!(
                diagnostic
                    .suggestion
                    .as_deref()
                    .is_some_and(|suggestion| suggestion.contains("replace_")),
                "an actionable suggestion: {diagnostic:?}"
            );
        }
    }
    outcome
}

/// The reviewer's probe `postfix_probe_f03_unproved_receivers`, verbatim:
/// an extension trait on `Option`/`Arc`/`&` of the generated request, a
/// chain through a user helper on the generated builder or a `set_`-prefixed
/// helper on the generated request, names rebound by a pattern macro and by a
/// crate-local `debug!`: none is renamed, each is reported. The genuine
/// controls (parameter, closure capture, builder, `Box`, explicit `unwrap`)
/// migrate, and the helper's `impl ApiBuilder` names the v3 support module.
#[test]
fn postfix_f03_reviewer_unproved_receivers_are_left_unchanged() {
    let callers = r#"
// (1) Extension trait on a standard wrapper / reference of the generated type:
// `o.set_x` calls the trait method, not the generated setter.
pub trait Patch {
    fn set_x(self, v: Option<u32>) -> u32;
    fn replace_x(self, v: Option<u32>) -> u32;
}
impl Patch for Option<api::ItemRequest> {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
impl Patch for std::sync::Arc<api::ItemRequest> {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
impl Patch for &api::ItemRequest {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
pub fn option_ext(o: Option<api::ItemRequest>) -> u32 {
    o.set_x(None) // PROBE:UNPROVED option_wrapper_trait
}
pub fn arc_ext(a: std::sync::Arc<api::ItemRequest>) -> u32 {
    a.set_x(None) // PROBE:UNPROVED arc_wrapper_trait
}
pub fn ref_ext(r: &api::ItemRequest) -> u32 {
    r.set_x(None) // PROBE:UNPROVED reference_trait
}

// (2) Method-chain returns: a user inherent helper on the generated builder
// (or a `set_`-prefixed helper on the generated request) returns a foreign
// type that offers both setter names.
impl ApiBuilder {
    pub fn into_locale(self) -> other::Locale { other::Locale }
}
impl api::ItemRequest {
    pub fn set_mode(self, _: u8) -> other::ItemRequest { other::ItemRequest }
}
pub fn builder_chain() -> u32 {
    Api::builder().into_locale().set_locale(None) // PROBE:UNPROVED builder_helper_chain
}
pub fn request_chain(r: api::ItemRequest) -> u32 {
    r.set_mode(1).set_x(None) // PROBE:UNPROVED request_set_prefixed_helper_chain
}

// (3) A pattern macro introduces a binding that shadows the generated one.
macro_rules! bind { ($n:ident) => { $n }; }
pub fn pattern_macro_let(request: api::ItemRequest) -> u32 {
    let _keep = request;
    let bind!(request) = other::ItemRequest;
    request.set_x(None) // PROBE:UNPROVED pattern_macro_let
}
pub fn pattern_macro_arm(request: api::ItemRequest) -> u32 {
    let _keep = request;
    match other::ItemRequest {
        bind!(request) => request.set_x(None), // PROBE:UNPROVED pattern_macro_arm
    }
}

// (4) A crate-local statement macro whose name is on the expression-macro
// list rebinds the name.
macro_rules! debug { ($n:ident) => { let $n = other::ItemRequest; }; }
pub fn listed_name_macro(request: api::ItemRequest) -> u32 {
    let _keep = request;
    debug!(request);
    request.set_x(None) // PROBE:UNPROVED user_macro_named_debug
}

// (5) Controls: genuine receivers still migrate.
pub fn genuine_param(r: api::ItemRequest) {
    let _ = r.set_x(None); // PROBE:GENUINE param
}
pub fn genuine_closure_capture(r: api::ItemRequest) {
    let f = move || r.set_x(None); // PROBE:GENUINE closure_capture
    let _ = f;
}
pub fn genuine_builder() {
    let _ = Api::builder().set_locale(None); // PROBE:GENUINE builder
}
pub fn genuine_boxed(b: Box<api::ItemRequest>) {
    let _ = b.set_x(None); // PROBE:GENUINE boxed
}
pub fn genuine_unwrap(o: Option<api::ItemRequest>) {
    let _ = o.unwrap().set_x(None); // PROBE:GENUINE unwrap
}
"#;
    assert_probe(
        POSTFIX_API,
        callers,
        &[("impl ApiBuilder {", "impl api::ApiBuilder {")],
        &[("C3-MIGRATE-BUILDER-PATH", "impl ApiBuilder {")],
    );
}

/// The reviewer's probe `postfix_probe_f03_variations_expected_safe`,
/// verbatim (it passed on the reviewed source; kept as a regression guard):
/// use groups, re-exports, `super::super`, `self::`, pattern, closure, `for`,
/// `while let`, `let`-`else`, tuple and nested-function shadowing, raw
/// identifiers, async capture and `impl Trait` arguments.
#[test]
fn postfix_f03_reviewer_variations_keep_their_decisions() {
    let callers = r#"
mod grp {
    use super::{other::ItemRequest, api as generated};
    pub fn group_foreign(r: ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNPROVED use_group_foreign
    pub fn group_genuine(r: generated::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE use_group_genuine
}
mod reexp {
    pub use super::other::ItemRequest;
}
mod reexp_glob {
    pub use super::api::*;
}
pub fn reexport_foreign(r: reexp::ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNPROVED reexport_foreign
pub fn reexport_genuine(r: reexp_glob::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE reexport_glob
mod deep {
    pub mod inner {
        pub fn up(r: super::super::other::ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNPROVED super_super_foreign
        pub fn up_gen(r: super::super::api::ItemRequest) { let _ = r.set_x(None); } // PROBE:GENUINE super_super_genuine
        pub fn selfy(r: self::Local) -> u32 { r.set_x(None) } // PROBE:UNPROVED self_local
        pub struct Local;
        impl Local { pub fn set_x(self, _: Option<u32>) -> u32 { 1 } }
    }
}
type Shared = std::rc::Rc<other::ItemRequest>;
pub fn alias_foreign_wrapper(r: Shared) -> u32 { Shared::clone(&r); 0 }
type Gen<T> = T;
pub fn alias_generic(r: Gen<api::ItemRequest>) { let _ = r.set_x(None); } // PROBE:UNKNOWN alias_generic
pub fn shadow_arm(r: api::ItemRequest, o: Option<other::ItemRequest>) -> u32 {
    let _keep = r;
    match o { Some(r) => r.set_x(None), None => 0 } // PROBE:UNPROVED match_arm_shadow
}
pub fn shadow_closure_param(r: api::ItemRequest) -> u32 {
    let _keep = r;
    let f = |r: other::ItemRequest| r.set_x(None); // PROBE:UNPROVED closure_param_typed
    f(other::ItemRequest)
}
pub fn shadow_for(r: api::ItemRequest) -> u32 {
    let _keep = r;
    let mut n = 0;
    for r in vec![other::ItemRequest] { n += r.set_x(None); } // PROBE:UNPROVED for_shadow
    n
}
pub fn shadow_while_let(r: api::ItemRequest) -> u32 {
    let _keep = r;
    let mut v = vec![other::ItemRequest];
    let mut n = 0;
    while let Some(r) = v.pop() { n += r.set_x(None); } // PROBE:UNPROVED while_let_shadow
    n
}
pub fn shadow_let_else(r: api::ItemRequest, o: Option<other::ItemRequest>) -> u32 {
    let _keep = r;
    let Some(r) = o else { return 0 };
    r.set_x(None) // PROBE:UNPROVED let_else_shadow
}
pub fn shadow_tuple(r: api::ItemRequest) -> u32 {
    let _keep = r;
    let (r, _) = (other::ItemRequest, 1);
    r.set_x(None) // PROBE:UNPROVED tuple_shadow
}
pub fn shadow_nested_fn(r: api::ItemRequest) {
    fn inner(r: other::ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNPROVED nested_fn_param
    let _ = r.set_x(None); // PROBE:GENUINE after_nested_fn
    let _ = inner;
}
pub fn raw_ident(r#r: api::ItemRequest) { let _ = r#r.set_x(None); } // PROBE:GENUINE raw_ident
pub fn async_block(r: api::ItemRequest) {
    let _f = async move { let _ = r.set_x(None); }; // PROBE:GENUINE async_capture
}
pub fn impl_trait_arg(r: impl Into<other::ItemRequest>) -> u32 { r.into().set_x(None) } // PROBE:UNPROVED impl_trait_into
"#;
    assert_probe(POSTFIX_API, callers, &[], &[]);
}

/// The reviewer's probe `postfix_probe_f03_odd_syntax_does_not_panic`,
/// verbatim (it passed on the reviewed source; kept as a regression guard):
/// cyclic and self globs, an alias loop, deep parentheses, unions, extern
/// blocks, nested modules in blocks and let chains resolve without a panic.
#[test]
fn postfix_f03_reviewer_odd_syntax_resolves_without_panics() {
    let callers = r#"
pub fn odd() {
    let _ = { { { { { { { { 1 } } } } } } } };
    let v: Vec<fn() -> api::ItemRequest> = Vec::new();
    let _ = v.iter().map(|f| f().set_x(None)).count(); // PROBE:UNKNOWN fn_pointer_element
    let _ = || -> u32 { 0 };
    let _ = <Vec<u8> as Default>::default();
    const C: usize = { struct Inner; 3 };
    static S: &str = "x";
    let _ = (C, S);
    'outer: loop { break 'outer; }
    let _ = if let Some(x) = Some(1) && x > 0 { 1 } else { 2 };
    union U { a: u32, b: f32 }
    let _ = U { a: 1 };
    extern "C" { fn abs(x: i32) -> i32; }
    let _ = unsafe { abs(-1) };
    trait T { type A; fn f() -> Self::A; }
    impl<X> T for Vec<X> { type A = u8; fn f() -> u8 { 0 } }
    mod m { pub mod n { pub mod o { pub mod p { pub fn q() {} } } } }
    m::n::o::p::q();
    use self::m::n::o::p as pp;
    pp::q();
    use super::*;
    use crate::*;
}
mod cyc_a { pub use super::cyc_b::*; }
mod cyc_b { pub use super::cyc_a::*; }
pub fn cyclic(r: cyc_a::ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNKNOWN cyclic_globs
mod self_glob { pub use self::*; pub use super::self_glob::*; }
pub fn self_glob_user(r: self_glob::Missing) -> u32 { r.set_x(None) } // PROBE:UNKNOWN self_glob
type Loop1 = Loop2;
type Loop2 = Loop1;
pub fn alias_loop(r: Loop1) -> u32 { r.set_x(None) } // PROBE:UNKNOWN alias_loop
pub fn deep_paren(r: api::ItemRequest) { let _ = ((((((((r)))))))).set_x(None); } // PROBE:GENUINE deep_paren
"#;
    assert_probe(POSTFIX_API, callers, &[], &[]);
}

/// The reviewer's glob DAG: module `mK` re-exports `mK-1` and `mK-2` through
/// globs, and the probed name is defined by none of them.
fn glob_dag_callers(levels: usize) -> String {
    let mut callers =
        String::from("\npub mod m0 { pub struct Leaf; }\npub mod m1 { pub use super::m0::*; }\n");
    for level in 2..levels {
        callers.push_str(&format!(
            "pub mod m{level} {{ pub use super::m{}::*; pub use super::m{}::*; }}\n",
            level - 1,
            level - 2
        ));
    }
    callers.push_str(&format!(
        "pub fn probe(r: m{}::Missing) -> u32 {{ r.set_x(None) }} // PROBE:UNKNOWN glob_dag\n",
        levels - 1
    ));
    callers
}

/// The reviewer's probe `postfix_probe_f03_glob_dag_timing`: resolving a name
/// through a DAG of glob re-exports visits each module once, so the time is
/// linear in the depth. The reviewed source re-walked every path (about 1.6x
/// per level: 30 levels took 5.7 s in a debug build, so 32 take about 14 s);
/// the name stays unresolved and the call is reported either way.
#[test]
fn postfix_f03_glob_dag_resolution_is_linear() {
    let callers = glob_dag_callers(32);
    let started = std::time::Instant::now();
    assert_probe(POSTFIX_API, &callers, &[], &[]);
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "resolving a 32-level glob DAG took {elapsed:?}"
    );
}

/// The reviewer's paginate declaration: `MyRepo`, an `Option` extension trait
/// and a `set_mode` helper on the generated request all offer `paginate`
/// taking Concord's `PageLimit` and a `collect_prefix` returning 2.
const POSTFIX_PAGINATE_API: &str = r#"use std::num::NonZeroU32;
use concord::{PageLimit, api};

api! {
    client Pages {
        base "https://pages.example.test/";
        timeout 5s;
        GET list(offset: u64 = 0, count: NonZeroU32 = crate::PAGE) -> Vec<String> {
            query { "offset" = endpoint.offset; "count" = endpoint.count; }
            paginate via concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.count;
            }
        }
    }
}

pub mod repo {
    pub struct MyRepo;
    impl MyRepo {
        pub fn paginate(self, _: concord::PageLimit) -> u32 { 1 }
        pub fn collect_prefix(self, _: u32) -> u32 { 2 }
    }
}
pub trait Paged {
    fn paginate(self, _: concord::PageLimit) -> u32;
    fn collect_prefix(self, _: u32) -> u32;
}
impl Paged for Option<pages::ListRequest> {
    fn paginate(self, _: concord::PageLimit) -> u32 { 1 }
    fn collect_prefix(self, _: u32) -> u32 { 2 }
}
impl pages::ListRequest {
    pub fn set_mode(self, _: u8) -> repo::MyRepo { repo::MyRepo }
}
// callers
"#;

/// Every diagnostic of an outcome as sorted `(code, line)` pairs.
fn all_diagnostics(outcome: &SourceOutcome) -> Vec<(String, usize)> {
    let mut all = outcome
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.code.as_str().to_owned(), diagnostic.line))
        .collect::<Vec<_>>();
    all.sort();
    all
}

/// Sorted `(code, line of fragment)` pairs.
fn expected_diagnostics(source: &str, expected: &[(&str, &str)]) -> Vec<(String, usize)> {
    let mut all = expected
        .iter()
        .map(|(code, fragment)| ((*code).to_owned(), unique_line(source, fragment)))
        .collect::<Vec<_>>();
    all.sort();
    all
}

/// The reviewer's probe `postfix_probe_f03_paginate_follow_up`: an `Option`
/// extension trait and a `set_`-prefixed helper on the generated request are
/// not the generated traversal, so their `.paginate(PageLimit::Prefix(2))` is
/// left unchanged and reported (with its remaining `PageLimit` references);
/// only the genuine call becomes `.collect_prefix(n)`.
#[test]
fn postfix_f03_reviewer_paginate_follow_up_requires_the_generated_traversal() {
    let callers = r#"
pub fn option_wrapper(o: Option<pages::ListRequest>) -> u32 {
    o.paginate(PageLimit::Prefix(2)) // PROBE:UNPROVED paginate_option_trait
}
pub fn set_prefixed_helper(client: Pages) -> u32 {
    client.list().set_mode(1).paginate(PageLimit::Prefix(2)) // PROBE:UNPROVED paginate_after_helper
}
pub fn genuine(client: Pages, n: u32) {
    let _ = client.list().paginate(PageLimit::Prefix(n)); // PROBE:GENUINE paginate
}
"#;
    let source = format!("{POSTFIX_PAGINATE_API}{callers}");
    let outcome = convert(&source);
    let (head, tail) = outcome
        .converted
        .split_once("// callers\n")
        .expect("caller marker");
    let expected = callers.replace(
        "client.list().paginate(PageLimit::Prefix(n)); // PROBE:GENUINE paginate",
        "client.list().collect_prefix(n); // PROBE:GENUINE paginate",
    );
    assert!(
        tail == expected,
        "caller text:\n{}",
        super::edits::unified_diff("callers", &expected, tail)
    );
    assert!(head.contains("use concord::{PageLimit, api};\n"), "{head}");
    assert_eq!(
        all_diagnostics(&outcome),
        expected_diagnostics(
            &source,
            &[
                (
                    "C3-MIGRATE-PAGINATION-INIT",
                    "paginate via concord::pagination::OffsetPagination"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "pub fn paginate(self, _: concord::PageLimit) -> u32 { 1 }"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "    fn paginate(self, _: concord::PageLimit) -> u32;"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "    fn paginate(self, _: concord::PageLimit) -> u32 { 1 }"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "// PROBE:UNPROVED paginate_option_trait"
                ),
                (
                    "C3-MIGRATE-PAGINATE-UNRESOLVED",
                    "// PROBE:UNPROVED paginate_option_trait"
                ),
                (
                    "C3-MIGRATE-PAGELIMIT-REFERENCE",
                    "// PROBE:UNPROVED paginate_after_helper"
                ),
                (
                    "C3-MIGRATE-PAGINATE-UNRESOLVED",
                    "// PROBE:UNPROVED paginate_after_helper"
                ),
                ("C3-MIGRATE-PAGINATE-PREFIX", "// PROBE:GENUINE paginate"),
            ]
        ),
        "{:#?}",
        outcome.diagnostics
    );
}

/// Post-fix review of F03, paginate classes beyond the reviewer's probe: a
/// user `paginate` on a request whose endpoint has no pagination (so v2.1
/// generates none), and `Arc`/reference receivers with an extension trait,
/// are left unchanged and reported; a boxed or explicitly unwrapped
/// generated request still migrates (an awaited prefix keeps its `Vec`).
#[test]
fn postfix_f03_paginate_needs_a_generated_paginate_method() {
    let api = POSTFIX_PAGINATE_API
        .replace(
            "    }\n}\n\npub mod repo {",
            "        GET one() -> String;\n    }\n}\n\npub mod repo {",
        )
        .replace(
            "// callers\n",
            "impl pages::OneRequest {
    pub fn paginate(self, _: concord::PageLimit) -> u32 { 1 }
    pub fn collect_prefix(self, _: u32) -> u32 { 2 }
}
impl Paged for std::sync::Arc<pages::ListRequest> {
    fn paginate(self, _: concord::PageLimit) -> u32 { 1 }
    fn collect_prefix(self, _: u32) -> u32 { 2 }
}
impl Paged for &pages::ListRequest {
    fn paginate(self, _: concord::PageLimit) -> u32 { 1 }
    fn collect_prefix(self, _: u32) -> u32 { 2 }
}
// callers
",
        );
    assert!(api.contains("GET one() -> String;"));
    let callers = r#"
pub fn unpaginated(client: Pages) -> u32 {
    client.one().paginate(PageLimit::Prefix(2)) // unproved: no generated paginate
}
pub fn arc_wrapper(a: std::sync::Arc<pages::ListRequest>) -> u32 {
    a.paginate(PageLimit::Prefix(2)) // unproved: arc
}
pub fn reference(r: &pages::ListRequest) -> u32 {
    r.paginate(PageLimit::Prefix(2)) // unproved: reference
}
pub async fn boxed(b: Box<pages::ListRequest>, n: u32) -> Result<Vec<String>, concord::ApiError> {
    b.paginate(PageLimit::Prefix(n)).await // genuine: boxed
}
pub fn unwrapped(o: Option<pages::ListRequest>, n: u32) {
    let _ = o.unwrap().paginate(PageLimit::Prefix(n)); // genuine: unwrapped
}
"#;
    let source = format!("{api}{callers}");
    let outcome = convert(&source);
    let (_, tail) = outcome
        .converted
        .split_once("// callers\n")
        .expect("caller marker");
    let expected = callers
        .replace(
            "b.paginate(PageLimit::Prefix(n)).await // genuine: boxed",
            "b.collect_prefix(n).await.map(|prefix| prefix.into_items()) // genuine: boxed",
        )
        .replace(
            "o.unwrap().paginate(PageLimit::Prefix(n)); // genuine: unwrapped",
            "o.unwrap().collect_prefix(n); // genuine: unwrapped",
        );
    assert!(
        tail == expected,
        "caller text:\n{}",
        super::edits::unified_diff("callers", &expected, tail)
    );
    let caller_diagnostics = |code: &str| {
        let marker = api.lines().count();
        diagnostic_lines(&outcome, code)
            .into_iter()
            .filter(|line| *line > marker)
            .collect::<Vec<_>>()
    };
    let lines = |fragments: &[&str]| {
        let mut lines = fragments
            .iter()
            .map(|fragment| unique_line(&source, fragment))
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines
    };
    let unproved = [
        "// unproved: no generated paginate",
        "// unproved: arc",
        "// unproved: reference",
    ];
    assert_eq!(
        caller_diagnostics("C3-MIGRATE-PAGINATE-UNRESOLVED"),
        lines(&unproved),
        "{:#?}",
        outcome.diagnostics
    );
    assert_eq!(
        caller_diagnostics("C3-MIGRATE-PAGELIMIT-REFERENCE"),
        lines(&unproved)
    );
    assert_eq!(
        caller_diagnostics("C3-MIGRATE-PAGINATE-PREFIX"),
        lines(&["// genuine: unwrapped"])
    );
    assert_eq!(
        caller_diagnostics("C3-MIGRATE-PAGINATE-PREFIX-ITEMS"),
        lines(&["// genuine: boxed"])
    );
}

/// Post-fix review of F03, wrapper and reference receivers beyond the
/// reviewer's probe: `Rc`, `Result` and `&mut` receivers, a borrowed binding,
/// a `ref` pattern, a borrow expression and `&self` inside a trait impl for
/// the generated request cannot reach the generated setter (it takes `self`
/// by value), so each is reported. An explicit `unwrap()`, `expect(..)`,
/// `?` or `*` on a box, a `Box` (including `Option<Box<_>>` after `unwrap()`),
/// `self` by value, and navigation through `&Client` or `Arc<Client>` (whose
/// child methods take `&self`) are proved.
#[test]
fn postfix_f03_wrapper_receivers_need_an_explicit_unwrap() {
    let callers = r#"
pub trait Patch {
    fn set_x(self, v: Option<u32>) -> u32;
    fn replace_x(self, v: Option<u32>) -> u32;
}
impl Patch for std::rc::Rc<api::ItemRequest> {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
impl Patch for Result<api::ItemRequest, ()> {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
impl Patch for &mut api::ItemRequest {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
impl Patch for &api::ItemRequest {
    fn set_x(self, _: Option<u32>) -> u32 { 1 }
    fn replace_x(self, _: Option<u32>) -> u32 { 2 }
}
pub trait Probe {
    fn by_ref(&self) -> u32;
    fn by_value(self);
}
impl Probe for api::ItemRequest {
    fn by_ref(&self) -> u32 { self.set_x(None) } // PROBE:UNPROVED ref_self
    fn by_value(self) { let _ = self.set_x(None); } // PROBE:GENUINE value_self
}
pub fn rc_ext(r: std::rc::Rc<api::ItemRequest>) -> u32 { r.set_x(None) } // PROBE:UNPROVED rc_trait
pub fn result_ext(r: Result<api::ItemRequest, ()>) -> u32 { r.set_x(None) } // PROBE:UNPROVED result_trait
pub fn mut_ext(r: &mut api::ItemRequest) -> u32 { r.set_x(None) } // PROBE:UNPROVED mut_reference_trait
pub fn borrowed(r: api::ItemRequest) -> u32 { let b = &r; b.set_x(None) } // PROBE:UNPROVED borrowed_binding
pub fn ref_binding(r: api::ItemRequest) -> u32 { let ref b = r; b.set_x(None) } // PROBE:UNPROVED ref_pattern
pub fn borrow_expression(r: api::ItemRequest) -> u32 { (&r).set_x(None) } // PROBE:UNPROVED borrow_expression
pub fn genuine_unwrap(o: Option<api::ItemRequest>) { let _ = o.unwrap().set_x(None); } // PROBE:GENUINE unwrap
pub fn genuine_expect(r: Result<api::ItemRequest, ()>) { let _ = r.expect("request").set_x(None); } // PROBE:GENUINE expect
pub fn genuine_try(o: Option<api::ItemRequest>) -> Option<()> { let _ = o?.set_x(None); Some(()) } // PROBE:GENUINE try
pub fn genuine_boxed(b: Box<api::ItemRequest>) { let _ = b.set_x(None); } // PROBE:GENUINE boxed
pub fn genuine_deref(b: Box<api::ItemRequest>) { let _ = (*b).set_x(None); } // PROBE:GENUINE deref_box
pub fn genuine_nested(o: Option<Box<api::ItemRequest>>) { let _ = o.unwrap().set_x(None); } // PROBE:GENUINE option_box
pub fn genuine_reference_client(client: &Api) { let _ = client.item().set_x(None); } // PROBE:GENUINE reference_client
pub fn genuine_arc_client(client: std::sync::Arc<Api>) { let _ = client.item().set_x(None); } // PROBE:GENUINE arc_client
"#;
    assert_probe(POSTFIX_API, callers, &[], &[]);
}

/// Post-fix review of F03, method chains beyond the reviewer's probe: only a
/// method the generated surface defines (with its known result) advances a
/// chain. A user `reset_x` where `x` has no default (so none is generated), a
/// user `timeout` on the unbound endpoint (which has none), a user `unwrap`
/// on the generated request, and a user inherent `clone` on the client all
/// make the receiver unknown. Chains through generated setters, `omit_*`,
/// `configure_http`, `build()` + `expect`, the request's `timeout`, the
/// unbound tree and `Client::request` are proved.
#[test]
fn postfix_f03_chains_advance_only_through_generated_methods() {
    let callers = r#"
impl ApiBuilder {
    pub fn into_locale(self) -> other::Locale { other::Locale }
}
impl api::ItemRequest {
    pub fn reset_x(self) -> other::ItemRequest { other::ItemRequest }
    pub fn unwrap(self) -> other::ItemRequest { other::ItemRequest }
}
impl api::Item {
    pub fn timeout(self, _: u8) -> other::ItemRequest { other::ItemRequest }
}
pub struct Shadow;
impl Shadow {
    pub fn item(&self) -> other::ItemRequest { other::ItemRequest }
}
impl Api {
    pub fn clone(&self) -> Shadow { Shadow }
}
pub fn builder_helper() -> u32 { Api::builder().into_locale().set_locale(None) } // PROBE:UNPROVED builder_helper
pub fn undeclared_reset(r: api::ItemRequest) -> u32 { r.reset_x().set_x(None) } // PROBE:UNPROVED undeclared_reset
pub fn unbound_timeout() -> u32 { Api::unbound().item().timeout(1).set_x(None) } // PROBE:UNPROVED unbound_timeout
pub fn user_unwrap(r: api::ItemRequest) -> u32 { r.unwrap().set_x(None) } // PROBE:UNPROVED user_unwrap
pub fn user_clone(client: Api) -> u32 { client.clone().item().set_x(None) } // PROBE:UNPROVED user_clone
pub fn genuine_builder() {
    let _ = Api::builder().omit_locale().set_locale(None).configure_http(|http| http).build(); // PROBE:GENUINE builder_chain
}
pub fn genuine_built() {
    let _ = Api::builder().build().expect("client").item().set_x(None); // PROBE:GENUINE built_chain
}
pub fn genuine_request(client: &Api) {
    let _ = client.item().omit_x().timeout(std::time::Duration::ZERO).set_x(None); // PROBE:GENUINE request_chain
}
pub fn genuine_unbound() {
    let _ = Api::unbound().item().omit_x().set_x(None); // PROBE:GENUINE unbound_chain
}
pub fn genuine_request_binding(client: &Api) {
    let _ = client.request(Api::unbound().item()).set_x(None); // PROBE:GENUINE request_binding
}
"#;
    assert_probe(
        POSTFIX_API,
        callers,
        &[("impl ApiBuilder {", "impl api::ApiBuilder {")],
        &[("C3-MIGRATE-BUILDER-PATH", "impl ApiBuilder {")],
    );
}

/// Post-fix review of F03, pattern macros beyond the reviewer's probe: a
/// macro in an `if let`, closure-parameter or tuple pattern may bind any name
/// it is given, so every name visible there becomes unknown. A later typed
/// binding is proved again, and a pattern macro in a match arm does not
/// affect the code after the `match`.
#[test]
fn postfix_f03_pattern_macros_hide_bindings() {
    let callers = r#"
macro_rules! bind { ($n:ident) => { $n }; }
pub fn if_let_macro(request: api::ItemRequest) -> u32 {
    let _keep = request;
    if let bind!(request) = other::ItemRequest { request.set_x(None) } else { 0 } // PROBE:UNPROVED if_let
}
pub fn closure_macro(request: api::ItemRequest) -> u32 {
    let _keep = request;
    let f = |bind!(request): other::ItemRequest| request.set_x(None); // PROBE:UNPROVED closure_parameter
    f(other::ItemRequest)
}
pub fn tuple_macro(request: api::ItemRequest) -> u32 {
    let _keep = request;
    let (bind!(request), _) = (other::ItemRequest, 1);
    request.set_x(None) // PROBE:UNPROVED tuple_member
}
pub fn after_macro(request: api::ItemRequest) -> u32 {
    let bind!(foreign) = other::ItemRequest;
    let again: api::ItemRequest = request;
    let _ = again.set_x(None); // PROBE:GENUINE typed_after_macro
    foreign.set_x(None) // PROBE:UNPROVED bound_by_macro
}
pub fn arm_scope(request: api::ItemRequest) {
    match 1 { bind!(_n) => {} }
    let _ = request.set_x(None); // PROBE:GENUINE after_match
}
"#;
    assert_probe(POSTFIX_API, callers, &[], &[]);
}

/// Post-fix review of F03, statement macros: one is binding-free only when
/// its path resolves to the standard library, `log` or `tracing`. A local
/// module named `log`, a crate-local `macro_rules!` with an allow-listed name
/// (`warn!`), an import from another crate, an allow-listed `log` name
/// without an import, a standard name under another crate's glob, and any
/// single-segment name next to `#[macro_use] extern crate` of another crate
/// make every earlier name unknown. `println!`, `std::println!`,
/// `::core::assert!`, `log::info!`, `tracing::warn!` and an imported
/// `tracing::debug` are binding-free.
#[test]
fn postfix_f03_statement_macros_must_resolve_to_std_log_or_tracing() {
    let callers = r#"
pub mod log {
    macro_rules! info_impl { ($n:ident) => { let $n = crate::other::ItemRequest; }; }
    pub(crate) use info_impl as info;
}
pub fn local_log_module(request: api::ItemRequest) -> u32 {
    let _keep = request;
    log::info!(request);
    request.set_x(None) // PROBE:UNPROVED local_module_named_log
}
mod listed_name {
    macro_rules! warn { ($n:ident) => { let $n = super::other::ItemRequest; }; }
    pub fn crate_macro(request: super::api::ItemRequest) -> u32 {
        let _keep = request;
        warn!(request);
        request.set_x(None) // PROBE:UNPROVED crate_local_macro_rules
    }
}
mod imported {
    use helpers::trace;
    pub fn other_crate(request: super::api::ItemRequest) -> u32 {
        let _keep = request;
        trace!(request);
        request.set_x(None) // PROBE:UNPROVED import_of_another_crate
    }
}
mod unimported {
    pub fn log_name(request: super::api::ItemRequest) -> u32 {
        let _keep = request;
        error!(request);
        request.set_x(None) // PROBE:UNPROVED log_name_without_import
    }
}
mod globbed {
    use helpers::*;
    pub fn std_name(request: super::api::ItemRequest) -> u32 {
        let _keep = request;
        println!(request);
        request.set_x(None) // PROBE:UNPROVED std_name_under_another_glob
    }
}
mod genuine {
    use tracing::debug;
    pub fn expression_macros(request: super::api::ItemRequest) {
        println!("unqualified std");
        std::println!("qualified std");
        ::core::assert!(true);
        log::info!("extern log");
        tracing::warn!("extern tracing");
        debug!("imported tracing");
        let _ = request.set_x(None); // PROBE:GENUINE after_expression_macros
    }
}
"#;
    assert_probe(POSTFIX_API, callers, &[], &[]);

    let macro_use = r#"
#[macro_use]
extern crate helpers;
pub fn std_name(request: api::ItemRequest) -> u32 {
    let _keep = request;
    println!(request);
    request.set_x(None) // PROBE:UNPROVED std_name_next_to_macro_use
}
"#;
    assert_probe(POSTFIX_API, macro_use, &[], &[]);
}

/// Post-fix review of F03, the adjacent class: v2.1 names the generated
/// builder at the level of the `api!` invocation (`ApiBuilder`), v3 in the
/// generated support module (`api::ApiBuilder`). A reference proved to be the
/// generated builder, whether defined there or reached through a glob, is
/// rewritten to the v3 path (`C3-MIGRATE-BUILDER-PATH`), and an import of it
/// is rewritten instead of its uses. An unrelated type with the same name is
/// untouched. A reference whose target is unknown (an opaque module) or whose
/// support-module name does not resolve at that place (a local item `api`
/// shadows it) is left unchanged and reported
/// (`C3-MIGRATE-BUILDER-PATH-UNRESOLVED`).
#[test]
fn postfix_f03_crate_level_builder_references_move_into_the_support_module() {
    let callers = r#"
impl ApiBuilder {
    pub fn keep(self) -> Self { self }
}
pub fn typed(builder: ApiBuilder) -> ApiBuilder { builder }
pub fn constructed() -> ApiBuilder { ApiBuilder::new() }
pub fn qualified() -> crate::ApiBuilder { crate::ApiBuilder::new() }
pub fn in_generics(builders: Vec<ApiBuilder>) -> Option<ApiBuilder> { builders.into_iter().next() }
mod nested {
    use super::ApiBuilder;
    use super::{Api, ApiBuilder as Renamed};
    pub fn imported(builder: ApiBuilder) -> Renamed { let _ = Api::builder(); builder }
    pub fn parent(builder: super::ApiBuilder) -> super::ApiBuilder { builder }
}
mod globbed {
    use super::*;
    pub fn through_glob(builder: ApiBuilder) -> ApiBuilder { builder }
}
mod unrelated {
    pub struct ApiBuilder;
    pub fn local(builder: ApiBuilder) -> ApiBuilder { builder }
}
mod support_name_taken {
    use super::*;
    pub struct api;
    pub fn shadowed(builder: ApiBuilder) -> u8 { let _ = builder; 0 }
}
mod opaque {
    helpers::items!();
    pub fn hidden(builder: ApiBuilder) -> u8 { let _ = builder; 0 }
}
"#;
    let rewritten = "C3-MIGRATE-BUILDER-PATH";
    let unresolved = "C3-MIGRATE-BUILDER-PATH-UNRESOLVED";
    let outcome = assert_probe(
        POSTFIX_API,
        callers,
        &[
            ("impl ApiBuilder {", "impl api::ApiBuilder {"),
            (
                "pub fn typed(builder: ApiBuilder) -> ApiBuilder { builder }",
                "pub fn typed(builder: api::ApiBuilder) -> api::ApiBuilder { builder }",
            ),
            (
                "pub fn constructed() -> ApiBuilder { ApiBuilder::new() }",
                "pub fn constructed() -> api::ApiBuilder { api::ApiBuilder::new() }",
            ),
            (
                "pub fn qualified() -> crate::ApiBuilder { crate::ApiBuilder::new() }",
                "pub fn qualified() -> crate::api::ApiBuilder { crate::api::ApiBuilder::new() }",
            ),
            (
                "pub fn in_generics(builders: Vec<ApiBuilder>) -> Option<ApiBuilder> {",
                "pub fn in_generics(builders: Vec<api::ApiBuilder>) -> Option<api::ApiBuilder> {",
            ),
            (
                "    use super::ApiBuilder;",
                "    use super::api::ApiBuilder;",
            ),
            (
                "    use super::{Api, ApiBuilder as Renamed};",
                "    use super::{Api, api::ApiBuilder as Renamed};",
            ),
            (
                "pub fn parent(builder: super::ApiBuilder) -> super::ApiBuilder { builder }",
                "pub fn parent(builder: super::api::ApiBuilder) -> super::api::ApiBuilder { builder }",
            ),
            (
                "pub fn through_glob(builder: ApiBuilder) -> ApiBuilder { builder }",
                "pub fn through_glob(builder: api::ApiBuilder) -> api::ApiBuilder { builder }",
            ),
        ],
        &[
            (rewritten, "impl ApiBuilder {"),
            (rewritten, "pub fn typed("),
            (rewritten, "pub fn typed("),
            (rewritten, "pub fn constructed("),
            (rewritten, "pub fn constructed("),
            (rewritten, "pub fn qualified("),
            (rewritten, "pub fn qualified("),
            (rewritten, "pub fn in_generics("),
            (rewritten, "pub fn in_generics("),
            (rewritten, "    use super::ApiBuilder;"),
            (rewritten, "ApiBuilder as Renamed"),
            (rewritten, "pub fn parent("),
            (rewritten, "pub fn parent("),
            (rewritten, "pub fn through_glob("),
            (rewritten, "pub fn through_glob("),
            (unresolved, "pub fn shadowed("),
            (unresolved, "pub fn hidden("),
        ],
    );
    for diagnostic in &outcome.diagnostics {
        if diagnostic.code.as_str() == unresolved {
            assert!(
                diagnostic.message.contains("left unchanged"),
                "{diagnostic:?}"
            );
            assert!(
                diagnostic
                    .suggestion
                    .as_deref()
                    .is_some_and(|suggestion| suggestion.contains("api::ApiBuilder")),
                "{diagnostic:?}"
            );
        }
    }
}

/// Second post-fix review of F03 (round 3): default-deny caller rewrites.
mod default_deny;

/// Third post-fix review of F03 (round 4).
mod round4;

/// Fourth post-fix review of F03 (round 5).
mod round5;

// Independent bounded-return review: declared names ending in _if_some.
mod suffix_names;

/// Bounded-return review §5.3: Concord dependency identity from the manifest.
mod dependency_identity;

/// R3-01: the bound request's v3-only `description()` shadowing an
/// application trait method (reviewer regressions).
mod description_shadow;
/// Focused review of R3 B01: v3-only generated names shadowing application
/// methods, and colliding v3 names of a v2.1 declaration.
mod generated_names;
/// R3-01 parity guard: the tool's v3-only name inventory against the
/// members the v2.1 and v3 generators actually emit.
mod surface_parity;

#[path = "tests/removed_member_review.rs"]
mod removed_member_review;
/// R5-01: removed members referenced as function values inside unexpanded
/// macros (reviewer regressions, reconstructed from the review brief).
#[path = "tests/removed_member_review_r5.rs"]
mod removed_member_review_r5;
/// R4-01: generated members v2.1 emitted and v3 removed (R5 tests).
mod removed_members;

// A namespace-dependent fixture needs its real original module graph. These
// helpers add context; captured input and golden output files remain immutable.
fn write_original_riot_context(workspace: &TempDir, riot: &str) -> PathBuf {
    workspace.write("src/lib.rs", "pub mod riot;\n");
    let mut files = Vec::new();
    golden_files(&golden_root(), &mut files);
    let generated = COMPILED_FIXTURES
        .iter()
        .map(|(_, target)| *target)
        .collect::<Vec<_>>();
    for file in files
        .iter()
        .filter(|file| file.starts_with("riot/") && !generated.contains(&file.as_str()))
    {
        let text =
            fs::read_to_string(golden_root().join(file)).expect("original Riot support source");
        workspace.write(&format!("src/{file}"), &text);
    }
    workspace.write(
        "src/riot/api/data_dragon.rs",
        &fixture("data_dragon_v2_1.rs.txt"),
    );
    workspace.write("src/riot/usage.rs", &fixture("riot_usage_v2_1.rs.txt"));
    workspace.write("src/riot/api/riot.rs", riot)
}

fn convert_riot_with_original_context(source: &str) -> SourceOutcome {
    let workspace = TempDir::new("riot-source-context");
    workspace.write(
        "Cargo.toml",
        "[package]\nname = \"riot-context\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    let path = write_original_riot_context(&workspace, source);
    let migration = migrate(&path, &Settings::default()).expect("original namespace context");
    let file = migration
        .files
        .into_iter()
        .find(|file| file.relative == "riot.rs")
        .expect("only requested Riot file");
    assert_eq!(
        file.original, source,
        "context must not change original fixture bytes"
    );
    SourceOutcome {
        converted: file.converted,
        edits: file.edits,
        declarations: migration.declarations,
        diagnostics: migration.diagnostics,
        identities: migration.identities,
    }
}

fn namespace_case(domain: Option<&str>, root_prefix: &str, import: &str) -> super::Migration {
    let workspace = TempDir::new("namespace-proof");
    workspace.write("Cargo.toml", "[package]\nname = \"namespace-proof\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dependencies]\nconcord = \"3\"\n");
    if let Some(domain) = domain {
        workspace.write("src/domain.rs", domain);
    }
    let source = format!(
        "{root_prefix}\nuse concord::api;\n{import}\napi! {{ client NamespaceProbe {{ base \"https://namespace.example.test/\"; timeout 5s; GET read() -> () via concord::Empty {{ path [\"ok\"]; }} }} }}\n"
    );
    workspace.write("src/lib.rs", &source);
    migrate(&workspace.0.join("src/lib.rs"), &Settings::default()).expect("namespace migration")
}

#[test]
fn s5_namespace_real_target_and_alias_positive_controls() {
    for import in [
        "use crate::domain::*;",
        "use crate::domain as exported; use exported::*;",
    ] {
        let migration =
            namespace_case(Some("pub struct DomainValue;\n"), "pub mod domain;", import);
        assert!(
            migration
                .converted("lib.rs")
                .unwrap()
                .contains("path \"ok\";")
        );
        assert!(
            !migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved)
        );
    }
    let original = fixture("riot_v2_1.rs.txt");
    let result = convert_riot_with_original_context(&original);
    assert!(errors(&result).is_empty(), "{:?}", errors(&result));
    assert_eq!(result.identities.len(), 46);
}

#[test]
fn s5_namespace_unknown_or_foreign_targets_never_supply_absence_proof() {
    for (label, domain) in [
        ("missing", None),
        ("external glob", Some("pub use unknown_external::*;\n")),
        ("local concord module", Some("pub mod concord {}\n")),
        (
            "pub reexport",
            Some("mod hidden { pub mod concord {} } pub use hidden::*;\n"),
        ),
        (
            "conditional binding",
            Some("#[cfg(feature=\"conditional\")] pub mod concord {}\n"),
        ),
        ("opaque macro", Some("make_namespace!();\n")),
        (
            "pending API",
            Some(
                "use concord::api; api! { client UnknownEmissions { base \"https://unknown.example.test/\"; GET f() -> () via concord::Empty { path [\"x\"]; } } }\n",
            ),
        ),
    ] {
        let migration = namespace_case(domain, "pub mod domain;", "use crate::domain::*;");
        let file = migration
            .files
            .iter()
            .find(|file| file.relative == "lib.rs")
            .unwrap();
        assert_eq!(
            file.original, file.converted,
            "{label}: no speculative edits"
        );
        assert!(
            migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved),
            "{label}: unresolved ownership must be reported"
        );
    }
    let repeated = namespace_case(
        Some("pub struct DomainValue;\n"),
        "#[path=\"domain.rs\"] mod one; #[path=\"domain.rs\"] mod two;",
        "use crate::one::*;",
    );
    let file = repeated
        .files
        .iter()
        .find(|file| file.relative == "lib.rs")
        .unwrap();
    assert_eq!(
        file.original, file.converted,
        "repeated mounts may not lend a single-origin proof"
    );
    assert!(
        repeated
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn s5_namespace_omitted_riot_context_stays_unresolved() {
    let source = fixture("riot_v2_1.rs.txt");
    let outcome = convert(&source);
    assert_eq!(outcome.converted, source);
    assert!(codes(&outcome).contains(&"C3-MIGRATE-API-MACRO-UNRESOLVED"));
}

#[test]
fn s5_namespace_macro_generated_without_glob_is_unresolved() {
    let workspace = TempDir::new("generated-namespace");
    let source = include_str!("../../tests/fixtures/trust_generated_namespace.rs");
    let input = workspace.write("input.rs", source);
    let migration = migrate(&input, &Settings::default()).expect("standalone source origin");
    let file = migration
        .files
        .iter()
        .find(|file| file.relative == "input.rs")
        .expect("input");
    assert_eq!(
        file.original, file.converted,
        "unknown producer must not authorize foreign token edits"
    );
    assert!(
        migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn s5_namespace_prefixes_and_exact_module_block_scopes() {
    let body = r#"use concord::api; use crate::domain::*;
api! { client ScopedNamespace { base "https://scope.example.test/"; timeout 5s;
GET f() -> () via concord::Empty { path ["scope"]; } } }"#;
    for prefix in [
        "",
        "\u{feff}",
        "#!/usr/bin/env rust-script\n",
        "\u{feff}#!/usr/bin/env rust-script\n",
    ] {
        for scope in [
            format!("mod nested {{ {body} }}"),
            format!("fn configure() {{ {body} }}"),
        ] {
            let workspace = TempDir::new("namespace-prefix");
            workspace.write("Cargo.toml", "[package]\nname=\"prefix-context\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n");
            workspace.write("src/domain.rs", "pub struct UnicodeContext;\n");
            let source =
                format!("{prefix}// Unicode Ï€ before anchored scopes\npub mod domain;\n{scope}\n");
            let input = workspace.write("src/lib.rs", &source);
            let migration = migrate(&input, &Settings::default()).expect("exact prefix scope");
            assert!(
                migration
                    .converted("lib.rs")
                    .expect("source")
                    .contains("path \"scope\";"),
                "prefix/scope failed: {:?}",
                migration
                    .diagnostics
                    .iter()
                    .map(|d| d.render())
                    .collect::<Vec<_>>()
            );
            assert!(
                !migration
                    .diagnostics
                    .iter()
                    .any(|d| d.code == Code::ApiMacroUnresolved)
            );
            let shadowed = source.replace("use concord::api;", "mod concord {} use concord::api;");
            workspace.write("src/lib.rs", &shadowed);
            let refused = migrate(&input, &Settings::default()).expect("local shadow");
            assert_eq!(refused.converted("lib.rs"), Some(shadowed.as_str()));
            assert!(
                refused
                    .diagnostics
                    .iter()
                    .any(|d| d.code == Code::ApiMacroUnresolved)
            );
        }
    }
}

#[test]
fn s5_namespace_declared_api_support_namespace_is_not_positive_origin_evidence() {
    let migration = namespace_case(None, "", "");
    assert!(
        !migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
    let workspace = TempDir::new("concord-support-shadow");
    workspace.write("Cargo.toml", "[package]\nname=\"support-shadow\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n");
    let source = "use concord::api; api! { client Concord { base \"https://scope.example.test/\"; GET f() -> () via concord::Empty { path [\"x\"]; } } }";
    let input = workspace.write("src/lib.rs", source);
    let migration = migrate(&input, &Settings::default()).expect("potential generated namespace");
    assert_eq!(migration.converted("lib.rs"), Some(source));
    assert!(
        migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn s5_namespace_source_path_and_bytes_must_match_actual_index() {
    let workspace = TempDir::new("namespace-exact-source");
    let source = "use concord::api; api! { client ExactSource { base \"https://exact.example.test/\"; GET f() -> () via concord::Empty { path [\"x\"]; } } }";
    let path = workspace.write("input.rs", source);
    let layout = super::resolve::Layout {
        base: workspace.0.clone(),
        standalone: true,
        ..Default::default()
    };
    let index = DeclIndex::build(&layout, &[(path.clone(), source.to_owned())]);
    let control = super::migrate_source_at(
        "input.rs",
        source,
        &index,
        Some(&path),
        &Settings::default(),
    );
    assert!(
        control.converted.contains("path \"x\";"),
        "{:?}",
        errors(&control)
    );
    let changed = format!("generate_namespace!();\n{source}");
    workspace.write("input.rs", &changed);
    for (selected, bytes) in [
        (&path, changed.as_str()),
        (&workspace.0.join("unindexed.rs"), source),
    ] {
        let result = super::migrate_source_at(
            "input.rs",
            bytes,
            &index,
            Some(selected),
            &Settings::default(),
        );
        assert_eq!(result.converted, bytes);
        assert_eq!(result.edits, 0);
        assert!(codes(&result).contains(&"C3-MIGRATE-API-MACRO-UNRESOLVED"));
    }
}

#[test]
fn s5_namespace_self_alias_is_bound_to_library_target_and_source_macro_origin() {
    let foreign = include_str!("../../tests/fixtures/trust_foreign_self_binary.rs");
    for target in [
        "src/main.rs",
        "src/bin/foreign.rs",
        "examples/foreign.rs",
        "tests/foreign.rs",
        "src/lib.rs",
    ] {
        let workspace = TempDir::new("namespace-own-target");
        workspace.write(
            "Cargo.toml",
            "[package]\nname=\"concord\"\nversion=\"3.0.0\"\nedition=\"2024\"\n",
        );
        if target != "src/lib.rs" {
            workspace.write("src/lib.rs", "pub struct LibraryMarker;\n");
        }
        let input = workspace.write(target, foreign);
        let migration = migrate(&input, &Settings::default()).expect("actual own target");
        let file = migration
            .files
            .iter()
            .find(|file| file.original == foreign)
            .expect("foreign target");
        assert_eq!(
            file.converted, foreign,
            "{target}: package name is not macro origin"
        );
        assert!(
            migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved),
            "{target}"
        );
    }
}

#[test]
fn s5_namespace_other_attribute_derive_and_statement_expansions_remain_unknown() {
    for prefix in [
        "#[unknown_attribute] struct Source;",
        "#[derive(UnknownDerive)] struct Source;",
        "generate_namespace!();",
    ] {
        let migration = namespace_case(None, prefix, "");
        let file = migration
            .files
            .iter()
            .find(|file| file.relative == "lib.rs")
            .expect("input");
        assert_eq!(file.original, file.converted, "{prefix}");
        assert!(
            migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved),
            "{prefix}"
        );
    }
}

#[test]
fn s5_namespace_own_library_canonical_macro_reexports_use_resolved_origins() {
    for prefix in [
        "pub use ::concord_macros::__api as api;",
        "pub use ::concord_macros::api;",
        "extern crate concord_macros as canonical; pub use canonical::__api as api;",
    ] {
        let workspace = TempDir::new("canonical-own-macro");
        workspace.write("Cargo.toml", "[package]\nname=\"concord\"\nversion=\"3.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord_macros={package=\"concord-macros\",version=\"3\"}\n");
        let source = format!(
            "{prefix}\nextern crate self as concord;\nconcord::api! {{ client CanonicalSource {{ base \"https://canonical.example.test/\"; GET f() -> () via concord::Empty {{ path [\"x\"]; }} }} }}\n"
        );
        let input = workspace.write("src/lib.rs", &source);
        let migration = migrate(&input, &Settings::default()).expect("resolved canonical source");
        assert!(
            migration
                .converted("lib.rs")
                .expect("library")
                .contains("path \"x\";"),
            "{prefix}: {:?}",
            migration
                .diagnostics
                .iter()
                .map(|d| d.render())
                .collect::<Vec<_>>()
        );
        assert!(
            !migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved),
            "{prefix}"
        );
    }
    for prefix in [
        "pub use ::application_macros::api;",
        "mod concord_macros { pub use crate::application_api as api; } pub use concord_macros::api;",
        "#[cfg(feature=\"maybe\")] pub use ::concord_macros::api;",
    ] {
        let workspace = TempDir::new("noncanonical-own-macro");
        workspace.write(
            "Cargo.toml",
            "[package]\nname=\"concord\"\nversion=\"3.0.0\"\nedition=\"2024\"\n",
        );
        let source = format!(
            "{prefix}\nextern crate self as concord;\nconcord::api! {{ client ForeignSource {{ base \"https://foreign.example.test/\"; GET f() -> () {{ path [\"foreign\"]; }} }} }}\n"
        );
        let input = workspace.write("src/lib.rs", &source);
        let migration = migrate(&input, &Settings::default()).expect("foreign source");
        assert_eq!(
            migration.converted("lib.rs"),
            Some(source.as_str()),
            "{prefix}"
        );
        assert!(
            migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved),
            "{prefix}"
        );
    }
}

#[test]
fn s5_known_standard_macro_and_attribute_origins_do_not_create_namespace_opacity() {
    let workspace = TempDir::new("known-source-origins");
    workspace.write("Cargo.toml", "[package]\nname=\"known-origins\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\ntokio=\"1\"\n");
    let source = r#"use concord::{api, Empty};
thread_local! { static SOURCE_COUNTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[derive(Clone)] struct SourceValue;
#[tokio::test] async fn source_test() {}
#[test] fn ordinary_test() {}
api! { client KnownOrigins { base "https://origins.example.test/";
GET f(x?: u32) -> () via Empty { path ["x"]; query { "x" = endpoint.x; } } } }
fn exact(r: known_origins::FRequest) { let _ = r.set_x(None); }
"#;
    let input = workspace.write("src/lib.rs", source);
    let migration = migrate(&input, &Settings::default()).expect("known origins");
    let output = migration.converted("lib.rs").expect("library");
    assert!(output.contains("path \"x\";"));
    assert!(output.contains("r.replace_x(None)"));
    assert!(
        !migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn s5_impl_only_macro_namespace_proof_preserves_trait_incompleteness() {
    let workspace = TempDir::new("impl-namespace-separation");
    workspace.write("Cargo.toml", "[package]\nname=\"impl-source\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n");
    let source = r#"use concord::{api, Empty};
api! { client ImplSource { base "https://impl.example.test/";
GET f(x?: u32) -> () via Empty { path ["x"]; query { "x" = endpoint.x; } } } }
pub trait Ext { fn set_x(self, _: Option<u32>) -> u32 where Self: Sized { 1 } }
macro_rules! impl_only { ($t:ty) => { impl Ext for $t {} }; }
impl_only!(::std::boxed::Box<impl_source::FRequest>);
fn guarded(r: ::std::boxed::Box<impl_source::FRequest>) { let _ = r.set_x(None); }
"#;
    let input = workspace.write("src/lib.rs", source);
    let migration = migrate(&input, &Settings::default()).expect("impl-only context");
    let output = migration.converted("lib.rs").expect("library");
    assert!(output.contains("path \"x\";"));
    assert!(output.contains("r.set_x(None)"));
    assert!(
        !migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
    assert!(
        migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::SetterUnresolved && d.message.contains("impl_only!")),
        "{:?}",
        migration
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    // Root expansion leaves ordinary lexical lookup uncertain; the explicit
    // std Box path above proves this receiver before testing trait uncertainty.
    // Removing the invocation must let that same receiver rewrite, proving
    // the negative reached actual crate trait incompleteness rather than an
    // unrelated unresolved receiver or an absent generated setter.
    let ordinary = source.replace("impl_only!(::std::boxed::Box<impl_source::FRequest>);", "");
    workspace.write("src/lib.rs", &ordinary);
    let control = migrate(&input, &Settings::default()).expect("no impl invocation");
    let ordinary_output = control.converted("lib.rs").expect("library");
    assert!(ordinary_output.contains("r.replace_x(None)"));
    assert!(
        !control
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
    assert!(
        !control
            .diagnostics
            .iter()
            .any(|d| d.code == Code::SetterUnresolved)
    );
    // Keep the producer at the root: a sibling-only mod would not shadow
    // this root's Concord import and would be a different namespace case.
    let producer = source.replace("impl Ext for $t {}", "mod concord {} impl Ext for $t {}");
    workspace.write("src/lib.rs", &producer);
    let refused = migrate(&input, &Settings::default()).expect("namespace producer");
    assert_eq!(refused.converted("lib.rs"), Some(producer.as_str()));
    assert!(
        refused
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn s5_unproved_declaration_names_provide_only_negative_caller_diagnostics() {
    let workspace = TempDir::new("diagnostic-only-namespace");
    workspace.write("Cargo.toml", "[package]\nname=\"negative-names\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\npath=\"src/actual.rs\"\n");
    workspace.write(
        "src/actual.rs",
        "pub struct Foreign; impl Foreign { pub fn set_x(self, _: Option<u32>) {} }\n",
    );
    workspace.write("src/lib.rs", default_deny::REVIEW2_API);
    let source = "fn call(r: negative_names::Foreign) { r.set_x(None); }\n";
    workspace.write("tests/caller.rs", source);
    let migration = migrate(&workspace.0, &Settings::default()).expect("negative-only names");
    assert_eq!(migration.converted("tests/caller.rs"), Some(source));
    assert!(
        migration
            .diagnostics
            .iter()
            .any(|d| d.file == "tests/caller.rs"
                && d.code == Code::SetterUnresolved
                && d.message.contains("not generated by this crate"))
    );
}

#[test]
fn s5_thread_local_forwarded_item_attributes_do_not_borrow_standard_authority() {
    for payload in [
        "#[unknown_attribute] static VALUE: u32 = 0;",
        "#[cfg_attr(feature=\"maybe\", unknown_attribute)] static VALUE: u32 = 0;",
    ] {
        let prefix = format!("thread_local! {{ {payload} }}");
        let migration = namespace_case(None, &prefix, "");
        let file = migration
            .files
            .iter()
            .find(|file| file.relative == "lib.rs")
            .expect("input");
        assert_eq!(file.original, file.converted, "{payload}");
        assert!(
            migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved)
        );
    }
    let ordinary = namespace_case(
        None,
        "thread_local! { #[allow(non_upper_case_globals)] static VALUE: u32 = 0; }",
        "",
    );
    assert!(
        ordinary
            .converted("lib.rs")
            .expect("input")
            .contains("path \"ok\";")
    );
}

#[test]
fn s5_unmodeled_metavariable_token_producers_never_supply_namespace_absence() {
    for template in ["${concat(namespace)}", "$$", "$[tokens]"] {
        let prefix = format!(
            "macro_rules! unknown_template {{ () => {{ {template} }}; }} unknown_template!();"
        );
        let migration = namespace_case(None, &prefix, "");
        let file = migration
            .files
            .iter()
            .find(|file| file.relative == "lib.rs")
            .expect("input");
        assert_eq!(
            file.original, file.converted,
            "unmodeled transcriber: {template}"
        );
        assert!(
            migration
                .diagnostics
                .iter()
                .any(|d| d.code == Code::ApiMacroUnresolved)
        );
    }
}

#[test]
fn s5_reflexive_self_import_never_invents_a_canonical_api_provider() {
    let workspace = TempDir::new("self-glob-foreign-source");
    workspace.write(
        "Cargo.toml",
        "[package]\nname=\"concord\"\nversion=\"3.0.0\"\nedition=\"2024\"\n",
    );
    let source = include_str!("../../tests/fixtures/trust_self_glob_api.rs");
    let input = workspace.write("src/lib.rs", source);
    let migration = migrate(&input, &Settings::default()).expect("actual library root");
    assert_eq!(migration.converted("lib.rs"), Some(source));
    assert!(
        migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn s5_unknown_standard_item_token_producers_do_not_supply_namespace_proof() {
    let source = include_str!("../../tests/fixtures/trust_std_cfg_select.rs");
    let workspace = TempDir::new("unknown-standard-producer");
    let input = workspace.write("input.rs", source);
    let migration = migrate(&input, &Settings::default()).expect("standard source prefix");
    assert_eq!(migration.converted("input.rs"), Some(source));
    assert!(
        migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn s5_thread_local_initializers_preserve_whole_crate_trait_uncertainty() {
    let workspace = TempDir::new("thread-local-impl-input");
    workspace.write("Cargo.toml", "[package]\nname=\"thread-impl\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n");
    let review_api = default_deny::REVIEW2_API;
    let source = format!(
        "pub mod generated {{\n{review_api}\n}}\npub trait Ext {{ fn set_x(self, _: Option<u32>) -> u32 where Self:Sized {{ 0 }} }}\nmacro_rules! add_impl {{ () => {{ {{ impl Ext for Box<generated::api::ItemRequest> {{}} 0 }} }}; }}\nstd::thread_local! {{ static VALUE: u32 = add_impl!(); }}\nfn call(r: Box<generated::api::ItemRequest>) {{ let _ = r.set_x(None); }}\n"
    );
    let input = workspace.write("src/lib.rs", &source);
    let migration = migrate(&input, &Settings::default()).expect("forwarded initializer");
    let output = migration.converted("lib.rs").expect("source");
    assert!(output.contains("r.set_x(None)"));
    assert!(
        !migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
    assert!(
        migration
            .diagnostics
            .iter()
            .any(|d| d.code == Code::SetterUnresolved && d.message.contains("thread_local!"))
    );
    let ordinary = source.replace("static VALUE: u32 = add_impl!();", "static VALUE: u32 = 0;");
    workspace.write("src/lib.rs", &ordinary);
    let control = migrate(&input, &Settings::default()).expect("ordinary initializer");
    assert!(
        control
            .converted("lib.rs")
            .expect("source")
            .contains("r.replace_x(None)")
    );
}

#[test]
fn bare_macro_use_authority_requires_stable_import_and_invocation_sources() {
    let workspace = TempDir::new("bare-api-source-origin");
    workspace.write("Cargo.toml", "[package]\nname=\"bare-origin\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n");
    let declaration = r#"api! { client Api { base "http://bare.example.test/";
GET item(x?: u32) -> () via ::concord::Empty { path ["item"]; query { "x" = endpoint.x; } } } }
fn call(r: api::ItemRequest) { let _ = r.set_x(None); }
"#;
    for prefix in [
        "",
        "#[allow(unused_imports)]\n",
        "#[cfg_attr(any(), unknown_attribute)]\n",
    ] {
        let source = format!("{prefix}#[macro_use]\nextern crate concord;\n{declaration}");
        let input = workspace.write("src/lib.rs", &source);
        let result = migrate(&input, &Settings::default()).expect("genuine macro-use origin");
        let output = result.converted("lib.rs").expect("source");
        assert!(output.contains("path \"item\";"));
        assert!(output.contains("r.replace_x(None)"));
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|entry| entry.code == Code::ApiMacroUnresolved)
        );
    }
    for (prefix, item_attribute, extra) in [
        ("#[unknown_attribute]\n", "", ""),
        (
            "#[cfg_attr(feature=\"maybe\", unknown_attribute)]\n",
            "",
            "",
        ),
        ("", "#[unknown_attribute]\n", ""),
        (
            "",
            "#[cfg_attr(feature=\"maybe\", unknown_attribute)]\n",
            "",
        ),
        (
            "",
            "",
            "macro_rules! possible_api { () => { macro_rules! api { () => {} } } } possible_api!();\n",
        ),
    ] {
        let source = format!(
            "{prefix}#[macro_use]\nextern crate concord;\n{extra}{item_attribute}{declaration}"
        );
        let input = workspace.write("src/lib.rs", &source);
        let result = migrate(&input, &Settings::default()).expect("unknown source origin");
        assert_eq!(result.converted("lib.rs"), Some(source.as_str()));
        assert_eq!(result.declarations, 0);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|entry| entry.code == Code::ApiMacroUnresolved)
        );
    }
}

#[test]
fn bare_macro_use_cannot_ignore_child_macro_namespace_authority() {
    let workspace = TempDir::new("bare-api-child-origin");
    workspace.write("Cargo.toml", "[package]\nname=\"bare-child-origin\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n");
    let source = "#[macro_use]\nextern crate concord;\nmod hidden;\napi! { client Api { base \"http://bare.example.test/\"; GET item() -> () via ::concord::Empty { path [\"item\"]; } } }\n";
    let input = workspace.write("src/lib.rs", source);
    let missing = migrate(&input, &Settings::default()).unwrap();
    assert_eq!(missing.converted("lib.rs"), Some(source));
    assert_eq!(missing.declarations, 0);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
    workspace.write("src/hidden.rs", "pub fn inert() {}\n");
    let complete = migrate(&workspace.0, &Settings::default()).unwrap();
    assert_eq!(complete.declarations, 1);
    assert!(
        complete
            .converted("src/lib.rs")
            .unwrap()
            .contains("path \"item\";")
    );
    assert!(
        !complete
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
    workspace.write("src/hidden.rs", "#[unknown_attribute]\nfn supply() {}\n");
    let attributed = migrate(&workspace.0, &Settings::default()).unwrap();
    assert_eq!(attributed.declarations, 0);
    assert_eq!(attributed.converted("src/lib.rs"), Some(source));
    assert!(
        attributed
            .diagnostics
            .iter()
            .any(|d| d.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn bare_macro_authority_cannot_ignore_cfg_attr_selected_child_paths() {
    let workspace = TempDir::new("bare-api-gated-child-origin");
    workspace.write("Cargo.toml", "[package]\nname=\"bare-gated-origin\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord=\"3\"\n[features]\nalt=[]\n");
    workspace.write("src/hidden.rs", "pub fn inert() {}\n");
    workspace.write(
        "src/alt.rs",
        include_str!("../../tests/fixtures/trust_bare_unread_export.rs"),
    );
    let declaration = include_str!("../../tests/fixtures/trust_bare_unread.rs");
    for import in [
        "#[macro_use]\nextern crate concord;",
        "extern crate concord;\nuse concord::*;",
    ] {
        for attribute in [
            "#[cfg_attr(all(), path=\"alt.rs\")]",
            "#[cfg_attr(feature=\"alt\", path=\"alt.rs\")]",
            "#[cfg_attr(all(), cfg_attr(all(), path=\"alt.rs\"))]",
        ] {
            let source = declaration
                .replace("#[macro_use]\nextern crate concord;", import)
                .replace("mod hidden;", &format!("{attribute}\nmod hidden;"));
            workspace.write("src/main.rs", &source);
            let result = migrate(&workspace.0, &Settings::default()).unwrap();
            assert_eq!(result.converted("src/main.rs"), Some(source.as_str()));
            assert_eq!(result.declarations, 0);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|entry| entry.code == Code::ApiMacroUnresolved)
            );
        }
    }
    // A constant-false path attribute cannot select the foreign sibling.
    let source = declaration.replace(
        "mod hidden;",
        "#[cfg_attr(any(), path=\"alt.rs\")]\nmod hidden;",
    );
    workspace.write("src/main.rs", &source);
    let result = migrate(&workspace.0, &Settings::default()).unwrap();
    assert_eq!(result.declarations, 1);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|entry| entry.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn own_library_macro_export_retains_traversed_source_provenance() {
    let workspace = TempDir::new("own-library-gated-export");
    workspace.write("Cargo.toml", "[package]\nname=\"concord\"\nversion=\"3.0.0\"\nedition=\"2024\"\n[dependencies]\nconcord_macros={package=\"concord-macros\",version=\"3\"}\n");
    workspace.write("src/hidden.rs", "pub(crate) use concord_macros::api;\n");
    workspace.write(
        "src/alt.rs",
        "#[unknown_attribute]\nstruct Placeholder;\npub(crate) use api;\n",
    );
    let declaration = "concord::api! { client Foreign { base \"http://foreign.example.test/\"; GET f() -> () { path [\"foreign\"]; } } }\n";
    for attribute in [
        r#"#[cfg_attr(all(), path="alt.rs")]"#,
        r#"#[cfg_attr(all(), cfg_attr(all(), path="alt.rs"))]"#,
        r#"#[cfg_attr(true, path="alt.rs")]"#,
        r#"#[cfg_attr(all(true, not(false)), path="alt.rs")]"#,
    ] {
        let source = format!(
            "extern crate self as concord;\n{attribute}\nmod hidden;\npub(crate) use hidden::api;\n{declaration}"
        );
        workspace.write("src/lib.rs", &source);
        let result = migrate(&workspace.0, &Settings::default()).unwrap();
        assert_eq!(result.converted("src/lib.rs"), Some(source.as_str()));
        assert_eq!(result.declarations, 0);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|entry| entry.code == Code::ApiMacroUnresolved)
        );
    }
    // A canonical strong root import does not derive authority from the
    // unrelated unread child's namespace.
    let source = format!(
        "pub use ::concord_macros::api;\nextern crate self as concord;\nmod unrelated;\n{declaration}"
    );
    workspace.write("src/lib.rs", &source);
    let result = migrate(&workspace.0, &Settings::default()).unwrap();
    assert_eq!(result.declarations, 1);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|entry| entry.code == Code::ApiMacroUnresolved)
    );
}

#[test]
fn boolean_cfg_attributes_preserve_path_and_macro_uncertainty() {
    use super::resolve::{attribute_applies, attribute_effects};
    let attrs = |source: &str| {
        syn::parse_file(source)
            .unwrap()
            .items
            .into_iter()
            .find_map(|item| {
                if let syn::Item::Mod(module) = item {
                    Some(module.attrs)
                } else {
                    None
                }
            })
            .unwrap()
    };
    for predicate in ["true", "all(true, not(false))", "any(false, true)"] {
        let attributes = attrs(&format!(
            "#[cfg_attr({predicate}, path=\"other.rs\", unknown_macro, macro_use)] mod child;"
        ));
        let effects = attribute_effects(&attributes);
        assert!(effects.gated_path);
        assert!(effects.may_be_macro());
        assert_eq!(attribute_applies(&attributes, "macro_use"), Some(true));
    }
    for predicate in ["false", "all(true, false)", "not(true)"] {
        let attributes = attrs(&format!(
            "#[cfg_attr({predicate}, path=\"other.rs\", unknown_macro, macro_use)] mod child;"
        ));
        let effects = attribute_effects(&attributes);
        assert!(!effects.gated_path);
        assert!(!effects.may_be_macro());
        assert_eq!(attribute_applies(&attributes, "macro_use"), None);
    }
    // Future/unreadable predicate syntax supplies no negative namespace proof.
    let attributes = attrs("#[cfg_attr(42, path=\"other.rs\", unknown_macro)] mod child;");
    let effects = attribute_effects(&attributes);
    assert!(effects.gated_path && effects.gated && effects.unreadable_cfg_attr);
    assert!(effects.may_be_macro());
    assert_eq!(attribute_applies(&attributes, "macro_use"), Some(false));
}
