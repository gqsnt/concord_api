use std::collections::HashSet;

use http::Method;
use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, braced, parenthesized};

use super::ast::{
    Clause, Layer, MethodKind, SourceApi, SourceChild, SourceClient, SourceEndpoint, SourceScope,
};
use super::credentials::{parse_auth, parse_credentials};
use super::diagnostics::{
    SpanError, insert_clause, merge_auth, validate_base, validate_contract_id,
    validate_snake_source_identifier,
};
use super::keyword;
use super::pagination::parse_pagination;
use super::rate_limits::{parse_rate_limit, parse_rate_limits};
use super::request::{parse_headers, parse_host, parse_path, parse_query, parse_timeout};
use super::values::{parse_parameters, parse_vars};
use crate::semantic::surface::canonical_ident;

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

        if !input.peek(keyword::client) {
            if input.peek(keyword::scope) || endpoint_starts(input) {
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
        let children = parse_children(input, 0, "client")?;
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
            return Err(input
                .error("unsupported `api!` root declaration; expected a scope or HTTP endpoint"));
        }
        Ok(Self { client, children })
    }
}

impl Parse for SourceClient {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<keyword::client>()?;
        let name: Ident = input.parse()?;
        let body;
        braced!(body in input);
        let owner = format!("client `{}`", canonical_ident(&name));
        let mut base = None;
        let mut values = None;
        let mut credentials = None;
        let mut rate_limits = None;
        let mut layer = Layer::default();

        while !body.is_empty() {
            if body.peek(keyword::base) {
                let key_span = body.span();
                body.parse::<keyword::base>()?;
                let value: LitStr = body.parse()?;
                body.parse::<Token![;]>()?;
                validate_base(&value)?;
                insert_clause(&mut base, Clause { key_span, value }, "base", &owner)?;
                continue;
            }
            if body.peek(keyword::vars) {
                let key_span = body.span();
                let value = parse_vars(&body)?;
                insert_clause(&mut values, Clause { key_span, value }, "vars", &owner)?;
                continue;
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
            if parse_layer_clause(&body, &mut layer, true, &owner)? {
                continue;
            }
            if body.peek(keyword::scope) || endpoint_starts(&body) {
                return Err(
                    body.error("scopes and endpoints are declared after the `client` block")
                );
            }
            if let Some(error) = retired_clause_diagnostic(&body) {
                return Err(error);
            }
            return Err(body.error(
                "unsupported client clause; expected `base`, `vars`, `credentials`, `rate_limits`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, or `rate_limit`",
            ));
        }
        let base = base.ok_or_else(|| {
            name.span()
                .error("a client requires exactly one fixed `base \"https://...\";` clause")
        })?;

        Ok(Self {
            name,
            base,
            values,
            credentials,
            rate_limits,
            layer,
        })
    }
}

fn parse_children(
    input: ParseStream<'_>,
    depth: usize,
    owner: &str,
) -> syn::Result<Vec<SourceChild>> {
    let mut children = Vec::new();
    while !input.is_empty() {
        if input.peek(keyword::scope) {
            children.push(SourceChild::Scope(Box::new(parse_scope(input, depth + 1)?)));
        } else if endpoint_starts(input) {
            children.push(SourceChild::Endpoint(parse_endpoint(input)?));
        } else {
            break;
        }
    }

    validate_child_names(&children, owner)?;
    Ok(children)
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
    input.parse::<keyword::scope>()?;
    let name: Ident = input.parse()?;
    validate_snake_source_identifier(&name, "scope")?;
    let parameters_content;
    parenthesized!(parameters_content in input);
    let parameters = parse_parameters(&parameters_content, "scope", false)?.0;

    let body;
    braced!(body in input);
    let owner = format!("scope `{}`", canonical_ident(&name));
    let mut layer = Layer::default();
    let mut children = Vec::new();
    while !body.is_empty() {
        if body.peek(keyword::scope) {
            children.push(SourceChild::Scope(Box::new(parse_scope(&body, depth + 1)?)));
            continue;
        }
        if endpoint_starts(&body) {
            children.push(SourceChild::Endpoint(parse_endpoint(&body)?));
            continue;
        }
        if parse_layer_clause(&body, &mut layer, true, &owner)? {
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
        return Err(body.error(
            "unsupported scope entry; expected a child scope, HTTP endpoint, or `host`, `path`, `query`, `headers`, `auth`, `timeout`, or `rate_limit` clause",
        ));
    }
    validate_child_names(&children, "scope")?;

    Ok(SourceScope {
        name,
        parameters,
        layer,
        children,
    })
}

fn parse_endpoint(input: ParseStream<'_>) -> syn::Result<SourceEndpoint> {
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
    let response = input.parse()?;

    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        Ok(SourceEndpoint {
            method,
            name,
            parameters,
            body,
            layer: Layer::default(),
            response,
            pagination: None,
            contract_id: None,
        })
    } else {
        let clauses;
        braced!(clauses in input);
        let owner = format!("endpoint `{}`", canonical_ident(&name));
        let mut layer = Layer::default();
        let mut pagination = None;
        let mut contract_id = None;
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
            if parse_layer_clause(&clauses, &mut layer, true, &owner)? {
                continue;
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
            return Err(clauses.error(
                "unsupported endpoint clause; expected `id`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, or `paginate`",
            ));
        }
        Ok(SourceEndpoint {
            method,
            name,
            parameters,
            body,
            layer,
            response,
            pagination,
            contract_id,
        })
    }
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
        "TRACE" => Ok(MethodKind::Trace),
        "CONNECT" => Ok(MethodKind::Connect),
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
            "`default { ... }` is not DSL syntax; declare a value default with `name: Type = VALUE` in `vars { ... }` or a scope or endpoint parameter list",
        ));
    }

    let spelling = input.fork().parse::<Ident>().ok()?;
    let message = match canonical_ident(&spelling).as_str() {
        "defaults" => {
            "`defaults { ... }` is not DSL syntax; declare defaults with `name: Type = VALUE` in `vars { ... }` or a scope or endpoint parameter list"
        }
        "policies" => {
            "`policies { ... }` is not DSL syntax; define reusable rate-limit rules in the client `rate_limits { ... }` block and install retry policy through the generated builder"
        }
        "behaviors" | "behavior" => {
            "behavior profiles are not DSL syntax; configure hooks and runtime policy through the generated builder"
        }
        "prefix" => {
            "`prefix` is not DSL syntax; use `path [...]` for path atoms or `host [...]` for host labels"
        }
        "params" => {
            "`params { ... }` is not DSL syntax; declare values in a scope or endpoint parameter list and shape query parameters with `query { ... }`"
        }
        "profile" => {
            "`profile` is not DSL syntax; define a named reusable rate-limit rule with `rate_limits { rule NAME { ... } }`"
        }
        "access_token" => {
            "`access_token` is not DSL syntax; declare a secret and a bearer credential inside `credentials { ... }`"
        }
        _ => return None,
    };
    Some(spelling.span().error(message))
}

