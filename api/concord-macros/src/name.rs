//! Step 2: names, receivers, owner views and generated type namespaces.

use std::collections::BTreeMap;

use proc_macro2::Span;
use quote::{quote, ToTokens};

use crate::{
    ast::*,
    diag::{self, Diagnostics, Written},
    flat,
};

/// C1.4's closed, edition-independent keyword set.
pub(crate) fn keyword(name: &str) -> bool {
    matches!(
        name,
        "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

/// C1.5: a digit never cuts a type name.
pub(crate) fn type_name(name: &str) -> String {
    name.split('_')
        .map(|piece| {
            let mut chars = piece.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect()
}

/// C1.5's acronym-aware conversion, without a casing dependency.
pub(crate) fn module_name(name: &str) -> String {
    let chars: Vec<_> = name.chars().collect();
    let mut out = String::new();
    for (i, ch) in chars.iter().enumerate() {
        let previous = i.checked_sub(1).and_then(|p| chars.get(p));
        let next = chars.get(i + 1);
        if ch.is_ascii_uppercase()
            && previous.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next.is_some_and(char::is_ascii_lowercase))
            })
        {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn snake(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && !name.ends_with('_')
        && !name.contains("__")
        && name
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'_')
}

/// A concrete usable suggestion for any rejected author spelling.
pub(crate) fn replacement(name: &str) -> String {
    let mut result = module_name(name.trim_start_matches("r#"));
    result = result
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    result = result
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if !result
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_lowercase)
    {
        result.insert_str(0, "input_");
    }
    if keyword(&result) {
        result.push_str("_value");
    }
    result
}

/// Reserved methods are exact and depend on the receiver, not the child kind.
pub(crate) fn reserved(name: &str, receiver: &NodeKind) -> bool {
    name.starts_with("with_")
        || name == "clone"
        || match receiver {
            NodeKind::Client => matches!(name, "call" | "new"),
            NodeKind::Scope => name == "call",
            NodeKind::Endpoint(_) => matches!(
                name,
                "send"
                    | "send_full"
                    | "send_raw"
                    | "send_if_none_match"
                    | "request"
                    | "into_request"
                    | "edit_request"
                    | "read_response"
                    | "pages"
                    | "at_url"
                    | "into_future"
            ),
            NodeKind::Part => false,
        }
}

fn method_replacement(name: &str, suffix: &str) -> String {
    let bare = name.strip_prefix("with_").unwrap_or(name);
    format!("{}_{suffix}", replacement(bare))
}

/// Check syntax names before parts or inheritance are resolved.
pub(crate) fn check(client: &Client, diagnostics: &mut Diagnostics) {
    let mut pending = vec![(&client.node, Vec::<(String, Span)>::new(), None::<&Node>)];
    while let Some((node, mut owners, parent)) = pending.pop() {
        if !node.parse_failed {
            let spelling = node.name.to_string();
            if matches!(node.kind, NodeKind::Client) {
                let module = module_name(&spelling);
                if !spelling
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_uppercase)
                    || !spelling.bytes().all(|b| b.is_ascii_alphanumeric())
                    || keyword(&module)
                    || spelling == "Self"
                {
                    let mut suggestion = type_name(&replacement(&spelling));
                    if keyword(&module_name(&suggestion)) {
                        suggestion.push_str("Api");
                    }
                    diagnostics.raise(node.name.span(), diag::m06_invalid_client(&suggestion));
                }
            } else {
                api_name(&node.name, node, parent, &owners, None, false, diagnostics);
            }
            if matches!(node.kind, NodeKind::Scope) {
                if let Some((_, first)) = owners.iter().find(|(name, _)| name == &spelling) {
                    diagnostics.raise(
                        node.name.span(),
                        diag::m07_owner_scope(&format!("{spelling}_scope"), *first).help("choose a distinct scope name and update every qualified reference to this scope's inputs"),
                    );
                }
            }
            if matches!(node.kind, NodeKind::Client | NodeKind::Scope) {
                owners.push((flat::owner_name(node), node.name.span()));
            }
            let forbidden = owners.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
            let mut inputs = BTreeMap::new();
            if let Some(signature) = &node.signature {
                for parameter in &signature.entries {
                    if let Parameter::Input(input) = parameter {
                        let name = input.name.to_string();
                        api_name(
                            &input.name,
                            node,
                            parent,
                            &owners,
                            Some(&input.ty),
                            input.optional.is_some(),
                            diagnostics,
                        );
                        if let Some(first) = remember(&mut inputs, name.clone(), input.span) {
                            diagnostics.raise(
                                input.span,
                                diag::m01_duplicate_input(&name, &format!("{name}_other"), first).help("remove the repeated input declaration, keeping its existing placements"),
                            );
                        }
                        if let Some((_, first)) = owners.iter().find(|(owner, _)| owner == &name) {
                            diagnostics.raise(
                                input.span,
                                diag::m07_owner_input(&name, &format!("{name}_id"), *first).help(
                                    rename_input_help(
                                        node,
                                        &name,
                                        &format!("{name}_id"),
                                        &forbidden,
                                    ),
                                ),
                            );
                        }
                        if input.optional.is_some()
                            && !matches!(node.kind, NodeKind::Part)
                            && reserved(&name, &node.kind)
                        {
                            diagnostics.raise(
                                input.span,
                                diag::m03_reserved_input(
                                    &method_replacement(&name, "value"),
                                    &name,
                                )
                                .help(rename_input_help(
                                    node,
                                    &name,
                                    &method_replacement(&name, "value"),
                                    &forbidden,
                                )),
                            );
                        }
                    }
                }
            }
            let mut parts = BTreeMap::new();
            for part in node.parts.iter().filter(|p| !p.parse_failed) {
                let name = part.name.to_string();
                if let Some(first) = remember(&mut parts, name.clone(), part.name.span()) {
                    diagnostics.raise(
                        part.name.span(),
                        diag::m01_duplicate_part(&name, &format!("{name}_other"), first).help("keep one declaration of this part and retain its intended inputs and statements"),
                    );
                }
            }
            let mut children = BTreeMap::new();
            let mut types = BTreeMap::<String, (&str, Span)>::new();
            let child_names: Vec<_> = node.children.iter().map(|c| c.name.to_string()).collect();
            for (child, name) in node
                .children
                .iter()
                .zip(&child_names)
                .filter(|(c, _)| !c.parse_failed)
            {
                if let Some(first) = remember(&mut children, name.clone(), child.name.span()) {
                    diagnostics.raise(
                        child.name.span(),
                        diag::m01_duplicate_child(&format!("{name}_latest"), name, first)
                            .help(if matches!(child.kind, NodeKind::Scope) {
                                "give this scope a distinct name, or remove the duplicate declaration"
                            } else {
                                "give this endpoint a distinct name, or remove the duplicate declaration"
                            })
                            .secondary_text("the first child is here"),
                    );
                }
                if reserved(name, &node.kind)
                    || (matches!(node.kind, NodeKind::Client)
                        && matches!(type_name(name).as_str(), "Error" | "Result" | "Retry"))
                {
                    diagnostics.raise(
                        child.name.span(),
                        diag::m03_reserved_child(name, &method_replacement(name, "item"))
                            .label(if reserved(name, &node.kind) {
                                "conflicts with a generated method"
                            } else {
                                "conflicts with a generated type alias"
                            })
                            .help(if child_candidate(node,child,&method_replacement(name,"item"),&owners.iter().map(|(n,_)|n.clone()).collect::<Vec<_>>()) {
                                format!("rename this child to `{}`",method_replacement(name,"item"))
                            } else {"choose a distinct child name that conflicts with no sibling, optional input or generated type or method, and update its uses".into()}),
                    );
                }
                let generated = type_name(name);
                if let Some((earlier, first)) = remember(
                    &mut types,
                    generated.clone(),
                    (name.as_str(), child.name.span()),
                ) {
                    if earlier != name {
                        diagnostics.raise(
                            child.name.span(),
                            diag::m08_generated_collision(
                                name,
                                earlier,
                                &generated,
                                &format!("{name}_other"),
                                first,
                            ).help(if child_candidate(node,child,&format!("{name}_other"), &owners.iter().map(|(n,_)|n.clone()).collect::<Vec<_>>()) {
                                format!("rename `{name}` to `{name}_other`")
                            } else {"choose a distinct child name whose generated type also differs from every sibling, and update its references".into()}),
                        );
                    }
                }
            }
        }
        for child in node.children.iter().rev() {
            pending.push((child, owners.clone(), Some(node)));
        }
        for part in node
            .parts
            .iter()
            .rev()
            .filter(|_| matches!(node.kind, NodeKind::Client))
        {
            pending.push((
                part,
                vec![("client".into(), client.node.name.span())],
                Some(node),
            ));
        }
    }
}

fn input_candidate(node: &Node, name: &str, optional: bool, owners: &[String]) -> bool {
    snake(name)
        && !keyword(name)
        && !owners.iter().any(|o| o == name)
        && !(optional && reserved(name, &node.kind))
        && !node.signature.as_ref().is_some_and(|s| {
            s.entries
                .iter()
                .any(|p| matches!(p,Parameter::Input(i) if i.name==name))
        })
        && !(optional && node.children.iter().any(|c| c.name == name))
}
fn child_candidate(parent: &Node, current: &Node, name: &str, owners: &[String]) -> bool {
    snake(name)
        && !keyword(name)
        && !reserved(name, &parent.kind)
        && !(matches!(current.kind, NodeKind::Scope) && owners.iter().any(|o| o == name))
        && !(matches!(parent.kind, NodeKind::Client)
            && matches!(type_name(name).as_str(), "Error" | "Result" | "Retry"))
        && !parent.children.iter().chain(&parent.parts).any(|c| {
            !std::ptr::eq(c, current)
                && (c.name == name || type_name(&c.name.to_string()) == type_name(name))
        })
        && !parent.signature.as_ref().is_some_and(|s| {
            s.entries
                .iter()
                .any(|p| matches!(p,Parameter::Input(i) if i.optional.is_some() && i.name==name))
        })
}

#[allow(clippy::too_many_arguments)]
fn api_name(
    name: &syn::Ident,
    node: &Node,
    parent: Option<&Node>,
    owners: &[(String, Span)],
    ty: Option<&syn::Type>,
    optional: bool,
    diagnostics: &mut Diagnostics,
) {
    let kind = &node.kind;
    let spelling = name.to_string();
    let bare = spelling.trim_start_matches("r#");
    let suggestion = replacement(&spelling);
    let owner_names = owners
        .iter()
        .map(|(n, _)| n.clone())
        .chain(std::iter::once("client".into()))
        .collect::<Vec<_>>();
    let safe = if ty.is_some() {
        input_candidate(node, &suggestion, optional, &owner_names)
    } else {
        parent.is_some_and(|p| child_candidate(p, node, &suggestion, &owner_names))
    };
    if keyword(bare) || spelling.starts_with("r#") {
        let line = Written::new(quote!(#name));
        let message = if let Some(ty) = ty {
            diag::m04_keyword_input(&suggestion, &flat::tokens(ty), bare).help(format!("rename this input to `{suggestion}` and update its references in the existing placements and expressions, retaining their wire keys"))
        } else if matches!(kind, NodeKind::Part) {
            diag::m04_keyword_part(&line, &suggestion)
        } else if matches!(kind, NodeKind::Scope) {
            diag::m04_keyword_scope(&line, &suggestion)
        } else {
            diag::m04_keyword_endpoint(&line, &suggestion)
        };
        diagnostics.raise(name.span(), if safe {message} else {message.help("choose a distinct legal Rust name that conflicts with no owner or generated method, and update its references while retaining wire names")});
    } else if !snake(&spelling) {
        let message = diag::m05_invalid_name(&suggestion);
        diagnostics.raise(name.span(), if safe {message.help(format!("rename it to `{suggestion}` and update its references, retaining existing wire names"))}
            else {message.help("choose a distinct lower snake case name that conflicts with no owner or generated method, and update its references while retaining wire names")});
    }
}

/// Receiver and owner checks for inputs contributed by parts.
pub(crate) fn expanded(
    node: &Node,
    inputs: &[flat::InputLine<'_>],
    owners: &[flat::Owner<'_>],
    diagnostics: &mut Diagnostics,
) {
    let forbidden = owners
        .iter()
        .map(|o| flat::owner_name(o.node))
        .chain(inputs.iter().map(|i| i.input.name.to_string()))
        .collect::<Vec<_>>();
    for input in inputs {
        let name = input.input.name.to_string();
        if !std::ptr::eq(input.written, node) {
            if input.input.optional.is_some() && reserved(&name, &node.kind) {
                diagnostics.raise(
                    input.input.span,
                    diag::m03_reserved_input(&method_replacement(&name, "value"), &name).help(
                        rename_input_help(
                            input.written,
                            &name,
                            &method_replacement(&name, "value"),
                            &forbidden,
                        ),
                    ),
                );
            }
            if let Some(owner) = owners.iter().find(|o| flat::owner_name(o.node) == name) {
                diagnostics.raise(
                    input.input.span,
                    diag::m07_owner_input(&name, &format!("{name}_id"), owner.node.name.span())
                        .help(rename_input_help(
                            input.written,
                            &name,
                            &format!("{name}_id"),
                            &forbidden,
                        )),
                );
            }
        }
        if input.input.optional.is_some() {
            if let Some(child) = node
                .children
                .iter()
                .find(|child| !child.parse_failed && child.name == name)
            {
                diagnostics.raise(
                    child.name.span(),
                    diag::m02_method_conflict(&name, &format!("fetch_{name}"), input.input.span).help(
                        if child_candidate(node,child,&format!("fetch_{name}"),&owners.iter().map(|o|flat::owner_name(o.node)).collect::<Vec<_>>()) {
                            format!("rename this child to `fetch_{name}`")
                        } else {"choose a distinct child name that conflicts with no sibling, optional input or generated method, and update its uses".into()}
                    ),
                );
            }
        }
    }
}

fn remember<K: Ord, V: Copy>(map: &mut BTreeMap<K, V>, key: K, value: V) -> Option<V> {
    match map.entry(key) {
        std::collections::btree_map::Entry::Occupied(first) => Some(*first.get()),
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(value);
            None
        }
    }
}

/// Name the actual placement kinds, including the statements of a part.
fn rename_input_help(node: &Node, name: &str, replacement: &str, forbidden: &[String]) -> String {
    if !input_candidate(node, replacement, true, forbidden) {
        return "choose a distinct legal input name that conflicts with no owner, input or generated method, and update its existing references without changing wire names".into();
    }
    let mut places = std::collections::BTreeSet::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        for statement in &node.statements {
            if statement_uses(statement, name) {
                places.insert(statement.keyword.to_string());
            }
        }
        pending.extend(&node.children);
    }
    let places = places.into_iter().collect::<Vec<_>>().join(", ");
    let location = if places.is_empty() {
        "placements and expressions".into()
    } else {
        format!("{places} placements and expressions")
    };
    format!("rename this input to `{replacement}`; update its references in the existing {location}, retaining the wire keys and fixed text for {name}")
}

pub(crate) fn expression_uses(expr: &syn::Expr, name: &str) -> bool {
    let mut pending = vec![expr.to_token_stream()];
    while let Some(tokens) = pending.pop() {
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Ident(ident) if ident == name => return true,
                proc_macro2::TokenTree::Group(group) => pending.push(group.stream()),
                proc_macro2::TokenTree::Literal(lit)
                    if syn::parse_str::<syn::LitStr>(&lit.to_string())
                        .is_ok_and(|lit| lit.value().contains(&format!("{{{name}}}"))) =>
                {
                    return true
                }
                _ => {}
            }
        }
    }
    false
}

fn statement_uses(statement: &Statement, name: &str) -> bool {
    fn template(t: &Template, name: &str) -> bool {
        t.pieces
            .iter()
            .any(|p| matches!(p, TemplatePiece::Hole { reference, .. } if reference.name == name))
    }
    fn entries(entries: &Entries, name: &str) -> bool {
        entries.entries.iter().any(|entry| match &entry.kind {
            EntryKind::Short(reference) => reference.name == name,
            EntryKind::Value {
                value: Value::Template(t),
                ..
            } => template(t, name),
            EntryKind::Value {
                value: Value::Expression(e),
                ..
            } => expression_uses(e, name),
            EntryKind::Remove(_) => false,
        })
    }
    match &statement.kind {
        StatementKind::Path(t) | StatementKind::Base(Base::Template(t)) => template(t, name),
        StatementKind::Base(Base::Reference(reference)) => reference.name == name,
        StatementKind::Query(e)
        | StatementKind::Headers(e)
        | StatementKind::Body(Body::Entries { entries: e, .. }) => entries(e, name),
        StatementKind::Body(Body::Expression(e))
        | StatementKind::Read(e)
        | StatementKind::Middleware(e)
        | StatementKind::Paginate(e)
        | StatementKind::IdleTimeout(e)
        | StatementKind::Idempotent(Some(e)) => expression_uses(e, name),
        StatementKind::Problem {
            read: Some((_, e)), ..
        } => expression_uses(e, name),
        StatementKind::Status(arms) => arms.arms.iter().any(|arm| {
            arm.expression
                .as_ref()
                .is_some_and(|e| expression_uses(e, name))
        }),
        _ => false,
    }
}
