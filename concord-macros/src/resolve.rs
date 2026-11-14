//! Expand parts and resolve each branch into source-preserving flat facts.

use std::collections::{BTreeMap, BTreeSet};

use quote::quote;
use syn::spanned::Spanned;

use crate::{
    ast::*,
    diag::{self, Diagnostics, Written},
    flat::{self, BodyLine, EntryLine, InputLine, Level, Line, Origin, Owner, PathLine},
    name,
};

#[derive(Clone, Default)]
struct State<'a> {
    owners: Vec<Owner<'a>>,
    inputs: Vec<InputLine<'a>>,
    base: Option<Line<'a, Base>>,
    generation: usize,
    paths: Vec<PathLine<'a>>,
    query: BTreeMap<String, EntryLine<'a>>,
    headers: BTreeMap<String, EntryLine<'a>>,
    query_positions: BTreeMap<String, usize>,
    header_positions: BTreeMap<String, usize>,
    sequence: usize,
    reader: Option<Line<'a, syn::Expr>>,
    arms: Vec<Line<'a, Arm>>,
    clock: Option<Line<'a, syn::Expr>>,
    problem: Option<Line<'a, Statement>>,
    problem_reader: Option<Line<'a, syn::Expr>>,
    tainted: bool,
}

/// A spelling suggestion must be close, unique, and independent of map order.
fn nearest_part<'a>(name: &str, candidates: impl Iterator<Item = &'a String>) -> Option<&'a str> {
    let mut best = None;
    let mut distance = usize::MAX;
    let mut tied = false;
    for candidate in candidates {
        let mut row: Vec<_> = (0..=candidate.chars().count()).collect();
        for (i, a) in name.chars().enumerate() {
            let mut diagonal = row.first().copied().unwrap_or(0);
            if let Some(first) = row.first_mut() {
                *first = i + 1;
            }
            for (j, b) in candidate.chars().enumerate() {
                let above = row.get(j + 1).copied().unwrap_or(0);
                let left = row.get(j).copied().unwrap_or(0);
                if let Some(cell) = row.get_mut(j + 1) {
                    *cell = (above + 1)
                        .min(left + 1)
                        .min(diagonal + usize::from(a != b));
                }
                diagonal = above;
            }
        }
        let current = row.last().copied().unwrap_or(usize::MAX);
        if current < distance {
            best = Some(candidate.as_str());
            distance = current;
            tied = false;
        } else if current == distance {
            tied = true;
        }
    }
    if !tied && distance <= (name.len() / 3).clamp(1, 3) {
        best
    } else {
        None
    }
}

