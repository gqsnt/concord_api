//! `docs/reference/reserved-names.json`: the names a description may use, as data.
//!
//! Three checks keep the file honest. It is written from the macro's own tables and from
//! the examples below, and must equal what is in the repository. A reader written here
//! from the file's text alone (it parses the JSON and applies its rules with its own code,
//! never the macro's functions) predicts the macro's verdict on every example, and on every
//! word of the repository's sources and pages, in every position. And every item that the
//! macro writes at the root of a client's module is tried as a child of the client.
use super::*;
use std::collections::BTreeSet;

/// Where a name is tried. Each probe is a description with `NAME` in that position; the
/// scopes around the name and the children of its node are listed, and a position that
/// inherits takes the rules of the position it names.
struct Position {
    id: &'static str,
    what: &'static str,
    probe: &'static str,
    enclosing: &'static [&'static str],
    children: &'static [&'static str],
    inherits: Option<&'static str>,
}

const POSITIONS: [Position; 15] = [
    Position {
        id: "client",
        what: "the name of the client",
        probe: r#"client NAME { base "https://a.test"; GET probe(); }"#,
        enclosing: &[],
        children: &["probe"],
        inherits: None,
    },
    Position {
        id: "client_input",
        what: "a required input of the client",
        probe: r#"client Probe(NAME: String) { base "https://a.test"; query { NAME; } GET probe(); }"#,
        enclosing: &[],
        children: &["probe"],
        inherits: None,
    },
    Position {
        id: "client_option",
        what: "an optional input of the client",
        probe: r#"client Probe(NAME?: String) { base "https://a.test"; query { NAME; } GET probe(); }"#,
        enclosing: &[],
        children: &["probe"],
        inherits: None,
    },
    Position {
        id: "scope_in_client",
        what: "a scope that is a child of the client",
        probe: r#"client Probe { base "https://a.test"; scope NAME { GET probe(); } }"#,
        enclosing: &[],
        children: &[],
        inherits: None,
    },
    Position {
        id: "scope_in_scope",
        what: "a scope that is a child of a scope",
        probe: r#"client Probe { base "https://a.test"; scope outer { scope NAME { GET probe(); } } }"#,
        enclosing: &["outer"],
        children: &[],
        inherits: None,
    },
    Position {
        id: "scope_input",
        what: "a required input of a scope",
        probe: r#"client Probe { base "https://a.test"; scope outer(NAME: String) { query { NAME; } GET probe(); } }"#,
        enclosing: &["outer"],
        children: &["probe"],
        inherits: None,
    },
    Position {
        id: "scope_option",
        what: "an optional input of a scope",
        probe: r#"client Probe { base "https://a.test"; scope outer(NAME?: String) { query { NAME; } GET probe(); } }"#,
        enclosing: &["outer"],
        children: &["probe"],
        inherits: None,
    },
    Position {
        id: "endpoint_in_client",
        what: "an endpoint that is a child of the client",
        probe: r#"client Probe { base "https://a.test"; GET NAME(); }"#,
        enclosing: &[],
        children: &[],
        inherits: None,
    },
    Position {
        id: "endpoint_in_scope",
        what: "an endpoint that is a child of a scope",
        probe: r#"client Probe { base "https://a.test"; scope outer { GET NAME(); } }"#,
        enclosing: &["outer"],
        children: &[],
        inherits: None,
    },
    Position {
        id: "endpoint_input",
        what: "a required input of an endpoint",
        probe: r#"client Probe { base "https://a.test"; scope outer { GET probe(NAME: String) { query { NAME; } } } }"#,
        enclosing: &["outer"],
        children: &[],
        inherits: None,
    },
    Position {
        id: "endpoint_option",
        what: "an optional input of an endpoint",
        probe: r#"client Probe { base "https://a.test"; scope outer { GET probe(NAME?: String) { query { NAME; } } } }"#,
        enclosing: &["outer"],
        children: &[],
        inherits: None,
    },
    Position {
        id: "part",
        what: "the name of a part",
        probe: r#"client Probe { base "https://a.test"; part NAME(shared?: String) { query { shared; } } GET probe(..NAME); }"#,
        enclosing: &[],
        children: &[],
        inherits: None,
    },
    Position {
        id: "part_input",
        what: "a required input of a part that an endpoint spreads: an input of that endpoint",
        probe: r#"client Probe { base "https://a.test"; part shared(NAME: String) { query { NAME; } } scope outer { GET probe(..shared); } }"#,
        enclosing: &["outer"],
        children: &[],
        inherits: Some("endpoint_input"),
    },
    Position {
        id: "part_option",
        what: "an optional input of a part that an endpoint spreads: an optional input of that endpoint",
        probe: r#"client Probe { base "https://a.test"; part shared(NAME?: String) { query { NAME; } } scope outer { GET probe(..shared); } }"#,
        enclosing: &["outer"],
        children: &[],
        inherits: Some("endpoint_option"),
    },
    Position {
        id: "part_option_in_scope",
        what: "an optional input of a part that a scope spreads: an optional input of that scope",
        probe: r#"client Probe { base "https://a.test"; part shared(NAME?: String) { query { NAME; } } scope outer { scope inner(..shared) { GET probe(); } } }"#,
        enclosing: &["outer", "inner"],
        children: &["probe"],
        inherits: Some("scope_option"),
    },
];

