//! Parse syn tokens into the written tree, with local recovery and messages 15–20.

use std::collections::{BTreeMap, VecDeque};

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::{
    braced,
    buffer::Cursor,
    ext::IdentExt,
    parenthesized,
    parse::{discouraged::Speculative, ParseStream, Parser},
    spanned::Spanned,
    visit::Visit,
    Attribute, Expr, Ident, Lit, LitStr, Token, Type, Visibility,
};

use crate::{
    ast::*,
    diag::{self, Diagnostics, Written},
};

/// A possibly recovered client and every error encountered while reading it.
pub(crate) struct Parsed {
    /// A successfully read root; invalid descendants may have been skipped.
    #[allow(dead_code)] // Used by name resolution and parser tests.
    pub(crate) client: Option<Client>,
    /// The shared diagnostic sink, passed on to name resolution and validation.
    pub(crate) diagnostics: Diagnostics,
}

/// Parse exactly one client from the tokens inside `api!`.
pub(crate) fn parse(tokens: TokenStream) -> Parsed {
    let mut diagnostics = Diagnostics::default();
    let mut pending_root = None;
    if let Err(error) = guard_tokens(tokens.clone(), 136, false) {
        diagnostics.push(error);
        return Parsed {
            client: None,
            diagnostics,
        };
    }
    let result = (|input: ParseStream<'_>| {
        let attrs = input.call(Attribute::parse_outer)?;
        let visibility = input.parse()?;
        let keyword = word(input, "client")?;
        pending_root = Some(declaration(
            input,
            attrs,
            visibility,
            keyword,
            0,
            &mut Boundary::DeclarationHeader,
        )?);
        // syn's Parser rejects any remaining tokens, including a second client.
        Ok(())
    })
    .parse2(tokens);
    if let Err(error) = result {
        diagnostics.push(error);
    }
    let client = pending_root
        .and_then(|root| build_tree(root, &mut diagnostics))
        .map(|node| Client { node });
    Parsed {
        client,
        diagnostics,
    }
}

fn word(input: ParseStream<'_>, expected: &str) -> syn::Result<Ident> {
    let ident = input.call(Ident::parse_any)?;
    if ident == expected {
        Ok(ident)
    } else {
        Err(syn::Error::new(
            ident.span(),
            format!("expected `{expected}`"),
        ))
    }
}

fn next_word(input: ParseStream<'_>) -> Option<String> {
    input
        .fork()
        .call(Ident::parse_any)
        .ok()
        .map(|id| id.to_string())
}

fn is_method(word: &str) -> bool {
    matches!(
        word,
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS" | "METHOD"
    )
}

fn is_statement(word: &str) -> bool {
    matches!(
        word,
        "base"
            | "path"
            | "query"
            | "headers"
            | "body"
            | "read"
            | "status"
            | "problem"
            | "middleware"
            | "paginate"
            | "idempotent"
            | "idle_timeout"
    )
}

struct PendingNode {
    node: Node,
    body: Option<TokenStream>,
    depth: usize,
}

/// Parse each node on the work queue, then assemble the owned AST bottom-up.
/// Neither DSL descent nor tree assembly consumes the Rust call stack.
fn build_tree(root: PendingNode, diagnostics: &mut Diagnostics) -> Option<Node> {
    let mut queue = VecDeque::from([(0usize, root)]);
    let mut records = Vec::new();
    let mut next_id = 1usize;
    while let Some((id, mut pending)) = queue.pop_front() {
        let mut descendants = Vec::new();
        if let Some(body) = pending.body.take() {
            let result = (|input: ParseStream<'_>| {
                block(
                    input,
                    &mut pending.node,
                    pending.depth,
                    diagnostics,
                    &mut descendants,
                );
                Ok(())
            })
            .parse2(body);
            if let Err(error) = result {
                pending.node.parse_failed = true;
                diagnostics.push(error);
            }
        }
        let mut links = Vec::new();
        for child in descendants {
            links.push((matches!(child.node.kind, NodeKind::Part), next_id));
            queue.push_back((next_id, child));
            next_id += 1;
        }
        records.push((id, pending.node, links));
    }
    let mut finished: Vec<Option<Node>> = std::iter::repeat_with(|| None).take(next_id).collect();
    for (id, mut node, links) in records.into_iter().rev() {
        for (part, child_id) in links {
            let Some(child) = finished.get_mut(child_id).and_then(Option::take) else {
                diagnostics.push(syn::Error::new(
                    node.span,
                    "unable to assemble a parsed child",
                ));
                return None;
            };
            if part {
                node.parts.push(child);
            } else {
                node.children.push(child);
            }
        }
        let Some(slot) = finished.get_mut(id) else {
            diagnostics.push(syn::Error::new(
                node.span,
                "unable to assemble a parsed node",
            ));
            return None;
        };
        *slot = Some(node);
    }
    finished.first_mut().and_then(Option::take)
}

