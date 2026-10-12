//! A span-preserving parser for the Concord v2.1 `api!` declaration grammar.
//!
//! This mirrors the v2.1 grammar in `docs/spec.md` and the v2.1 macro parser.
//! Every node records absolute byte ranges into the containing Rust file so
//! the converter can edit exact tokens. Rust types and constant paths are
//! parsed with `syn` and kept as opaque source ranges; no protocol meaning is
//! assigned to their spelling.

use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::ext::IdentExt;
use syn::parse::{ParseStream, Parser};
use syn::{
    Attribute, Expr, ExprLit, Ident, Lit, LitBool, LitChar, LitFloat, LitInt, LitStr, Meta, Token,
    Type, braced, bracketed, parenthesized,
};

use super::edits::Rng;

mod kw {
    syn::custom_keyword!(allow);
    syn::custom_keyword!(append);
    syn::custom_keyword!(auth);
    syn::custom_keyword!(base);
    syn::custom_keyword!(body);
    syn::custom_keyword!(client);
    syn::custom_keyword!(concat);
    syn::custom_keyword!(cost);
    syn::custom_keyword!(credential);
    syn::custom_keyword!(credentials);
    syn::custom_keyword!(each);
    syn::custom_keyword!(effect);
    syn::custom_keyword!(endpoint);
    syn::custom_keyword!(explicit);
    syn::custom_keyword!(expose);
    syn::custom_keyword!(headers);
    syn::custom_keyword!(id);
    syn::custom_keyword!(key);
    syn::custom_keyword!(off);
    syn::custom_keyword!(origin);
    syn::custom_keyword!(paginate);
    syn::custom_keyword!(path);
    syn::custom_keyword!(per);
    syn::custom_keyword!(policy);
    syn::custom_keyword!(query);
    syn::custom_keyword!(rate_limit);
    syn::custom_keyword!(rate_limits);
    syn::custom_keyword!(remove);
    syn::custom_keyword!(replace);
    syn::custom_keyword!(request);
    syn::custom_keyword!(response);
    syn::custom_keyword!(scope);
    syn::custom_keyword!(status);
    syn::custom_keyword!(subdomain);
    syn::custom_keyword!(timeout);
    syn::custom_keyword!(values);
    syn::custom_keyword!(via);
    syn::custom_keyword!(when_present);
    syn::custom_keyword!(METHOD);
}

/// A declared identifier with its exact source spelling and logical name.
#[derive(Clone, Debug)]
pub(crate) struct Name {
    /// Source spelling, including any raw `r#` marker.
    pub(crate) text: String,
    /// Logical name without the raw marker.
    pub(crate) logical: String,
    pub(crate) range: Rng,
}

#[derive(Debug)]
pub(crate) struct Api {
    pub(crate) client: Client,
}

/// A user-written Rust path inside a declaration: a type (or a segment of a
/// type's generic arguments) or a stable-value constant path. v3 emits these
/// inside generated support modules, where v2.1 resolved them in a separate
/// module that glob-imported the facade prelude.
#[derive(Clone, Debug)]
pub(crate) struct UserPath {
    /// A stable-value constant path (value namespace) rather than a type.
    pub(crate) constant: bool,
    pub(crate) leading_colon: bool,
    /// Logical segment identifiers, without generic arguments.
    pub(crate) segments: Vec<String>,
    /// Range of the first segment identifier.
    pub(crate) range: Rng,
}

impl UserPath {
    pub(crate) fn rendered(&self) -> String {
        self.segments.join("::")
    }
}

#[derive(Debug)]
pub(crate) struct Client {
    pub(crate) name: Name,
    pub(crate) values: Vec<ValueDecl>,
    pub(crate) credentials: Vec<CredentialDecl>,
    /// `secret NAME;` declarations (each generates a builder method `NAME`).
    pub(crate) secrets: Vec<Name>,
    pub(crate) policies: Vec<RatePolicy>,
    pub(crate) site: Site,
    pub(crate) children: Vec<Child>,
    /// User paths written directly in the client (values, defaults, adapters).
    pub(crate) paths: Vec<UserPath>,
}

#[derive(Debug)]
pub(crate) enum Child {
    Scope(Box<Scope>),
    Endpoint(Box<Endpoint>),
}

#[derive(Debug)]
pub(crate) struct Scope {
    pub(crate) name: Name,
    pub(crate) params: Vec<ValueDecl>,
    pub(crate) site: Site,
    pub(crate) children: Vec<Child>,
    /// User paths written directly in the scope.
    pub(crate) paths: Vec<UserPath>,
}

#[derive(Debug)]
pub(crate) struct Endpoint {
    pub(crate) method: String,
    pub(crate) method_range: Rng,
    pub(crate) name: Name,
    pub(crate) params: Vec<ValueDecl>,
    pub(crate) body: Option<BodyDecl>,
    pub(crate) site: Site,
    pub(crate) pagination: Option<Pagination>,
    pub(crate) id: Option<(String, Rng)>,
    /// The clause block braces, when the endpoint has a clause block.
    pub(crate) clause_block: Option<(Rng, Rng)>,
    /// Ranges of clauses in source order, for insertion points.
    pub(crate) clause_ranges: Vec<Rng>,
    /// The terminating `;` of a clause-free endpoint.
    pub(crate) semicolon: Option<Rng>,
    /// User paths written in the endpoint (parameters, body, result, adapters).
    pub(crate) paths: Vec<UserPath>,
}