const CHILDREN: [&str; 4] = [
    "scope_in_client",
    "scope_in_scope",
    "endpoint_in_client",
    "endpoint_in_scope",
];
const INPUTS: [&str; 6] = [
    "client_input",
    "client_option",
    "scope_input",
    "scope_option",
    "endpoint_input",
    "endpoint_option",
];
const OPTIONS: [&str; 3] = ["client_option", "scope_option", "endpoint_option"];

/// How a reservation matches a name.
enum Match {
    Names(Vec<&'static str>),
    Prefix(&'static str),
    Types(Vec<String>),
    Enclosing,
    Children,
}

struct Reservation {
    id: &'static str,
    what: &'static str,
    positions: Vec<&'static str>,
    matches: Match,
    accept: &'static [(&'static str, &'static str)],
    refuse: &'static [(&'static str, &'static str)],
}

/// The receivers' methods: those of every receiver, then each one's own, sorted.
fn methods(own: &[&'static str]) -> Vec<&'static str> {
    let mut all: Vec<_> = name::EVERY_RECEIVER.iter().chain(own).copied().collect();
    all.sort_unstable();
    all
}

/// The items at the root of a client's module, read from the module the macro writes.
fn root_items() -> Vec<String> {
    let file: syn::File = syn::parse2(crate::expand(
        r#"pub client Minimal { base "https://a.test"; }"#.parse().unwrap(),
    ))
    .unwrap();
    let module = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Mod(module) if module.ident == "minimal" => Some(module),
            _ => None,
        })
        .unwrap();
    let mut root: Vec<String> = module
        .content
        .as_ref()
        .unwrap()
        .1
        .iter()
        .filter_map(|item| match item {
            syn::Item::Struct(item) => Some(item.ident.to_string()),
            syn::Item::Trait(item) => Some(item.ident.to_string()),
            syn::Item::Type(item) => Some(item.ident.to_string()),
            syn::Item::Enum(item) => Some(item.ident.to_string()),
            syn::Item::Mod(item) => Some(item.ident.to_string()),
            syn::Item::Const(item) => Some(item.ident.to_string()),
            syn::Item::Fn(item) => Some(item.sig.ident.to_string()),
            _ => None,
        })
        .filter(|ident| !ident.starts_with("__"))
        .collect();
    root.sort_unstable();
    root
}

fn reservations() -> Vec<Reservation> {
    let both = |a: &[&'static str], b: &[&'static str]| a.iter().chain(b).copied().collect();
    vec![
        Reservation {
            id: "keywords",
            what: "the Rust keywords, raw (`r#type`) or not; `Self` is here for the client's module name",
            positions: POSITIONS
                .iter()
                .filter(|p| p.id != "client" && p.inherits.is_none())
                .map(|p| p.id)
                .collect(),
            matches: Match::Names(name::KEYWORDS.to_vec()),
            accept: &[("endpoint_input", "kind"), ("scope_in_client", "matches")],
            refuse: &[
                ("endpoint_input", "type"),
                ("endpoint_input", "r#type"),
                ("scope_in_client", "match"),
                ("part", "self"),
                ("part_option", "loop"),
            ],
        },
        Reservation {
            id: "client_owner",
            what: "the name by which the client's inputs are written (`client.key`)",
            positions: both(&INPUTS, &["scope_in_client", "scope_in_scope"]),
            matches: Match::Names(vec!["client"]),
            accept: &[("endpoint_in_client", "client"), ("part", "client")],
            refuse: &[
                ("endpoint_input", "client"),
                ("scope_in_client", "client"),
                ("part_input", "client"),
            ],
        },
        Reservation {
            id: "enclosing_scopes",
            what: "the names of the scopes above the name, and for an input of a scope, that scope's own name (the position's `enclosing`)",
            positions: vec![
                "scope_input",
                "scope_option",
                "endpoint_input",
                "endpoint_option",
                "scope_in_scope",
            ],
            matches: Match::Enclosing,
            accept: &[("endpoint_in_scope", "outer"), ("scope_in_client", "outer")],
            refuse: &[
                ("endpoint_input", "outer"),
                ("scope_input", "outer"),
                ("scope_in_scope", "outer"),
                ("part_option", "outer"),
                ("part_option_in_scope", "inner"),
            ],
        },
        Reservation {
            id: "client_methods",
            what: "the methods of a client: a child of the client or an optional input of the client would add one",
            positions: vec!["scope_in_client", "endpoint_in_client", "client_option"],
            matches: Match::Names(methods(&name::CLIENT_METHODS)),
            accept: &[("endpoint_in_scope", "new"), ("client_input", "new")],
            refuse: &[("endpoint_in_client", "new"), ("client_option", "clone")],
        },
        Reservation {
            id: "handle_methods",
            what: "the methods of a scope's handle: a child of a scope or an optional input of a scope would add one",
            positions: vec!["scope_in_scope", "endpoint_in_scope", "scope_option"],
            matches: Match::Names(methods(&name::HANDLE_METHODS)),
            accept: &[("scope_input", "call")],
            refuse: &[
                ("endpoint_in_scope", "call"),
                ("scope_option", "clone"),
                ("part_option_in_scope", "call"),
            ],
        },
        Reservation {
            id: "call_methods",
            what: "the methods of a call: an optional input of an endpoint would add one",
            positions: vec!["endpoint_option"],
            matches: Match::Names(methods(&name::CALL_METHODS)),
            accept: &[
                ("endpoint_input", "send"),
                ("endpoint_in_scope", "pages"),
                ("part_input", "send"),
            ],
            refuse: &[
                ("endpoint_option", "send"),
                ("endpoint_option", "at_url"),
                ("part_option", "send"),
            ],
        },
        Reservation {
            id: "settings",
            what: "the prefix of the settings that Concord may add to a client, a handle or a call",
            positions: both(&CHILDREN, &OPTIONS),
            matches: Match::Prefix(name::SETTER_PREFIX),
            accept: &[
                ("endpoint_input", "with_tag"),
                ("client_input", "with_tag"),
                ("part", "with_tag"),
            ],
            refuse: &[
                ("endpoint_option", "with_tag"),
                ("endpoint_in_client", "with_tag"),
                ("scope_in_scope", "with_tag"),
                ("part_option", "with_tag"),
            ],
        },
        Reservation {
            id: "root_items",
            what: "the items at the root of a client's module: a child of the client whose type is one of them",
            positions: vec!["scope_in_client", "endpoint_in_client"],
            matches: Match::Types(root_items()),
            accept: &[("endpoint_in_scope", "retry"), ("scope_in_scope", "error")],
            refuse: &[
                ("endpoint_in_client", "retry"),
                ("scope_in_client", "endpoint"),
                ("endpoint_in_client", "result"),
            ],
        },
        Reservation {
            id: "children",
            what: "the names of the children of the node that an optional input belongs to (the position's `children`): its setter and the child's method would collide",
            positions: vec!["client_option", "scope_option"],
            matches: Match::Children,
            accept: &[("client_input", "probe"), ("endpoint_option", "probe")],
            refuse: &[
                ("client_option", "probe"),
                ("scope_option", "probe"),
                ("part_option_in_scope", "probe"),
            ],
        },
    ]
}

/// Examples in their positions: those to accept, then those to refuse.
type Examples = (
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
);

const NAME_EXAMPLES: Examples = (
    &[
        ("endpoint_input", "by_puuid"),
        ("scope_in_client", "v2"),
        ("endpoint_in_scope", "get_sha256sum"),
        ("part", "lol_filter"),
    ],
    &[
        ("endpoint_input", "a__b"),
        ("endpoint_input", "tail_"),
        ("endpoint_input", "r#ok"),
        ("endpoint_input", "_x"),
        ("scope_in_client", "Get"),
        ("endpoint_in_client", "getMany"),
    ],
);

const CLIENT_EXAMPLES: Examples = (
    &[
        ("client", "Riot"),
        ("client", "GitHub"),
        ("client", "V2Api"),
        ("client", "Types"),
    ],
    &[
        ("client", "riot"),
        ("client", "Git_Hub"),
        ("client", "Self"),
        ("client", "Type"),
        ("client", "Mod"),
    ],
);

/// An identifier as written, in JSON, and as Rust gives it to the macro, after NFC. The
/// first spells `Kite` with U+212A KELVIN SIGN; `tests/accepted/nfc_client.rs` compiles it.
const NORMALIZATION: [(&str, &str); 1] = [("\\u212Aite", "Kite")];

const TYPE_EXAMPLES: [&str; 3] = ["get_sha256sum", "one_2", "by_riot_id"];
const MODULE_EXAMPLES: [&str; 4] = ["Riot", "GitHub", "XMLHttpClient", "V2API"];

/// Whole descriptions: rules between several names, which no single position shows.
const COLLISIONS: [(&str, &str, &str, &str); 4] = [
    (
        "sibling_types",
        "two children of one node may not have the same type (the `type` conversion)",
        r#"client Probe { base "https://a.test"; scope one2 { GET probe(); } scope one_3 { GET probe(); } }"#,
        r#"client Probe { base "https://a.test"; scope one2 { GET probe(); } scope one_2 { GET probe(); } }"#,
    ),
    (
        "duplicate_children",
        "two children of one node, or two parts, may not have the same name",
        r#"client Probe { base "https://a.test"; GET probe(); scope inner { GET probe(); } }"#,
        r#"client Probe { base "https://a.test"; GET probe(); POST probe(); }"#,
    ),
    (
        "duplicate_inputs",
        "two inputs of one node, its parts' inputs included, may not have the same name",
        r#"client Probe { base "https://a.test"; part p(a?: String) { query { a; } } GET probe(b: String, ..p) { query { b; } } }"#,
        r#"client Probe { base "https://a.test"; part p(a?: String) { query { a; } } GET probe(a: String, ..p) { query { a; } } }"#,
    ),
    (
        "repeated_spread",
        "a node may not spread one part twice, even a part with no input",
        r#"client Probe { base "https://a.test"; part p() {} part q() {} GET probe(..p, ..q); }"#,
        r#"client Probe { base "https://a.test"; part p() {} GET probe(..p, ..p); }"#,
    ),
];

/// The macro's verdict on a whole description: accepted when it reports nothing.
fn accepted_whole(source: &str) -> bool {
    let Ok(tokens) = source.parse::<proc_macro2::TokenStream>() else {
        return false;
    };
    let mut parsed = parse::parse(tokens);
    if let Some(client) = &parsed.client {
        name::check(client, &mut parsed.diagnostics);
        crate::write::check_client_type_names(client, &mut parsed.diagnostics);
        let model = resolve::resolve(client, &mut parsed.diagnostics);
        check::check(client, &model, &mut parsed.diagnostics);
    } else {
        return false;
    }
    parsed.diagnostics.is_empty()
}

fn list<S: AsRef<str>>(items: &[S]) -> String {
    let items: Vec<_> = items
        .iter()
        .map(|item| format!("{:?}", item.as_ref()))
        .collect();
    format!("[{}]", items.join(", "))
}

fn pairs(items: &[(&str, &str)]) -> String {
    let items: Vec<_> = items
        .iter()
        .map(|(position, word)| format!("[{position:?}, {word:?}]"))
        .collect();
    format!("[{}]", items.join(", "))
}

/// The file, from the macro's tables and the examples of this test.
fn render(reservations: &[Reservation]) -> String {
    let positions: Vec<_> = POSITIONS
        .iter()
        .map(|p| {
            let inherits = p
                .inherits
                .map_or(String::new(), |base| format!(", \"inherits\": {base:?}"));
            format!(
                "    {:?}: {{ \"what\": {:?}, \"probe\": {:?}, \"enclosing\": {}, \"children\": {}{inherits} }}",
                p.id,
                p.what,
                p.probe,
                list(p.enclosing),
                list(p.children)
            )
        })
        .collect();
    let name_positions: Vec<_> = POSITIONS
        .iter()
        .filter(|p| p.id != "client" && p.inherits.is_none())
        .map(|p| p.id)
        .collect();
    let reserved: Vec<_> = reservations
        .iter()
        .map(|r| {
            let matcher = match &r.matches {
                Match::Names(names) => format!("\"names\": {}", list(names)),
                Match::Prefix(prefix) => format!("\"prefix\": {prefix:?}"),
                Match::Types(types) => format!("\"types\": {}", list(types)),
                Match::Enclosing => "\"enclosing\": true".into(),
                Match::Children => "\"children\": true".into(),
            };
            format!(
                "    {{\n      \"id\": {:?},\n      \"what\": {:?},\n      \"positions\": {},\n      {matcher},\n      \"accept\": {},\n      \"refuse\": {}\n    }}",
                r.id,
                r.what,
                list(&r.positions),
                pairs(r.accept),
                pairs(r.refuse)
            )
        })
        .collect();
    let collisions: Vec<_> = COLLISIONS
        .iter()
        .map(|(id, what, accept, refuse)| {
            format!("    {{ \"id\": {id:?}, \"what\": {what:?}, \"accept\": [{accept:?}], \"refuse\": [{refuse:?}] }}")
        })
        .collect();
    let normalization: Vec<_> = NORMALIZATION
        .iter()
        .map(|(written, read)| format!("[\"{written}\", {read:?}]"))
        .collect();
    let types: Vec<_> = TYPE_EXAMPLES
        .iter()
        .map(|word| format!("[{word:?}, {:?}]", name::type_name(word)))
        .collect();
    let modules: Vec<_> = MODULE_EXAMPLES
        .iter()
        .map(|word| format!("[{word:?}, {:?}]", name::module_name(word)))
        .collect();
    format!(
        r#"{{
  "schema": 1,
  "about": "The names that a description of an API may use, for programs that write descriptions: their spelling, the names reserved in each position, and the collisions between names. Every other rule of a description is in docs/reference/rules.md, and every message in docs/reference/messages.md. docs/reference/rules.md explains each key. Concord's tests write this file from the macro's own tables and check every example against the macro.",
  "readers": "A reader must stop, not guess, when schema is a version that it does not know, or when it meets a key, a kind of match or a collision id that it does not know: each is a rule that the reader cannot apply. Later releases may add keys and collisions; a change in the meaning of a key changes schema.",
  "positions": {{
{positions}
  }},
  "spelling": {{
    "normalization": {{ "form": "NFC", "rule": "Rust puts every identifier in Unicode normalization form C before the macro reads it; the spelling rules apply to the normalized name", "examples": [{normalization}] }},
    "name": {{ "positions": {name_positions}, "first": {name_first:?}, "characters": {name_characters:?}, "doubled_underscore": false, "final_underscore": false, "accept": {name_accept}, "refuse": {name_refuse} }},
    "client": {{ "positions": ["client"], "first": {client_first:?}, "characters": {client_characters:?}, "module_is_keyword": false, "accept": {client_accept}, "refuse": {client_refuse} }}
  }},
  "conversions": {{
    "type": {{ "rule": "split the name at each _, put the first character of each part in upper case, and join the parts; a digit never splits a part", "examples": [{types}] }},
    "module": {{ "rule": "the client's name in lower case, with a _ before each capital that follows a lower-case letter or a digit, and before the last capital of a run that a lower-case letter follows", "examples": [{modules}] }}
  }},
  "reserved": [
{reserved}
  ],
  "collisions": [
{collisions}
  ]
}}
"#,
        positions = positions.join(",\n"),
        normalization = normalization.join(", "),
        name_positions = list(&name_positions),
        name_first = name::NAME_FIRST,
        name_characters = name::NAME_CHARACTERS,
        name_accept = pairs(NAME_EXAMPLES.0),
        name_refuse = pairs(NAME_EXAMPLES.1),
        client_first = name::CLIENT_FIRST,
        client_characters = name::CLIENT_CHARACTERS,
        client_accept = pairs(CLIENT_EXAMPLES.0),
        client_refuse = pairs(CLIENT_EXAMPLES.1),
        types = types.join(", "),
        modules = modules.join(", "),
        reserved = reserved.join(",\n"),
        collisions = collisions.join(",\n"),
    )
}