fn declaration(
    input: ParseStream<'_>,
    attrs: Vec<Attribute>,
    visibility: Visibility,
    keyword: Ident,
    depth: usize,
    recovery: &mut Boundary,
) -> syn::Result<PendingNode> {
    *recovery = Boundary::DeclarationHeader;
    if depth > 64 {
        return Err(depth_error(keyword.span()));
    }
    let kind = match keyword.to_string().as_str() {
        "client" => NodeKind::Client,
        "scope" => NodeKind::Scope,
        "part" => NodeKind::Part,
        "METHOD" => {
            let custom = string(input)?;
            NodeKind::Endpoint(Method {
                keyword: keyword.clone(),
                custom: Some(custom),
            })
        }
        method if is_method(method) => NodeKind::Endpoint(Method {
            keyword: keyword.clone(),
            custom: None,
        }),
        _ => {
            return Err(syn::Error::new(
                keyword.span(),
                "expected a scope or an HTTP method",
            ))
        }
    };
    let name = input.call(Ident::parse_any)?;
    *recovery = Boundary::DeclarationBody;
    let required = matches!(kind, NodeKind::Part | NodeKind::Endpoint(_));
    let signature = if required || input.peek(syn::token::Paren) {
        Some(signature(input)?)
    } else {
        None
    };
    let output = if matches!(kind, NodeKind::Endpoint(_)) && input.peek(Token![->]) {
        Some(Output {
            arrow: input.parse()?,
            ty: input.parse()?,
        })
    } else {
        None
    };
    let mut node = Node {
        parse_failed: false,
        signatures_uncertain: false,
        span: keyword.span(),
        kind,
        attrs,
        visibility,
        name,
        signature,
        output,
        statements: Vec::new(),
        parts: Vec::new(),
        children: Vec::new(),
        braces: None,
    };
    if matches!(node.kind, NodeKind::Endpoint(_)) && input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        return Ok(PendingNode {
            node,
            body: None,
            depth,
        });
    }
    let content;
    node.braces = Some(braced!(content in input));
    Ok(PendingNode {
        node,
        body: Some(content.parse()?),
        depth,
    })
}

