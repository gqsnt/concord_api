use std::collections::{BTreeSet, HashSet};

use http::Method;
use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Ident, LitStr, Token, braced, parenthesized};

use super::ast::{
    AttachmentSite, Clause, EffectDecl, MethodKind, RepresentationOperation, SourceApi,
    SourceChild, SourceClient, SourceEndpoint, SourceScope, StatusDecl,
};
use super::credentials::{parse_auth, parse_credentials};
use super::diagnostics::{
    SpanError, insert_clause, merge_auth, validate_base, validate_contract_id,
    validate_snake_source_identifier,
};
use super::keyword;
use super::pagination::parse_pagination;
use super::rate_limits::{parse_rate_limit, parse_rate_limits};
use super::request::{parse_headers, parse_path, parse_query, parse_subdomain, parse_timeout};
use super::values::{parse_declaration_attributes, parse_parameters, parse_values};
use crate::semantic::surface::canonical_ident;

const UNSUPPORTED_CLIENT_ENTRY: &str = "unsupported client entry; expected a child scope, HTTP endpoint, or `base`, `values`, `credentials`, `rate_limits`, `request`, `response`, `subdomain`, `path`, `query`, `headers`, `auth`, `timeout`, or `rate_limit` clause";
const UNSUPPORTED_SCOPE_ENTRY: &str = "unsupported scope entry; expected a child scope, HTTP endpoint, or `request`, `response`, `subdomain`, `path`, `query`, `headers`, `auth`, `timeout`, or `rate_limit` clause";
const UNSUPPORTED_ENDPOINT_CLAUSE: &str = "unsupported endpoint clause; expected `id`, an attachment, `status`, `effect`, `paginate`, or `expose accepted_bytes`";

pub(crate) fn parse(input: TokenStream) -> syn::Result<SourceApi> {
    syn::parse2(input)
}

impl Parse for SourceApi {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(
                input.error("an `api!` invocation requires exactly one `client` declaration")
            );
        }

        let fork = input.fork();
        let _ = fork.call(Attribute::parse_outer)?;
        if !fork.peek(keyword::client) {
            if fork.peek(keyword::scope) || endpoint_starts(&fork) {
                return Err(input
                    .error("the `client` must be the first declaration in an `api!` invocation"));
            }
            if is_client_configuration_start(input) {
                return Err(
                    input.error("client configuration must be declared inside the `client` block")
                );
            }
            return Err(input
                .error("an `api!` invocation must begin with exactly one `client` declaration"));
        }

        let client: SourceClient = input.parse()?;
        if !input.is_empty() {
            if input.peek(keyword::client) {
                return Err(input.error(
                    "an `api!` invocation accepts exactly one `client`; declare additional clients in separate `api!` invocations",
                ));
            }
            if is_client_configuration_start(input) {
                return Err(
                    input.error("client configuration must be declared inside the `client` block")
                );
            }
            return Err(input.error(
                "v1 children after the `client` block were removed; move every scope and endpoint inside its owning `client { ... }` block",
            ));
        }
        Ok(Self { client })
    }
}

impl Parse for SourceClient {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attributes = parse_declaration_attributes(input, "client")?;
        input.parse::<keyword::client>()?;
        let name: Ident = input.parse()?;
        let body;
        braced!(body in input);
        let owner = format!("client `{}`", canonical_ident(&name));
        let mut base = None;
        let mut values = None;
        let mut credentials = None;
        let mut rate_limits = None;
        let mut attachment_site = AttachmentSite::default();
        let mut children = Vec::new();

