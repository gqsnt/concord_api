//! Validate inputs, templates, paths, headers, and response patterns.

use crate::{
    ast::*,
    diag::{self, Diagnostics, Written},
    flat::{self, InputLine, Line, Origin},
};
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{quote, ToTokens};
use std::collections::{BTreeMap, BTreeSet};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

/// Check written facts too: pruning cannot hide typos or missing placements.
pub(crate) fn check(client: &Client, resolved: &flat::Client<'_>, diagnostics: &mut Diagnostics) {
    let mut checker = Checker {
        diagnostics,
        used: BTreeSet::new(),
        seen: BTreeSet::new(),
        writer_places: BTreeMap::new(),
        inherited_headers: BTreeMap::new(),
        local_header_keys: BTreeMap::new(),
        query_nodes: resolved
            .levels
            .iter()
            .filter(|l| {
                l.statements
                    .iter()
                    .any(|s| matches!(s.value.kind, StatementKind::Query(_)))
            })
            .map(|l| l.node as *const Node)
            .collect(),
    };
    for level in &resolved.levels {
        for line in &level.statements {
            checker
                .writer_places
                .entry(line.origin.written)
                .or_default()
                .insert(line.origin.placed);
        }
    }
    let levels = resolved
        .levels
        .iter()
        .map(|l| (l.node as *const Node, l))
        .collect::<BTreeMap<_, _>>();
    let mut headers = vec![(&client.node, BTreeSet::<String>::new())];
    while let Some((node, mut inherited)) = headers.pop() {
        checker.inherited_headers.insert(node, inherited.clone());
        let mut local = BTreeSet::new();
        if let Some(level) = levels.get(&(node as *const Node)) {
            for line in &level.statements {
                if let StatementKind::Headers(entries) = &line.value.kind {
                    for entry in &entries.entries {
                        let key = flat::key(entry, true);
                        local.insert(key.clone());
                        if matches!(entry.kind, EntryKind::Remove(_)) {
                            inherited.remove(&key);
                        } else {
                            inherited.insert(key);
                        }
                    }
                }
            }
        }
        checker.local_header_keys.insert(node, local);
        headers.extend(node.children.iter().map(|c| (c, inherited.clone())));
    }
    for level in &resolved.levels {
        if level.node.parse_failed {
            continue;
        }
        for line in &level.statements {
            checker.statement(line);
        }
        if let Some(output) = &level.node.output {
            self_types(&output.ty, checker.diagnostics);
        }
    }
    for part in &client.node.parts {
        if part.parse_failed {
            continue;
        }
        let locals = inputs(part);
        for statement in &part.statements {
            let origin = Origin {
                written: part,
                placed: part,
                path: part.name.to_string(),
                owners: vec![flat::Owner {
                    node: &client.node,
                    inputs: inputs(&client.node),
                }],
                locals: locals.clone(),
                span: statement.keyword.span(),
                base_generation: 0,
                tainted: client.node.parse_failed,
            };
            checker.statement(&Line {
                value: statement,
                origin,
            });
        }
    }
    for endpoint in &resolved.endpoints {
        let NodeKind::Endpoint(method) = &endpoint.node.kind else {
            continue;
        };
        let head = method
            .custom
            .as_ref()
            .map_or_else(|| method.keyword.to_string(), syn::LitStr::value)
            == "HEAD";
        if !head {
            continue;
        }
        for arm in &endpoint.arms {
            if std::ptr::eq(arm.origin.placed, endpoint.node) {
                if let Some(field) = flat::body_field(arm.value) {
                    let key = &arm.value.key;
                    checker.diagnostics.raise(
                        field.name.span(),
                        diag::head_body(&Written::new(quote!(#key => true))).help("remove the body binding and rewrite this arm's expression to compute its result from status and headers only"),
                    );
                }
            }
        }
        if let Some(output) = &endpoint.node.output {
            if !endpoint.tainted
                && !endpoint
                    .arms
                    .iter()
                    .any(|arm| flat::status(&arm.value.key).is_some_and(|(n, _)| n / 100 == 2))
            {
                checker.diagnostics.raise_spanned(
                    {
                        let arrow = &output.arrow;
                        let ty = &output.ty;
                        quote!(#arrow #ty)
                    },
                    diag::head_success(&Written::new(quote!(status { 2xx => true }))).help(
                        if crate::resolve::has_status(endpoint.node) {
                            "in this `status` block, add a success arm returning the declared result without reading a body"
                        } else { "add a status block with a success arm returning the declared result without reading a body" }
                    ),
                );
            }
        }
    }
    let mut pending = vec![&client.node];
    while let Some(node) = pending.pop() {
        if !node.parse_failed {
            attributes(node, checker.diagnostics);
            let mut declared = BTreeSet::new();
            for input in inputs(node) {
                let unique = declared.insert(input.input.name.to_string());
                input_type(input.input, checker.diagnostics);
                if unique
                    && !checker.used.contains(&input_id(input.input))
                    && !damaged_below(node)
                    && !(matches!(node.kind, NodeKind::Part)
                        && part_usage_uncertain(node, &client.node))
                {
                    checker.diagnostics.raise(
                        input.input.span,
                        diag::unused_input(&input.input.name.to_string()).help("place this input in the intended path, base, query, headers, body or behaviour, or remove its declaration"),
                    );
                }
            }
        }
        pending.extend(&node.children);
        if matches!(node.kind, NodeKind::Client) {
            pending.extend(&node.parts);
        }
    }
}

fn inputs(node: &Node) -> Vec<InputLine<'_>> {
    node.signature
        .as_ref()
        .into_iter()
        .flat_map(|s| &s.entries)
        .filter_map(|p| {
            if let Parameter::Input(input) = p {
                Some(InputLine {
                    input,
                    owner: node,
                    written: node,
                })
            } else {
                None
            }
        })
        .collect()
}
fn input_id(input: &Input) -> usize {
    std::ptr::from_ref(input) as usize
}
fn part_usage_uncertain(part: &Node, client: &Node) -> bool {
    let mut pending = vec![(client, false)];
    while let Some((node, ancestor_damaged)) = pending.pop() {
        let damaged = ancestor_damaged || damaged_below(node);
        if node.signatures_uncertain
            || (damaged
                && node.signature.as_ref().is_some_and(|s| {
                    s.entries
                        .iter()
                        .any(|p| matches!(p, Parameter::Spread(spread) if spread.name == part.name))
                }))
        {
            return true;
        }
        pending.extend(
            node.children
                .iter()
                .map(|child| (child, ancestor_damaged || node.parse_failed)),
        );
    }
    false
}

fn reader_in_reach(origin: &Origin<'_>) -> bool {
    let read = |node: &Node| {
        node.statements
            .iter()
            .any(|s| matches!(s.kind, StatementKind::Read(_)))
    };
    read(origin.placed)
        || read(origin.written)
        || origin.locals.iter().any(|i| read(i.written))
        || origin.owners.iter().any(|o| read(o.node))
}

fn field_repair(arm: &Arm, field: &Field, origin: &Origin<'_>) -> String {
    let replacement = match field.name.to_string().as_str() {
        "header" => "headers",
        "bodi" => "body",
        _ => return "choose the intended answer field and retain the binding used by this arm's expression".into(),
    };
    if arm.fields.as_ref().is_some_and(|fields| {
        fields
            .fields
            .iter()
            .any(|other| !std::ptr::eq(other, field) && other.name == replacement)
    }) {
        return "choose the intended answer fields without repeating a field or binding, and retain the names used by the expression".into();
    }
    if replacement == "body"
        && (!reader_in_reach(origin)
            || origin.placed.output.is_none()
            || matches!(&origin.placed.kind, NodeKind::Endpoint(method) if method.keyword == "HEAD" || method.custom.as_ref().is_some_and(|m| m.value() == "HEAD")))
    {
        return "choose the intended answer field and retain its expression's binding; reading a body also needs an arrow, a reader and a method other than HEAD".into();
    }
    let key = &arm.key;
    let edit = &arm.edit;
    let expression = &arm.expression;
    let fields = arm
        .fields
        .as_ref()
        .into_iter()
        .flat_map(|f| &f.fields)
        .map(|f| {
            let name = if std::ptr::eq(f, field) {
                syn::Ident::new(replacement, f.name.span())
            } else {
                f.name.clone()
            };
            let binding = f.binding.as_ref().unwrap_or(&f.name);
            quote!(#name: #binding)
        });
    let repaired = Written::new(quote!(#edit #key { #(#fields),* } => #expression));
    format!("write `{}`", repaired.text())
}

fn own_type(ty: &mut syn::Type) {
    use syn::visit_mut::{self, VisitMut};
    struct Owned;
    impl VisitMut for Owned {
        fn visit_type_mut(&mut self, ty: &mut syn::Type) {
            if let syn::Type::Reference(reference) = ty {
                let mut elem = (*reference.elem).clone();
                self.visit_type_mut(&mut elem);
                *ty = match elem {
                    syn::Type::Path(ref path) if path.path.is_ident("str") => {
                        syn::parse_quote!(String)
                    }
                    syn::Type::Slice(slice) => {
                        let elem = slice.elem;
                        syn::parse_quote!(Vec<#elem>)
                    }
                    syn::Type::TraitObject(object) => syn::parse_quote!(Box<#object>),
                    other => other,
                };
            } else {
                visit_mut::visit_type_mut(self, ty);
            }
        }
    }
    Owned.visit_type_mut(ty);
}
fn damaged_below(node: &Node) -> bool {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if node.parse_failed || node.statements.iter().any(|s| matches!(&s.kind, StatementKind::Status(arms) if arms.arms.iter().any(|arm| flat::status(&arm.key).is_none()))) {
            return true;
        }
        pending.extend(&node.children);
    }
    false
}
struct Checker<'a> {
    diagnostics: &'a mut Diagnostics,
    used: BTreeSet<usize>,
    seen: BTreeSet<usize>,
    inherited_headers: BTreeMap<*const Node, BTreeSet<String>>,
    local_header_keys: BTreeMap<*const Node, BTreeSet<String>>,
    query_nodes: BTreeSet<*const Node>,
    writer_places: BTreeMap<*const Node, BTreeSet<*const Node>>,
}
impl Checker<'_> {
    fn statement(&mut self, line: &Line<'_, Statement>) {
        if !self.seen.insert(std::ptr::from_ref(line.value) as usize) {
            return;
        }
        let origin = &line.origin;
        match &line.value.kind {
            StatementKind::Base(Base::Template(template)) => {
                self.template(template, origin, Place::Base)
            }
            StatementKind::Base(Base::Reference(reference)) => {
                self.reference(reference, origin, Place::Base, false, true)
            }
            StatementKind::Path(template) => self.template(template, origin, Place::Path),
            StatementKind::Query(entries) => self.entries(entries, origin, Place::Query),
            StatementKind::Headers(entries) => self.entries(entries, origin, Place::Header),
            StatementKind::Body(Body::Entries { function, entries }) => {
                ExpressionTypes(self.diagnostics).visit_expr_path(function);
                self.entries(entries, origin, Place::Body)
            }
            StatementKind::Body(Body::Expression(expr))
            | StatementKind::Read(expr)
            | StatementKind::Paginate(expr)
            | StatementKind::IdleTimeout(expr) => self.expression(expr, origin, false),
            StatementKind::Idempotent(Some(expr)) => self.expression(expr, origin, false),
            StatementKind::Middleware(expr) => self.expression(expr, origin, true),
            StatementKind::Problem { ty, read } => {
                self_types(ty, self.diagnostics);
                if let Some((_, expr)) = read {
                    self.expression(expr, origin, false);
                }
            }
            StatementKind::Status(arms) if !arms.parse_failed => {
                for arm in &arms.arms {
                    if flat::status(&arm.key).is_none() {
                        self.diagnostics.raise(
                            arm.key.span(),
                            diag::status_key(&arm.key.to_string(), "503", "5xx").help(
                                "choose the intended exact status from 100 to 599 or class from 1xx to 5xx",
                            ),
                        );
                        continue; // no field/expression facts below a refused arm.
                    }
                    if let Some(fields) = &arm.fields {
                        let mut field_names = BTreeMap::new();
                        let mut bindings = BTreeMap::new();
                        for field in &fields.fields {
                            let name = field.name.to_string();
                            let key = &arm.key;
                            let replacement = Written::new(quote!(#key { body } => body));
                            if !matches!(name.as_str(), "body" | "headers" | "status") {
                                self.diagnostics.raise(
                                    field.name.span(),
                                    diag::arm_field(
                                        &replacement,
                                        &Written::new(quote!(body: value)),
                                    )
                                    .help(field_repair(arm, field, origin)),
                                );
                            }
                            let repeated = field_names.insert(name.clone(), field.name.span());
                            if let Some(first) = repeated {
                                self.diagnostics.raise(
                                    field.name.span(),
                                    diag::duplicate_field(&name, &replacement, first).help(
                                        "bind each answer field once; retain the bindings used by this arm's expression",
                                    ),
                                );
                            }
                            let binding = field.binding.as_ref().unwrap_or(&field.name);
                            if let Some(first) =
                                bindings.insert(binding.to_string(), binding.span())
                            {
                                if repeated.is_none() {
                                    self.diagnostics.raise(
                                        binding.span(),
                                        diag::duplicate_binding(
                                            &binding.to_string(),
                                            &replacement,
                                            first,
                                        ).help("give these fields distinct binding names and update their uses in this arm's expression"),
                                    );
                                }
                            }
                        }
                    }
                    if let Some(expr) = &arm.expression {
                        self.expression(expr, origin, false);
                    }
                }
            }
            _ => {}
        }
    }
    fn expression(&mut self, expr: &syn::Expr, origin: &Origin<'_>, middleware: bool) {
        ExpressionTypes(self.diagnostics).visit_expr(expr);
        let mut names = Vec::new();
        expression_names(expr.to_token_stream(), false, &mut names);
        let mut reported = BTreeSet::new();
        for input in origin
            .locals
            .iter()
            .chain(origin.owners.iter().flat_map(|o| &o.inputs))
        {
            let name = input.input.name.to_string();
            if let Some((_, span)) = names.iter().find(|(n, _)| n == &name) {
                self.used.insert(input_id(input.input));
                if middleware
                    && input.input.optional.is_some()
                    && reported.insert(input_id(input.input))
                {
                    self.diagnostics.raise(
                        *span,
                        diag::optional_middleware(&name, &flat::tokens(&input.input.ty)),
                    );
                }
            }
        }
    }
    fn reference(
        &mut self,
        reference: &Reference,
        origin: &Origin<'_>,
        place: Place,
        short: bool,
        base_reference: bool,
    ) {
        let name = reference.name.to_string();
        let input = if let Some(owner) = &reference.owner {
            origin
                .owners
                .iter()
                .find(|o| *owner == flat::owner_name(o.node))
                .and_then(|o| o.inputs.iter().find(|i| i.input.name == name))
        } else {
            origin.locals.iter().find(|i| i.input.name == name)
        };
        if short && reference.owner.is_some() {
            self.diagnostics.raise(
                reference.span,
                diag::short_entry(
                    &reference
                        .owner
                        .as_ref()
                        .map_or_else(String::new, ToString::to_string),
                    &name,
                ),
            );
        }
        if let Some(input) = input {
            self.used.insert(input_id(input.input));
            if matches!(place, Place::Base | Place::Path) {
                if let Some(optional) = &input.input.optional {
                    let message = if base_reference {
                        let ident = &input.input.name;
                        let ty = &input.input.ty;
                        diag::optional_base(&Written::new(quote!(#ident: #ty)), optional.span())
                    } else {
                        diag::optional_url_hole(
                            &name,
                            &flat::tokens(&input.input.ty),
                            optional.span(),
                        )
                    };
                    self.diagnostics.raise(reference.span, message);
                }
            }
            return;
        }
        if origin.tainted {
            return;
        }
        if reference.owner.is_none() {
            if let Some(owner) = origin
                .owners
                .iter()
                .rev()
                .find(|o| o.inputs.iter().any(|i| i.input.name == name))
            {
                let owner_name = flat::owner_name(owner.node);
                if let Some(input) = owner.inputs.iter().find(|i| i.input.name == name) {
                    self.used.insert(input_id(input.input));
                }
                let message = if short {
                    diag::short_entry(&owner_name, &name)
                } else if base_reference {
                    diag::ancestor_base(&owner_name, &name).label(format!(
                        "`{name}` belongs to {}",
                        if owner_name == "client" {
                            "the client"
                        } else {
                            &owner_name
                        }
                    ))
                } else {
                    diag::ancestor_hole(
                        &format!("{owner_name}.{name}"),
                        &name,
                        if owner_name == "client" {
                            "the client"
                        } else {
                            &owner_name
                        },
                    )
                };
                self.diagnostics.raise(reference.span, message);
                return;
            }
        }
        self.used.extend(
            origin
                .locals
                .iter()
                .chain(origin.owners.iter().flat_map(|o| o.inputs.iter()))
                .map(|i| input_id(i.input)),
        );
        if matches!(origin.written.kind, NodeKind::Part)
            && reference.owner.as_ref().is_some_and(|o| o != "client")
        {
            let ident = &reference.name;
            self.diagnostics.raise(
                reference.span,
                diag::scope_input_in_part(
                    &flat::reference_text(reference),
                    &Written::new(quote!(#ident: String)),
                ).help("a part can use only its own inputs and the client's; move this placement to the scope or endpoint that owns the input"),
            );
        } else {
            self.diagnostics.raise(
                reference.span,
                diag::unknown_input(&flat::reference_text(reference), "String").help(
                    if reference.owner.is_some() {
                        "declare the missing input in the named owner's signature, or correct the owner and input name"
                    } else if matches!(origin.written.kind, NodeKind::Part) {
                        "a part sees only its own inputs and the client's; correct this reference, or move the placement to the scope or endpoint that owns the input and remove any unnecessary part or spread"
                    } else { "correct this input name, or declare the intended input in this signature and place every declared input" }
                ),
            );
        }
    }
    fn entries(&mut self, entries: &Entries, origin: &Origin<'_>, place: Place) {
        if entries.parse_failed {
            return;
        }
        for entry in &entries.entries {
            if place == Place::Header {
                let key = flat::key(entry, true);
                let span = match &entry.kind {
                    EntryKind::Short(r) => r.span,
                    EntryKind::Value { key, .. } | EntryKind::Remove(key) => key.span(),
                };
                if key == "host" {
                    self.diagnostics.raise(
                        span,
                        diag::host_header(&Written::new(
                            quote!(base "https://{region}.api.example.com";),
                        ))
                        .help("remove this host header and put its intended host in the base"),
                    );
                }
                if key.is_empty() || !key.bytes().all(http_token) {
                    let replacement = syn::LitStr::new(&header_suggestion(&key), span);
                    self.diagnostics.raise(
                        span,
                        diag::header_name(&Written::new(quote!(#replacement))).label("expected a nonempty ASCII HTTP token").help("choose the intended nonempty ASCII HTTP header name, retaining this entry's value"),
                    );
                }
                if let EntryKind::Short(reference) = &entry.kind {
                    let name = reference.name.to_string();
                    if name.contains('_') && reference.owner.is_none() {
                        let header = name.replace('_', "-");
                        let places = self.writer_places.get(&(origin.written as *const Node));
                        let inherited_at = |place: &*const Node| {
                            self.inherited_headers
                                .get(place)
                                .is_some_and(|keys| keys.contains(&header))
                        };
                        let inherited = inherited_at(&(origin.placed as *const Node));
                        let mixed = places.is_some_and(|places| {
                            places.iter().any(|place| inherited_at(place) != inherited)
                        });
                        let literal = syn::LitStr::new(&header, reference.span);
                        let value = &reference.name;
                        let edit = inherited.then(|| quote!(override));
                        let replacement = Written::new(quote!(#edit #literal = #value;));
                        let collision = places.is_some_and(|places| {
                            places.iter().any(|place| {
                                self.local_header_keys
                                    .get(place)
                                    .is_some_and(|keys| keys.contains(&header))
                            })
                        });
                        let m = diag::short_header(&name, &header);
                        self.diagnostics.raise(reference.span, if collision || mixed {
                            m.help("write the intended hyphenated header name explicitly, consolidating entries for that name and retaining its intended input")
                        } else {m.help(format!("write `{}`",replacement.text()))});
                    }
                }
            }
            match &entry.kind {
                EntryKind::Short(reference) => {
                    self.reference(reference, origin, place, true, false)
                }
                EntryKind::Value {
                    value: Value::Template(template),
                    ..
                } => self.template(template, origin, place),
                EntryKind::Value {
                    value: Value::Expression(expr),
                    ..
                } => self.expression(expr, origin, false),
                EntryKind::Remove(_) => {}
            }
        }
    }
    fn template(&mut self, template: &Template, origin: &Origin<'_>, place: Place) {
        let span = template.literal.span();
        let mut skeleton = String::new();
        let mut displayed = String::new();
        let mut stars = Vec::new();
        for piece in &template.pieces {
            match piece {
                TemplatePiece::Text { text, .. } => {
                    skeleton.push_str(text);
                    displayed.push_str(text);
                    if matches!(place, Place::Base | Place::Path)
                        && text.chars().any(|ch| {
                            ch == '\\'
                                || ch.is_ascii_control()
                                || (place == Place::Path && matches!(ch, ' ' | '#' | '%'))
                        })
                    {
                        self.diagnostics.raise(
                            span,
                            diag::path_character(
                                &Written::new(quote!(value: String)),
                                &Written::new(quote!(path "{value}";)),
                            ).help("move the forbidden fixed text into an input hole at this position; retain the other path text and holes"),
                        );
                    }
                }
                TemplatePiece::Hole {
                    reference, star, ..
                } => {
                    self.reference(reference, origin, place, false, false);
                    if *star {
                        stars.push((skeleton.len(), displayed.len(), reference));
                    }
                    skeleton.push('\u{fffc}');
                    displayed.push_str(&format!(
                        "{{{}{}}}",
                        if *star { "*" } else { "" },
                        flat::reference_text(reference)
                    ));
                }
            }
        }
        if place == Place::Path {
            if skeleton.is_empty() || skeleton.starts_with('/') || skeleton.contains("//") {
                let written_path = template.literal.value();
                let replacement = written_path
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("/");
                let replacement = syn::LitStr::new(
                    if replacement.is_empty() {
                        "target"
                    } else {
                        &replacement
                    },
                    span,
                );
                self.diagnostics.raise(
                    span,
                    diag::empty_path(&Written::new(quote!(path #replacement;))).help(
                        "remove leading or repeated slashes while keeping the path's segments and holes; omit an empty path statement",
                    ),
                );
            }
            for segment in skeleton.split('/') {
                if matches!(segment, "." | "..") {
                    self.diagnostics.raise(
                        span,
                        diag::dot_path(segment, &Written::new(quote!(path "target";))).help(
                            "write the intended path without dot segments, preserving the other segments and holes",
                        ).label(format!("`{segment}` is a dot segment")),
                    );
                }
            }
            if let Some((path, query)) = template.literal.value().split_once('?') {
                let path = syn::LitStr::new(path, span);
                let pairs = query.split('&').map(|pair| {
                    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                    let key = syn::LitStr::new(key, span);
                    let value = syn::LitStr::new(value, span);
                    quote!(#key = #value;)
                });
                let replacement = if query.is_empty() {
                    Written::new(quote!(path #path;))
                } else {
                    Written::new(quote!(path #path; query { #(#pairs)* }))
                };
                let has_query = |node: &Node| {
                    node.statements
                        .iter()
                        .any(|s| matches!(s.kind, StatementKind::Query(_)))
                };
                let duplicate_pairs = {
                    let keys = query
                        .split('&')
                        .map(|pair| pair.split_once('=').map_or(pair, |(k, _)| k))
                        .collect::<Vec<_>>();
                    let unique = keys.iter().collect::<BTreeSet<_>>();
                    unique.len() != keys.len()
                };
                let mut descendants = vec![origin.placed];
                let mut below = false;
                while let Some(n) = descendants.pop() {
                    below |= self.query_nodes.contains(&(n as *const Node));
                    descendants.extend(&n.children);
                }
                let existing = below
                    || has_query(origin.placed)
                    || origin.locals.iter().any(|input| has_query(input.written))
                    || origin.owners.iter().any(|owner| has_query(owner.node));
                let message = diag::path_query(&replacement);
                self.diagnostics.raise(span, if existing || duplicate_pairs || query.contains('%') || query.contains('+') || path.value().is_empty() {
                    message.help("merge every query pair with the existing query entries from this node, its parts and ancestors; resolve repeated keys deliberately and decode URL escapes; retain the path before the question mark")
                } else { message });
            }
        } else if place == Place::Base {
            if let Some((_, rest)) = skeleton.split_once("://") {
                if let Some((_, path)) = rest.split_once('/') {
                    for segment in path.split('/') {
                        if segment == "." || segment == ".." {
                            self.diagnostics.raise(
                                span,
                                diag::dot_path(
                                    segment,
                                    &Written::new(quote!(base "https://api.example.com/target";)),
                                ).help("write the intended base without dot segments, retaining its scheme, host and other path segments").label(format!("`{segment}` is a dot segment")),
                            );
                        }
                    }
                    if path.starts_with('/') || path.contains("//") {
                        self.diagnostics.raise(
                            span,
                            diag::empty_path(&Written::new(
                                quote!(base "https://api.example.com/target";),
                            )).help("remove doubled slashes in the base's path, retaining its scheme, host, segments and holes"),
                        );
                    }
                    if path.chars().any(|ch| matches!(ch, ' ' | '#' | '%')) {
                        self.diagnostics.raise(
                            span,
                            diag::path_character(
                                &Written::new(quote!(value: String)),
                                &Written::new(quote!(base "https://api.example.com/{value}";)),
                            ).help("move the forbidden fixed text into an input hole at this position; retain the rest of this base"),
                        );
                    }
                }
            }
        }
        for (offset, display_offset, reference) in stars {
            let name = flat::reference_text(reference);
            if place != Place::Path {
                let replacement = syn::LitStr::new(
                    &template
                        .literal
                        .value()
                        .replace(&format!("{{*{name}}}"), &format!("{{{name}}}")),
                    span,
                );
                self.diagnostics.raise(
                    span,
                    diag::star_outside_path(&name, &Written::new(quote!(#replacement))),
                );
            } else {
                let before = skeleton
                    .get(..offset)
                    .and_then(|s| s.rsplit('/').next())
                    .unwrap_or("");
                let after = skeleton
                    .get(offset + '\u{fffc}'.len_utf8()..)
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("");
                if !before.is_empty() || !after.is_empty() {
                    let before = displayed
                        .get(..display_offset)
                        .and_then(|s| s.rsplit('/').next())
                        .unwrap_or("");
                    let after = displayed
                        .get(display_offset + name.len() + 3..)
                        .and_then(|s| s.split('/').next())
                        .unwrap_or("");
                    let replacement = syn::LitStr::new(
                        &template
                            .literal
                            .value()
                            .replace(&format!("{{*{name}}}"), &format!("{{{name}}}")),
                        span,
                    );
                    self.diagnostics.raise(
                        span,
                        diag::star_segment(
                            &name,
                            &format!("{before}{after}"),
                            &Written::new(quote!(path #replacement;)),
                        ),
                    );
                }
            }
        }
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Place {
    Base,
    Path,
    Query,
    Header,
    Body,
}
fn http_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}
fn header_suggestion(key: &str) -> String {
    let result: String = key
        .chars()
        .map(|ch| {
            if ch.is_ascii() && http_token(ch as u8) {
                ch
            } else {
                '-'
            }
        })
        .collect();
    if result.is_empty() {
        "x-header".into()
    } else {
        result
    }
}
fn expression_names(tokens: TokenStream, in_macro: bool, names: &mut Vec<(String, Span)>) {
    let mut pending = vec![(tokens, in_macro)];
    while let Some((tokens, in_macro)) = pending.pop() {
        let mut macro_group = false;
        let mut identifier = false;
        for token in tokens {
            match token {
                TokenTree::Ident(ident) => {
                    names.push((ident.to_string(), ident.span()));
                    identifier = true;
                    macro_group = false;
                }
                TokenTree::Punct(punct) => {
                    macro_group = punct.as_char() == '!' && identifier;
                    identifier = false;
                }
                TokenTree::Group(group) => {
                    pending.push((group.stream(), in_macro || macro_group));
                    identifier = false;
                    macro_group = false;
                }
                TokenTree::Literal(lit) => {
                    if in_macro {
                        if let Ok(string) = syn::parse_str::<syn::LitStr>(&lit.to_string()) {
                            names.extend(
                                string
                                    .value()
                                    .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                                    .filter(|s| !s.is_empty())
                                    .map(|s| (s.into(), lit.span())),
                            );
                        }
                    }
                    identifier = false;
                    macro_group = false;
                }
            }
        }
    }
}

struct ExpressionTypes<'a>(&'a mut Diagnostics);
impl<'ast> Visit<'ast> for ExpressionTypes<'_> {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        expression_self(expr.to_token_stream(), self.0);
    }
    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        expression_self(expr.to_token_stream(), self.0);
    }
}
fn expression_self(tokens: TokenStream, diagnostics: &mut Diagnostics) {
    for token in tokens {
        match token {
            TokenTree::Ident(id) if id == "Self" => diagnostics.raise(id.span(), diag::self_type()),
            TokenTree::Group(g) => expression_self(g.stream(), diagnostics),
            _ => {}
        }
    }
}
fn self_types(ty: &syn::Type, diagnostics: &mut Diagnostics) {
    struct SelfTypes<'a>(&'a mut Diagnostics);
    impl<'ast> Visit<'ast> for SelfTypes<'_> {
        fn visit_ident(&mut self, ident: &'ast syn::Ident) {
            if ident == "Self" {
                self.0.raise(ident.span(), diag::self_type());
            }
        }
    }
    SelfTypes(diagnostics).visit_type(ty);
}
fn input_type(input: &Input, diagnostics: &mut Diagnostics) {
    struct Borrowed<'a> {
        input: &'a Input,
        diagnostics: &'a mut Diagnostics,
    }
    impl<'ast> Visit<'ast> for Borrowed<'_> {
        fn visit_type_reference(&mut self, reference: &'ast syn::TypeReference) {
            let mut owned = self.input.ty.clone();
            own_type(&mut owned);
            let name = &self.input.name;
            let optional = &self.input.optional;
            let declaration = Written::new(quote!(#name #optional: #owned));
            self.diagnostics.raise_spanned(
                reference,
                diag::borrowed_input(&self.input.name.to_string(), "").help(format!(
                    "write `{}`; callers may still pass references",
                    declaration.text()
                )),
            );
            visit::visit_type_reference(self, reference);
        }
    }
    self_types(&input.ty, diagnostics);
    Borrowed { input, diagnostics }.visit_type(&input.ty);
}

fn attributes(node: &Node, diagnostics: &mut Diagnostics) {
    fn doc(attr: &syn::Attribute) -> bool {
        attr.path().is_ident("doc") && matches!(attr.meta, syn::Meta::NameValue(_))
    }
    for attr in &node.attrs {
        let valid = doc(attr)
            || (attr.path().is_ident("cfg") && !matches!(node.kind, NodeKind::Part))
            || (matches!(node.kind, NodeKind::Scope | NodeKind::Endpoint(_))
                && (attr.path().is_ident("deprecated")
                    || (attr.path().is_ident("doc")
                        && matches!(&attr.meta, syn::Meta::List(list) if list.tokens.to_string() == "hidden"))));
        if !valid {
            let written = Written::new(attr.to_token_stream());
            let message = match node.kind {
                NodeKind::Client => diag::client_attribute(&written, "a scope or endpoint"),
                NodeKind::Part => diag::part_attribute(&written, "a scope or endpoint"),
                NodeKind::Scope => diag::scope_attribute(&written),
                NodeKind::Endpoint(_) => diag::endpoint_attribute(&written),
            };
            diagnostics.raise_spanned(attr, message);
        }
    }
    for input in inputs(node) {
        for attr in &input.input.attrs {
            if !doc(attr) {
                let name = input.input.name.to_string();
                let mut key = name.clone();
                if attr.path().is_ident("serde") {
                    let _ = attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("rename") {
                            key = meta.value()?.parse::<syn::LitStr>()?.value();
                        }
                        Ok(())
                    });
                }
                diagnostics.raise_spanned(attr, diag::input_attribute(&name, &key).help(
                    format!("remove this attribute; retain its intended wire name ({key:?}) in the existing placement, updating that entry rather than adding another"),
                ));
            }
        }
    }
    for statement in &node.statements {
        for attr in &statement.attrs {
            if !(matches!(statement.kind, StatementKind::Middleware(_)) && doc(attr)) {
                diagnostics.raise_spanned(
                    attr,
                    diag::statement_attribute(
                        &Written::new(attr.to_token_stream()),
                        &statement.keyword.to_string(),
                    ),
                );
            }
        }
    }
}