fn signature(input: ParseStream<'_>) -> syn::Result<Signature> {
    let content;
    let parens = parenthesized!(content in input);
    let mut entries = Vec::new();
    let mut spread_seen = false;
    while !content.is_empty() {
        if content.peek(Token![..]) {
            // Client/part spreads are retained for resolve's message 37.
            entries.push(Parameter::Spread(Spread {
                dots: content.parse()?,
                name: content.call(Ident::parse_any)?,
            }));
            spread_seen = true;
        } else {
            if spread_seen {
                return Err(content.error("inputs must precede part spreads"));
            }
            let attrs = content.call(Attribute::parse_outer)?;
            let name = content.call(Ident::parse_any)?;
            let optional = content.parse()?;
            content.parse::<Token![:]>()?;
            let ty = content.parse()?;
            entries.push(Parameter::Input(Box::new(Input {
                span: name.span(),
                attrs,
                name,
                optional,
                ty,
            })));
        }
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    Ok(Signature { parens, entries })
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Phase {
    Statements,
    Parts,
    Children,
}

struct BlockState {
    phase: Phase,
    seen: BTreeMap<String, Span>,
    recovery: Boundary,
}

fn block(
    input: ParseStream<'_>,
    node: &mut Node,
    depth: usize,
    diagnostics: &mut Diagnostics,
    descendants: &mut Vec<PendingNode>,
) {
    let mut state = BlockState {
        phase: Phase::Statements,
        seen: BTreeMap::new(),
        recovery: Boundary::Node,
    };
    while !input.is_empty() {
        let fork = input.fork();
        let start = fork.cursor();
        state.recovery = Boundary::Node;
        match block_item(&fork, node, depth, &mut state, diagnostics, descendants) {
            Ok(()) => input.advance_to(&fork),
            Err(error) => {
                node.parse_failed = true;
                node.signatures_uncertain |= matches!(
                    state.recovery,
                    Boundary::DeclarationHeader | Boundary::DeclarationBody
                );
                let boundary = if error_is_depth(&error) {
                    Boundary::DepthLine
                } else {
                    state.recovery
                };
                diagnostics.push(error);
                recover(&fork, boundary, fork.cursor() != start);
                resume_after_error(input, &fork);
            }
        }
    }
}

fn block_item(
    input: ParseStream<'_>,
    node: &mut Node,
    depth: usize,
    state: &mut BlockState,
    diagnostics: &mut Diagnostics,
    descendants: &mut Vec<PendingNode>,
) -> syn::Result<()> {
    let attrs = input.call(Attribute::parse_outer)?;
    let line_start = input.cursor();
    let mut keyword = input.call(Ident::parse_any)?;
    let edit = if matches!(keyword.to_string().as_str(), "override" | "remove")
        && next_word(input).is_some_and(|word| is_statement(&word))
    {
        let edit = keyword;
        keyword = input.call(Ident::parse_any)?;
        Some(edit)
    } else {
        None
    };
    let spelling = keyword.to_string();
    // A contextual word followed by parentheses is a missing-method endpoint
    // before it can be interpreted as a statement (status(), read(), ...).
    if !matches!(node.kind, NodeKind::Endpoint(_)) && missing_method(input, &spelling) {
        state.phase = Phase::Children;
        recover(input, Boundary::DeclarationHeader, true);
        node.parse_failed = true;
        node.signatures_uncertain = true;
        diagnostics.raise(
            keyword.span(),
            diag::missing_method(&written(line_start, input.cursor())),
        );
        return Ok(());
    }
    if is_statement(&spelling) {
        let parsed = statement(input, attrs, edit, keyword, diagnostics)?;
        let line = written(line_start, input.cursor());
        match state.phase {
            Phase::Statements => {}
            Phase::Parts => {
                node.parse_failed = true;
                diagnostics.raise(parsed.keyword.span(), diag::after_part(&line));
            }
            Phase::Children => {
                node.parse_failed = true;
                diagnostics.raise(parsed.keyword.span(), diag::after_child(&line));
            }
        }
        if spelling != "middleware" {
            if let Some(first) = state.seen.get(&spelling) {
                node.parse_failed = true;
                diagnostics.raise(
                    parsed.keyword.span(),
                    diag::duplicate_statement(&line, &spelling, *first).help(
                        if matches!(parsed.kind, StatementKind::Headers(_) | StatementKind::Query(_) | StatementKind::Status(_)) {
                            "consolidate these blocks into one, retaining every intended entry or arm and resolving repeated keys"
                        } else { "keep one declaration of this behaviour, choosing its intended value" }
                    ),
                );
            } else {
                state.seen.insert(spelling.clone(), parsed.keyword.span());
            }
        }
        if let Some(mut message) = placement(&spelling, &node.kind, &line) {
            if let StatementKind::Middleware(expr) = &parsed.kind {
                let loses_input = node.signature.as_ref().is_some_and(|signature| {
                    signature.entries.iter().any(|p| match p {
                        Parameter::Spread(_) => true,
                        Parameter::Input(i) => {
                            crate::name::expression_uses(expr, &i.name.to_string())
                        }
                    })
                });
                if loses_input || depth > 1 {
                    message=message.help("construct this middleware in the client block using required client inputs; qualify or move the values it needs and consolidate existing middleware declarations");
                }
            }
            node.parse_failed = true;
            diagnostics.raise(parsed.keyword.span(), message);
        }
        node.parse_failed |= match &parsed.kind {
            StatementKind::Query(entries) | StatementKind::Headers(entries) => entries.parse_failed,
            StatementKind::Body(Body::Entries { entries, .. }) => entries.parse_failed,
            StatementKind::Status(arms) => arms.parse_failed,
            _ => false,
        };
        node.statements.push(parsed);
        return Ok(());
    }
    if spelling == "part" {
        let part = declaration(
            input,
            attrs,
            Visibility::Inherited,
            keyword.clone(),
            depth,
            &mut state.recovery,
        )?;
        let line = written(line_start, input.cursor());
        if state.phase == Phase::Children {
            node.parse_failed = true;
            diagnostics.raise(keyword.span(), diag::part_after_child(&line));
        }
        if !matches!(node.kind, NodeKind::Client | NodeKind::Part) {
            node.parse_failed = true;
            diagnostics.raise(keyword.span(), diag::part_placement(&line).help("declare parts on the client; retain only permitted part statements and place tree behaviour on the appropriate scope or endpoint"));
        }
        state.phase = state.phase.max(Phase::Parts);
        descendants.push(part);
        return Ok(());
    }
    if spelling == "scope" || is_method(&spelling) {
        state.phase = Phase::Children;
        if matches!(node.kind, NodeKind::Endpoint(_)) {
            return Err(syn::Error::new(
                keyword.span(),
                "expected a statement; this block cannot hold children",
            ));
        }
        let next_depth = depth + usize::from(spelling == "scope");
        let mut child = declaration(
            input,
            attrs,
            Visibility::Inherited,
            keyword,
            next_depth,
            &mut state.recovery,
        )?;
        if let NodeKind::Endpoint(method) = &child.node.kind {
            if let Some(custom) = &method.custom {
                if !http_token(&custom.value()) {
                    let line = written(line_start, input.cursor());
                    let corrected: String = custom
                        .value()
                        .chars()
                        .filter(|ch| ch.is_ascii() && http_token(&ch.to_string()))
                        .collect();
                    let corrected = if corrected.is_empty() {
                        "X"
                    } else {
                        &corrected
                    };
                    child.node.parse_failed = true;
                    diagnostics.raise(
                        custom.span(),
                        diag::invalid_method(&line.method_replacement(corrected)),
                    );
                }
            }
        }
        descendants.push(child);
        return Ok(());
    }
    let expected = "expected a statement, part, scope or HTTP method";
    let help = match spelling.as_str() {
        "timeout" => Some("configure the timeout with `with_timeout` on the client or the call"),
        "retry" | "tries" => Some("configure retries with `with_retry` on the client or the call"),
        "base_url" => Some("set the request URL with the `base` statement"),
        "auth" => Some("place credentials in the appropriate `headers` entries"),
        _ => None,
    };
    let message = help.map_or_else(
        || expected.to_owned(),
        |help| format!("{expected}\nhelp: {help}"),
    );
    Err(syn::Error::new(keyword.span(), message))
}

fn placement(word: &str, kind: &NodeKind, line: &Written) -> Option<diag::Message> {
    if matches!(word, "body" | "paginate" | "idempotent")
        && matches!(kind, NodeKind::Client | NodeKind::Scope)
    {
        let node = if matches!(kind, NodeKind::Client) {
            "client"
        } else {
            "scope"
        };
        return Some(diag::requires_endpoint(line, word, node).help("place this behaviour on an endpoint, consolidating it with any statement supplied there or by a part"));
    }
    // Part contents belong to message 37 in resolve, including problem and
    // middleware; the parser keeps them without applying the tree placement rule.
    if matches!(kind, NodeKind::Scope | NodeKind::Endpoint(_)) {
        match word {
            "problem" => return Some(diag::problem_placement(line).help("declare the intended problem type on the client; consolidate it with an existing problem declaration")),
            "middleware" => return Some(diag::middleware_placement(line)),
            _ => {}
        }
    }
    None
}

fn missing_method(input: ParseStream<'_>, word: &str) -> bool {
    if !input.peek(syn::token::Paren) || is_method(word) {
        return false;
    }
    // A complete unit-valued statement (notably `problem () read R;`)
    // takes precedence over recovery for a contextual endpoint name.
    if word == "problem" {
        let after_type = input.fork();
        let complete = after_type.parse::<Type>().is_ok()
            && (next_word(&after_type).as_deref() == Some("read") || after_type.peek(Token![;]));
        if let Ok(keyword) = syn::parse_str::<Ident>(word) {
            if complete
                && statement(
                    &input.fork(),
                    Vec::new(),
                    None,
                    keyword,
                    &mut Diagnostics::default(),
                )
                .is_ok()
            {
                return false;
            }
        }
    }
    // Parenthesized Rust on a statement's right side remains ordinary Rust.
    // An arrow or a leaf after the parentheses unambiguously identifies the
    // endpoint form, even when its name is a contextual statement word.
    let fork = input.fork();
    let empty = matches!(fork.parse::<TokenTree>(), Ok(TokenTree::Group(group)) if group.stream().is_empty());
    !matches!(
        word,
        "body" | "read" | "middleware" | "paginate" | "idempotent" | "idle_timeout" | "problem"
    ) || empty
        || signature(&input.fork()).is_ok()
        || fork.peek(Token![->])
        || fork.peek(syn::token::Brace)
}

fn written(mut start: Cursor<'_>, end: Cursor<'_>) -> Written {
    let mut tokens = TokenStream::new();
    while start != end {
        let Some((token, next)) = start.token_tree() else {
            break;
        };
        tokens.extend([token]);
        start = next;
    }
    Written::new(tokens)
}

fn statement(
    input: ParseStream<'_>,
    attrs: Vec<Attribute>,
    edit: Option<Ident>,
    keyword: Ident,
    diagnostics: &mut Diagnostics,
) -> syn::Result<Statement> {
    let spelling = keyword.to_string();
    let kind = match spelling.as_str() {
        "query" => StatementKind::Query(entries(input, diagnostics)?),
        "headers" => StatementKind::Headers(entries(input, diagnostics)?),
        "status" => StatementKind::Status(arms(input, diagnostics)?),
        "body" => {
            let fork = input.fork();
            if let Ok(function) = fork.parse::<syn::ExprPath>() {
                if fork.peek(syn::token::Brace) {
                    input.advance_to(&fork);
                    let entries = entries(input, diagnostics)?;
                    return Ok(Statement {
                        keyword,
                        edit,
                        attrs,
                        kind: StatementKind::Body(Body::Entries { function, entries }),
                    });
                }
            }
            let expr = expression(input)?;
            input.parse::<Token![;]>()?;
            StatementKind::Body(Body::Expression(expr))
        }
        "base" => {
            let base = if input.peek(LitStr) {
                Base::Template(template(string(input)?)?)
            } else {
                Base::Reference(reference(input)?)
            };
            input.parse::<Token![;]>()?;
            StatementKind::Base(base)
        }
        "path" => {
            let template = template(string(input)?)?;
            input.parse::<Token![;]>()?;
            StatementKind::Path(template)
        }
        "problem" => {
            let ty: Type = input.parse()?;
            let read = if next_word(input).as_deref() == Some("read") {
                Some((word(input, "read")?, expression(input)?))
            } else {
                None
            };
            input.parse::<Token![;]>()?;
            StatementKind::Problem {
                ty: Box::new(ty),
                read,
            }
        }
        "idempotent" => {
            let expr = if input.peek(Token![;]) {
                None
            } else {
                Some(expression(input)?)
            };
            input.parse::<Token![;]>()?;
            StatementKind::Idempotent(expr)
        }
        _ => {
            let expr = expression(input)?;
            input.parse::<Token![;]>()?;
            match spelling.as_str() {
                "read" => StatementKind::Read(expr),
                "middleware" => StatementKind::Middleware(expr),
                "paginate" => StatementKind::Paginate(expr),
                "idle_timeout" => StatementKind::IdleTimeout(expr),
                _ => return Err(syn::Error::new(keyword.span(), "expected a statement")),
            }
        }
    };
    Ok(Statement {
        keyword,
        edit,
        attrs,
        kind,
    })
}

fn reference(input: ParseStream<'_>) -> syn::Result<Reference> {
    let first = input.call(Ident::parse_any)?;
    let span = first.span();
    let (owner, name) = if input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        (Some(first), input.call(Ident::parse_any)?)
    } else {
        (None, first)
    };
    Ok(Reference { span, owner, name })
}

fn entries(input: ParseStream<'_>, diagnostics: &mut Diagnostics) -> syn::Result<Entries> {
    let content;
    let braces = braced!(content in input);
    let mut entries = Vec::new();
    let mut parse_failed = false;
    while !content.is_empty() {
        let fork = content.fork();
        let start = fork.cursor();
        match entry(&fork) {
            Ok(entry) => {
                entries.push(entry);
                content.advance_to(&fork);
            }
            Err(error) => {
                parse_failed = true;
                diagnostics.push(error);
                recover(&fork, Boundary::Entry, fork.cursor() != start);
                resume_after_error(&content, &fork);
            }
        }
    }
    Ok(Entries {
        parse_failed,
        braces,
        entries,
    })
}

fn entry(input: ParseStream<'_>) -> syn::Result<Entry> {
    let span = input.span();
    let edit = if matches!(next_word(input).as_deref(), Some("override" | "remove")) {
        let fork = input.fork();
        let keyword = fork.call(Ident::parse_any)?;
        if fork.peek(LitStr) {
            // A body's edits are retained for resolve's messages 22 and 23.
            input.advance_to(&fork);
            Some(keyword)
        } else {
            None
        }
    } else {
        None
    };
    let kind = if edit.as_ref().is_some_and(|edit| edit == "remove") {
        EntryKind::Remove(string(input)?)
    } else if input.peek(LitStr) {
        let key = string(input)?;
        input.parse::<Token![=]>()?;
        let expr: Expr = expression(input)?;
        let value = match expr {
            Expr::Lit(syn::ExprLit {
                attrs,
                lit: Lit::Str(literal),
                ..
            }) if attrs.is_empty() => Value::Template(template(literal)?),
            expr => Value::Expression(Box::new(expr)),
        };
        EntryKind::Value { key, value }
    } else {
        EntryKind::Short(reference(input)?)
    };
    input.parse::<Token![;]>()?;
    Ok(Entry { span, edit, kind })
}

fn arms(input: ParseStream<'_>, diagnostics: &mut Diagnostics) -> syn::Result<Arms> {
    let content;
    let braces = braced!(content in input);
    guard_tokens(content.fork().parse()?, 64, true)?;
    let mut arms = Vec::new();
    let mut parse_failed = false;
    while !content.is_empty() {
        let fork = content.fork();
        let start = fork.cursor();
        let result = (|| {
            let arm = arm(&fork)?;
            if !fork.is_empty() {
                fork.parse::<Token![,]>()?;
            }
            Ok(arm)
        })();
        match result {
            Ok(arm) => {
                arms.push(arm);
                content.advance_to(&fork);
            }
            Err(error) => {
                parse_failed = true;
                diagnostics.push(error);
                recover(&fork, Boundary::Arm, fork.cursor() != start);
                resume_after_error(&content, &fork);
            }
        }
    }
    Ok(Arms {
        parse_failed,
        braces,
        arms,
    })
}

fn arm(input: ParseStream<'_>) -> syn::Result<Arm> {
    let span = input.span();
    let edit = if matches!(next_word(input).as_deref(), Some("override" | "remove")) {
        Some(input.call(Ident::parse_any)?)
    } else {
        None
    };
    let key = input.parse()?;
    if edit.as_ref().is_some_and(|edit| edit == "remove") {
        return Ok(Arm {
            span,
            edit,
            key,
            fields: None,
            expression: None,
        });
    }
    let fields = if input.peek(syn::token::Brace) {
        let content;
        let braces = braced!(content in input);
        let mut fields = Vec::new();
        // The grammar requires at least one field if braces are present.
        loop {
            let name = content.call(Ident::parse_any)?;
            let binding = if content.peek(Token![:]) {
                content.parse::<Token![:]>()?;
                Some(content.call(Ident::parse_any)?)
            } else {
                None
            };
            fields.push(Field { name, binding });
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
            if content.is_empty() {
                break;
            }
        }
        Some(Fields { braces, fields })
    } else {
        None
    };
    input.parse::<Token![=>]>()?;
    Ok(Arm {
        span,
        edit,
        key,
        fields,
        expression: Some(arm_expression(input)?),
    })
}

/// Decode braces after Rust string decoding; all hole spans are the literal.
fn template(literal: LitStr) -> syn::Result<Template> {
    no_string_suffix(&literal)?;
    let span = literal.span();
    let decoded = literal.value();
    let mut chars = decoded.chars().peekable();
    let mut text = String::new();
    let mut pieces = Vec::new();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '{' => {
                if !text.is_empty() {
                    pieces.push(TemplatePiece::Text {
                        text: std::mem::take(&mut text),
                        span,
                    });
                }
                let star = chars.peek() == Some(&'*');
                if star {
                    chars.next();
                }
                let mut hole = String::new();
                let mut closed = false;
                for ch in chars.by_ref() {
                    if ch == '}' {
                        closed = true;
                        break;
                    }
                    if ch == '{' || ch.is_whitespace() {
                        return Err(syn::Error::new(span, "expected a template hole with a name or owner.name, without spaces or comments"));
                    }
                    hole.push(ch);
                }
                if !closed {
                    return Err(syn::Error::new(span, "unclosed template hole"));
                }
                let mut reference = (|input: ParseStream<'_>| reference(input)).parse_str(&hole)
                    .map_err(|_| syn::Error::new(span, "expected a template hole with a name or owner.name, without spaces or comments"))?;
                // Rust's lexer discards comments. The spelling check prevents that
                // from admitting comments inside a decoded hole.
                let spelling = match &reference.owner {
                    Some(owner) => format!("{owner}.{}", reference.name),
                    None => reference.name.to_string(),
                };
                if spelling != hole {
                    return Err(syn::Error::new(span, "expected a template hole with a name or owner.name, without spaces or comments"));
                }
                reference.span = span;
                reference.name.set_span(span);
                if let Some(owner) = &mut reference.owner {
                    owner.set_span(span);
                }
                pieces.push(TemplatePiece::Hole {
                    reference,
                    star,
                    span,
                });
            }
            '}' => {
                return Err(syn::Error::new(
                    span,
                    "unmatched `}` in template; write `}}` for a literal brace",
                ))
            }
            ch => text.push(ch),
        }
    }
    if !text.is_empty() {
        pieces.push(TemplatePiece::Text { text, span });
    }
    Ok(Template { literal, pieces })
}