        while !body.is_empty() {
            if child_starts(&body) {
                children.push(parse_child(&body, 1)?);
                continue;
            }
            if body.peek(keyword::base) {
                let key_span = body.span();
                body.parse::<keyword::base>()?;
                let value: LitStr = body.parse()?;
                body.parse::<Token![;]>()?;
                validate_base(&value)?;
                insert_clause(&mut base, Clause { key_span, value }, "base", &owner)?;
                continue;
            }
            if body.peek(keyword::values) {
                let key_span = body.span();
                let value = parse_values(&body)?;
                insert_clause(&mut values, Clause { key_span, value }, "values", &owner)?;
                continue;
            }
            if body.peek(keyword::vars) {
                return Err(body.error("v1 `vars { ... }` was removed; write `values { ... }`"));
            }
            if body.peek(keyword::credentials) {
                let key_span = body.span();
                let value = parse_credentials(&body)?;
                insert_clause(
                    &mut credentials,
                    Clause { key_span, value },
                    "credentials",
                    &owner,
                )?;
                continue;
            }
            if body.peek(keyword::rate_limits) {
                let key_span = body.span();
                let value = parse_rate_limits(&body)?;
                insert_clause(
                    &mut rate_limits,
                    Clause { key_span, value },
                    "rate_limits",
                    &owner,
                )?;
                continue;
            }
            if body.peek(keyword::retry) {
                return Err(removed_retry_diagnostic(&body));
            }
            if parse_representation_clause(&body, &mut attachment_site, false, &owner)? {
                continue;
            }
            if parse_attachment_site_clause(&body, &mut attachment_site, true, &owner)? {
                continue;
            }
            if let Some(error) = retired_clause_diagnostic(&body) {
                return Err(error);
            }
            return Err(body.error(UNSUPPORTED_CLIENT_ENTRY));
        }
        let base = base.ok_or_else(|| {
            name.span()
                .error("a client requires exactly one fixed `base \"https://...\";` clause")
        })?;

        if attachment_site.timeout.is_none() {
            return Err(name.span().error(
                "a v2.1 client must explicitly declare one positive finite `timeout DURATION;`",
            ));
        }
        validate_child_names(&children, "client")?;

        Ok(Self {
            attributes,
            name,
            base,
            values,
            credentials,
            rate_limits,
            attachment_site,
            children,
        })
    }
}

fn child_starts(input: ParseStream<'_>) -> bool {
    let fork = input.fork();
    if fork.call(Attribute::parse_outer).is_err() {
        return false;
    }
    fork.peek(keyword::scope) || endpoint_starts(&fork)
}

fn parse_child(input: ParseStream<'_>, depth: usize) -> syn::Result<SourceChild> {
    let fork = input.fork();
    let _ = fork.call(Attribute::parse_outer)?;
    if fork.peek(keyword::scope) {
        Ok(SourceChild::Scope(Box::new(parse_scope(input, depth)?)))
    } else if endpoint_starts(&fork) {
        Ok(SourceChild::Endpoint(parse_endpoint(input)?))
    } else {
        Err(input.error("expected a scope or endpoint declaration"))
    }
}

fn validate_child_names(children: &[SourceChild], owner: &str) -> syn::Result<()> {
    let mut names = HashSet::new();
    for child in children {
        let name = match child {
            SourceChild::Scope(scope) => &scope.name,
            SourceChild::Endpoint(endpoint) => &endpoint.name,
        };
        let canonical = canonical_ident(name);
        if !names.insert(canonical.clone()) {
            return Err(name.span().error(format!(
                "child name `{canonical}` is declared more than once within this {owner}"
            )));
        }
    }
    Ok(())
}

fn parse_scope(input: ParseStream<'_>, depth: usize) -> syn::Result<SourceScope> {
    if depth > 64 {
        return Err(input
            .span()
            .error("scope nesting exceeds the maximum supported depth of 64"));
    }
    let attributes = parse_declaration_attributes(input, "scope")?;
    input.parse::<keyword::scope>()?;
    let name: Ident = input.parse()?;
    validate_snake_source_identifier(&name, "scope")?;
    let parameters_content;
    parenthesized!(parameters_content in input);
    let parameters = parse_parameters(&parameters_content, "scope", false)?.0;

    let body;
    braced!(body in input);
    let owner = format!("scope `{}`", canonical_ident(&name));
    let mut attachment_site = AttachmentSite::default();
    let mut children = Vec::new();
    while !body.is_empty() {
        if child_starts(&body) {
            children.push(parse_child(&body, depth + 1)?);
            continue;
        }
        if parse_attachment_site_clause(&body, &mut attachment_site, true, &owner)? {
            continue;
        }
        if parse_representation_clause(&body, &mut attachment_site, true, &owner)? {
            continue;
        }
        if body.peek(keyword::retry) {
            return Err(removed_retry_diagnostic(&body));
        }
        if is_client_only_declaration_start(&body) {
            return Err(body.error("this declaration is client-only and cannot appear in a scope"));
        }
        if body.peek(keyword::paginate) || body.peek(keyword::id) {
            return Err(body.error("`paginate` and `id` are endpoint-only clauses"));
        }
        if let Some(error) = retired_clause_diagnostic(&body) {
            return Err(error);
        }
        return Err(body.error(UNSUPPORTED_SCOPE_ENTRY));
    }
    validate_child_names(&children, "scope")?;

    Ok(SourceScope {
        attributes,
        name,
        parameters,
        attachment_site,
        children,
    })
}