// A reader of the file that knows only the file: a JSON parser, and the file's rules
// written again from their text.

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Json>),
    Map(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> &Json {
        match self {
            Json::Map(entries) => entries
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or_else(|| panic!("missing key {key}")),
            _ => panic!("{key} of a non-object"),
        }
    }
    fn find(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn keys(&self) -> Vec<&str> {
        match self {
            Json::Map(entries) => entries.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }
    fn text(&self) -> &str {
        match self {
            Json::Text(text) => text,
            other => panic!("not a string: {other:?}"),
        }
    }
    fn list(&self) -> &[Json] {
        match self {
            Json::List(items) => items,
            other => panic!("not a list: {other:?}"),
        }
    }
    fn texts(&self) -> Vec<&str> {
        self.list().iter().map(Json::text).collect()
    }
    fn truth(&self) -> bool {
        match self {
            Json::Bool(value) => *value,
            other => panic!("not a boolean: {other:?}"),
        }
    }
}

fn parse_json(text: &str) -> Json {
    fn skip(chars: &[char], at: &mut usize) {
        while chars.get(*at).is_some_and(|c| c.is_whitespace()) {
            *at += 1;
        }
    }
    fn value(chars: &[char], at: &mut usize) -> Json {
        skip(chars, at);
        match chars[*at] {
            '{' => {
                *at += 1;
                let mut entries = Vec::new();
                loop {
                    skip(chars, at);
                    if chars[*at] == '}' {
                        *at += 1;
                        return Json::Map(entries);
                    }
                    let Json::Text(key) = value(chars, at) else {
                        panic!("a key is a string")
                    };
                    skip(chars, at);
                    assert_eq!(chars[*at], ':');
                    *at += 1;
                    entries.push((key, value(chars, at)));
                    skip(chars, at);
                    if chars[*at] == ',' {
                        *at += 1;
                    }
                }
            }
            '[' => {
                *at += 1;
                let mut items = Vec::new();
                loop {
                    skip(chars, at);
                    if chars[*at] == ']' {
                        *at += 1;
                        return Json::List(items);
                    }
                    items.push(value(chars, at));
                    skip(chars, at);
                    if chars[*at] == ',' {
                        *at += 1;
                    }
                }
            }
            '"' => {
                *at += 1;
                let mut text = String::new();
                loop {
                    let c = chars[*at];
                    *at += 1;
                    match c {
                        '"' => return Json::Text(text),
                        '\\' => {
                            let escaped = chars[*at];
                            *at += 1;
                            match escaped {
                                'n' => text.push('\n'),
                                't' => text.push('\t'),
                                'u' => {
                                    let hex: String = chars[*at..*at + 4].iter().collect();
                                    *at += 4;
                                    text.push(
                                        char::from_u32(u32::from_str_radix(&hex, 16).unwrap())
                                            .unwrap(),
                                    );
                                }
                                other => text.push(other),
                            }
                        }
                        other => text.push(other),
                    }
                }
            }
            't' => {
                *at += 4;
                Json::Bool(true)
            }
            'f' => {
                *at += 5;
                Json::Bool(false)
            }
            _ => {
                let start = *at;
                while chars
                    .get(*at)
                    .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '.' | 'e' | '+'))
                {
                    *at += 1;
                }
                Json::Number(
                    chars[start..*at]
                        .iter()
                        .collect::<String>()
                        .parse()
                        .unwrap(),
                )
            }
        }
    }
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let parsed = value(&chars, &mut at);
    skip(&chars, &mut at);
    assert_eq!(at, chars.len(), "text after the JSON value");
    parsed
}

