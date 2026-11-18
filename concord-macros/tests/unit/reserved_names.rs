//! `docs/reference/reserved-names.json`: the names a description may not use, as data.
//!
//! The file is written from the macro's own tables and from the examples below, and the
//! test asks the macro for the verdict on every example in its position. A model that reads
//! only what the file says then predicts the macro's verdict for every word of the
//! repository's sources and pages, in every position: a reservation added to the macro
//! outside its tables refuses a name that the file accepts, and fails this test.
use super::*;
use std::collections::BTreeSet;

/// Where a name is tried: each probe is a description with `NAME` in that position.
const POSITIONS: [(&str, &str, &str); 12] = [
    (
        "client",
        "the name of the client",
        r#"client NAME { base "https://a.test"; GET probe(); }"#,
    ),
    (
        "client_input",
        "a required input of the client",
        r#"client Probe(NAME: String) { base "https://a.test"; query { NAME; } GET probe(); }"#,
    ),
    (
        "client_option",
        "an optional input of the client",
        r#"client Probe(NAME?: String) { base "https://a.test"; query { NAME; } GET probe(); }"#,
    ),
    (
        "scope_in_client",
        "a scope that is a child of the client",
        r#"client Probe { base "https://a.test"; scope NAME { GET probe(); } }"#,
    ),
    (
        "scope_in_scope",
        "a scope that is a child of a scope",
        r#"client Probe { base "https://a.test"; scope outer { scope NAME { GET probe(); } } }"#,
    ),
    (
        "scope_input",
        "a required input of a scope",
        r#"client Probe { base "https://a.test"; scope outer(NAME: String) { query { NAME; } GET probe(); } }"#,
    ),
    (
        "scope_option",
        "an optional input of a scope",
        r#"client Probe { base "https://a.test"; scope outer(NAME?: String) { query { NAME; } GET probe(); } }"#,
    ),
    (
        "endpoint_in_client",
        "an endpoint that is a child of the client",
        r#"client Probe { base "https://a.test"; GET NAME(); }"#,
    ),
    (
        "endpoint_in_scope",
        "an endpoint that is a child of a scope",
        r#"client Probe { base "https://a.test"; scope outer { GET NAME(); } }"#,
    ),
    (
        "endpoint_input",
        "a required input of an endpoint",
        r#"client Probe { base "https://a.test"; scope outer { GET probe(NAME: String) { query { NAME; } } } }"#,
    ),
    (
        "endpoint_option",
        "an optional input of an endpoint",
        r#"client Probe { base "https://a.test"; scope outer { GET probe(NAME?: String) { query { NAME; } } } }"#,
    ),
    (
        "part",
        "the name of a part",
        r#"client Probe { base "https://a.test"; part NAME(shared?: String) { query { shared; } } GET probe(..NAME); }"#,
    ),
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
    /// One of these names, exactly.
    Names(Vec<&'static str>),
    /// A name that starts with this text.
    Prefix(&'static str),
    /// A name whose type (the `type` conversion) is one of these.
    Types(Vec<String>),
    /// The name of a scope that encloses the position, or that the input belongs to.
    Enclosing,
    /// The name of a child of the node that the optional input belongs to.
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
            _ => None,
        })
        .filter(|ident| !ident.starts_with("__"))
        .collect();
    root.sort_unstable();
    for alias in name::ROOT_ALIASES {
        assert!(root.iter().any(|item| item == alias), "{alias}");
    }
    root
}

