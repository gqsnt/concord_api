//! Client, scope and endpoint declarations (§3.2).
//!
//! Every owner is parsed with its owner label (``client `Api` ``,
//! ``scope `Api::regional` ``, ``endpoint `Api::regional::item` ``) so that
//! each coded diagnostic names the owning declaration (DIAG-01).

use std::collections::{BTreeSet, HashMap};

use http::Method;
use proc_macro2::{Span, TokenStream};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Attribute, Ident, LitStr, Token, braced, parenthesized};

use super::ast::{
    AttachmentSite, Clause, EffectDecl, MethodKind, PlacementFamily, RepresentationSelection,
    SourceApi, SourceChild, SourceClient, SourceEndpoint, SourceScope, StatusDecl,
};
use super::credentials::{parse_auth, parse_credentials};
use super::diagnostics::{
    Code, client_label, coded, endpoint_label, insert_clause, related, scope_label, validate_base,
    validate_endpoint_id, validate_snake_source_identifier,
};
use super::keyword;
use super::pagination::parse_pagination;
use super::rate_limits::{parse_rate_attachment, parse_rate_registry};
use super::request::{parse_key_block, parse_path, parse_subdomain, parse_timeout};
use super::values::{AttrSite, attrs_from, parse_parameters, parse_values};
use crate::semantic::surface::canonical_ident;

const MAX_SCOPE_DEPTH: usize = 64;

/// The owner a child is declared in: its display path (`Api::regional`) and
/// its diagnostic label (``scope `Api::regional` ``).
#[derive(Clone, Copy)]
struct Parent<'a> {
    path: &'a str,
    label: &'a str,
}

pub(crate) fn parse(input: TokenStream) -> syn::Result<SourceApi> {
    syn::parse2(input)
}

impl Parse for SourceApi {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(coded(
                input.span(),
                Code::Owner,
                "an `api!` invocation requires exactly one `client` declaration; write `client Name { base \"https://...\"; ... }`",
            ));
        }
        let fork = input.fork();
        let _ = fork.call(Attribute::parse_outer)?;
        if !fork.peek(keyword::client) {
            return Err(coded(
                input.span(),
                Code::Owner,
                "an `api!` invocation begins with exactly one `client Name { ... }` declaration",
            ));
        }
        let client: SourceClient = input.parse()?;
        if !input.is_empty() {
            return Err(coded(
                input.span(),
                Code::Owner,
                format!(
                    "an `api!` invocation accepts exactly one closed `client` ({} is already declared); move scopes and endpoints inside it, or declare another client in a separate invocation",
                    client_label(&canonical_ident(&client.name))
                ),
            ));
        }
        Ok(Self { client })
    }
}