/// Resolve even a recovered tree, without deriving absence from damaged facts.
pub(crate) fn resolve<'a>(client: &'a Client, diagnostics: &mut Diagnostics) -> flat::Client<'a> {
    let mut result = flat::Client::default();
    let mut used = BTreeSet::new();
    let mut nodes = vec![&client.node];
    let mut damaged = false;
    while let Some(node) = nodes.pop() {
        damaged |= node.signatures_uncertain;
        nodes.extend(&node.children);
    }
    let parts: BTreeMap<_, _> = client
        .node
        .parts
        .iter()
        .map(|p| (p.name.to_string(), p))
        .collect();
    let mut pending = vec![(
        &client.node,
        name::module_name(&client.node.name.to_string()),
        State::default(),
    )];
    while let Some((node, path, mut state)) = pending.pop() {
        let mut local_inputs = own_inputs(node, node);
        let mut sources = Vec::new();
        let mut local_taint = node.parse_failed;
        if !node.parse_failed {
            let mut spreads = BTreeMap::new();
            if let Some(signature) = &node.signature {
                for parameter in &signature.entries {
                    let Parameter::Spread(spread) = parameter else {
                        continue;
                    };
                    let part_name = spread.name.to_string();
                    if matches!(node.kind, NodeKind::Client | NodeKind::Part) {
                        let spread_name = &spread.name;
                        let written = Written::new(quote!(..#spread_name));
                        // Identifiers below are constructed from the actual spread, never an example.
                        let written = if let Ok(tokens) = format!("..{part_name}").parse() {
                            Written::new(tokens)
                        } else {
                            written
                        };
                        diagnostics.raise(
                            spread.name.span(),
                            if matches!(node.kind, NodeKind::Client) {
                                diag::client_spread(&written)
                            } else {
                                diag::part_spread(&written)
                            },
                        );
                        continue;
                    }
                    if let Some(first) = spreads.insert(part_name.clone(), spread.name.span()) {
                        diagnostics.raise(
                            spread.name.span(),
                            diag::duplicate_spread(&part_name, first),
                        );
                        continue;
                    }
                    let Some(part) = parts.get(&part_name).copied() else {
                        let suggestion = nearest_part(&part_name, parts.keys()).filter(|candidate| {
                            !signature.entries.iter().any(|entry| matches!(entry, Parameter::Spread(other) if other.name == *candidate))
                        });
                        diagnostics.raise(
                            spread.name.span(),
                            diag::unknown_part(&part_name, &part_name).help(suggestion.map_or_else(
                                || "declare the intended part in the client block, or correct this spread's name".to_owned(),
                                |name| format!("did you mean `..{name}`? check the inputs and statements of that part"),
                            )),
                        );
                        local_taint = true;
                        continue;
                    };
                    used.insert(part_name.clone());
                    if part.parse_failed {
                        local_taint = true;
                        continue;
                    }
                    for input in own_inputs(part, node) {
                        if let Some(first) = local_inputs
                            .iter()
                            .find(|i| i.input.name == input.input.name)
                        {
                            let message = if matches!(first.written.kind, NodeKind::Part) {
                                diag::two_part_inputs(
                                    &input.input.name.to_string(),
                                    &first.written.name.to_string(),
                                    &part_name,
                                    first.input.span,
                                )
                            } else {
                                diag::part_input(
                                    &input.input.name.to_string(),
                                    &part_name,
                                    input.input.span,
                                )
                            };
                            if matches!(first.written.kind, NodeKind::Part) {
                                let dots = &spread.dots;
                                let name = &spread.name;
                                diagnostics.raise_spanned(quote!(#dots #name), message);
                            } else {
                                diagnostics.raise(first.input.span, message);
                            }
                        } else {
                            local_inputs.push(input);
                        }
                    }
                    for statement in &part.statements {
                        if matches!(node.kind, NodeKind::Scope)
                            && matches!(
                                statement.kind,
                                StatementKind::Body(_)
                                    | StatementKind::Paginate(_)
                                    | StatementKind::Idempotent(_)
                            )
                        {
                            let name = &spread.name;
                            diagnostics.raise(
                                spread.dots.spans[0],
                                diag::part_requires_endpoint(
                                    &statement.keyword.to_string(),
                                    &Written::new(quote!(..#name)),
                                ),
                            );
                        } else {
                            sources.push((part, statement));
                        }
                    }
                }
            }
            sources.extend(node.statements.iter().map(|s| (node, s)));
        } else {
            // Its retained signature is still evidence of part use, even when
            // this node's body cannot be resolved.
            if let Some(signature) = &node.signature {
                used.extend(signature.entries.iter().filter_map(|p| {
                    if let Parameter::Spread(spread) = p {
                        Some(spread.name.to_string())
                    } else {
                        None
                    }
                }));
            }
            local_inputs.clear();
        }
        local_taint |= sources.iter().any(|(_, statement)| {
            matches!(&statement.kind,
            StatementKind::Status(arms) if arms.arms.iter().any(|a| flat::status(&a.key).is_none()))
        });
        state.tainted |= local_taint;
        state.inputs.extend(local_inputs.clone());
        if matches!(node.kind, NodeKind::Client | NodeKind::Scope) {
            state.owners.push(Owner {
                node,
                inputs: local_inputs.clone(),
            });
        }
        if !node.parse_failed {
            name::expanded(node, &local_inputs, &state.owners, diagnostics);
        }
        let has_base = sources
            .iter()
            .any(|(_, s)| matches!(s.kind, StatementKind::Base(_)) && s.edit.is_none());
        if has_base {
            state.generation += 1;
        }
        let statements: Vec<_> = sources
            .into_iter()
            .map(|(written, statement)| {
                let is_part = matches!(written.kind, NodeKind::Part);
                let owners = if is_part {
                    state
                        .owners
                        .iter()
                        .filter(|o| matches!(o.node.kind, NodeKind::Client))
                        .cloned()
                        .collect()
                } else {
                    state.owners.clone()
                };
                let locals = if is_part {
                    own_inputs(written, node)
                } else {
                    local_inputs.clone()
                };
                Line {
                    value: statement,
                    origin: Origin {
                        written,
                        placed: node,
                        path: path.clone(),
                        owners,
                        locals,
                        span: statement.keyword.span(),
                        base_generation: state.generation,
                        tainted: if is_part {
                            client.node.parse_failed
                        } else {
                            state.tainted
                        },
                    },
                }
            })
            .collect();
        let level = Level {
            node,
            inputs: local_inputs,
            statements,
            tainted: local_taint,
        };
        let mut singles = BTreeMap::<String, &Line<'_, Statement>>::new();
        let mut body = None;
        let mut pager = None;
        let mut idempotent = None;
        let mut local_query = BTreeMap::new();
        let mut local_headers = BTreeMap::new();
        let mut local_body = BTreeMap::new();
        let mut body_positions = BTreeMap::new();
        let mut local_arms = BTreeMap::new();
        let inherited_arms = state.arms.clone();
        let edited_exacts: BTreeSet<_> = level
            .statements
            .iter()
            .filter_map(|s| {
                if let StatementKind::Status(arms) = &s.value.kind {
                    Some(arms)
                } else {
                    None
                }
            })
            .flat_map(|a| &a.arms)
            .filter(|a| a.edit.is_some())
            .map(|a| a.key.to_string())
            .collect();
        for line in &level.statements {
            let statement = line.value;
            let word = statement.keyword.to_string();
            if let Some(edit) = &statement.edit {
                let replacement = edited_statement_repair(statement, &state);
                let message = if word == "path" {
                    diag::edit_path(&replacement)
                } else {
                    diag::edited_statement(&word, &replacement)
                };
                let duplicate = match &statement.kind {
                    StatementKind::Query(e) | StatementKind::Headers(e) => {
                        let keys = e
                            .entries
                            .iter()
                            .map(|e| flat::key(e, word == "headers"))
                            .collect::<BTreeSet<_>>();
                        keys.len() != e.entries.len()
                    }
                    _ => false,
                };
                diagnostics.raise(edit.span(),if duplicate {message.help("put edits on individual entries inside this block, consolidate repeated keys and retain every required input placement")} else {message});
                continue;
            }
            if matches!(
                statement.kind,
                StatementKind::Base(_)
                    | StatementKind::Read(_)
                    | StatementKind::Body(_)
                    | StatementKind::Paginate(_)
                    | StatementKind::Idempotent(_)
                    | StatementKind::IdleTimeout(_)
            ) {
                if let Some(first) = singles.get(&word) {
                    let join = matches!((&first.value.kind, &statement.kind), (StatementKind::Body(Body::Entries { function: a, .. }), StatementKind::Body(Body::Entries { function: b, .. })) if flat::tokens(a) == flat::tokens(b));
                    if !join {
                        conflict(&word, line, first, None, diagnostics);
                        continue;
                    }
                } else {
                    singles.insert(word.clone(), line);
                }
            }
            match &statement.kind {
                StatementKind::Base(base) => {
                    state.base = Some(reline(line, base, statement.keyword.span()))
                }
                StatementKind::Path(template) => state.paths.push(PathLine {
                    line: reline(line, template, template.literal.span()),
                    crosses_base: false,
                }),
                StatementKind::Read(reader) => {
                    state.reader = Some(reline(line, reader, reader.span()))
                }
                StatementKind::IdleTimeout(expr) => {
                    state.clock = Some(reline(line, expr, expr.span()))
                }
                StatementKind::Query(entries) if !entries.parse_failed => resolve_entries(
                    entries,
                    line,
                    "query pair",
                    &mut local_query,
                    &mut state.query,
                    &mut state.query_positions,
                    &mut state.sequence,
                    diagnostics,
                ),
                StatementKind::Headers(entries) if !entries.parse_failed => resolve_entries(
                    entries,
                    line,
                    "header",
                    &mut local_headers,
                    &mut state.headers,
                    &mut state.header_positions,
                    &mut state.sequence,
                    diagnostics,
                ),
                StatementKind::Body(Body::Expression(expr)) => {
                    body = Some(BodyLine::Expression(reline(line, expr, expr.span())))
                }
                StatementKind::Body(Body::Entries { function, entries })
                    if !entries.parse_failed =>
                {
                    let mut new_entries = BTreeMap::new();
                    resolve_entries(
                        entries,
                        line,
                        "body entry",
                        &mut local_body,
                        &mut new_entries,
                        &mut body_positions,
                        &mut state.sequence,
                        diagnostics,
                    );
                    let mut new_entries: Vec<_> = new_entries.into_values().collect();
                    new_entries.sort_by_key(|entry| entry.sequence);
                    if let Some(BodyLine::Entries { entries, .. }) = &mut body {
                        entries.extend(new_entries);
                    } else {
                        body = Some(BodyLine::Entries {
                            function: reline(line, function, function.span()),
                            entries: new_entries,
                        });
                    }
                }
                StatementKind::Status(arms) if !arms.parse_failed => resolve_arms(
                    arms,
                    line,
                    &mut local_arms,
                    &mut state.arms,
                    &inherited_arms,
                    &edited_exacts,
                    diagnostics,
                ),
                StatementKind::Problem { ty, read } => {
                    state.problem = Some(line.clone());
                    state.problem_reader = read
                        .as_ref()
                        .map(|(_, expr)| reline(line, expr, expr.span()));
                    if state.problem_reader.is_none() && !state.tainted {
                        // A client's read may be written after its problem line.
                        state.problem_reader = level.statements.iter().find_map(|s| {
                            if let StatementKind::Read(expr) = &s.value.kind {
                                Some(reline(s, expr, expr.span()))
                            } else {
                                None
                            }
                        });
                    }
                    if state.problem_reader.is_none() && !state.tainted {
                        diagnostics.raise_spanned(
                            ty,
                            diag::problem_reader(&statement_written(statement), "Json").help(
                                "supply a reader on the client or after the problem type, before the semicolon",
                            ),
                        );
                    }
                }
                StatementKind::Middleware(expr) => {
                    result.middleware.push(reline(line, expr, expr.span()))
                }
                StatementKind::Paginate(expr) => pager = Some(reline(line, expr, expr.span())),
                StatementKind::Idempotent(expr) => {
                    idempotent = Some(reline(line, expr, statement.keyword.span()))
                }
                _ => {}
            }
        }
        if let NodeKind::Endpoint(method) = &node.kind {
            if !node.parse_failed {
                let head = method
                    .custom
                    .as_ref()
                    .map_or_else(|| method.keyword.to_string(), syn::LitStr::value)
                    == "HEAD";
                if head {
                    state.arms.retain(|a| {
                        std::ptr::eq(a.origin.placed, node) || flat::body_field(a.value).is_none()
                    });
                }
                state.arms.sort_by_key(|a| {
                    flat::status(&a.value.key).map_or((true, 0), |(n, class)| (class, n))
                });
                let mut query: Vec<_> = state.query.values().cloned().collect();
                let mut headers: Vec<_> = state.headers.values().cloned().collect();
                query.sort_by_key(|e| e.sequence);
                headers.sort_by_key(|e| e.sequence);
                for entry in query.iter_mut().chain(&mut headers) {
                    entry.crosses_base =
                        crosses(entry.line.origin.base_generation, state.generation);
                }
                for path in &mut state.paths {
                    path.crosses_base = crosses(path.line.origin.base_generation, state.generation);
                }
                let (wrapper, read_type) = node.output.as_ref().map_or((None, None), |o| {
                    let (w, t) = flat::arrow(&o.ty);
                    (w, Some(t))
                });
                let endpoint = flat::Endpoint {
                    node,
                    path: path.clone(),
                    inputs: state.inputs.clone(),
                    idle_timeout: state.clock.clone(),
                    idempotent,
                    base: state.base.clone(),
                    paths: state.paths.clone(),
                    query,
                    headers,
                    body,
                    reader: state.reader.clone(),
                    arms: state.arms.clone(),
                    problem: state.problem.clone(),
                    problem_reader: state.problem_reader.clone(),
                    pager,
                    read_type,
                    wrapper,
                    tainted: state.tainted,
                };
                endpoint_checks(&endpoint, head, diagnostics);
                result.endpoints.push(endpoint);
            }
        }
        result.levels.push(level);
        for child in node.children.iter().rev() {
            pending.push((child, format!("{path}.{}", child.name), state.clone()));
        }
    }
    // Parts are checked once in their own environment, also when no node uses them.
    for part in &client.node.parts {
        if part.parse_failed {
            continue;
        }
        if !used.contains(&part.name.to_string()) && !damaged {
            diagnostics.raise(part.name.span(), diag::unused_part(&part.name.to_string()));
        }
        for statement in &part.statements {
            if matches!(
                statement.kind,
                StatementKind::Path(_)
                    | StatementKind::Problem { .. }
                    | StatementKind::Middleware(_)
            ) {
                diagnostics.raise(
                    statement.keyword.span(),
                    diag::part_statement(
                        &statement.keyword.to_string(),
                        &statement_written(statement),
                    ).help("place this behaviour on the appropriate tree node; consolidate it with any existing statement there"),
                );
            }
        }
        for nested in &part.parts {
            if !nested.parse_failed {
                let name = &nested.name;
                diagnostics.raise(
                    nested.span,
                    diag::part_placement(&Written::new(quote!(part #name() { }))),
                );
            }
        }
        for child in &part.children {
            if !child.parse_failed {
                let name = &child.name;
                diagnostics.raise(child.span, diag::part_child(&Written::new(quote!(#name))));
            }
        }
        if let Some(signature) = &part.signature {
            for parameter in &signature.entries {
                if let Parameter::Spread(spread) = parameter {
                    let name = &spread.name;
                    diagnostics.raise(
                        spread.name.span(),
                        diag::part_spread(&Written::new(quote!(..#name))),
                    );
                }
            }
        }
    }
    result
}

fn own_inputs<'a>(written: &'a Node, owner: &'a Node) -> Vec<InputLine<'a>> {
    written.signature.as_ref().map_or_else(Vec::new, |s| {
        s.entries
            .iter()
            .filter_map(|p| {
                if let Parameter::Input(input) = p {
                    Some(InputLine {
                        input,
                        owner,
                        written,
                    })
                } else {
                    None
                }
            })
            .collect()
    })
}

/// The base in reach at the writer counts; a missing one binds at the first below.
fn crosses(written: usize, endpoint: usize) -> bool {
    if written == 0 {
        endpoint > 1
    } else {
        endpoint > written
    }
}

fn reline<'a, T>(line: &Line<'a, Statement>, value: &'a T, span: proc_macro2::Span) -> Line<'a, T> {
    let mut origin = line.origin.clone();
    origin.span = span;
    Line { value, origin }
}

fn conflict(
    item: &str,
    second: &Line<'_, Statement>,
    first: &Line<'_, Statement>,
    key: Option<&str>,
    diagnostics: &mut Diagnostics,
) {
    let a = &first.origin.written.name.to_string();
    let b = &second.origin.written.name.to_string();
    let first_part = matches!(first.origin.written.kind, NodeKind::Part);
    let second_part = matches!(second.origin.written.kind, NodeKind::Part);
    let message = if first_part && second_part && a != b {
        diag::two_parts(item, a, b, first.origin.span)
    } else if let Some(kind) = key {
        diag::part_key(
            kind,
            item,
            if first_part { a } else { b },
            first.origin.span,
        )
    } else if item == "body" {
        diag::body_conflict(
            if first_part { a } else { b },
            &statement_written(second.value),
            first.origin.span,
        )
    } else {
        diag::part_conflict(
            item,
            if first_part { a } else { b },
            if matches!(second.origin.placed.kind, NodeKind::Scope) {
                "scope"
            } else {
                "endpoint"
            },
            first.origin.span,
        )
    };
    let span = if second_part && first_part {
        second
            .origin
            .placed
            .signature
            .as_ref()
            .and_then(|signature| {
                signature.entries.iter().find_map(|parameter| {
                    if let Parameter::Spread(spread) = parameter {
                        (spread.name == second.origin.written.name).then_some(spread.dots.spans[0])
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(second.origin.span)
    } else {
        second.origin.span
    };
    if second_part && first_part {
        let name = &second.origin.written.name;
        if let Some(spread) = second.origin.placed.signature.as_ref().and_then(|s| {
            s.entries.iter().find_map(|p| {
                if let Parameter::Spread(spread) = p {
                    (spread.name == *name).then_some(spread)
                } else {
                    None
                }
            })
        }) {
            let dots = &spread.dots;
            let name = &spread.name;
            diagnostics.raise_spanned(quote!(#dots #name), message.help("consolidate the conflicting contributions of these parts, retaining the intended values and input placements, or choose compatible parts"));
            return;
        }
    }
    diagnostics.raise(span, message.help("consolidate these contributions, retaining the intended value or body function and every input placement; use compatible parts or remove inputs that are no longer needed"));
}

type Seen<'a> = BTreeMap<String, Line<'a, Statement>>;

#[allow(clippy::too_many_arguments)]
fn resolve_entries<'a>(
    entries: &'a Entries,
    source: &Line<'a, Statement>,
    kind: &str,
    seen: &mut Seen<'a>,
    values: &mut BTreeMap<String, EntryLine<'a>>,
    positions: &mut BTreeMap<String, usize>,
    sequence: &mut usize,
    diagnostics: &mut Diagnostics,
) {
    let header = kind == "header";
    let body = kind == "body entry";
    for entry in &entries.entries {
        let key = flat::key(entry, header);
        let mut site = source.clone();
        site.origin.span = entry.span;
        if let Some(first) = seen.get(&key) {
            if std::ptr::eq(first.origin.written, source.origin.written) {
                diagnostics.raise(
                    entry.span,
                    diag::duplicate_key(
                        kind,
                        &key,
                        &Written::new(flat::entry_tokens(entry)),
                        first.origin.span,
                    ).help("consolidate these entries into one intended value, retaining every needed input placement or removing declarations that are no longer needed"),
                );
            } else {
                conflict(&key, &site, first, Some(kind), diagnostics);
            }
            continue;
        }
        seen.insert(key.clone(), site);
        let old = values.get(&key);
        let edit = entry.edit.as_ref().map(ToString::to_string);
        let written = Written::new(flat::entry_replacement(entry));
        if edit.as_deref() == Some("remove") {
            if old.is_some() {
                values.remove(&key);
            } else if !source.origin.tainted {
                diagnostics.raise(
                    entry.span,
                    if header {
                        diag::remove_header(&key, &written)
                    } else if body {
                        diag::remove_body(&key, &written)
                    } else {
                        diag::remove_query(&key, &written)
                    },
                );
            }
            continue;
        }
        if edit.as_deref() == Some("override") && (old.is_none() || body) && !source.origin.tainted
        {
            diagnostics.raise(
                entry.span,
                if header {
                    diag::override_header(&key, &written)
                } else if body {
                    diag::override_body(&key, &written)
                } else {
                    diag::override_query(&key, &written)
                },
            );
        } else if let Some(first) = old {
            if edit.is_none() {
                diagnostics.raise(
                    entry.span,
                    if header {
                        diag::inherited_header(&key, &written, first.line.origin.span)
                    } else {
                        diag::inherited_query(&key, &written, first.line.origin.span)
                    },
                );
            }
        }
        let next = positions.len();
        let position = *positions.entry(key.clone()).or_insert(next);
        values.insert(
            key.clone(),
            EntryLine {
                line: reline(source, entry, entry.span),
                key,
                position,
                sequence: *sequence,
                crosses_base: false,
            },
        );
        *sequence += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_arms<'a>(
    arms: &'a Arms,
    source: &Line<'a, Statement>,
    seen: &mut Seen<'a>,
    inherited: &mut Vec<Line<'a, Arm>>,
    before: &[Line<'a, Arm>],
    edited: &BTreeSet<String>,
    diagnostics: &mut Diagnostics,
) {
    for arm in &arms.arms {
        let key = arm.key.to_string();
        if flat::status(&arm.key).is_none() {
            continue;
        }
        let mut site = source.clone();
        site.origin.span = arm.key.span();
        if let Some(first) = seen.get(&key) {
            if std::ptr::eq(first.origin.written, source.origin.written) {
                diagnostics.raise(
                    arm.key.span(),
                    diag::duplicate_key(
                        "status arm",
                        &key,
                        &Written::new(quote!(#key)),
                        first.origin.span,
                    ).help("consolidate these arms into one intended arm, retaining every needed input reference or removing declarations that are no longer needed"),
                );
            } else {
                conflict(&key, &site, first, Some("status arm"), diagnostics);
            }
            continue;
        }
        seen.insert(key.clone(), site);
        let old = inherited
            .iter()
            .position(|a| a.value.key.to_string() == key);
        let class = flat::status(&arm.key).map(|(n, _)| format!("{}xx", n / 100));
        let class_arm = before.iter().find(|a| {
            Some(a.value.key.to_string()) == class
                && !std::ptr::eq(a.origin.placed, source.origin.placed)
        });
        let edit = arm.edit.as_ref().map(ToString::to_string);
        let key_tokens = &arm.key;
        let plain = Written::new(flat::arm_tokens(arm));
        if edit.as_deref() == Some("remove") {
            if let Some(index) = old {
                inherited.remove(index);
            } else if !source.origin.tainted {
                let removal = Written::new(quote!(remove #key_tokens));
                let message = if let Some(class) = class_arm {
                    diag::remove_class_member(&key, &class.value.key.to_string(), &plain).help(
                        "an exact removal cannot exclude a member of a class; write an overriding exact arm with the intended fields and expression",
                    )
                } else {
                    diag::remove_arm(&key, &removal)
                };
                diagnostics.raise(arm.span, message);
            }
            continue;
        }
        let covered = old.and_then(|i| inherited.get(i)).or(class_arm);
        if edit.as_deref() == Some("override") {
            let exact_targets: Vec<_> = before
                .iter()
                .filter(|a| {
                    !std::ptr::eq(a.origin.placed, source.origin.placed)
                        && flat::status(&a.value.key).is_some_and(|(n, class)| {
                            !class
                                && flat::status(&arm.key)
                                    .is_some_and(|(m, is_class)| is_class && n / 100 == m / 100)
                        })
                })
                .collect();
            if covered.is_none() && exact_targets.is_empty() && !source.origin.tainted {
                diagnostics.raise(arm.span, diag::override_arm(&key, &plain));
            }
            for exact in exact_targets {
                let exact_key = exact.value.key.to_string();
                if !edited.contains(&exact_key) {
                    diagnostics.raise_spanned(
                        {
                            let edit = &arm.edit;
                            let key = &arm.key;
                            quote!(#edit #key)
                        },
                        {
                            let message=diag::class_override(&key,&exact_key,&Written::new(flat::arm_tokens(exact.value)),exact.origin.span);
                            if arm_move_safe(exact) {message} else {message.help("override or remove each inherited exact arm of this class; if you copy an arm here, qualify its ancestor inputs with their owners")}
                        },
                    );
                }
            }
        } else if let Some(first) = covered {
            diagnostics.raise(
                arm.key.span(),
                diag::inherited_arm(&key, &plain, first.origin.span),
            );
        }
        if let Some(index) = old {
            inherited.remove(index);
        }
        inherited.push(reline(source, arm, arm.key.span()));
    }
}

fn arm_move_safe(arm: &Line<'_, Arm>) -> bool {
    !arm.value.expression.as_ref().is_some_and(|expr| {
        arm.origin
            .locals
            .iter()
            .any(|input| crate::name::expression_uses(expr, &input.input.name.to_string()))
    })
}

fn endpoint_checks(endpoint: &flat::Endpoint<'_>, head: bool, diagnostics: &mut Diagnostics) {
    if endpoint.base.is_none() && !endpoint.tainted {
        diagnostics.raise(
            endpoint.node.name.span(),
            diag::missing_base(&Written::new(quote!(base "https://api.example.com";)))
                .help("set the intended base URL on this endpoint or an ancestor"),
        );
    }
    let mut no_arrow_reported = false;
    for arm in &endpoint.arms {
        if let Some(field) = flat::body_field(arm.value) {
            if !head && endpoint.node.output.is_none() && !no_arrow_reported {
                no_arrow_reported = true;
                diagnostics.raise(
                    endpoint.node.name.span(),
                    diag::no_arrow(
                        &Written::new(quote!(-> Ignored)),
                        &Written::new({
                            let key = &arm.value.key;
                            quote!(status { remove #key })
                        }),
                        field.name.span(),
                    ).help("declare the intended result type after an arrow, or stop the arms in reach from reading a body"),
                );
            }
            if !head
                && endpoint.wrapper.is_some()
                && !std::ptr::eq(arm.origin.placed, endpoint.node)
                && flat::status(&arm.value.key).is_some_and(|(n, _)| n / 100 == 2)
            {
                let key = &arm.value.key;
                let fields = arm.value.fields.as_ref().map(|fields| {
                    let fields = fields.fields.iter().map(|field| {
                        let name = &field.name;
                        let alias = field.binding.as_ref().map(|binding| quote!(: #binding));
                        quote!(#name #alias)
                    });
                    quote!({ #(#fields),* })
                });
                let expr = &arm.value.expression;
                let expression = if endpoint.wrapper == Some(flat::Wrapper::Result) {
                    quote!(Ok(#expr))
                } else {
                    quote!(Some(#expr))
                };
                let replacement = Written::new(quote!(#key #fields => #expression));
                let message = if endpoint.wrapper == Some(flat::Wrapper::Result) {
                    diag::result_arm(&key.to_string(), &replacement, arm.origin.span)
                } else {
                    diag::option_exact_arm(&key.to_string(), &replacement, arm.origin.span)
                }
                .help(if !arm_move_safe(arm) {
                    "write the intended success arm on this endpoint with the wrapper the result needs; qualify its ancestor inputs with their owners and consolidate the existing status block".into()
                } else if has_status(endpoint.node) {
                    format!(
                        "in this `status` block, add `override {}`",
                        replacement.text()
                    )
                } else {
                    format!(
                        "add `status {{ override {} }}` to this endpoint",
                        replacement.text()
                    )
                });
                if let Some(output) = &endpoint.node.output {
                    let arrow = &output.arrow;
                    diagnostics.raise_spanned(quote!(#arrow), message);
                }
            }
            if !head && endpoint.reader.is_none() && !endpoint.tainted {
                missing_reader(field.name.span(), diagnostics);
            }
        }
    }
    let success_covered = (200..300).all(|n| {
        endpoint.arms.iter().any(|a| {
            flat::status(&a.value.key).is_some_and(|(key, class)| {
                if class {
                    n / 100 == key / 100
                } else {
                    n == key
                }
            })
        })
    });
    if !head && !success_covered && endpoint.reader.is_none() && !endpoint.tainted {
        if let Some(output) = &endpoint.node.output {
            let arrow = &output.arrow;
            let ty = &output.ty;
            diagnostics.raise_spanned(
                quote!(#arrow #ty),
                diag::missing_reader(
                    &Written::new(quote!(read Json;)),
                    &Written::new(quote!(
                        use concord::Json;
                    )),
                )
                .help("supply the intended reader on this endpoint or an ancestor"),
            );
        }
    }
}

fn missing_reader(span: proc_macro2::Span, diagnostics: &mut Diagnostics) {
    diagnostics.raise(
        span,
        diag::missing_reader(
            &Written::new(quote!(read Json;)),
            &Written::new(quote!(
                use concord::Json;
            )),
        )
        .help("supply the intended reader on this endpoint or an ancestor"),
    );
}

fn statement_written(statement: &Statement) -> Written {
    let keyword = &statement.keyword;
    let rhs = match &statement.kind {
        StatementKind::Path(t) | StatementKind::Base(Base::Template(t)) => {
            let lit = &t.literal;
            quote!(#lit;)
        }
        StatementKind::Read(e)
        | StatementKind::Middleware(e)
        | StatementKind::Paginate(e)
        | StatementKind::IdleTimeout(e)
        | StatementKind::Body(Body::Expression(e)) => quote!(#e;),
        StatementKind::Body(Body::Entries { function, entries }) => {
            let entries = entries.entries.iter().map(flat::entry_tokens);
            quote!(#function { #(#entries)* })
        }
        StatementKind::Query(entries) | StatementKind::Headers(entries) => {
            let entries = entries.entries.iter().map(|entry| {
                let edit = &entry.edit;
                let tokens = flat::entry_tokens(entry);
                if matches!(entry.kind, EntryKind::Remove(_)) {
                    tokens
                } else {
                    quote!(#edit #tokens)
                }
            });
            quote!({ #(#entries)* })
        }
        StatementKind::Status(arms) => {
            let arms = arms.arms.iter().map(|arm| {
                let edit = &arm.edit;
                let key = &arm.key;
                if arm.expression.is_none() {
                    quote!(#edit #key)
                } else {
                    let tokens = flat::arm_tokens(arm);
                    quote!(#edit #tokens)
                }
            });
            quote!({ #(#arms),* })
        }
        StatementKind::Idempotent(e) => quote!(#e;),
        StatementKind::Problem { ty, read } => {
            let reader = read.as_ref().map(|(_, expr)| quote!(read #expr));
            quote!(#ty #reader;)
        }
        StatementKind::Base(Base::Reference(r)) => {
            let name = &r.name;
            let owner = r.owner.as_ref().map(|o| quote!(#o.));
            quote!(#owner #name;)
        }
    };
    Written::new(quote!(#keyword #rhs))
}

/// An explicit block on this node must be extended rather than duplicated.
pub(crate) fn has_status(node: &Node) -> bool {
    node.statements
        .iter()
        .any(|s| matches!(s.kind, StatementKind::Status(_)))
}

/// Dropping an invalid block edit must not drop entries or expose message 21.
fn edited_statement_repair(statement: &Statement, state: &State<'_>) -> Written {
    let (entries, inherited, header) = match &statement.kind {
        StatementKind::Query(entries) => (entries, &state.query, false),
        StatementKind::Headers(entries) => (entries, &state.headers, true),
        StatementKind::Status(arms) => {
            let keyword = &statement.keyword;
            let arms = arms.arms.iter().map(|arm| {
                let edit = &arm.edit;
                let tokens = flat::arm_tokens(arm);
                let key = &arm.key;
                if arm.expression.is_none() {
                    return quote!(#edit #key);
                }
                let covered = state.arms.iter().any(|a| {
                    flat::status(&a.value.key).is_some_and(|(n, class)| {
                        flat::status(&arm.key)
                            .is_some_and(|(m, _)| n == m || (class && n / 100 == m / 100))
                    })
                });
                if edit.is_none() && covered {
                    quote!(override #tokens)
                } else {
                    quote!(#edit #tokens)
                }
            });
            return Written::new(quote!(#keyword { #(#arms),* }));
        }
        StatementKind::Idempotent(_) if statement.edit.as_ref().is_some_and(|e| e == "remove") => {
            let keyword = &statement.keyword;
            return Written::new(quote!(#keyword false;));
        }
        _ => return statement_written(statement),
    };
    let keyword = &statement.keyword;
    let entries = entries.entries.iter().map(|entry| {
        let tokens = flat::entry_replacement(entry);
        let edit = &entry.edit;
        if edit.is_none() && inherited.contains_key(&flat::key(entry, header)) {
            quote!(override #tokens)
        } else if matches!(entry.kind, EntryKind::Remove(_)) {
            tokens
        } else {
            quote!(#edit #tokens)
        }
    });
    Written::new(quote!(#keyword { #(#entries)* }))
}