fn parse_layer_clause(
    input: ParseStream<'_>,
    layer: &mut Layer,
    include_rate_limit: bool,
    owner: &str,
) -> syn::Result<bool> {
    if input.peek(keyword::host) {
        let key_span = input.span();
        let value = parse_host(input)?;
        insert_clause(&mut layer.host, Clause { key_span, value }, "host", owner)?;
        return Ok(true);
    }
    if input.peek(keyword::path) {
        let key_span = input.span();
        let value = parse_path(input)?;
        insert_clause(&mut layer.path, Clause { key_span, value }, "path", owner)?;
        return Ok(true);
    }
    if input.peek(keyword::query) {
        let key_span = input.span();
        let value = parse_query(input)?;
        insert_clause(&mut layer.query, Clause { key_span, value }, "query", owner)?;
        return Ok(true);
    }
    if input.peek(keyword::headers) {
        let key_span = input.span();
        let value = parse_headers(input)?;
        insert_clause(
            &mut layer.headers,
            Clause { key_span, value },
            "headers",
            owner,
        )?;
        return Ok(true);
    }
    if input.peek(keyword::auth) {
        merge_auth(&mut layer.auth, parse_auth(input)?, owner)?;
        return Ok(true);
    }
    if input.peek(keyword::timeout) {
        let key_span = input.span();
        let value = parse_timeout(input)?;
        insert_clause(
            &mut layer.timeout,
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
            if let Some(first) = layer.rate_limits.iter().find(|clause| {
                matches!(clause.value, super::ast::RateLimitAttachmentDecl::Inline(_))
            }) {
                let mut error = key_span.error(format!(
                    "a {owner} permits at most one anonymous direct `rate_limit {{ ... }}` rule; use a named reusable rule for additional local policies"
                ));
                error.combine(
                    first
                        .key_span
                        .error("the first anonymous direct rate-limit rule is here"),
                );
                return Err(error);
            }
        }
        layer.rate_limits.push(Clause { key_span, value });
        return Ok(true);
    }
    Ok(false)
}

fn is_client_configuration_start(input: ParseStream<'_>) -> bool {
    is_client_only_declaration_start(input)
        || input.peek(keyword::host)
        || input.peek(keyword::path)
        || input.peek(keyword::query)
        || input.peek(keyword::headers)
        || input.peek(keyword::auth)
        || input.peek(keyword::timeout)
        || input.peek(keyword::rate_limit)
}

fn is_client_only_declaration_start(input: ParseStream<'_>) -> bool {
    input.peek(keyword::base)
        || input.peek(keyword::vars)
        || input.peek(keyword::credentials)
        || input.peek(keyword::rate_limits)
        || input.peek(keyword::retry)
        || input.peek(keyword::secret)
        || input.peek(keyword::credential)
}

#[cfg(test)]
mod tests;