impl Parse for SourceClient {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attributes = input.call(Attribute::parse_outer)?;
        input.parse::<keyword::client>()?;
        let name: Ident = input.parse()?;
        let logical = canonical_ident(&name);
        let owner = client_label(&logical);
        let attrs = attrs_from(attributes, AttrSite::Declaration, "a client", &owner)?;
        let body;
        braced!(body in input);
        let parent = Parent {
            path: &logical,
            label: &owner,
        };
        let mut base = None;
        let mut values = None;
        let mut credentials = None;
        let mut rate_registry = None;
        let mut site = AttachmentSite::default();
        let mut children = Vec::new();
        while !body.is_empty() {
            if child_starts(&body) {
                children.push(parse_child(&body, 1, parent)?);
                continue;
            }
            let clause_span = body.span();
            let parsed_clause = if body.peek(keyword::base) {
                body.parse::<keyword::base>()?;
                let value: LitStr = body.parse()?;
                body.parse::<Token![;]>()?;
                validate_base(&value, &owner)?;
                insert_clause(
                    &mut base,
                    Clause {
                        key_span: clause_span,
                        value,
                    },
                    "base",
                    &owner,
                )?;
                true
            } else if body.peek(keyword::values) {
                let value = parse_values(&body, &owner)?;
                insert_clause(
                    &mut values,
                    Clause {
                        key_span: clause_span,
                        value,
                    },
                    "values",
                    &owner,
                )?;
                true
            } else if body.peek(keyword::credentials) {
                let value = parse_credentials(&body, &owner)?;
                insert_clause(
                    &mut credentials,
                    Clause {
                        key_span: clause_span,
                        value,
                    },
                    "credentials",
                    &owner,
                )?;
                true
            } else if body.peek(keyword::rate_limits) {
                let value = parse_rate_registry(&body, &owner)?;
                insert_clause(
                    &mut rate_registry,
                    Clause {
                        key_span: clause_span,
                        value,
                    },
                    "rate_limits",
                    &owner,
                )?;
                true
            } else if body.peek(keyword::vars) {
                return Err(coded(
                    clause_span,
                    Code::Migrate,
                    format!("`vars {{ ... }}` was removed; write `values {{ ... }}` in {owner}"),
                ));
            } else {
                parse_site_clause(&body, &mut site, &owner, OwnerKind::Client)?
            };
            if !parsed_clause {
                return Err(unsupported(&body, OwnerKind::Client, &owner));
            }
            if let Some(first_child) = children.first() {
                let error = coded(
                    clause_span,
                    Code::Owner,
                    format!(
                        "{owner} preamble clauses must precede its child scopes and endpoints; move this clause above the first child"
                    ),
                );
                return Err(related(
                    error,
                    child_span(first_child),
                    "the first child is declared here",
                ));
            }
        }
        let base = base.ok_or_else(|| {
            coded(
                name.span(),
                Code::Owner,
                format!(
                    "{owner} requires exactly one fixed base; write `base \"https://...\";` in its preamble"
                ),
            )
        })?;
        validate_child_names(&children, &owner)?;
        Ok(Self {
            attrs,
            name,
            base,
            values,
            credentials,
            rate_registry,
            site,
            children,
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OwnerKind {
    Client,
    Scope,
    Endpoint,
}

fn child_span(child: &SourceChild) -> Span {
    match child {
        SourceChild::Scope(scope) => scope.name.span(),
        SourceChild::Endpoint(endpoint) => endpoint.name.span(),
    }
}

fn unsupported(input: ParseStream<'_>, kind: OwnerKind, owner: &str) -> syn::Error {
    if input.peek(keyword::revalidate) {
        return coded(
            input.span(),
            Code::Owner,
            format!(
                "`revalidate etag` is an endpoint-only clause and cannot appear in {owner}; move it into each endpoint that needs it"
            ),
        );
    }
    if input.peek(keyword::retry) {
        return coded(
            input.span(),
            Code::Owner,
            format!(
                "the `retry` DSL clause was removed from {owner}; install a managed `RetryPolicy` through `configure_runtime(|runtime| runtime.retry_policy(policy))`"
            ),
        );
    }
    if input.peek(keyword::secret) || input.peek(keyword::credential) {
        return coded(
            input.span(),
            Code::Owner,
            format!(
                "`secret` and `credential` declarations in {owner} belong inside the client `credentials {{ ... }}` block"
            ),
        );
    }
    if input.peek(keyword::host) {
        return coded(
            input.span(),
            Code::Migrate,
            format!("`host [...]` was removed; use `subdomain [...]` in {owner}"),
        );
    }
    match kind {
        OwnerKind::Client => coded(
            input.span(),
            Code::Owner,
            format!(
                "unsupported client entry in {owner}; expected a child scope or endpoint, or a `base`, `values`, `credentials`, `rate_limits`, `request`, `response`, `subdomain`, `path`, `query`, `headers`, `auth`, `timeout` or `rate_limit` clause"
            ),
        ),
        OwnerKind::Scope => {
            if input.peek(keyword::base)
                || input.peek(keyword::values)
                || input.peek(keyword::credentials)
                || input.peek(keyword::rate_limits)
            {
                return coded(
                    input.span(),
                    Code::Owner,
                    format!(
                        "this declaration is client-only and cannot appear in {owner}; move it to the client preamble (scope inputs are scope parameters)"
                    ),
                );
            }
            if input.peek(keyword::paginate)
                || input.peek(keyword::id)
                || input.peek(keyword::status)
                || input.peek(keyword::effect)
                || input.peek(keyword::absent)
                || input.peek(keyword::expose)
            {
                return coded(
                    input.span(),
                    Code::Owner,
                    format!(
                        "`id`, `status`, `effect`, `absent on`, `paginate` and `expose` are endpoint-only clauses and cannot appear in {owner}; move the clause into each endpoint that needs it"
                    ),
                );
            }
            coded(
                input.span(),
                Code::Owner,
                format!(
                    "unsupported scope entry in {owner}; expected a child scope or endpoint, or a `request`, `response`, `subdomain`, `path`, `query`, `headers`, `auth`, `timeout` or `rate_limit` clause"
                ),
            )
        }
        OwnerKind::Endpoint => {
            if input.peek(keyword::subdomain) {
                return coded(
                    input.span(),
                    Code::Owner,
                    format!(
                        "{owner} cannot own a subdomain clause; select DNS labels on a client or scope"
                    ),
                );
            }
            if input.peek(keyword::request) || input.peek(keyword::response) {
                return coded(
                    input.span(),
                    Code::Rep,
                    format!(
                        "representation exceptions of {owner} are inline: write `body name: Input via RequestRepresentation` or `-> Output via ResponseRepresentation`"
                    ),
                );
            }
            if input.peek(keyword::base)
                || input.peek(keyword::values)
                || input.peek(keyword::credentials)
                || input.peek(keyword::rate_limits)
            {
                return coded(
                    input.span(),
                    Code::Owner,
                    format!(
                        "this declaration is client-only and cannot appear on {owner}; move it to the client preamble (endpoint inputs are endpoint parameters)"
                    ),
                );
            }
            if input.peek(keyword::accept) {
                return coded(
                    input.span(),
                    Code::Migrate,
                    format!(
                        "endpoint `accept` was removed from {owner}; negotiation comes from the response representation's static `ACCEPT`, so select it with `-> Output via Representation`"
                    ),
                );
            }
            coded(
                input.span(),
                Code::Owner,
                format!(
                    "unsupported endpoint clause in {owner}; expected `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, `id`, `status`, `effect`, `absent on`, `paginate`, `revalidate` or `expose accepted_bytes`"
                ),
            )
        }
    }
}

fn child_starts(input: ParseStream<'_>) -> bool {
    let fork = input.fork();
    if fork.call(Attribute::parse_outer).is_err() {
        return false;
    }
    fork.peek(keyword::scope) || endpoint_starts(&fork)
}

fn parse_child(
    input: ParseStream<'_>,
    depth: usize,
    parent: Parent<'_>,
) -> syn::Result<SourceChild> {
    let fork = input.fork();
    let _ = fork.call(Attribute::parse_outer)?;
    if fork.peek(keyword::scope) {
        Ok(SourceChild::Scope(Box::new(parse_scope(
            input, depth, parent,
        )?)))
    } else {
        Ok(SourceChild::Endpoint(Box::new(parse_endpoint(
            input, parent,
        )?)))
    }
}

fn validate_child_names(children: &[SourceChild], owner: &str) -> syn::Result<()> {
    let mut names: HashMap<String, Span> = HashMap::new();
    for child in children {
        let name = match child {
            SourceChild::Scope(scope) => &scope.name,
            SourceChild::Endpoint(endpoint) => &endpoint.name,
        };
        let canonical = canonical_ident(name);
        if let Some(first) = names.insert(canonical.clone(), name.span()) {
            let error = coded(
                name.span(),
                Code::Generated,
                format!(
                    "child name `{canonical}` is declared more than once within {owner}; rename one child (generated names are never suffixed)"
                ),
            );
            return Err(related(
                error,
                first,
                "the first child with this name is here",
            ));
        }
    }
    Ok(())
}

fn parse_scope(
    input: ParseStream<'_>,
    depth: usize,
    parent: Parent<'_>,
) -> syn::Result<SourceScope> {
    if depth > MAX_SCOPE_DEPTH {
        return Err(coded(
            input.span(),
            Code::Owner,
            format!(
                "scope nesting in {} exceeds the maximum supported depth of 64; flatten the scope tree",
                parent.label
            ),
        ));
    }
    let attributes = input.call(Attribute::parse_outer)?;
    input.parse::<keyword::scope>()?;
    let name: Ident = input.parse()?;
    validate_snake_source_identifier(&name, "scope", parent.label)?;
    let logical = canonical_ident(&name);
    if matches!(logical.as_str(), "client" | "endpoint") || logical.starts_with("__concord") {
        return Err(coded(
            name.span(),
            Code::Name,
            format!(
                "`client`, `endpoint` and the `__concord` prefix are reserved owner names; rename scope `{logical}` in {}",
                parent.label
            ),
        ));
    }
    let path = format!("{}::{logical}", parent.path);
    let owner = scope_label(&path);
    let attrs = attrs_from(attributes, AttrSite::Declaration, "a scope", &owner)?;
    let parameters_content;
    parenthesized!(parameters_content in input);
    let parameters = parse_parameters(&parameters_content, "scope parameter", false, &owner)?.0;
    let body;
    braced!(body in input);
    let child_parent = Parent {
        path: &path,
        label: &owner,
    };
    let mut site = AttachmentSite::default();
    let mut children = Vec::new();
    while !body.is_empty() {
        if child_starts(&body) {
            children.push(parse_child(&body, depth + 1, child_parent)?);
            continue;
        }
        let clause_span = body.span();
        if !parse_site_clause(&body, &mut site, &owner, OwnerKind::Scope)? {
            return Err(unsupported(&body, OwnerKind::Scope, &owner));
        }
        if let Some(first_child) = children.first() {
            let error = coded(
                clause_span,
                Code::Owner,
                format!(
                    "{owner} preamble clauses must precede its child scopes and endpoints; move this clause above the first child"
                ),
            );
            return Err(related(
                error,
                child_span(first_child),
                "the first child is declared here",
            ));
        }
    }
    validate_child_names(&children, &owner)?;
    Ok(SourceScope {
        attrs,
        name,
        parameters,
        site,
        children,
    })
}

fn parse_endpoint(input: ParseStream<'_>, parent: Parent<'_>) -> syn::Result<SourceEndpoint> {
    let attributes = input.call(Attribute::parse_outer)?;
    let method_span = input.span();
    let method = parse_method(input, parent.label)?;
    let name: Ident = input.parse().map_err(|_| {
        input.error(format!(
            "[C3-OWNER] an endpoint method in {} is followed by the endpoint identifier; write `GET name(...) -> Output;`",
            parent.label
        ))
    })?;
    validate_snake_source_identifier(&name, "endpoint", parent.label)?;
    let path = format!("{}::{}", parent.path, canonical_ident(&name));
    let owner = endpoint_label(&path);
    let attrs = attrs_from(attributes, AttrSite::Declaration, "an endpoint", &owner)?;
    if !input.peek(syn::token::Paren) {
        return Err(coded(
            input.span(),
            Code::Owner,
            format!(
                "{owner} requires parentheses, including `()`; write `{} {}() -> Output;`",
                method_text(&method),
                canonical_ident(&name)
            ),
        ));
    }
    let parameters_content;
    parenthesized!(parameters_content in input);
    let (parameters, body) =
        parse_parameters(&parameters_content, "endpoint parameter", true, &owner)?;
    input.parse::<Token![->]>()?;
    let output = input.parse()?;
    let output_via = if input.peek(keyword::via) {
        input.parse::<keyword::via>()?;
        Some(input.parse()?)
    } else if input.peek(keyword::using) {
        return Err(coded(
            input.span(),
            Code::Migrate,
            format!(
                "result `using Representation` was replaced by `via Representation` in {owner}"
            ),
        ));
    } else {
        None
    };
    let mut endpoint = SourceEndpoint {
        attrs,
        method,
        method_span,
        name,
        parameters,
        body,
        output,
        output_via,
        site: AttachmentSite::default(),
        id: None,
        status: None,
        effect: None,
        absence: None,
        pagination: None,
        revalidate_etag: None,
        expose_accepted_bytes: None,
    };
    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        return Ok(endpoint);
    }
    let clauses;
    braced!(clauses in input);
    while !clauses.is_empty() {
        let key_span = clauses.span();
        if clauses.peek(keyword::revalidate) {
            clauses.parse::<keyword::revalidate>()?;
            if !clauses.peek(keyword::etag) {
                return Err(coded(
                    clauses.span(),
                    Code::Owner,
                    format!(
                        "the only supported revalidation mode of {owner} is ETag; write `revalidate etag;`"
                    ),
                ));
            }
            clauses.parse::<keyword::etag>()?;
            clauses.parse::<Token![;]>().map_err(|_| {
                coded(
                    clauses.span(),
                    Code::Owner,
                    format!("`revalidate etag` in {owner} ends with `;`; write `revalidate etag;`"),
                )
            })?;
            insert_clause(
                &mut endpoint.revalidate_etag,
                Clause {
                    key_span,
                    value: (),
                },
                "revalidate etag",
                &owner,
            )?;
        } else if clauses.peek(keyword::paginate) {
            let value = parse_pagination(&clauses, &owner)?;
            insert_clause(
                &mut endpoint.pagination,
                Clause { key_span, value },
                "paginate",
                &owner,
            )?;
        } else if clauses.peek(keyword::effect) {
            clauses.parse::<keyword::effect>()?;
            let value = if clauses.peek(keyword::safe) {
                clauses.parse::<keyword::safe>()?;
                EffectDecl::Safe
            } else if clauses.peek(keyword::idempotent) {
                clauses.parse::<keyword::idempotent>()?;
                EffectDecl::Idempotent
            } else if clauses.peek(keyword::non_idempotent) {
                clauses.parse::<keyword::non_idempotent>()?;
                EffectDecl::NonIdempotent
            } else {
                return Err(coded(
                    clauses.span(),
                    Code::Owner,
                    format!(
                        "`effect` in {owner} requires `safe`, `idempotent`, or `non_idempotent`; write `effect idempotent;`"
                    ),
                ));
            };
            clauses.parse::<Token![;]>()?;
            insert_clause(
                &mut endpoint.effect,
                Clause { key_span, value },
                "effect",
                &owner,
            )?;
        } else if clauses.peek(keyword::status) {
            clauses.parse::<keyword::status>()?;
            let value = parse_status(&clauses, &owner)?;
            clauses.parse::<Token![;]>()?;
            insert_clause(
                &mut endpoint.status,
                Clause { key_span, value },
                "status",
                &owner,
            )?;
        } else if clauses.peek(keyword::absent) {
            clauses.parse::<keyword::absent>()?;
            clauses.parse::<keyword::on>().map_err(|_| {
                clauses.error(format!(
                    "[C3-REP] expected `absent on STATUS (, STATUS)*;` in {owner}; write `absent on 404;`"
                ))
            })?;
            let mut codes = BTreeSet::new();
            loop {
                let literal: syn::LitInt = clauses.parse()?;
                let code = parse_status_code(&literal, &owner)?;
                if code < 200 {
                    return Err(coded(
                        literal.span(),
                        Code::Rep,
                        format!(
                            "an expected-absence status of {owner} must be a final status (200..=599); write a status such as `absent on 404;`"
                        ),
                    ));
                }
                if !codes.insert(code) {
                    return Err(coded(
                        literal.span(),
                        Code::Rep,
                        format!(
                            "duplicate expected-absence status {code} in {owner}; list each status once"
                        ),
                    ));
                }
                if clauses.peek(Token![,]) {
                    clauses.parse::<Token![,]>()?;
                    continue;
                }
                break;
            }
            clauses.parse::<Token![;]>()?;
            insert_clause(
                &mut endpoint.absence,
                Clause {
                    key_span,
                    value: codes.into_iter().collect(),
                },
                "absent on",
                &owner,
            )?;
        } else if clauses.peek(keyword::expose) {
            clauses.parse::<keyword::expose>()?;
            if !clauses.peek(keyword::accepted_bytes) {
                return Err(coded(
                    clauses.span(),
                    Code::Owner,
                    format!("the only supported exposure of {owner} is `expose accepted_bytes;`"),
                ));
            }
            clauses.parse::<keyword::accepted_bytes>()?;
            clauses.parse::<Token![;]>()?;
            if let Some(first) = endpoint.expose_accepted_bytes {
                let error = coded(
                    key_span,
                    Code::Owner,
                    format!(
                        "duplicate `expose accepted_bytes` clause for {owner}; singleton clauses are not last-writer-wins, so keep exactly one"
                    ),
                );
                return Err(related(error, first, "the first exposure is here"));
            }
            endpoint.expose_accepted_bytes = Some(key_span);
        } else if clauses.peek(keyword::id) {
            clauses.parse::<keyword::id>()?;
            let value: LitStr = clauses.parse().map_err(|_| {
                clauses.error(format!(
                    "[C3-GENERATED] the `id` of {owner} requires one string literal; write `id \"catalog.items.get\";`"
                ))
            })?;
            clauses.parse::<Token![;]>()?;
            validate_endpoint_id(&value, &owner)?;
            insert_clause(&mut endpoint.id, Clause { key_span, value }, "id", &owner)?;
        } else if !parse_site_clause(&clauses, &mut endpoint.site, &owner, OwnerKind::Endpoint)? {
            return Err(unsupported(&clauses, OwnerKind::Endpoint, &owner));
        }
    }
    Ok(endpoint)
}

fn parse_site_clause(
    input: ParseStream<'_>,
    site: &mut AttachmentSite,
    owner: &str,
    kind: OwnerKind,
) -> syn::Result<bool> {
    let key_span = input.span();
    if input.peek(keyword::subdomain) {
        if kind == OwnerKind::Endpoint {
            return Ok(false);
        }
        let value = parse_subdomain(input, owner)?;
        insert_clause(
            &mut site.subdomain,
            Clause { key_span, value },
            "subdomain",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::path) {
        let value = parse_path(input, owner)?;
        insert_clause(&mut site.path, Clause { key_span, value }, "path", owner)?;
        return Ok(true);
    }
    if input.peek(keyword::query) {
        let value = parse_key_block(input, PlacementFamily::Query, owner)?;
        insert_clause(&mut site.query, Clause { key_span, value }, "query", owner)?;
        return Ok(true);
    }
    if input.peek(keyword::headers) {
        let value = parse_key_block(input, PlacementFamily::Header, owner)?;
        insert_clause(
            &mut site.headers,
            Clause { key_span, value },
            "headers",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::auth) {
        let value = parse_auth(input, owner)?;
        insert_clause(&mut site.auth, Clause { key_span, value }, "auth", owner)?;
        return Ok(true);
    }
    if input.peek(keyword::timeout) {
        let value = parse_timeout(input, owner)?;
        insert_clause(
            &mut site.timeout,
            Clause { key_span, value },
            "timeout",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::rate_limit) {
        let value = parse_rate_attachment(input, owner)?;
        site.rate_limits.push(Clause { key_span, value });
        return Ok(true);
    }
    if kind != OwnerKind::Endpoint
        && (input.peek(keyword::request) || input.peek(keyword::response))
    {
        let request = input.peek(keyword::request);
        if request {
            input.parse::<keyword::request>()?;
        } else {
            input.parse::<keyword::response>()?;
        }
        let direction = if request { "request" } else { "response" };
        let value = if input.peek(keyword::explicit) {
            let token: keyword::explicit = input.parse()?;
            if kind == OwnerKind::Client {
                return Err(coded(
                    token.span(),
                    Code::Rep,
                    format!(
                        "`explicit` representation fences are scope-only; {owner} simply omits the `{direction}` default"
                    ),
                ));
            }
            RepresentationSelection::Explicit
        } else {
            RepresentationSelection::Select(Box::new(input.parse().map_err(|_| {
                input.error(format!(
                    "[C3-REP] `{direction}` in {owner} requires a representation type or `explicit`; write `{direction} Json;`"
                ))
            })?))
        };
        input.parse::<Token![;]>()?;
        let slot = if request {
            &mut site.request
        } else {
            &mut site.response
        };
        insert_clause(slot, Clause { key_span, value }, direction, owner)?;
        return Ok(true);
    }
    Ok(false)
}

fn parse_status(input: ParseStream<'_>, owner: &str) -> syn::Result<StatusDecl> {
    if input.peek(keyword::success) {
        input.parse::<keyword::success>()?;
        return Ok(StatusDecl::Success);
    }
    let mut codes = BTreeSet::new();
    if input.peek(syn::token::Bracket) {
        let content;
        syn::bracketed!(content in input);
        if content.is_empty() {
            return Err(coded(
                content.span(),
                Code::Rep,
                format!(
                    "an explicit status list of {owner} cannot be empty; write `status [200, 201];` or `status success;`"
                ),
            ));
        }
        while !content.is_empty() {
            parse_status_item(&content, &mut codes, owner)?;
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
    } else {
        parse_status_item(input, &mut codes, owner)?;
    }
    Ok(StatusDecl::Codes(codes.into_iter().collect()))
}

fn parse_status_item(
    input: ParseStream<'_>,
    codes: &mut BTreeSet<u16>,
    owner: &str,
) -> syn::Result<()> {
    let first: syn::LitInt = input.parse().map_err(|_| {
        input.error(format!(
            "[C3-REP] a status item of {owner} requires a three-digit HTTP status; write `200` or `200..=204`"
        ))
    })?;
    let start = parse_status_code(&first, owner)?;
    let end = if input.peek(Token![..=]) {
        input.parse::<Token![..=]>()?;
        let last: syn::LitInt = input.parse()?;
        let end = parse_status_code(&last, owner)?;
        if end < start {
            return Err(coded(
                last.span(),
                Code::Rep,
                format!(
                    "an inclusive status range of {owner} cannot descend; write the lower status first (`{end}..={start}`)"
                ),
            ));
        }
        end
    } else {
        start
    };
    if start < 200 {
        return Err(coded(
            first.span(),
            Code::Rep,
            format!(
                "informational statuses (1xx) are not a final accepted outcome of {owner}; accept final statuses in 200..=599"
            ),
        ));
    }
    for code in start..=end {
        if !codes.insert(code) {
            return Err(coded(
                first.span(),
                Code::Rep,
                format!(
                    "accepted status items of {owner} cannot overlap or repeat (status {code} is listed twice); list each status once"
                ),
            ));
        }
    }
    Ok(())
}

fn parse_status_code(literal: &syn::LitInt, owner: &str) -> syn::Result<u16> {
    if literal.suffix().is_empty() && literal.base10_digits().len() == 3 {
        let value = literal.base10_parse::<u16>()?;
        if (100..=599).contains(&value) {
            return Ok(value);
        }
    }
    Err(coded(
        literal.span(),
        Code::Rep,
        format!(
            "HTTP status in {owner} must be a three-digit value in 100..=599; write an unsuffixed status such as `404`"
        ),
    ))
}

fn endpoint_starts(input: ParseStream<'_>) -> bool {
    if input.peek(keyword::METHOD) {
        return true;
    }
    input.fork().parse::<Ident>().is_ok_and(|ident| {
        matches!(
            ident.to_string().as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "TRACE" | "CONNECT"
        )
    })
}

fn method_text(method: &MethodKind) -> String {
    match method {
        MethodKind::Get => "GET".to_owned(),
        MethodKind::Post => "POST".to_owned(),
        MethodKind::Put => "PUT".to_owned(),
        MethodKind::Patch => "PATCH".to_owned(),
        MethodKind::Delete => "DELETE".to_owned(),
        MethodKind::Head => "HEAD".to_owned(),
        MethodKind::Options => "OPTIONS".to_owned(),
        MethodKind::Custom(literal) => format!("METHOD({:?})", literal.value()),
    }
}

fn parse_method(input: ParseStream<'_>, parent: &str) -> syn::Result<MethodKind> {
    if input.peek(keyword::METHOD) {
        input.parse::<keyword::METHOD>()?;
        let content;
        parenthesized!(content in input);
        let literal: LitStr = content.parse()?;
        if !content.is_empty() {
            return Err(coded(
                content.span(),
                Code::Owner,
                format!(
                    "`METHOD` accepts exactly one string literal (endpoint in {parent}); write `METHOD(\"PURGE\")`"
                ),
            ));
        }
        let value = literal.value();
        if value.is_empty() || value.len() > 128 || Method::from_bytes(value.as_bytes()).is_err() {
            return Err(coded(
                literal.span(),
                Code::Owner,
                format!(
                    "a custom method must be a valid HTTP token of at most 128 bytes (endpoint in {parent}); write a token such as `METHOD(\"PURGE\")`"
                ),
            ));
        }
        if value.eq_ignore_ascii_case("CONNECT") || value.eq_ignore_ascii_case("TRACE") {
            return Err(coded(
                literal.span(),
                Code::Owner,
                format!(
                    "CONNECT and TRACE are unsupported, including custom spelling (endpoint in {parent}); declare a supported method"
                ),
            ));
        }
        return Ok(MethodKind::Custom(literal));
    }
    let method: Ident = input.parse()?;
    match method.to_string().as_str() {
        "GET" => Ok(MethodKind::Get),
        "POST" => Ok(MethodKind::Post),
        "PUT" => Ok(MethodKind::Put),
        "PATCH" => Ok(MethodKind::Patch),
        "DELETE" => Ok(MethodKind::Delete),
        "HEAD" => Ok(MethodKind::Head),
        "OPTIONS" => Ok(MethodKind::Options),
        _ => Err(coded(
            method.span(),
            Code::Owner,
            format!(
                "CONNECT and TRACE are unsupported; Concord does not expose tunnel, trace or upgrade semantics (endpoint in {parent}); declare GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS or `METHOD(\"...\")`"
            ),
        )),
    }
}