fn parse_endpoint(input: ParseStream<'_>) -> syn::Result<SourceEndpoint> {
    let attributes = parse_declaration_attributes(input, "endpoint")?;
    let method = parse_method(input)?;
    let name: Ident = input.parse().map_err(|_| {
        input.error("an endpoint method must be followed by an endpoint identifier")
    })?;
    validate_snake_source_identifier(&name, "endpoint")?;
    if !input.peek(syn::token::Paren) {
        return Err(input.error("every endpoint declaration requires parentheses, including `()`"));
    }
    let parameters_content;
    parenthesized!(parameters_content in input);
    let (parameters, body) = parse_parameters(&parameters_content, "endpoint", true)?;

    input.parse::<Token![->]>()?;
    let output = input.parse()?;
    let response_spec = if input.peek(keyword::via) {
        input.parse::<keyword::via>()?;
        Some(input.parse()?)
    } else if input.peek(keyword::using) {
        return Err(input.error(
            "v2.1 replaced result `using Representation` with inline `via Representation`",
        ));
    } else {
        None
    };

    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        Ok(SourceEndpoint {
            attributes,
            method,
            name,
            parameters,
            body,
            attachment_site: AttachmentSite::default(),
            output,
            response_spec,
            pagination: None,
            contract_id: None,
            status: None,
            effect: None,
            expose_accepted_bytes: None,
        })
    } else {
        let clauses;
        braced!(clauses in input);
        let owner = format!("endpoint `{}`", canonical_ident(&name));
        let mut attachment_site = AttachmentSite::default();
        let mut pagination = None;
        let mut contract_id = None;
        let mut status = None;
        let mut effect = None;
        let mut expose_accepted_bytes: Option<proc_macro2::Span> = None;
        while !clauses.is_empty() {
            if clauses.peek(keyword::paginate) {
                let key_span = clauses.span();
                let value = parse_pagination(&clauses)?;
                insert_clause(
                    &mut pagination,
                    Clause { key_span, value },
                    "paginate",
                    &owner,
                )?;
                continue;
            }
            if clauses.peek(keyword::effect) {
                let key_span = clauses.span();
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
                    return Err(clauses
                        .error("`effect` requires `safe`, `idempotent`, or `non_idempotent`"));
                };
                clauses.parse::<Token![;]>()?;
                insert_clause(&mut effect, Clause { key_span, value }, "effect", &owner)?;
                continue;
            }
            if clauses.peek(keyword::status) {
                let key_span = clauses.span();
                clauses.parse::<keyword::status>()?;
                let value = parse_status(&clauses)?;
                clauses.parse::<Token![;]>()?;
                insert_clause(&mut status, Clause { key_span, value }, "status", &owner)?;
                continue;
            }
            if clauses.peek(keyword::accept) {
                return Err(clauses.error(
                    "v2.1 removed endpoint `accept`; select response negotiation with `-> Output via ResponseRepresentation` or an inherited `response ResponseRepresentation;`",
                ));
            }
            if clauses.peek(keyword::expose) {
                let span = clauses.span();
                clauses.parse::<keyword::expose>()?;
                if !clauses.peek(keyword::accepted_bytes) {
                    return Err(
                        clauses.error("the only supported exposure is `expose accepted_bytes;`")
                    );
                }
                clauses.parse::<keyword::accepted_bytes>()?;
                clauses.parse::<Token![;]>()?;
                if let Some(first) = expose_accepted_bytes {
                    let mut error = span.error("duplicate `expose accepted_bytes` clause");
                    error.combine(first.error("the first exposure is here"));
                    return Err(error);
                }
                expose_accepted_bytes = Some(span);
                continue;
            }
            if clauses.peek(keyword::id) {
                let key_span = clauses.span();
                clauses.parse::<keyword::id>()?;
                let value: LitStr = clauses
                    .parse()
                    .map_err(|_| clauses.error("an endpoint `id` requires one string literal"))?;
                clauses.parse::<Token![;]>()?;
                validate_contract_id(&value)?;
                insert_clause(&mut contract_id, Clause { key_span, value }, "id", &owner)?;
                continue;
            }
            if parse_attachment_site_clause(&clauses, &mut attachment_site, true, &owner)? {
                continue;
            }
            if clauses.peek(keyword::request) || clauses.peek(keyword::response) {
                return Err(clauses.error(
                    "endpoint representation exceptions are inline: use `body name: Input via RequestRepresentation` or `-> Output via ResponseRepresentation`",
                ));
            }
            if clauses.peek(keyword::retry) {
                return Err(removed_retry_diagnostic(&clauses));
            }
            if is_client_only_declaration_start(&clauses) {
                return Err(clauses
                    .error("this declaration is client-only and cannot appear on an endpoint"));
            }
            if let Some(error) = retired_clause_diagnostic(&clauses) {
                return Err(error);
            }
            return Err(clauses.error(UNSUPPORTED_ENDPOINT_CLAUSE));
        }
        Ok(SourceEndpoint {
            attributes,
            method,
            name,
            parameters,
            body,
            attachment_site,
            output,
            response_spec,
            pagination,
            contract_id,
            status,
            effect,
            expose_accepted_bytes,
        })
    }
}