#[derive(Clone, Debug)]
pub(crate) struct ValueDecl {
    pub(crate) name: Name,
    pub(crate) optional: bool,
    pub(crate) sensitive: bool,
    /// Offset of the identifier, after any attributes.
    pub(crate) ident_start: usize,
    /// End of the payload type, before any initializer.
    pub(crate) type_end: usize,
    /// Declaration order within its owner (body excluded).
    pub(crate) position: usize,
    /// `= VALUE` or `#[default]`.
    pub(crate) initialized: bool,
    /// `#[default]`: the payload's `Default::default()` initializes it.
    pub(crate) trait_default: bool,
    /// Carries `#[deprecated]` (a v3 no-op migration bridge when unused).
    pub(crate) deprecated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct BodyDecl {
    pub(crate) name: Name,
    pub(crate) optional: bool,
}

#[derive(Debug)]
pub(crate) struct CredentialDecl {
    pub(crate) name: Name,
    /// Root-relative source endpoint path for `endpoint a::b` sources.
    pub(crate) endpoint_source: Option<Vec<String>>,
    /// A `custom Provider` source (the builder gains `<name>_provider`).
    pub(crate) custom: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Site {
    pub(crate) subdomain: Option<RouteClause>,
    pub(crate) path: Option<RouteClause>,
    pub(crate) query: Option<KeyClause>,
    pub(crate) headers: Option<KeyClause>,
    pub(crate) rate_limits: Vec<RateAttachment>,
    pub(crate) has_auth: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClauseMode {
    Modify,
    Replace,
    Off,
}

/// A `subdomain`/`path` clause; `list` is absent for the `off` form.
#[derive(Debug)]
pub(crate) struct RouteClause {
    pub(crate) atoms: Vec<Atom>,
    /// The `[...]` list including brackets.
    pub(crate) list: Option<Rng>,
}

#[derive(Clone, Debug)]
pub(crate) struct Reference {
    pub(crate) namespace: Name,
    pub(crate) name: Name,
    pub(crate) range: Rng,
}

#[derive(Clone, Debug)]
pub(crate) enum Atom {
    Str {
        value: String,
        range: Rng,
    },
    Char {
        value: char,
        range: Rng,
    },
    Bool {
        value: bool,
        range: Rng,
    },
    Int {
        text: String,
        range: Rng,
    },
    Ref(Reference),
    Adapted {
        inner: Box<Atom>,
        adapter: Rng,
        range: Rng,
    },
    Concat {
        parts: Vec<Atom>,
        range: Rng,
    },
}

impl Atom {
    pub(crate) fn range(&self) -> Rng {
        match self {
            Self::Str { range, .. }
            | Self::Char { range, .. }
            | Self::Bool { range, .. }
            | Self::Int { range, .. }
            | Self::Adapted { range, .. }
            | Self::Concat { range, .. } => range.clone(),
            Self::Ref(reference) => reference.range.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct KeyClause {
    pub(crate) mode: ClauseMode,
    pub(crate) statements: Vec<KeyStatement>,
    pub(crate) range: Rng,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeyOperation {
    Assign,
    WhenPresent,
    Append,
    Remove,
}

#[derive(Debug)]
pub(crate) struct KeyStatement {
    pub(crate) operation: KeyOperation,
    pub(crate) key: String,
    pub(crate) key_range: Rng,
    pub(crate) emission: Option<Emission>,
    pub(crate) range: Rng,
}

#[derive(Debug)]
pub(crate) enum Emission {
    One(Atom),
    Each {
        reference: Reference,
        adapter: Option<Rng>,
    },
}

#[derive(Debug)]
pub(crate) enum RateAttachment {
    Inline(InlineRate),
    Named(NamedRate),
}

#[derive(Debug)]
pub(crate) struct InlineRate {
    pub(crate) header: Vec<HeaderClause<Dimension>>,
    pub(crate) windows: Vec<Window>,
}

#[derive(Debug)]
pub(crate) struct NamedRate {
    pub(crate) policy: Name,
    pub(crate) bindings: Vec<(Name, Reference)>,
    pub(crate) cost: Option<u32>,
    pub(crate) range: Rng,
}

#[derive(Debug)]
pub(crate) struct RatePolicy {
    pub(crate) name: Name,
    pub(crate) header: Vec<HeaderClause<FormalDimension>>,
    pub(crate) windows: Vec<Window>,
}

#[derive(Debug)]
pub(crate) enum HeaderClause<D> {
    Id { value: String, range: Rng },
    Key { dimensions: Vec<D>, range: Rng },
    Cost { value: u32, range: Rng },
}

impl<D> HeaderClause<D> {
    pub(crate) fn range(&self) -> Rng {
        match self {
            Self::Id { range, .. } | Self::Key { range, .. } | Self::Cost { range, .. } => {
                range.clone()
            }
        }
    }

    pub(crate) fn canonical_rank(&self) -> u8 {
        match self {
            Self::Id { .. } => 0,
            Self::Key { .. } => 1,
            Self::Cost { .. } => 2,
        }
    }
}

/// An inline (bound) v2.1 rate dimension.
#[derive(Debug)]
pub(crate) enum Dimension {
    Origin,
    Endpoint,
    Value {
        reference: Reference,
        adapter: Option<Rng>,
        range: Rng,
    },
}

/// A named-policy (formal) v2.1 rate dimension.
#[derive(Debug)]
pub(crate) enum FormalDimension {
    Origin,
    Endpoint,
    Formal {
        name: Name,
        adapter: Option<Rng>,
        range: Rng,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Window {
    pub(crate) capacity: u64,
    pub(crate) duration_ms: u128,
    pub(crate) range: Rng,
}

#[derive(Debug)]
pub(crate) struct Pagination {
    pub(crate) controller: Rng,
    /// Path segments of the controller type when it is a plain type path.
    pub(crate) controller_segments: Vec<String>,
    /// Source range of the last controller path segment identifier.
    pub(crate) last_segment: Option<Rng>,
    /// Source range of the last segment's generic arguments, including `<>`.
    pub(crate) generics: Option<Rng>,
    pub(crate) fields: Vec<PageField>,
    pub(crate) brace_open: Rng,
    pub(crate) brace_close: Rng,
}

#[derive(Debug)]
pub(crate) struct PageField {
    pub(crate) name: Name,
    pub(crate) value: PageValue,
}

#[derive(Debug)]
pub(crate) enum PageValue {
    Live(Reference),
    Stable(Rng),
    Reference(Reference),
}

struct Cx {
    base: usize,
    /// One frame of collected user paths per owner being parsed.
    paths: std::cell::RefCell<Vec<Vec<UserPath>>>,
}

impl Cx {
    fn open_paths(&self) {
        self.paths.borrow_mut().push(Vec::new());
    }

    fn close_paths(&self) -> Vec<UserPath> {
        self.paths.borrow_mut().pop().unwrap_or_default()
    }

    fn push_path(&self, path: UserPath) {
        if let Some(frame) = self.paths.borrow_mut().last_mut() {
            frame.push(path);
        }
    }

    /// Records every path of a Rust type, including generic arguments.
    fn record_type(&self, ty: &Type) {
        match ty {
            Type::Path(path) => {
                if let Some(qself) = &path.qself {
                    self.record_type(&qself.ty);
                } else if let Some(first) = path.path.segments.first() {
                    self.push_path(UserPath {
                        constant: false,
                        leading_colon: path.path.leading_colon.is_some(),
                        segments: path
                            .path
                            .segments
                            .iter()
                            .map(|segment| {
                                let text = segment.ident.to_string();
                                text.strip_prefix("r#").unwrap_or(&text).to_owned()
                            })
                            .collect(),
                        range: self.span(first.ident.span()),
                    });
                }
                for segment in &path.path.segments {
                    if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
                        for argument in &arguments.args {
                            if let syn::GenericArgument::Type(inner) = argument {
                                self.record_type(inner);
                            }
                        }
                    }
                }
            }
            Type::Reference(reference) => self.record_type(&reference.elem),
            Type::Slice(slice) => self.record_type(&slice.elem),
            Type::Array(array) => self.record_type(&array.elem),
            Type::Paren(paren) => self.record_type(&paren.elem),
            Type::Group(group) => self.record_type(&group.elem),
            Type::Tuple(tuple) => tuple.elems.iter().for_each(|elem| self.record_type(elem)),
            _ => {}
        }
    }

    fn record_constant(&self, path: &syn::Path) {
        if let Some(first) = path.segments.first() {
            self.push_path(UserPath {
                constant: true,
                leading_colon: path.leading_colon.is_some(),
                segments: path
                    .segments
                    .iter()
                    .map(|segment| {
                        let text = segment.ident.to_string();
                        text.strip_prefix("r#").unwrap_or(&text).to_owned()
                    })
                    .collect(),
                range: self.span(first.ident.span()),
            });
        }
    }

    fn span(&self, span: Span) -> Rng {
        let range = span.byte_range();
        (range.start + self.base)..(range.end + self.base)
    }

    fn node(&self, node: &impl ToTokens) -> Rng {
        self.tokens(node.to_token_stream())
    }

    fn tokens(&self, tokens: TokenStream) -> Rng {
        let mut spans = tokens.into_iter().map(|token| token.span());
        let Some(first) = spans.next() else {
            return self.base..self.base;
        };
        let last = spans.last().unwrap_or(first);
        self.span(first).start..self.span(last).end
    }

    fn name(&self, ident: &Ident) -> Name {
        let text = ident.to_string();
        Name {
            logical: text.strip_prefix("r#").unwrap_or(&text).to_owned(),
            text,
            range: self.span(ident.span()),
        }
    }
}

/// Parses one v2.1 declaration token stream. `base` is the byte offset of the
/// text that `proc_macro2` parsed relative to the original file.
pub(crate) fn parse_api(tokens: TokenStream, base: usize) -> syn::Result<Api> {
    let cx = Cx {
        base,
        paths: std::cell::RefCell::new(Vec::new()),
    };
    (|input: ParseStream<'_>| {
        let client = parse_client(input, &cx)?;
        if !input.is_empty() {
            return Err(input.error(
                "a v2.1 `api!` invocation accepts exactly one client; unexpected tokens follow it",
            ));
        }
        Ok(Api { client })
    })
    .parse2(tokens)
}

fn parse_client(input: ParseStream<'_>, cx: &Cx) -> syn::Result<Client> {
    input.call(Attribute::parse_outer)?;
    input.parse::<kw::client>()?;
    let name: Ident = input.parse()?;
    let content;
    braced!(content in input);
    cx.open_paths();
    let mut client = Client {
        name: cx.name(&name),
        values: Vec::new(),
        credentials: Vec::new(),
        secrets: Vec::new(),
        policies: Vec::new(),
        site: Site::default(),
        children: Vec::new(),
        paths: Vec::new(),
    };
    while !content.is_empty() {
        if child_starts(&content) {
            client.children.push(parse_child(&content, cx, 1)?);
            continue;
        }
        if content.peek(kw::base) {
            content.parse::<kw::base>()?;
            content.parse::<LitStr>()?;
            content.parse::<Token![;]>()?;
            continue;
        }
        if content.peek(kw::values) {
            content.parse::<kw::values>()?;
            let body;
            braced!(body in content);
            let mut position = 0;
            while !body.is_empty() {
                let attributes = body.call(Attribute::parse_outer)?;
                client
                    .values
                    .push(parse_value(&body, cx, attributes, position)?);
                body.parse::<Token![;]>()?;
                position += 1;
            }
            continue;
        }
        if content.peek(kw::credentials) {
            let (credentials, secrets) = parse_credentials(&content, cx)?;
            client.credentials = credentials;
            client.secrets = secrets;
            continue;
        }
        if content.peek(kw::rate_limits) {
            client.policies = parse_policies(&content, cx)?;
            continue;
        }
        if parse_site_clause(&content, cx, &mut client.site)? {
            continue;
        }
        return Err(content
            .error("unsupported v2.1 client entry; expected a scope, endpoint or client clause"));
    }
    client.paths = cx.close_paths();
    Ok(client)
}

fn child_starts(input: ParseStream<'_>) -> bool {
    let fork = input.fork();
    if fork.call(Attribute::parse_outer).is_err() {
        return false;
    }
    fork.peek(kw::scope) || endpoint_starts(&fork)
}

fn endpoint_starts(input: ParseStream<'_>) -> bool {
    if input.peek(kw::METHOD) {
        return true;
    }
    input.fork().parse::<Ident>().is_ok_and(|ident| {
        matches!(
            ident.to_string().as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
        )
    })
}

fn parse_child(input: ParseStream<'_>, cx: &Cx, depth: usize) -> syn::Result<Child> {
    if depth > 64 {
        return Err(input.error("scope nesting exceeds the supported depth of 64"));
    }
    let fork = input.fork();
    fork.call(Attribute::parse_outer)?;
    if fork.peek(kw::scope) {
        Ok(Child::Scope(Box::new(parse_scope(input, cx, depth)?)))
    } else {
        Ok(Child::Endpoint(Box::new(parse_endpoint(input, cx)?)))
    }
}

fn parse_scope(input: ParseStream<'_>, cx: &Cx, depth: usize) -> syn::Result<Scope> {
    input.call(Attribute::parse_outer)?;
    input.parse::<kw::scope>()?;
    let name: Ident = input.parse()?;
    let parameters;
    parenthesized!(parameters in input);
    cx.open_paths();
    let (params, body) = parse_parameters(&parameters, cx)?;
    if let Some(body) = body {
        return Err(syn::Error::new(
            Span::call_site(),
            format!("scope parameter `{}` cannot be a body", body.name.logical),
        ));
    }
    let content;
    braced!(content in input);
    let mut scope = Scope {
        name: cx.name(&name),
        params,
        site: Site::default(),
        children: Vec::new(),
        paths: Vec::new(),
    };
    while !content.is_empty() {
        if child_starts(&content) {
            scope.children.push(parse_child(&content, cx, depth + 1)?);
            continue;
        }
        if parse_site_clause(&content, cx, &mut scope.site)? {
            continue;
        }
        return Err(content.error("unsupported v2.1 scope entry"));
    }
    scope.paths = cx.close_paths();
    Ok(scope)
}

fn parse_endpoint(input: ParseStream<'_>, cx: &Cx) -> syn::Result<Endpoint> {
    input.call(Attribute::parse_outer)?;
    let (method, method_range) = if input.peek(kw::METHOD) {
        let marker: kw::METHOD = input.parse()?;
        let content;
        let paren = parenthesized!(content in input);
        let literal: LitStr = content.parse()?;
        (
            format!("METHOD({})", literal.value()),
            cx.span(marker.span).start..cx.span(paren.span.close()).end,
        )
    } else {
        let method: Ident = input.parse()?;
        (method.to_string(), cx.span(method.span()))
    };
    let name: Ident = input.parse()?;
    let parameters;
    parenthesized!(parameters in input);
    cx.open_paths();
    let (params, body) = parse_parameters(&parameters, cx)?;
    input.parse::<Token![->]>()?;
    cx.record_type(&input.parse::<Type>()?);
    if input.peek(kw::via) {
        input.parse::<kw::via>()?;
        cx.record_type(&input.parse::<Type>()?);
    }
    let mut endpoint = Endpoint {
        method,
        method_range,
        name: cx.name(&name),
        params,
        body,
        site: Site::default(),
        pagination: None,
        id: None,
        clause_block: None,
        clause_ranges: Vec::new(),
        semicolon: None,
        paths: Vec::new(),
    };
    if input.peek(Token![;]) {
        let semicolon: Token![;] = input.parse()?;
        endpoint.semicolon = Some(cx.span(semicolon.spans[0]));
        endpoint.paths = cx.close_paths();
        return Ok(endpoint);
    }
    let content;
    let brace = braced!(content in input);
    endpoint.clause_block = Some((cx.span(brace.span.open()), cx.span(brace.span.close())));
    while !content.is_empty() {
        let start = cx.span(content.span()).start;
        let end = if content.peek(kw::paginate) {
            let pagination = parse_pagination(&content, cx)?;
            let end = pagination.brace_close.end;
            endpoint.pagination = Some(pagination);
            end
        } else if content.peek(kw::id) {
            content.parse::<kw::id>()?;
            let literal: LitStr = content.parse()?;
            let semicolon: Token![;] = content.parse()?;
            endpoint.id = Some((literal.value(), cx.span(literal.span())));
            cx.span(semicolon.spans[0]).end
        } else if content.peek(kw::effect) || content.peek(kw::status) || content.peek(kw::expose) {
            skip_to_semicolon(&content, cx)?
        } else {
            let before = content.fork();
            if !parse_site_clause(&content, cx, &mut endpoint.site)? {
                return Err(before.error("unsupported v2.1 endpoint clause"));
            }
            last_consumed_end(&before, &content, cx)
        };
        endpoint.clause_ranges.push(start..end);
    }
    endpoint.paths = cx.close_paths();
    Ok(endpoint)
}

/// Consumes tokens through the next top-level `;` and returns its end offset.
fn skip_to_semicolon(input: ParseStream<'_>, cx: &Cx) -> syn::Result<usize> {
    loop {
        if input.peek(Token![;]) {
            let semicolon: Token![;] = input.parse()?;
            return Ok(cx.span(semicolon.spans[0]).end);
        }
        if input.is_empty() {
            return Err(input.error("expected `;`"));
        }
        input.parse::<proc_macro2::TokenTree>()?;
    }
}

/// Computes the end offset of the tokens consumed between `before` (a fork
/// taken before parsing) and `after`.
fn last_consumed_end(
    before: &syn::parse::ParseBuffer<'_>,
    after: &syn::parse::ParseBuffer<'_>,
    cx: &Cx,
) -> usize {
    let mut end = cx.span(before.span()).end;
    let mut cursor = before.cursor();
    let stop = after.cursor();
    while cursor != stop {
        let Some((token, next)) = cursor.token_tree() else {
            break;
        };
        end = cx.span(token.span()).end;
        cursor = next;
    }
    end
}

fn parse_parameters(
    input: ParseStream<'_>,
    cx: &Cx,
) -> syn::Result<(Vec<ValueDecl>, Option<BodyDecl>)> {
    let mut params = Vec::new();
    let mut body = None;
    let mut position = 0;
    while !input.is_empty() {
        let attributes = input.call(Attribute::parse_outer)?;
        if input.peek(kw::body) {
            input.parse::<kw::body>()?;
            let name: Ident = input.parse()?;
            let optional = input.peek(Token![?]);
            if optional {
                input.parse::<Token![?]>()?;
            }
            input.parse::<Token![:]>()?;
            cx.record_type(&input.parse::<Type>()?);
            if input.peek(kw::via) {
                input.parse::<kw::via>()?;
                cx.record_type(&input.parse::<Type>()?);
            }
            body = Some(BodyDecl {
                name: cx.name(&name),
                optional,
            });
        } else {
            params.push(parse_value(input, cx, attributes, position)?);
            position += 1;
        }
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok((params, body))
}

fn parse_value(
    input: ParseStream<'_>,
    cx: &Cx,
    attributes: Vec<Attribute>,
    position: usize,
) -> syn::Result<ValueDecl> {
    let name: Ident = input.parse()?;
    let optional = input.peek(Token![?]);
    if optional {
        input.parse::<Token![?]>()?;
    }
    input.parse::<Token![:]>()?;
    let ty: Type = input.parse()?;
    cx.record_type(&ty);
    let ty_range = cx.node(&ty);
    let mut sensitive = false;
    let mut initialized = false;
    let mut trait_default = false;
    let mut deprecated = false;
    for attribute in &attributes {
        if attribute.path().is_ident("sensitive") {
            sensitive = true;
        } else if attribute.path().is_ident("default") {
            initialized = true;
            trait_default = true;
        } else if attribute.path().is_ident("deprecated") {
            deprecated = true;
        } else if attribute.path().is_ident("doc")
            && !matches!(
                &attribute.meta,
                Meta::NameValue(meta)
                    if matches!(&meta.value, Expr::Lit(ExprLit { lit: Lit::Str(_), .. }))
            )
        {
            return Err(syn::Error::new(
                attribute.pound_token.spans[0],
                "malformed Rust doc attribute",
            ));
        }
    }
    if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
        parse_stable_value(input, cx)?;
        initialized = true;
    }
    Ok(ValueDecl {
        ident_start: cx.span(name.span()).start,
        name: cx.name(&name),
        optional,
        type_end: ty_range.end,
        sensitive,
        position,
        initialized,
        trait_default,
        deprecated,
    })
}

/// Parses a v2.1 `stable_value` and returns its exact source range.
fn parse_stable_value(input: ParseStream<'_>, cx: &Cx) -> syn::Result<Rng> {
    if input.peek(Token![-]) {
        let minus: Token![-] = input.parse()?;
        let start = cx.span(minus.spans[0]).start;
        let end = if input.peek(LitInt) {
            cx.node(&input.parse::<LitInt>()?).end
        } else {
            cx.node(&input.parse::<LitFloat>()?).end
        };
        return Ok(start..end);
    }
    if input.peek(LitStr)
        || input.peek(LitChar)
        || input.peek(LitInt)
        || input.peek(LitFloat)
        || input.peek(LitBool)
    {
        let literal: Lit = input.parse()?;
        return Ok(cx.node(&literal));
    }
    let path: syn::Path = input.parse()?;
    cx.record_constant(&path);
    Ok(cx.node(&path))
}

/// The credentials block: its credentials and its `secret` names.
fn parse_credentials(
    input: ParseStream<'_>,
    cx: &Cx,
) -> syn::Result<(Vec<CredentialDecl>, Vec<Name>)> {
    input.parse::<kw::credentials>()?;
    let body;
    braced!(body in input);
    let mut credentials = Vec::new();
    let mut secrets = Vec::new();
    while !body.is_empty() {
        body.call(Attribute::parse_outer)?;
        let marker: Ident = body.call(Ident::parse_any)?;
        let name: Ident = body.call(Ident::parse_any)?;
        let mut endpoint_source = None;
        if marker == "secret" {
            secrets.push(cx.name(&name));
        }
        if marker == "credential" {
            body.parse::<Token![=]>()?;
            let custom = body
                .fork()
                .call(Ident::parse_any)
                .is_ok_and(|source| source == "custom");
            if body.peek(kw::endpoint) {
                body.parse::<kw::endpoint>()?;
                let mut segments = vec![body.call(Ident::parse_any)?.to_string()];
                while body.peek(Token![::]) {
                    body.parse::<Token![::]>()?;
                    segments.push(body.call(Ident::parse_any)?.to_string());
                }
                endpoint_source = Some(
                    segments
                        .into_iter()
                        .map(|segment| segment.trim_start_matches("r#").to_owned())
                        .collect(),
                );
            } else {
                while !body.peek(Token![;]) {
                    if body.is_empty() {
                        return Err(body.error("expected `;` after a credential source"));
                    }
                    body.parse::<proc_macro2::TokenTree>()?;
                }
            }
            credentials.push(CredentialDecl {
                name: cx.name(&name),
                endpoint_source,
                custom,
            });
        }
        body.parse::<Token![;]>()?;
    }
    Ok((credentials, secrets))
}

fn parse_site_clause(input: ParseStream<'_>, cx: &Cx, site: &mut Site) -> syn::Result<bool> {
    if input.peek(kw::subdomain) {
        input.parse::<kw::subdomain>()?;
        site.subdomain = Some(parse_route(input, cx)?);
        return Ok(true);
    }
    if input.peek(kw::path) {
        input.parse::<kw::path>()?;
        site.path = Some(parse_route(input, cx)?);
        return Ok(true);
    }
    if input.peek(kw::query) {
        let marker: kw::query = input.parse()?;
        site.query = Some(parse_keys(input, cx, cx.span(marker.span).start)?);
        return Ok(true);
    }
    if input.peek(kw::headers) {
        let marker: kw::headers = input.parse()?;
        site.headers = Some(parse_keys(input, cx, cx.span(marker.span).start)?);
        return Ok(true);
    }
    if input.peek(kw::auth) {
        input.parse::<kw::auth>()?;
        if input.peek(kw::off) {
            input.parse::<kw::off>()?;
            input.parse::<Token![;]>()?;
        } else {
            if input.peek(kw::replace) {
                input.parse::<kw::replace>()?;
            }
            let body;
            braced!(body in input);
            body.parse::<TokenStream>()?;
        }
        site.has_auth = true;
        return Ok(true);
    }
    if input.peek(kw::timeout) {
        input.parse::<kw::timeout>()?;
        input.parse::<LitInt>()?;
        input.parse::<Token![;]>()?;
        return Ok(true);
    }
    if input.peek(kw::request) || input.peek(kw::response) {
        input.parse::<Ident>()?;
        if input.peek(kw::explicit) {
            input.parse::<kw::explicit>()?;
        } else {
            cx.record_type(&input.parse::<Type>()?);
        }
        input.parse::<Token![;]>()?;
        return Ok(true);
    }
    if input.peek(kw::rate_limit) {
        let marker: kw::rate_limit = input.parse()?;
        site.rate_limits.push(parse_rate_attachment(
            input,
            cx,
            cx.span(marker.span).start,
        )?);
        return Ok(true);
    }
    Ok(false)
}

fn parse_route(input: ParseStream<'_>, cx: &Cx) -> syn::Result<RouteClause> {
    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(RouteClause {
            atoms: Vec::new(),
            list: None,
        });
    }
    if input.peek(kw::replace) {
        input.parse::<kw::replace>()?;
    }
    let content;
    let bracket = bracketed!(content in input);
    let mut atoms = Vec::new();
    while !content.is_empty() {
        atoms.push(parse_atom(&content, cx, 0)?);
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    input.parse::<Token![;]>()?;
    Ok(RouteClause {
        atoms,
        list: Some(cx.span(bracket.span.open()).start..cx.span(bracket.span.close()).end),
    })
}

fn parse_reference(input: ParseStream<'_>, cx: &Cx) -> syn::Result<Reference> {
    let namespace: Ident = input.call(Ident::parse_any)?;
    input.parse::<Token![.]>()?;
    let name: Ident = input.call(Ident::parse_any)?;
    let namespace = cx.name(&namespace);
    let name = cx.name(&name);
    Ok(Reference {
        range: namespace.range.start..name.range.end,
        namespace,
        name,
    })
}

fn parse_atom(input: ParseStream<'_>, cx: &Cx, depth: usize) -> syn::Result<Atom> {
    if depth > 64 {
        return Err(input.error("`concat` nesting exceeds the supported depth of 64"));
    }
    let atom = if input.peek(LitStr) {
        let literal: LitStr = input.parse()?;
        Atom::Str {
            value: literal.value(),
            range: cx.span(literal.span()),
        }
    } else if input.peek(LitChar) {
        let literal: LitChar = input.parse()?;
        Atom::Char {
            value: literal.value(),
            range: cx.span(literal.span()),
        }
    } else if input.peek(LitBool) {
        let literal: LitBool = input.parse()?;
        Atom::Bool {
            value: literal.value,
            range: cx.span(literal.span),
        }
    } else if input.peek(Token![-]) {
        let minus: Token![-] = input.parse()?;
        let literal: LitInt = input.parse()?;
        Atom::Int {
            // Integer formatting in v2 evaluates unary minus: -0 writes "0".
            // Do not range-parse other literals or reject previously accepted values.
            text: if literal.base10_digits().bytes().all(|byte| byte == b'0') {
                "0".to_owned()
            } else {
                format!("-{}", literal.base10_digits())
            },
            range: cx.span(minus.spans[0]).start..cx.span(literal.span()).end,
        }
    } else if input.peek(LitInt) {
        let literal: LitInt = input.parse()?;
        Atom::Int {
            text: literal.base10_digits().to_owned(),
            range: cx.span(literal.span()),
        }
    } else if input.peek(kw::concat) {
        let marker: kw::concat = input.parse()?;
        let content;
        let bracket = bracketed!(content in input);
        let mut parts = Vec::new();
        while !content.is_empty() {
            parts.push(parse_atom(&content, cx, depth + 1)?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        Atom::Concat {
            parts,
            range: cx.span(marker.span).start..cx.span(bracket.span.close()).end,
        }
    } else {
        Atom::Ref(parse_reference(input, cx)?)
    };
    if input.peek(kw::via) {
        input.parse::<kw::via>()?;
        let adapter: Type = input.parse()?;
        cx.record_type(&adapter);
        let adapter = cx.node(&adapter);
        let range = atom.range().start..adapter.end;
        return Ok(Atom::Adapted {
            inner: Box::new(atom),
            adapter,
            range,
        });
    }
    Ok(atom)
}

fn parse_keys(input: ParseStream<'_>, cx: &Cx, start: usize) -> syn::Result<KeyClause> {
    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        let semicolon: Token![;] = input.parse()?;
        return Ok(KeyClause {
            mode: ClauseMode::Off,
            statements: Vec::new(),
            range: start..cx.span(semicolon.spans[0]).end,
        });
    }
    let mode = if input.peek(kw::replace) {
        input.parse::<kw::replace>()?;
        ClauseMode::Replace
    } else {
        ClauseMode::Modify
    };
    let content;
    let brace = braced!(content in input);
    let mut statements = Vec::new();
    while !content.is_empty() {
        let statement_start = cx.span(content.span()).start;
        let operation = if content.peek(kw::remove) {
            content.parse::<kw::remove>()?;
            KeyOperation::Remove
        } else if content.peek(kw::when_present) {
            content.parse::<kw::when_present>()?;
            KeyOperation::WhenPresent
        } else if content.peek(kw::append) {
            content.parse::<kw::append>()?;
            KeyOperation::Append
        } else {
            KeyOperation::Assign
        };
        let key: LitStr = content.parse()?;
        let emission = if operation == KeyOperation::Remove {
            None
        } else {
            content.parse::<Token![=]>()?;
            Some(if content.peek(kw::each) {
                content.parse::<kw::each>()?;
                let reference = parse_reference(&content, cx)?;
                let adapter = if content.peek(kw::via) {
                    content.parse::<kw::via>()?;
                    let adapter = content.parse::<Type>()?;
                    cx.record_type(&adapter);
                    Some(cx.node(&adapter))
                } else {
                    None
                };
                Emission::Each { reference, adapter }
            } else {
                Emission::One(parse_atom(&content, cx, 0)?)
            })
        };
        let semicolon: Token![;] = content.parse()?;
        statements.push(KeyStatement {
            operation,
            key: key.value(),
            key_range: cx.span(key.span()),
            emission,
            range: statement_start..cx.span(semicolon.spans[0]).end,
        });
    }
    Ok(KeyClause {
        mode,
        statements,
        range: start..cx.span(brace.span.close()).end,
    })
}

fn parse_rate_attachment(
    input: ParseStream<'_>,
    cx: &Cx,
    start: usize,
) -> syn::Result<RateAttachment> {
    if input.peek(syn::token::Brace) {
        let content;
        braced!(content in input);
        let (header, windows) = parse_rate_body(&content, cx, &|input, cx| {
            if input.peek(kw::origin) {
                input.parse::<kw::origin>()?;
                return Ok(Dimension::Origin);
            }
            if input.peek(kw::endpoint) && !input.peek2(Token![.]) {
                input.parse::<kw::endpoint>()?;
                return Ok(Dimension::Endpoint);
            }
            let reference = parse_reference(input, cx)?;
            let (adapter, end) = parse_optional_adapter(input, cx, reference.range.end)?;
            Ok(Dimension::Value {
                range: reference.range.start..end,
                reference,
                adapter,
            })
        })?;
        return Ok(RateAttachment::Inline(InlineRate { header, windows }));
    }
    let policy: Ident = input.call(Ident::parse_any)?;
    let content;
    parenthesized!(content in input);
    let mut bindings = Vec::new();
    while !content.is_empty() {
        let formal: Ident = content.call(Ident::parse_any)?;
        content.parse::<Token![=]>()?;
        bindings.push((cx.name(&formal), parse_reference(&content, cx)?));
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    let mut cost = None;
    let end = if input.peek(syn::token::Brace) {
        let overrides;
        let brace = braced!(overrides in input);
        overrides.parse::<kw::cost>()?;
        cost = Some(parse_positive(&overrides.parse::<LitInt>()?)?);
        overrides.parse::<Token![;]>()?;
        cx.span(brace.span.close()).end
    } else {
        let semicolon: Token![;] = input.parse()?;
        cx.span(semicolon.spans[0]).end
    };
    Ok(RateAttachment::Named(NamedRate {
        policy: cx.name(&policy),
        bindings,
        cost,
        range: start..end,
    }))
}

fn parse_optional_adapter(
    input: ParseStream<'_>,
    cx: &Cx,
    end: usize,
) -> syn::Result<(Option<Rng>, usize)> {
    if input.peek(kw::via) {
        input.parse::<kw::via>()?;
        let adapter = cx.node(&input.parse::<Type>()?);
        let end = adapter.end;
        return Ok((Some(adapter), end));
    }
    Ok((None, end))
}

type DimensionParser<D> = dyn Fn(ParseStream<'_>, &Cx) -> syn::Result<D>;

fn parse_rate_body<D>(
    input: ParseStream<'_>,
    cx: &Cx,
    dimension: &DimensionParser<D>,
) -> syn::Result<(Vec<HeaderClause<D>>, Vec<Window>)> {
    let mut header = Vec::new();
    let mut windows = Vec::new();
    while !input.is_empty() {
        let start = cx.span(input.span()).start;
        if input.peek(kw::id) {
            input.parse::<kw::id>()?;
            let literal: LitStr = input.parse()?;
            let semicolon: Token![;] = input.parse()?;
            header.push(HeaderClause::Id {
                value: literal.value(),
                range: start..cx.span(semicolon.spans[0]).end,
            });
        } else if input.peek(kw::key) {
            input.parse::<kw::key>()?;
            let content;
            bracketed!(content in input);
            let mut dimensions = Vec::new();
            while !content.is_empty() {
                dimensions.push(dimension(&content, cx)?);
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
            let semicolon: Token![;] = input.parse()?;
            header.push(HeaderClause::Key {
                dimensions,
                range: start..cx.span(semicolon.spans[0]).end,
            });
        } else if input.peek(kw::cost) {
            input.parse::<kw::cost>()?;
            let value = parse_positive(&input.parse::<LitInt>()?)?;
            let semicolon: Token![;] = input.parse()?;
            header.push(HeaderClause::Cost {
                value,
                range: start..cx.span(semicolon.spans[0]).end,
            });
        } else if input.peek(kw::allow) {
            input.parse::<kw::allow>()?;
            let capacity: LitInt = input.parse()?;
            input.parse::<kw::per>()?;
            let duration: LitInt = input.parse()?;
            let semicolon: Token![;] = input.parse()?;
            windows.push(Window {
                capacity: capacity.base10_parse::<u64>()?,
                duration_ms: parse_duration_ms(&duration)?,
                range: start..cx.span(semicolon.spans[0]).end,
            });
        } else {
            return Err(input.error(
                "a v2.1 rate policy accepts `id`, `key`, `cost` and `allow N per D;` entries",
            ));
        }
    }
    Ok((header, windows))
}

fn parse_positive(literal: &LitInt) -> syn::Result<u32> {
    let value = literal.base10_parse::<u32>()?;
    if value == 0 {
        return Err(syn::Error::new(
            literal.span(),
            "a rate-limit cost must be positive",
        ));
    }
    Ok(value)
}

fn parse_duration_ms(literal: &LitInt) -> syn::Result<u128> {
    let value = u128::from(literal.base10_parse::<u64>()?);
    let factor = match literal.suffix() {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        _ => {
            return Err(syn::Error::new(
                literal.span(),
                "a duration requires an `ms`, `s`, `m` or `h` suffix",
            ));
        }
    };
    Ok(value * factor)
}

fn parse_policies(input: ParseStream<'_>, cx: &Cx) -> syn::Result<Vec<RatePolicy>> {
    input.parse::<kw::rate_limits>()?;
    let body;
    braced!(body in input);
    let mut policies = Vec::new();
    while !body.is_empty() {
        body.call(Attribute::parse_outer)?;
        body.parse::<kw::policy>()?;
        let name: Ident = body.call(Ident::parse_any)?;
        let formals;
        parenthesized!(formals in body);
        formals.parse::<TokenStream>()?;
        let content;
        braced!(content in body);
        let (header, windows) = parse_rate_body(&content, cx, &|input, cx| {
            if input.peek(kw::origin) {
                input.parse::<kw::origin>()?;
                return Ok(FormalDimension::Origin);
            }
            if input.peek(kw::endpoint) {
                input.parse::<kw::endpoint>()?;
                return Ok(FormalDimension::Endpoint);
            }
            let formal: Ident = input.call(Ident::parse_any)?;
            let name = cx.name(&formal);
            let (adapter, end) = parse_optional_adapter(input, cx, name.range.end)?;
            Ok(FormalDimension::Formal {
                range: name.range.start..end,
                name,
                adapter,
            })
        })?;
        policies.push(RatePolicy {
            name: cx.name(&name),
            header,
            windows,
        });
    }
    Ok(policies)
}

fn parse_pagination(input: ParseStream<'_>, cx: &Cx) -> syn::Result<Pagination> {
    input.parse::<kw::paginate>()?;
    input.parse::<kw::via>()?;
    let controller: Type = input.parse()?;
    cx.record_type(&controller);
    let controller_range = cx.node(&controller);
    let (controller_segments, last_segment, generics) = match &controller {
        Type::Path(path) if path.qself.is_none() => {
            let segments = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>();
            let last = path.path.segments.last();
            let last_segment = last.map(|segment| cx.span(segment.ident.span()));
            let generics = last.and_then(|segment| match &segment.arguments {
                syn::PathArguments::AngleBracketed(arguments) => Some(cx.node(arguments)),
                _ => None,
            });
            (segments, last_segment, generics)
        }
        _ => (Vec::new(), None, None),
    };
    let content;
    let brace = braced!(content in input);
    let mut fields = Vec::new();
    while !content.is_empty() {
        let name: Ident = content.call(Ident::parse_any)?;
        let value = if content.peek(Token![->]) {
            content.parse::<Token![->]>()?;
            PageValue::Live(parse_reference(&content, cx)?)
        } else {
            content.parse::<Token![=]>()?;
            let fork = content.fork();
            if fork.call(Ident::parse_any).is_ok() && fork.peek(Token![.]) {
                PageValue::Reference(parse_reference(&content, cx)?)
            } else {
                PageValue::Stable(parse_stable_value(&content, cx)?)
            }
        };
        content.parse::<Token![;]>()?;
        fields.push(PageField {
            name: cx.name(&name),
            value,
        });
    }
    Ok(Pagination {
        controller: controller_range,
        controller_segments,
        last_segment,
        generics,
        fields,
        brace_open: cx.span(brace.span.open()),
        brace_close: cx.span(brace.span.close()),
    })
}