fn http_token(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

#[derive(Clone, Copy)]
enum Boundary {
    Node,
    DeclarationHeader,
    DeclarationBody,
    DepthLine,
    Entry,
    Arm,
}

/// Groups are atomic token trees: skipping a failed node cannot consume a
/// sibling's closing brace. Every path consumes a token or returns.
fn recover(input: ParseStream<'_>, boundary: Boundary, mut progressed: bool) {
    while !input.is_empty() {
        if progressed
            && matches!(boundary, Boundary::Node)
            && (input.peek(Token![#])
                || next_word(input).is_some_and(|word| {
                    is_statement(&word)
                        || is_method(&word)
                        || matches!(word.as_str(), "scope" | "part")
                }))
        {
            return;
        }
        if progressed
            && matches!(boundary, Boundary::DeclarationBody)
            && next_word(input)
                .is_some_and(|word| is_method(&word) || matches!(word.as_str(), "scope" | "part"))
        {
            return;
        }
        match input.parse::<TokenTree>() {
            Ok(TokenTree::Group(group))
                if group.delimiter() == proc_macro2::Delimiter::Brace
                    && matches!(
                        boundary,
                        Boundary::Node | Boundary::DeclarationHeader | Boundary::DeclarationBody
                    ) =>
            {
                // The optional semicolon belongs to this same damaged line.
                if input.peek(Token![;]) {
                    let _ = input.parse::<Token![;]>();
                }
                return;
            }
            Ok(TokenTree::Punct(punct))
                if (punct.as_char() == ';' && !matches!(boundary, Boundary::Arm))
                    || (punct.as_char() == ',' && matches!(boundary, Boundary::Arm)) =>
            {
                return
            }
            Ok(_) => progressed = true,
            Err(_) => return,
        }
    }
}

/// Commit the cursor after recovery, without importing syn's pending error for
/// an already diagnosed, abandoned inner delimiter. This fork shares the input's
/// token buffer. No consumed prefix is scanned or parsed a second time.
fn resume_after_error(input: ParseStream<'_>, failed: ParseStream<'_>) {
    // A fresh fork shares the already advanced cursor, but not syn's pending
    // unexpected-token flag from an abandoned inner delimiter. Committing it
    // is O(1): no consumed token is visited, parsed or recovered again.
    input.advance_to(&failed.fork());
}

fn string(input: ParseStream<'_>) -> syn::Result<LitStr> {
    let literal: LitStr = input.parse()?;
    no_string_suffix(&literal)?;
    Ok(literal)
}

fn no_string_suffix(literal: &LitStr) -> syn::Result<()> {
    if literal.suffix().is_empty() {
        Ok(())
    } else {
        Err(syn::Error::new(
            literal.span(),
            "a language string cannot have a suffix",
        ))
    }
}

fn expression(input: ParseStream<'_>) -> syn::Result<Expr> {
    let mut tokens = TokenStream::new();
    let cursor = input.fork();
    while !cursor.is_empty() && !cursor.peek(Token![;]) {
        tokens.extend([cursor.parse::<TokenTree>()?]);
    }
    guard_tokens(tokens, 64, true)?;
    arm_expression(input)
}

fn arm_expression(input: ParseStream<'_>) -> syn::Result<Expr> {
    let expr: Expr = input.parse()?;
    let mut depth = ExpressionDepth {
        depth: 0,
        operators: 0,
        error: None,
    };
    depth.visit_expr(&expr);
    match depth.error {
        Some(error) => Err(error),
        None => Ok(expr),
    }
}

struct ExpressionDepth {
    depth: usize,
    operators: usize,
    error: Option<syn::Error>,
}
impl<'ast> Visit<'ast> for ExpressionDepth {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if self.error.is_some() {
            return;
        }
        let operation = matches!(
            expr,
            Expr::Binary(_)
                | Expr::MethodCall(_)
                | Expr::Call(_)
                | Expr::Field(_)
                | Expr::Index(_)
                | Expr::Unary(_)
                | Expr::Reference(_)
                | Expr::Try(_)
                | Expr::Await(_)
                | Expr::Cast(_)
                | Expr::Assign(_)
                | Expr::Range(_)
        );
        self.operators += usize::from(operation);
        if self.operators > 256 {
            self.error = Some(length_error(expr.span()));
            return;
        }
        // The left spine of an infix or postfix chain is flat Rust syntax.
        // Arguments, delimiters, closures and prefix operators still nest.
        match expr {
            Expr::MethodCall(call) => {
                self.visit_expr(&call.receiver);
                self.depth += 1;
                for arg in &call.args {
                    self.visit_expr(arg);
                }
                self.depth -= 1;
            }
            Expr::Call(call) => {
                self.visit_expr(&call.func);
                self.depth += 1;
                for arg in &call.args {
                    self.visit_expr(arg);
                }
                self.depth -= 1;
            }
            _ => {
                let level = usize::from(!matches!(
                    expr,
                    Expr::Lit(_)
                        | Expr::Path(_)
                        | Expr::Macro(_)
                        | Expr::Binary(_)
                        | Expr::Field(_)
                        | Expr::Try(_)
                        | Expr::Await(_)
                        | Expr::Cast(_)
                ));
                self.depth += level;
                if self.depth > 64 {
                    self.error = Some(depth_error(expr.span()));
                } else {
                    syn::visit::visit_expr(self, expr);
                }
                self.depth -= level;
            }
        }
    }
    // Type recursion has its own preliminary token bound; this limit concerns
    // expressions, including those nested in ordinary Rust blocks.
    fn visit_type(&mut self, _ty: &'ast Type) {}
}

fn error_is_depth(error: &syn::Error) -> bool {
    matches!(
        error.to_string().as_str(),
        "nesting exceeds the parser limit of 64 levels"
            | "expression is too long: exceeds the parser limit of 256 operators"
    )
}

fn length_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "expression is too long: exceeds the parser limit of 256 operators",
    )
}

fn depth_error(span: Span) -> syn::Error {
    syn::Error::new(span, "nesting exceeds the parser limit of 64 levels")
}

/// Inspect token trees iteratively, before syn can recurse into them. The first
/// pass also protects syn's token-buffer construction; local Rust passes reset
/// their depth at the expression boundary. Flat recursive operator/postfix
/// chains are bounded too, including unary expressions without any delimiters.
fn guard_tokens(tokens: TokenStream, group_limit: usize, expression: bool) -> syn::Result<()> {
    let mut pending = vec![(tokens, 0usize, 0usize, false)];
    while let Some((tokens, groups, inherited, opaque)) = pending.pop() {
        let mut operators = 0usize;
        let mut member = false;
        let mut after_else = false;
        let mut conditional_chain = 0usize;
        let mut pattern_depth = 0usize;
        let mut angles = 0usize;
        let mut previous = None;
        let mut previous_name = false;
        let mut macro_bang = false;
        let mut prefix = 0usize;
        let mut closure_parameters = false;
        let mut closures = 0usize;
        let mut operand = true;
        let mut binary_pipe = false;
        for token in tokens {
            let span = token.span();
            match token {
                TokenTree::Group(group) => {
                    prefix = 0;
                    operand = false;
                    binary_pipe = false;
                    let depth = groups + 1;
                    let budget = inherited + 1;
                    if depth > group_limit {
                        return Err(depth_error(group.span()));
                    }
                    // Syn stores macro arguments as tokens, not parsed Rust.
                    // The initial pass bounds buffer nesting inside them; a
                    // local expression pass need not examine their contents.
                    if !(expression && macro_bang) {
                        pending.push((
                            group.stream(),
                            depth,
                            if expression { budget } else { 0 },
                            opaque || macro_bang,
                        ));
                    }
                    if group.delimiter() == proc_macro2::Delimiter::Brace {
                        if !expression {
                            operators = 0;
                        }
                    } else if !macro_bang && !member {
                        operators += 1;
                    }
                    member = false;
                    previous = None;
                    previous_name = false;
                    macro_bang = false;
                }
                TokenTree::Punct(punct) => {
                    let ch = punct.as_char();
                    if ch == '@' && closure_parameters {
                        pattern_depth += 1;
                        if pattern_depth > 64 {
                            return Err(depth_error(span));
                        }
                    }
                    if !opaque && matches!(ch, '!' | '-' | '*' | '&') {
                        prefix += 1;
                        // One binary && may precede 63 nested borrows. Permit
                        // that extra glyph, then let the AST count decide.
                        if prefix > 65 {
                            return Err(depth_error(span));
                        }
                    } else {
                        prefix = 0;
                    }
                    if ch == '|' && !opaque {
                        if binary_pipe && previous == Some('|') {
                            binary_pipe = false;
                        } else if closure_parameters {
                            closure_parameters = false;
                            operand = true;
                        } else if operand {
                            closures += 1;
                            if closures > 64 {
                                return Err(depth_error(span));
                            }
                            closure_parameters = true;
                        } else {
                            binary_pipe = true;
                            operand = true;
                        }
                    } else {
                        binary_pipe = false;
                    }
                    macro_bang = ch == '!' && previous_name;
                    if !opaque {
                        if ch == ';'
                            || (ch == ',' && angles == 0 && !closure_parameters)
                            || (ch == '>' && previous == Some('='))
                        {
                            operators = 0;
                            closures = 0;
                            operand = true;
                        } else if ch == '>' && angles > 0 {
                            angles -= 1;
                        } else if !macro_bang
                            && matches!(
                                ch,
                                '!' | '-'
                                    | '+'
                                    | '*'
                                    | '/'
                                    | '%'
                                    | '^'
                                    | '&'
                                    | '|'
                                    | '='
                                    | '.'
                                    | '?'
                                    | '@'
                                    | '<'
                                    | '>'
                            )
                        {
                            let joint = matches!(
                                (previous, ch),
                                (Some('|'), '|')
                                    | (Some('&'), '&')
                                    | (Some('<'), '<')
                                    | (Some('>'), '>')
                                    | (
                                        Some(
                                            '+' | '-'
                                                | '*'
                                                | '/'
                                                | '%'
                                                | '^'
                                                | '&'
                                                | '|'
                                                | '='
                                                | '!'
                                                | '<'
                                                | '>'
                                        ),
                                        '='
                                    )
                            );
                            if !joint && ch != '<' {
                                operators += 1;
                            }
                            if ch == '<' {
                                angles += 1;
                                if angles > 256 {
                                    return Err(depth_error(span));
                                }
                            }
                        }
                    }
                    previous = Some(ch);
                    previous_name = false;
                }
                TokenTree::Ident(ident) => {
                    let control = matches!(
                        ident.to_string().as_str(),
                        "if" | "else"
                            | "match"
                            | "loop"
                            | "while"
                            | "for"
                            | "return"
                            | "break"
                            | "yield"
                            | "async"
                            | "move"
                            | "unsafe"
                            | "as"
                            | "const"
                            | "try"
                    );
                    if control && !opaque {
                        if ident == "if" {
                            conditional_chain = if after_else { conditional_chain + 1 } else { 1 };
                            if conditional_chain > 64 {
                                return Err(depth_error(span));
                            }
                        }
                        after_else = ident == "else";
                        operators += 1;
                    }
                    member = previous == Some('.');
                    previous = None;
                    previous_name = !control;
                    operand = control;
                    prefix = 0;
                    macro_bang = false;
                }
                TokenTree::Literal(_) => {
                    prefix = 0;
                    operand = false;
                    previous = None;
                    previous_name = false;
                    macro_bang = false;
                }
            }
            if !opaque && operators > if expression { 256 } else { 512 } {
                return Err(length_error(span));
            }
        }
    }
    Ok(())
}