fn reservations() -> Vec<Reservation> {
    let both = |a: &[&'static str], b: &[&'static str]| a.iter().chain(b).copied().collect();
    vec![
        Reservation {
            id: "keywords",
            what: "the Rust keywords, raw (`r#type`) or not; `Self` is here for the client's module name",
            positions: POSITIONS.iter().map(|p| p.0).filter(|p| *p != "client").collect(),
            matches: Match::Names(name::KEYWORDS.to_vec()),
            accept: &[("endpoint_input", "kind"), ("scope_in_client", "matches")],
            refuse: &[
                ("endpoint_input", "type"),
                ("endpoint_input", "r#type"),
                ("scope_in_client", "match"),
                ("part", "self"),
            ],
        },
        Reservation {
            id: "client_owner",
            what: "the name by which the client's inputs are written (`client.key`)",
            positions: both(&INPUTS, &["scope_in_client", "scope_in_scope"]),
            matches: Match::Names(vec!["client"]),
            accept: &[("endpoint_in_client", "client"), ("part", "client")],
            refuse: &[("endpoint_input", "client"), ("scope_in_client", "client")],
        },
        Reservation {
            id: "enclosing_scopes",
            what: "the names of the scopes above the name, and for an input of a scope, that scope's own name",
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
            refuse: &[("endpoint_in_scope", "call"), ("scope_option", "clone")],
        },
        Reservation {
            id: "call_methods",
            what: "the methods of a call: an optional input of an endpoint would add one",
            positions: vec!["endpoint_option"],
            matches: Match::Names(methods(&name::CALL_METHODS)),
            accept: &[("endpoint_input", "send"), ("endpoint_in_scope", "pages")],
            refuse: &[("endpoint_option", "send"), ("endpoint_option", "at_url")],
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
            what: "the names of the children of the node that an optional input belongs to: its setter and the child's method would collide",
            positions: vec!["client_option", "scope_option"],
            matches: Match::Children,
            accept: &[("client_input", "probe"), ("endpoint_option", "probe")],
            refuse: &[("client_option", "probe"), ("scope_option", "probe")],
        },
    ]
}