/// The file's `type` conversion, written from its rule.
fn type_of(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The file's `module` conversion, written from its rule.
fn module_of(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut module = String::new();
    for (at, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && at > 0 {
            let before = chars[at - 1];
            let after = chars.get(at + 1).copied();
            let after_lower = before.is_ascii_lowercase() || before.is_ascii_digit();
            let ends_run =
                before.is_ascii_uppercase() && after.is_some_and(|a| a.is_ascii_lowercase());
            if after_lower || ends_run {
                module.push('_');
            }
        }
        module.push(c.to_ascii_lowercase());
    }
    module
}

/// The file read as rules. A reader stops on what it does not know.
struct Reader<'a> {
    file: &'a Json,
}

impl Reader<'_> {
    fn new(file: &Json) -> Reader<'_> {
        assert_eq!(file.get("schema"), &Json::Number(1.0), "unknown schema");
        let known = [
            "schema",
            "about",
            "readers",
            "positions",
            "spelling",
            "conversions",
            "reserved",
            "collisions",
        ];
        for key in file.keys() {
            assert!(known.contains(&key), "unknown key {key}");
        }
        // Every nested object too: an unknown key anywhere is a rule the reader cannot apply.
        let only = |object: &Json, allowed: &[&str], what: &str| {
            for key in object.keys() {
                assert!(allowed.contains(&key), "unknown key {key} in {what}");
            }
        };
        for (id, position) in match file.get("positions") {
            Json::Map(entries) => entries.as_slice(),
            _ => panic!("positions is an object"),
        } {
            only(
                position,
                &["what", "probe", "enclosing", "children", "inherits"],
                id,
            );
        }
        let spelling = file.get("spelling");
        only(spelling, &["normalization", "name", "client"], "spelling");
        only(
            spelling.get("normalization"),
            &["form", "rule", "examples"],
            "normalization",
        );
        assert_eq!(
            spelling.get("normalization").get("form").text(),
            "NFC",
            "unknown form"
        );
        let spelled = ["positions", "first", "characters", "accept", "refuse"];
        let name_keys = [&spelled[..], &["doubled_underscore", "final_underscore"]].concat();
        only(spelling.get("name"), &name_keys, "spelling.name");
        let client_keys = [&spelled[..], &["module_is_keyword"]].concat();
        only(spelling.get("client"), &client_keys, "spelling.client");
        let conversions = file.get("conversions");
        only(conversions, &["type", "module"], "conversions");
        for key in ["type", "module"] {
            only(conversions.get(key), &["rule", "examples"], key);
        }
        for collision in file.get("collisions").list() {
            only(
                collision,
                &["id", "what", "accept", "refuse"],
                "a collision",
            );
        }
        for reservation in file.get("reserved").list() {
            let kinds = ["names", "prefix", "types", "enclosing", "children"];
            let found = reservation
                .keys()
                .into_iter()
                .filter(|key| !["id", "what", "positions", "accept", "refuse"].contains(key))
                .collect::<Vec<_>>();
            assert!(
                found.len() == 1 && kinds.contains(&found[0]),
                "unknown kind of match {found:?}"
            );
        }
        let collisions = [
            "sibling_types",
            "duplicate_children",
            "duplicate_inputs",
            "repeated_spread",
        ];
        for collision in file.get("collisions").list() {
            assert!(
                collisions.contains(&collision.get("id").text()),
                "unknown collision"
            );
        }
        Reader { file }
    }

    fn position(&self, id: &str) -> &Json {
        self.file.get("positions").get(id)
    }

    fn probe(&self, id: &str, word: &str) -> String {
        self.position(id).get("probe").text().replace("NAME", word)
    }

    fn keywords(&self) -> Vec<&str> {
        self.file
            .get("reserved")
            .list()
            .iter()
            .find(|r| r.get("id").text() == "keywords")
            .unwrap()
            .get("names")
            .texts()
    }

    /// What the file says the macro does with `word` at `position`.
    fn accepts(&self, id: &str, word: &str) -> bool {
        let position = self.position(id);
        let rules_of = position.find("inherits").map_or(id, Json::text);
        let spelling = self.file.get("spelling");
        let spelled = |rule: &Json| {
            let first = rule.get("first").text();
            let characters = rule.get("characters").text();
            word.chars().next().is_some_and(|c| first.contains(c))
                && word.chars().all(|c| characters.contains(c))
        };
        if id == "client" {
            let client = spelling.get("client");
            let module = module_of(word);
            return spelled(client)
                && (client.get("module_is_keyword").truth()
                    || !self.keywords().contains(&module.as_str()));
        }
        let name = spelling.get("name");
        if !spelled(name)
            || (!name.get("doubled_underscore").truth() && word.contains("__"))
            || (!name.get("final_underscore").truth() && word.ends_with('_'))
        {
            return false;
        }
        !self.file.get("reserved").list().iter().any(|reservation| {
            reservation.get("positions").texts().contains(&rules_of)
                && if let Some(names) = reservation.find("names") {
                    names.texts().contains(&word)
                } else if let Some(prefix) = reservation.find("prefix") {
                    word.starts_with(prefix.text())
                } else if let Some(types) = reservation.find("types") {
                    types.texts().contains(&type_of(word).as_str())
                } else if reservation.find("enclosing").is_some() {
                    position.get("enclosing").texts().contains(&word)
                } else {
                    position.get("children").texts().contains(&word)
                }
        })
    }
}