fn parse_status(input: ParseStream<'_>) -> syn::Result<StatusDecl> {
    if input.peek(keyword::success) {
        input.parse::<keyword::success>()?;
        return Ok(StatusDecl::Success);
    }

    let mut codes = BTreeSet::new();
    if input.peek(syn::token::Bracket) {
        let content;
        syn::bracketed!(content in input);
        if content.is_empty() {
            return Err(content.error("an explicit status list cannot be empty"));
        }
        while !content.is_empty() {
            parse_status_item(&content, &mut codes)?;
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
    } else {
        parse_status_item(input, &mut codes)?;
    }
    Ok(StatusDecl::Codes(codes.into_iter().collect()))
}

fn parse_status_item(input: ParseStream<'_>, codes: &mut BTreeSet<u16>) -> syn::Result<()> {
    let first: syn::LitInt = input
        .parse()
        .map_err(|_| input.error("a status item requires a three-digit HTTP status"))?;
    let start = parse_status_code(&first)?;
    let end = if input.peek(Token![..=]) {
        input.parse::<Token![..=]>()?;
        let last: syn::LitInt = input.parse().map_err(|_| {
            input.error("an inclusive status range requires a three-digit upper bound")
        })?;
        let end = parse_status_code(&last)?;
        if end < start {
            return Err(last
                .span()
                .error("an inclusive status range cannot descend"));
        }
        end
    } else {
        start
    };
    for code in start..=end {
        if !codes.insert(code) {
            return Err(first
                .span()
                .error("accepted status items cannot overlap or repeat"));
        }
    }
    Ok(())
}

fn parse_status_code(literal: &syn::LitInt) -> syn::Result<u16> {
    if literal.suffix().is_empty() && literal.base10_digits().len() == 3 {
        let value = literal.base10_parse::<u16>()?;
        if (100..=599).contains(&value) {
            return Ok(value);
        }
    }
    Err(literal
        .span()
        .error("HTTP status must be a three-digit value in 100..=599"))
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

fn parse_method(input: ParseStream<'_>) -> syn::Result<MethodKind> {
    if input.peek(keyword::METHOD) {
        input.parse::<keyword::METHOD>()?;
        let content;
        parenthesized!(content in input);
        let literal: LitStr = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("`METHOD` accepts exactly one string literal"));
        }
        Method::from_bytes(literal.value().as_bytes()).map_err(|_| {
            literal
                .span()
                .error("custom endpoint method must be a valid HTTP method token")
        })?;
        if matches!(literal.value().as_str(), "CONNECT" | "TRACE") {
            return Err(literal
                .span()
                .error("CONNECT and TRACE are unsupported, including custom spelling"));
        }
        return Ok(MethodKind::Custom(literal));
    }

    let method: Ident = input.parse().map_err(|_| {
        input.error(
            "unsupported endpoint declaration; expected a standard method or `METHOD(\"TOKEN\")`",
        )
    })?;
    match method.to_string().as_str() {
        "GET" => Ok(MethodKind::Get),
        "POST" => Ok(MethodKind::Post),
        "PUT" => Ok(MethodKind::Put),
        "PATCH" => Ok(MethodKind::Patch),
        "DELETE" => Ok(MethodKind::Delete),
        "HEAD" => Ok(MethodKind::Head),
        "OPTIONS" => Ok(MethodKind::Options),
        "TRACE" => Err(method
            .span()
            .error("TRACE is unsupported, including custom spelling")),
        "CONNECT" => Err(method.span().error(
            "CONNECT is unsupported because Concord does not expose tunnel or upgrade semantics",
        )),
        _ => Err(method.span().error(
            "unsupported endpoint method; use a standard method keyword or `METHOD(\"TOKEN\")`",
        )),
    }
}