const NAME_EXAMPLES: (&[(&str, &str)], &[(&str, &str)]) = (
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

const CLIENT_EXAMPLES: (&[(&str, &str)], &[(&str, &str)]) = (
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

const TYPE_EXAMPLES: [&str; 3] = ["get_sha256sum", "one_2", "by_riot_id"];
const MODULE_EXAMPLES: [&str; 4] = ["Riot", "GitHub", "XMLHttpClient", "V2API"];

/// Whole descriptions: rules between several names, which no single position shows.
const COLLISIONS: [(&str, &str, &str, &str); 3] = [
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
];

fn template(position: &str) -> &'static str {
    POSITIONS.iter().find(|p| p.0 == position).unwrap().2
}

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

fn accepted_at(position: &str, word: &str) -> bool {
    accepted_whole(&template(position).replace("NAME", word))
}

/// What the file predicts, from its own content only.
fn predicted(position: &str, word: &str, reservations: &[Reservation]) -> bool {
    let has_spelling = |first: &str, characters: &str| {
        word.chars().next().is_some_and(|ch| first.contains(ch))
            && word.chars().all(|ch| characters.contains(ch))
    };
    if position == "client" {
        return has_spelling(name::CLIENT_FIRST, name::CLIENT_CHARACTERS)
            && !name::KEYWORDS.contains(&name::module_name(word).as_str());
    }
    if !has_spelling(name::NAME_FIRST, name::NAME_CHARACTERS)
        || word.contains("__")
        || word.ends_with('_')
    {
        return false;
    }
    let enclosing = ["outer"];
    let children = ["probe"];
    !reservations.iter().any(|reservation| {
        reservation.positions.contains(&position)
            && match &reservation.matches {
                Match::Names(names) => names.contains(&word),
                Match::Prefix(prefix) => word.starts_with(prefix),
                Match::Types(types) => types.contains(&name::type_name(word)),
                Match::Enclosing => enclosing.contains(&word),
                Match::Children => children.contains(&word),
            }
    })
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

fn render(reservations: &[Reservation]) -> String {
    let positions: Vec<_> = POSITIONS
        .iter()
        .map(|(id, what, probe)| {
            format!("    {id:?}: {{ \"what\": {what:?}, \"probe\": {probe:?} }}")
        })
        .collect();
    let name_positions: Vec<_> = POSITIONS
        .iter()
        .map(|p| p.0)
        .filter(|p| *p != "client")
        .collect();
    let reserved: Vec<_> = reservations
        .iter()
        .map(|r| {
            let matcher = match &r.matches {
                Match::Names(names) => format!("\"names\": {}", list(names)),
                Match::Prefix(prefix) => format!("\"prefix\": {prefix:?}"),
                Match::Types(types) => format!("\"types\": {}", list(types)),
                Match::Enclosing => "\"enclosing_scopes\": true".into(),
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
  "about": "The names that a description of an API may use, for programs that write descriptions. docs/reference/rules.md explains each key. Concord's tests write this file from the macro's own tables and check every example against the macro.",
  "readers": "A reader that meets a key it does not know must stop, not skip it: the key is a rule that the reader cannot apply. Later releases may add keys; a change in the meaning of a key changes schema.",
  "positions": {{
{positions}
  }},
  "spelling": {{
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
    let mut wrong = Vec::new();

    // Every example of the file, through the whole macro, in its position.
    let mut examples: Vec<(&str, &str, bool)> = Vec::new();
    for (examples_of, verdict) in [
        (NAME_EXAMPLES.0, true),
        (NAME_EXAMPLES.1, false),
        (CLIENT_EXAMPLES.0, true),
        (CLIENT_EXAMPLES.1, false),
    ] {
        examples.extend(examples_of.iter().map(|(p, w)| (*p, *w, verdict)));
    }
    for reservation in &reservations {
        examples.extend(reservation.accept.iter().map(|(p, w)| (*p, *w, true)));
        examples.extend(reservation.refuse.iter().map(|(p, w)| (*p, *w, false)));
        for (position, _) in reservation.refuse {
            assert!(
                reservation.positions.contains(position),
                "{}: a refused example outside the rule's positions",
                reservation.id
            );
        }
    }
    for (position, word, verdict) in examples {
        let source = template(position).replace("NAME", word);
        let expanded = !crate::expand(source.parse().unwrap())
            .to_string()
            .contains("compile_error");
        if expanded != verdict || accepted_at(position, word) != verdict {
            wrong.push(format!(
                "example {position} `{word}`: the macro says {expanded}"
            ));
        }
        if predicted(position, word, &reservations) != verdict {
            wrong.push(format!(
                "example {position} `{word}`: the file's rules disagree"
            ));
        }
    }
    for (id, _, accept, refuse) in COLLISIONS {
        if !accepted_whole(accept) || accepted_whole(refuse) {
            wrong.push(format!("collision {id}"));
        }
    }
    for word in TYPE_EXAMPLES {
        assert!(name::type_name(word)
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric()));
    }

    // The model of the file against the macro, for every word, in every position.
    let words = corpus();
    assert!(words.len() > 1000, "{} words", words.len());
    for (position, _, _) in POSITIONS {
        for word in &words {
            let macro_verdict = accepted_at(position, word);
            if macro_verdict != predicted(position, word, &reservations) {
                wrong.push(format!(
                    "{position} `{word}`: the macro {} it, the file does not say so",
                    if macro_verdict { "accepts" } else { "refuses" }
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

    let expected = render(&reservations);
    let file = path().join("../docs/reference/reserved-names.json");
    let written = fs::read_to_string(&file).unwrap_or_default();
    if written != expected {
        let fresh = path().join("../target/reserved-names.json");
        fs::write(&fresh, &expected).unwrap();
        panic!(
            "{} differs from the macro's tables; the expected file is {}",
            file.display(),
            fresh.display()
        );
    }
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
    tables.extend(root_items().iter().map(|item| name::module_name(item)));
    // Used only to build a suggested replacement, never to refuse a name.
    let suggestions = [
        "Api",
        "_value",
        "input_",
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