/// Every word of the repository's sources and reference pages: the names that a
/// reservation outside the tables would most likely be.
fn corpus() -> BTreeSet<String> {
    let root = path().join("..");
    let mut files = Vec::new();
    for directory in [
        "concord-core/src",
        "concord-core/src/__private",
        "concord-extras/src",
        "concord-macros/src",
        "concord-macros/src/write",
        "concord-api/src",
        "tests/tour/src",
        "docs/reference",
        "docs/guide",
    ] {
        for entry in fs::read_dir(root.join(directory)).unwrap() {
            let file = entry.unwrap().path();
            if file
                .extension()
                .is_some_and(|extension| extension == "rs" || extension == "md")
            {
                files.push(file);
            }
        }
    }
    let mut words = BTreeSet::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for word in text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')) {
            if word
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                && word.len() <= 40
            {
                words.insert(word.to_owned());
            }
        }
    }
    words
}

#[test]
fn c1_4_c1_6_reserved_names_file_is_the_macro_s_rule_with_checked_examples() {
    let reservations = reservations();
    let expected = render(&reservations);
    let path_of_file = path().join("../docs/reference/reserved-names.json");
    let written = fs::read_to_string(&path_of_file).unwrap_or_default();
    if written != expected {
        let fresh = path().join("../target/reserved-names.json");
        fs::write(&fresh, &expected).unwrap();
        panic!(
            "{} differs from the macro's tables; the expected file is {}",
            path_of_file.display(),
            fresh.display()
        );
    }

    // From here on, only the file.
    let file = parse_json(&written);
    let reader = Reader::new(&file);
    let mut wrong = Vec::new();
    let verdict = |source: &str| {
        !crate::expand(source.parse().unwrap())
            .to_string()
            .contains("compile_error")
    };

    // Every example, through the whole macro, in its position.
    let mut examples = Vec::new();
    let spelling = file.get("spelling");
    for rule in [spelling.get("name"), spelling.get("client")]
        .into_iter()
        .chain(file.get("reserved").list())
    {
        for (key, accepted) in [("accept", true), ("refuse", false)] {
            for example in rule.get(key).list() {
                let [position, word] = example.texts()[..] else {
                    panic!("an example is a position and a name")
                };
                examples.push((position.to_owned(), word.to_owned(), accepted));
            }
        }
    }
    for (position, word, accepted) in &examples {
        let source = reader.probe(position, word);
        if verdict(&source) != *accepted || accepted_whole(&source) != *accepted {
            wrong.push(format!("example {position} `{word}`: the macro disagrees"));
        }
        if reader.accepts(position, word) != *accepted {
            wrong.push(format!(
                "example {position} `{word}`: the file's rules disagree"
            ));
        }
    }
    for collision in file.get("collisions").list() {
        let id = collision.get("id").text();
        if !collision.get("accept").texts().iter().all(|s| verdict(s))
            || collision.get("refuse").texts().iter().any(|s| verdict(s))
        {
            wrong.push(format!("collision {id}"));
        }
    }
    for (key, convert) in [
        ("type", type_of as fn(&str) -> String),
        ("module", module_of),
    ] {
        for example in file.get("conversions").get(key).get("examples").list() {
            let [from, to] = example.texts()[..] else {
                panic!("a conversion example is a pair")
            };
            let by_macro = if key == "type" {
                name::type_name(from)
            } else {
                name::module_name(from)
            };
            if convert(from) != to || by_macro != to {
                wrong.push(format!("conversion {key} `{from}`"));
            }
        }
    }

    // Every item at the root of the module the macro writes is in the file, and is
    // refused as the type of a child of the client.
    let root = root_items();
    let listed: Vec<&str> = file
        .get("reserved")
        .list()
        .iter()
        .find(|r| r.get("id").text() == "root_items")
        .unwrap()
        .get("types")
        .texts();
    if root.iter().map(String::as_str).collect::<Vec<_>>() != listed {
        wrong.push(format!(
            "root items: the macro writes {root:?}, the file lists {listed:?}"
        ));
    }
    for item in &root {
        let child = module_of(item);
        for position in ["endpoint_in_client", "scope_in_client"] {
            if verdict(&reader.probe(position, &child)) {
                wrong.push(format!("{position} `{child}`: the macro accepts a child named after the root item {item}"));
            }
        }
    }

    // The file against the macro, for every word, in every position.
    let words = corpus();
    assert!(words.len() > 1000, "{} words", words.len());
    for position in file.get("positions").keys() {
        for word in &words {
            let accepted = accepted_whole(&reader.probe(position, word));
            if accepted != reader.accepts(position, word) {
                wrong.push(format!(
                    "{position} `{word}`: the macro {} it, the file does not say so",
                    if accepted { "accepts" } else { "refuses" }
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} disagreements:\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every name that the name check spells out is in its tables, or is one of the few
/// that it uses only to build a suggestion: a name added to the check by hand fails here.
#[test]
fn c1_4_c1_6_the_name_check_spells_no_name_outside_its_tables() {
    use syn::visit::Visit;
    struct Literals(Vec<String>);
    impl<'ast> Visit<'ast> for Literals {
        fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
            let text = literal.value();
            if text
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                && !text.is_empty()
            {
                self.0.push(text);
            }
        }
        fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
            // The unit tests of the module are not the check.
            if !module.attrs.iter().any(|attr| attr.path().is_ident("cfg")) {
                syn::visit::visit_item_mod(self, module);
            }
        }
    }
    let mut tables: BTreeSet<String> = name::KEYWORDS
        .iter()
        .chain(&name::EVERY_RECEIVER)
        .chain(&name::CLIENT_METHODS)
        .chain(&name::HANDLE_METHODS)
        .chain(&name::CALL_METHODS)
        .chain(&name::ROOT_ALIASES)
        .chain(&[
            name::SETTER_PREFIX,
            name::NAME_FIRST,
            name::NAME_CHARACTERS,
            name::CLIENT_FIRST,
            name::CLIENT_CHARACTERS,
        ])
        .map(|name| (*name).to_owned())
        .collect();
    // The rules of the file that are not a table of `name.rs`: the client's owner name,
    // the doubled and final `_` of the spelling, the root items' children.
    tables.extend(["client", "_", "__"].map(str::to_owned));
    tables.extend(root_items().iter().map(|item| module_of(item)));
    // Used only to build a suggested replacement, never to refuse a name.
    let suggestions = [
        "Api",
        "_value",
        "input_",
        "input",
        "param",
        "item",
        "value",
        "endpoint_item",
        "EndpointItem",
    ];

    let mut literals = Literals(Vec::new());
    literals.visit_file(&syn::parse_file(include_str!("../../src/name.rs")).unwrap());
    let writer = syn::parse_file(include_str!("../../src/write/mod.rs")).unwrap();
    for item in &writer.items {
        if let syn::Item::Fn(function) = item {
            if function.sig.ident == "check_client_type_names" {
                literals.visit_item_fn(function);
            }
        }
    }
    let unknown: Vec<_> = literals
        .0
        .iter()
        .filter(|text| !tables.contains(*text) && !suggestions.contains(&text.as_str()))
        .collect();
    assert!(unknown.is_empty(), "names outside the tables: {unknown:?}");
}

/// The reader of the test obeys the file's own rule for readers: an unknown key, at any
/// depth, stops it.
#[test]
fn c1_4_the_reader_of_the_names_file_stops_on_an_unknown_key_at_any_depth() {
    let written = fs::read_to_string(path().join("../docs/reference/reserved-names.json")).unwrap();
    Reader::new(&parse_json(&written));
    for (anchor, added) in [
        ("\"name\": { ", "\"ascii_only\": true, "),
        ("\"normalization\": { ", "\"also\": \"NFKC\", "),
        ("\"type\": { ", "\"plural\": true, "),
        ("\"client_input\": { ", "\"also\": [], "),
    ] {
        assert_eq!(written.matches(anchor).count(), 1, "{anchor}");
        let changed = written.replacen(anchor, &format!("{anchor}{added}"), 1);
        let file = parse_json(&changed);
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Reader::new(&file);
        }));
        assert!(stopped.is_err(), "the reader went past {added}");
    }
}