fn removed_retry_diagnostic(input: ParseStream<'_>) -> syn::Error {
    input.error(
        "the `retry` DSL clause was removed; install a managed `RetryPolicy` with the generated builder `.retry(policy)`",
    )
}

fn retired_clause_diagnostic(input: ParseStream<'_>) -> Option<syn::Error> {
    if input.peek(keyword::secret) {
        return Some(input.error(
            "a `secret` declaration is valid only inside the client `credentials { ... }` block",
        ));
    }
    if input.peek(keyword::credential) {
        return Some(input.error(
            "a `credential` declaration is valid only inside the client `credentials { ... }` block",
        ));
    }
    if input.peek(Token![default]) {
        return Some(input.error(
            "`default { ... }` is not DSL syntax; use `#[default]` on a value in `values { ... }` or a scope or endpoint parameter list",
        ));
    }

    let spelling = input.fork().parse::<Ident>().ok()?;
    let message = match canonical_ident(&spelling).as_str() {
        "defaults" => "`defaults { ... }` is not DSL syntax; use `#[default]` on individual values",
        "policies" => {
            "`policies { ... }` is not DSL syntax; define rate-limit profiles in the client `rate_limits { ... }` block and install retry policy through the generated builder"
        }
        "behaviors" | "behavior" => {
            "behavior profiles are not DSL syntax; configure decision gates, completion observation, and runtime policy through the generated builder"
        }
        "prefix" => {
            "`prefix` is not DSL syntax; use `path [...]` for path atoms or `subdomain [...]` for generated DNS labels"
        }
        "params" => {
            "`params { ... }` is not DSL syntax; declare values in a scope or endpoint parameter list and shape query parameters with `query { ... }`"
        }
        "profile" => {
            "`profile` is not a DSL keyword; define a rate-limit profile with `rate_limits { NAME(formals...) { key [...]; allow N per D; } }`"
        }
        "access_token" => {
            "`access_token` is not DSL syntax; declare a secret and a bearer credential inside `credentials { ... }`"
        }
        _ => return None,
    };
    Some(spelling.span().error(message))
}

fn parse_attachment_site_clause(
    input: ParseStream<'_>,
    attachment_site: &mut AttachmentSite,
    include_rate_limit: bool,
    owner: &str,
) -> syn::Result<bool> {
    if input.peek(keyword::subdomain) {
        let key_span = input.span();
        let value = parse_subdomain(input)?;
        insert_clause(
            &mut attachment_site.subdomain,
            Clause { key_span, value },
            "subdomain",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::host) {
        return Err(input.error(
            "v1 `host [...]` was removed; use composable `subdomain [...]`, `subdomain replace [...]`, or `subdomain off`",
        ));
    }
    if input.peek(keyword::path) {
        let key_span = input.span();
        let value = parse_path(input)?;
        insert_clause(
            &mut attachment_site.path,
            Clause { key_span, value },
            "path",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::query) {
        let key_span = input.span();
        let value = parse_query(input)?;
        insert_clause(
            &mut attachment_site.query,
            Clause { key_span, value },
            "query",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::headers) {
        let key_span = input.span();
        let value = parse_headers(input)?;
        insert_clause(
            &mut attachment_site.headers,
            Clause { key_span, value },
            "headers",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::auth) {
        merge_auth(&mut attachment_site.auth, parse_auth(input)?, owner)?;
        return Ok(true);
    }
    if input.peek(keyword::timeout) {
        let key_span = input.span();
        let value = parse_timeout(input)?;
        insert_clause(
            &mut attachment_site.timeout,
            Clause { key_span, value },
            "timeout",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::rate_limit) {
        if !include_rate_limit {
            return Err(input.error("`rate_limit` is not valid in this contract location"));
        }
        let key_span = input.span();
        let value = parse_rate_limit(input)?;
        if matches!(&value, super::ast::RateLimitAttachmentDecl::Inline(_)) {
            if let Some(first) = attachment_site.rate_limits.iter().find(|clause| {
                matches!(clause.value, super::ast::RateLimitAttachmentDecl::Inline(_))
            }) {
                let mut error = key_span.error(format!(
                    "a {owner} permits at most one anonymous `rate_limit {{ ... }}` attachment; use a rate-limit profile for additional policies"
                ));
                error.combine(
                    first
                        .key_span
                        .error("the first anonymous rate-limit attachment is here"),
                );
                return Err(error);
            }
        }
        attachment_site.rate_limits.push(Clause { key_span, value });
        return Ok(true);
    }
    Ok(false)
}

fn parse_representation_clause(
    input: ParseStream<'_>,
    attachment_site: &mut AttachmentSite,
    allow_explicit: bool,
    owner: &str,
) -> syn::Result<bool> {
    let (direction, target) = if input.peek(keyword::request) {
        input.parse::<keyword::request>()?;
        ("request", &mut attachment_site.request)
    } else if input.peek(keyword::response) {
        input.parse::<keyword::response>()?;
        ("response", &mut attachment_site.response)
    } else {
        return Ok(false);
    };
    let key_span = input.span();
    let value = if input.peek(keyword::explicit) {
        let explicit: keyword::explicit = input.parse()?;
        if !allow_explicit {
            return Err(explicit
                .span
                .error("`explicit` representation fences are scope-only"));
        }
        RepresentationOperation::Explicit
    } else {
        RepresentationOperation::Select(Box::new(input.parse().map_err(|_| {
            input.error(format!(
                "`{direction}` requires a representation type or `explicit`"
            ))
        })?))
    };
    input.parse::<Token![;]>()?;
    insert_clause(target, Clause { key_span, value }, direction, owner)?;
    Ok(true)
}

fn is_client_configuration_start(input: ParseStream<'_>) -> bool {
    is_client_only_declaration_start(input)
        || input.peek(keyword::subdomain)
        || input.peek(keyword::host)
        || input.peek(keyword::path)
        || input.peek(keyword::query)
        || input.peek(keyword::headers)
        || input.peek(keyword::auth)
        || input.peek(keyword::timeout)
        || input.peek(keyword::rate_limit)
        || input.peek(keyword::request)
        || input.peek(keyword::response)
}

fn is_client_only_declaration_start(input: ParseStream<'_>) -> bool {
    input.peek(keyword::base)
        || input.peek(keyword::values)
        || input.peek(keyword::vars)
        || input.peek(keyword::credentials)
        || input.peek(keyword::rate_limits)
        || input.peek(keyword::retry)
        || input.peek(keyword::secret)
        || input.peek(keyword::credential)
}

#[cfg(test)]
mod tests;
