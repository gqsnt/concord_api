use std::collections::HashSet;
use std::time::Duration;

use http::{HeaderName, Method};
use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Attribute, Ident, LitBool, LitInt, LitStr, Meta, Token, Type, TypePath, braced, bracketed,
    parenthesized,
};
use url::Url;

use crate::ast::{
    AuthChallenge, AuthOperation, AuthPlacement, AuthPlacementKind, BodyDecl, Clause,
    CredentialDecl, CredentialSource, CredentialsBlock, DefaultExpr, DurationLiteral, HeaderEntry,
    HeaderOperation, HostOperation, Layer, MethodKind, PaginationDecl, PaginationField,
    PaginationInitializer, PaginationValue, PathOperation, PublicExpr, QueryEntry, QueryOperation,
    RateLimitDecl, RateLimitWindowLiteral, RetrySelection, SecretDecl, SourceApi, SourceChild,
    SourceClient, SourceEndpoint, SourceScope, TimeoutOperation, ValueDecl, ValueKind,
    syntactic_option_inner,
};
use crate::names::{canonical_ident, to_snake_case};

const MAX_FORMAT_DEPTH: usize = 64;

mod keyword {
    syn::custom_keyword!(api_key);
    syn::custom_keyword!(auth);
    syn::custom_keyword!(base);
    syn::custom_keyword!(basic);
    syn::custom_keyword!(bind);
    syn::custom_keyword!(bearer);
    syn::custom_keyword!(by);
    syn::custom_keyword!(challenge);
    syn::custom_keyword!(client);
    syn::custom_keyword!(credential);
    syn::custom_keyword!(credentials);
    syn::custom_keyword!(cost);
    syn::custom_keyword!(custom);
    syn::custom_keyword!(endpoint);
    syn::custom_keyword!(fmt);
    syn::custom_keyword!(headers);
    syn::custom_keyword!(header);
    syn::custom_keyword!(host);
    syn::custom_keyword!(id);
    syn::custom_keyword!(METHOD);
    syn::custom_keyword!(method);
    syn::custom_keyword!(off);
    syn::custom_keyword!(observe);
    syn::custom_keyword!(paginate);
    syn::custom_keyword!(path);
    syn::custom_keyword!(query);
    syn::custom_keyword!(rate_limit);
    syn::custom_keyword!(rate_limit_observer);
    syn::custom_keyword!(rate_limits);
    syn::custom_keyword!(remove);
    syn::custom_keyword!(replace);
    syn::custom_keyword!(reqwest_default);
    syn::custom_keyword!(retry);
    syn::custom_keyword!(scope);
    syn::custom_keyword!(secret);
    syn::custom_keyword!(timeout);
    syn::custom_keyword!(var);
    syn::custom_keyword!(never);
    syn::custom_keyword!(unauthorized);
    syn::custom_keyword!(unauthorized_or_forbidden);
    syn::custom_keyword!(vars);
}

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
        let mut retry = None;
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
            if body.peek(keyword::retry) {
                let key_span = body.span();
                let value = parse_retry(&body)?;
                insert_clause(&mut retry, Clause { key_span, value }, "retry", &owner)?;
                continue;
            }
            if parse_layer_clause(&body, &mut layer, true, &owner)? {
                continue;
            }
            if body.peek(keyword::rate_limit_observer) {
                return Err(body.error(
                    "`rate_limit_observer` is runtime policy; configure it with `Client::builder().configure_runtime(...)`",
                ));
            }
            if body.peek(keyword::rate_limits) {
                return Err(body.error(
                    "`rate_limits { ... }` was removed; declare `rate_limit { ... }` at each lexical owner",
                ));
            }
            if body.peek(keyword::observe) {
                return Err(body.error(
                    "`observe PATH;` was removed; configure response-head observation on the runtime builder",
                ));
            }
            if body.peek(keyword::var) {
                return Err(body.error("client values must be declared in a `vars { ... }` block"));
            }
            if body.peek(keyword::scope) || endpoint_starts(&body) {
                return Err(
                    body.error("scopes and endpoints are declared after the `client` block")
                );
            }
            return Err(body.error(
                "unsupported client clause; expected `base`, `vars`, `credentials`, `retry`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, or `rate_limit`",
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
            retry,
            layer,
        })
    }
}

fn parse_vars(input: ParseStream<'_>) -> syn::Result<Vec<ValueDecl>> {
    input.parse::<keyword::vars>()?;
    let body;
    braced!(body in input);
    let mut values = Vec::new();
    let mut names = HashSet::new();
    while !body.is_empty() {
        if body.peek(keyword::var) {
            return Err(body.error("client variables must be declared in a `vars { ... }` block without the `var` keyword"));
        }
        let value = parse_value(&body, values.len())?;
        body.parse::<Token![;]>()?;
        insert_unique_value(&mut names, &value, "client value")?;
        values.push(value);
    }
    Ok(values)
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
            return Err(body.error("`retry` is client-only and cannot be declared in a scope"));
        }
        if body.peek(keyword::rate_limit_observer) {
            return Err(body.error(
                "`rate_limit_observer` is runtime policy; configure it on the client builder",
            ));
        }
        if is_client_only_declaration_start(&body) {
            return Err(body.error("this declaration is client-only and cannot appear in a scope"));
        }
        if body.peek(keyword::paginate) || body.peek(keyword::id) {
            return Err(body.error("`paginate` and `id` are endpoint-only clauses"));
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

    if input.peek(Token![as]) {
        return Err(input.error(
            "endpoint aliases were removed; use the desired snake_case method name as the endpoint identifier",
        ));
    }
    input.parse::<Token![->]>()?;
    let response = input.parse()?;

    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        return Ok(SourceEndpoint {
            method,
            name,
            parameters,
            body,
            layer: Layer::default(),
            response,
            pagination: None,
            contract_id: None,
        });
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
                return Err(
                    clauses.error("`retry` is client-only and cannot be declared on an endpoint")
                );
            }
            if clauses.peek(keyword::rate_limit_observer) {
                return Err(clauses.error(
                    "`rate_limit_observer` is runtime policy; configure it on the client builder",
                ));
            }
            if is_client_only_declaration_start(&clauses) {
                return Err(clauses
                    .error("this declaration is client-only and cannot appear on an endpoint"));
            }
            return Err(clauses.error(
                "unsupported endpoint clause; expected `id`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, or `paginate`",
            ));
        }
        if pagination.is_some() && body.is_some() {
            return Err(clauses
                .span()
                .error("pagination requires a bodyless endpoint"));
        }
        return Ok(SourceEndpoint {
            method,
            name,
            parameters,
            body,
            layer,
            response,
            pagination,
            contract_id,
        });
    }
}

fn parse_pagination(input: ParseStream<'_>) -> syn::Result<PaginationDecl> {
    input.parse::<keyword::paginate>()?;
    if input.peek(keyword::custom) {
        return Err(input.error(
            "old `paginate custom factory(...)` syntax is no longer supported; use `paginate TYPE { field: bind ep.field, ... }`",
        ));
    }

    let controller_type: Type = input.parse().map_err(|_| {
        input.error("pagination requires one Rust controller type followed by a struct field block")
    })?;
    if !input.peek(syn::token::Brace) {
        return Err(controller_type.span().error(
            "pagination requires a struct field block; use `paginate TYPE { field: bind ep.field, ... }`",
        ));
    }
    let fields_body;
    braced!(fields_body in input);
    let mut fields = Vec::new();
    let mut seen_fields = HashSet::new();
    let mut seen_bindings = HashSet::new();
    while !fields_body.is_empty() {
        if fields_body.peek(Token![..]) {
            return Err(fields_body
                .error("pagination controller construction does not accept struct rest syntax"));
        }
        let name: Ident = fields_body.parse().map_err(|_| {
            fields_body.error("a pagination controller field begins with its field name")
        })?;
        let canonical = canonical_ident(&name);
        if !seen_fields.insert(canonical.clone()) {
            return Err(name.span().error(format!(
                "duplicate pagination controller field `{canonical}`"
            )));
        }
        if fields_body.peek(Token![=]) {
            return Err(fields_body.error(
                "old `field = ep.field` pagination syntax is no longer supported; use `field: bind ep.field`",
            ));
        }
        fields_body.parse::<Token![:]>().map_err(|_| {
            name.span()
                .error("pagination field shorthand is not supported; provide `field: initializer`")
        })?;
        let initializer = if fields_body.peek(keyword::bind) {
            fields_body.parse::<keyword::bind>()?;
            let namespace: Ident = fields_body.parse().map_err(|_| {
                fields_body.error("`bind` requires direct `ep.FIELD` or `scope.FIELD`")
            })?;
            if !matches!(namespace.to_string().as_str(), "ep" | "scope") {
                return Err(namespace
                    .span()
                    .error("`bind` accepts only direct `ep.FIELD` or `scope.FIELD` destinations"));
            }
            fields_body.parse::<Token![.]>().map_err(|_| {
                namespace
                    .span()
                    .error("`bind` requires direct `ep.FIELD` or `scope.FIELD`")
            })?;
            let destination: Ident = fields_body.parse()?;
            if fields_body.peek(Token![.])
                || fields_body.peek(syn::token::Paren)
                || fields_body.peek(syn::token::Bracket)
                || fields_body.peek(Token![!])
            {
                return Err(fields_body.error(
                    "pagination bindings must be direct `ep.FIELD` or `scope.FIELD` destinations",
                ));
            }
            let binding_key = format!(
                "{}.{}",
                canonical_ident(&namespace),
                canonical_ident(&destination)
            );
            if !seen_bindings.insert(binding_key.clone()) {
                return Err(destination.span().error(format!(
                    "duplicate pagination binding destination `{binding_key}`"
                )));
            }
            PaginationInitializer::Bind {
                namespace,
                destination,
            }
        } else {
            PaginationInitializer::Value(parse_pagination_value(&fields_body)?)
        };
        fields.push(PaginationField { name, initializer });
        if fields_body.is_empty() {
            break;
        }
        fields_body.parse::<Token![,]>().map_err(|_| {
            fields_body.error("pagination controller fields must be comma-separated")
        })?;
    }
    Ok(PaginationDecl {
        controller: controller_type,
        fields,
    })
}

fn parse_pagination_value(input: ParseStream<'_>) -> syn::Result<PaginationValue> {
    if input.peek(LitStr) || input.peek(LitInt) || input.peek(LitBool) || input.peek(Token![-]) {
        return parse_default(input).map(PaginationValue::Stable);
    }
    let fork = input.fork();
    if let Ok(namespace) = fork.parse::<Ident>() {
        if matches!(namespace.to_string().as_str(), "vars" | "scope" | "ep") && fork.peek(Token![.])
        {
            let namespace: Ident = input.parse()?;
            input.parse::<Token![.]>()?;
            let name: Ident = input.parse()?;
            if input.peek(Token![.])
                || input.peek(syn::token::Paren)
                || input.peek(syn::token::Bracket)
                || input.peek(Token![!])
            {
                return Err(input.error(
                    "pagination value references must be direct `vars.FIELD`, `scope.FIELD`, or `ep.FIELD`",
                ));
            }
            return Ok(PaginationValue::Reference { namespace, name });
        }
    }
    parse_default(input).map(PaginationValue::Stable).map_err(|_| {
        input.error(
            "pagination initializers accept only direct public value references, string/integer/boolean literals, or Rust const paths; calls, methods, macros, closures, async blocks, blocks, and formatting expressions are not supported",
        )
    })
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

fn parse_parameters(
    input: ParseStream<'_>,
    owner: &str,
    body_is_special: bool,
) -> syn::Result<(Vec<ValueDecl>, Option<BodyDecl>)> {
    let mut parameters = Vec::new();
    let mut body = None;
    let mut names = std::collections::HashMap::new();
    let mut position = 0;

    while !input.is_empty() {
        let attributes = input.call(Attribute::parse_outer)?;
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let canonical = canonical_ident(&name);
        if let Some(first_span) = names.insert(canonical.clone(), name.span()) {
            let mut error = name
                .span()
                .error(format!("duplicate {owner} parameter `{canonical}`"));
            error.combine(first_span.error("the first parameter is here"));
            return Err(error);
        }

        let is_body = if attributes.is_empty() {
            false
        } else if !body_is_special {
            return Err(attributes[0]
                .span()
                .error("scope parameters do not accept attributes; `#[body]` is endpoint-only"));
        } else {
            for attribute in &attributes {
                if !attribute.path().is_ident("body") || !matches!(&attribute.meta, Meta::Path(_)) {
                    return Err(attribute.span().error(
                        "endpoint parameters accept only the bare `#[body]` role attribute",
                    ));
                }
            }
            if attributes.len() != 1 {
                return Err(attributes[1]
                    .span()
                    .error("a request body parameter accepts exactly one `#[body]` attribute"));
            }
            true
        };

        if body_is_special && canonical == "body" && !is_body {
            return Err(name.span().error(
                "request body inference by parameter name was removed; mark the parameter explicitly as `#[body] body: Type`",
            ));
        }

        if is_body {
            if input.peek(Token![=]) {
                return Err(input.error("a request body parameter cannot have a default"));
            }
            if let Some(first) = &body {
                let first: &BodyDecl = first;
                let mut error = name.span().error(format!(
                    "endpoint declares two request body parameters: `{}` and `{canonical}`",
                    canonical_ident(&first.name)
                ));
                error.combine(first.name.span().error("the first request body is here"));
                return Err(error);
            }
            body = Some(BodyDecl { name, ty, position });
        } else {
            let kind = parse_value_kind(input, &ty)?;
            parameters.push(ValueDecl {
                name,
                ty,
                kind,
                position,
            });
        }
        position += 1;

        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok((parameters, body))
}

fn parse_value(input: ParseStream<'_>, position: usize) -> syn::Result<ValueDecl> {
    let name: Ident = input.parse()?;
    input.parse::<Token![:]>()?;
    let ty: Type = input.parse()?;
    let kind = parse_value_kind(input, &ty)?;
    Ok(ValueDecl {
        name,
        ty,
        kind,
        position,
    })
}

fn parse_value_kind(input: ParseStream<'_>, ty: &Type) -> syn::Result<ValueKind> {
    let optional = syntactic_option_inner(ty)?;
    if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
        return Ok(ValueKind::Defaulted(parse_default(input)?));
    }
    Ok(if optional.is_some() {
        ValueKind::Optional
    } else {
        ValueKind::Required
    })
}

fn parse_default(input: ParseStream<'_>) -> syn::Result<DefaultExpr> {
    if input.peek(LitStr) {
        return input.parse().map(DefaultExpr::String);
    }
    if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let literal: LitInt = input
            .parse()
            .map_err(|_| input.error("a negative default requires an integer literal"))?;
        validate_integer(&literal, "default")?;
        return Ok(DefaultExpr::Integer {
            negative: true,
            literal,
        });
    }
    if input.peek(LitInt) {
        let literal: LitInt = input.parse()?;
        validate_integer(&literal, "default")?;
        return Ok(DefaultExpr::Integer {
            negative: false,
            literal,
        });
    }
    if input.peek(LitBool) {
        return input.parse::<LitBool>().map(DefaultExpr::Boolean);
    }
    if input.fork().parse::<syn::Path>().is_ok() {
        let path = input.parse()?;
        if input.peek(syn::token::Paren)
            || input.peek(syn::token::Brace)
            || input.peek(syn::token::Bracket)
            || input.peek(Token![.])
            || input.peek(Token![!])
        {
            return Err(input.error(
                "defaults accept a Rust const path, not a call, method, macro, block, or collection expression",
            ));
        }
        return Ok(DefaultExpr::Path(path));
    }
    Err(input.error(
        "defaults accept only a string literal, signed integer literal, boolean literal, or Rust const path",
    ))
}

fn parse_retry(input: ParseStream<'_>) -> syn::Result<RetrySelection> {
    input.parse::<keyword::retry>()?;
    let selection = if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        RetrySelection::Disabled
    } else if input.peek(keyword::reqwest_default) {
        input.parse::<keyword::reqwest_default>()?;
        RetrySelection::ReqwestDefault
    } else if input.peek(keyword::custom) {
        return Err(input.error(
            "`retry custom PATH` is runtime policy; use `Client::builder().configure_http(...)` with the dangerous custom-retry feature",
        ));
    } else {
        return Err(input.error("retry must be `off` or `reqwest_default`"));
    };
    input.parse::<Token![;]>()?;
    Ok(selection)
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
        insert_clause(
            &mut layer.rate_limit,
            Clause { key_span, value },
            "rate_limit",
            owner,
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn insert_clause<T>(
    slot: &mut Option<Clause<T>>,
    clause: Clause<T>,
    name: &str,
    owner: &str,
) -> syn::Result<()> {
    if let Some(first) = slot {
        let mut error = clause
            .key_span
            .error(format!("duplicate `{name}` clause for {owner}"));
        error.combine(
            first
                .key_span
                .error(format!("the first `{name}` clause is here")),
        );
        return Err(error);
    }
    *slot = Some(clause);
    Ok(())
}

fn merge_auth(
    slot: &mut Option<AuthOperation>,
    operation: AuthOperation,
    owner: &str,
) -> syn::Result<()> {
    let Some(existing) = slot else {
        *slot = Some(operation);
        return Ok(());
    };
    match (existing, operation) {
        (
            AuthOperation::Modify {
                replace,
                placements,
                clause_spans,
            },
            AuthOperation::Modify {
                replace: incoming_replace,
                placements: mut incoming,
                clause_spans: incoming_spans,
            },
        ) => {
            if incoming_replace {
                let duplicate_span = incoming_spans[0];
                let mut error = duplicate_span.error(format!(
                    "`auth replace` must be the first authentication entry for {owner}"
                ));
                error.combine(clause_spans[0].error("the first authentication entry is here"));
                return Err(error);
            }
            *replace |= incoming_replace;
            placements.append(&mut incoming);
            clause_spans.extend(incoming_spans);
            Ok(())
        }
        (AuthOperation::Off { key_span }, incoming) => {
            let incoming_span = match incoming {
                AuthOperation::Modify { clause_spans, .. } => clause_spans[0],
                AuthOperation::Off { key_span } => key_span,
            };
            let mut error = incoming_span.error(format!("`auth off` is exclusive within {owner}"));
            error.combine((*key_span).error("the exclusive `auth off` clause is here"));
            Err(error)
        }
        (AuthOperation::Modify { clause_spans, .. }, AuthOperation::Off { key_span }) => {
            let mut error = key_span.error(format!("`auth off` is exclusive within {owner}"));
            error.combine(clause_spans[0].error("the first authentication entry is here"));
            Err(error)
        }
    }
}

fn parse_rate_limit(input: ParseStream<'_>) -> syn::Result<RateLimitDecl> {
    input.parse::<keyword::rate_limit>()?;
    if !input.peek(syn::token::Brace) {
        return Err(input.error(
            "rate-limit rules are now direct `rate_limit { ... }` declarations; named attachments, `replace`, and `off` were removed",
        ));
    }
    let body;
    braced!(body in input);
    let cost = if body.peek(keyword::cost) {
        body.parse::<keyword::cost>()?;
        let literal: LitInt = body.parse().map_err(|_| {
            body.error("rate-limit cost requires a positive unsuffixed base-10 integer")
        })?;
        let cost = parse_positive_u32(&literal, "rate-limit cost")?;
        body.parse::<Token![;]>()?;
        cost
    } else {
        1
    };

    let mut windows = Vec::new();
    while !body.is_empty() {
        if body.peek(keyword::by) {
            return Err(body.error("`by [...]` dimensions were removed; normal generated rules use declaration-scoped global buckets"));
        }
        if body.peek(keyword::cost) {
            return Err(
                body.error("a rate-limit rule permits at most one cost clause before windows")
            );
        }
        let capacity_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit rule requires at least one `CAPACITY / DURATION;` window")
        })?;
        let capacity = parse_positive_u32(&capacity_literal, "rate-limit window capacity")?;
        body.parse::<Token![/]>()?;
        let duration_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit window requires a positive duration after `/`")
        })?;
        let duration = parse_rate_duration(&duration_literal)?;
        body.parse::<Token![;]>()?;
        windows.push(RateLimitWindowLiteral {
            capacity,
            duration: DurationLiteral {
                value: duration,
                span: duration_literal.span(),
            },
        });
    }
    if windows.is_empty() {
        return Err(input.error("a rate-limit declaration requires at least one positive window"));
    }
    if let Some(window) = windows.iter().find(|window| cost > window.capacity) {
        return Err(window
            .duration
            .span
            .error("rate-limit cost must not exceed any window capacity"));
    }
    Ok(RateLimitDecl { cost, windows })
}

fn parse_positive_u32(literal: &LitInt, owner: &str) -> syn::Result<u32> {
    validate_integer(literal, owner)?;
    let value = literal.base10_parse::<u32>().map_err(|_| {
        literal
            .span()
            .error(format!("{owner} is not representable as `u32`"))
    })?;
    if value == 0 {
        return Err(literal
            .span()
            .error(format!("{owner} must be greater than zero")));
    }
    Ok(value)
}

fn parse_rate_duration(literal: &LitInt) -> syn::Result<Duration> {
    validate_duration_integer(literal, "rate-limit duration")?;
    let value = literal.base10_parse::<u64>().map_err(|_| {
        literal
            .span()
            .error("rate-limit duration value is not representable")
    })?;
    if value == 0 {
        return Err(literal
            .span()
            .error("rate-limit window duration must be greater than zero"));
    }
    match literal.suffix() {
        "ms" => Ok(Duration::from_millis(value)),
        "s" => Ok(Duration::from_secs(value)),
        "m" => value
            .checked_mul(60)
            .map(Duration::from_secs)
            .ok_or_else(|| {
                literal
                    .span()
                    .error("rate-limit duration overflows `std::time::Duration`")
            }),
        "h" => value
            .checked_mul(3_600)
            .map(Duration::from_secs)
            .ok_or_else(|| {
                literal
                    .span()
                    .error("rate-limit duration overflows `std::time::Duration`")
            }),
        _ => Err(literal
            .span()
            .error("rate-limit duration unit must be one of `ms`, `s`, `m`, or `h`")),
    }
}

fn parse_credentials(input: ParseStream<'_>) -> syn::Result<CredentialsBlock> {
    input.parse::<keyword::credentials>()?;
    let body;
    braced!(body in input);
    let mut secrets = Vec::new();
    let mut credentials = Vec::new();
    let mut secret_names = HashSet::new();
    let mut credential_names = HashSet::new();
    while !body.is_empty() {
        if body.peek(keyword::secret) {
            body.parse::<keyword::secret>()?;
            let name: Ident = body.parse()?;
            body.parse::<Token![:]>()?;
            let ty: Type = body.parse()?;
            if !is_named_marker(&ty, "String") {
                return Err(ty
                    .span()
                    .error("Concord 0.1 secrets are required `String` values without defaults"));
            }
            body.parse::<Token![;]>()?;
            let canonical = canonical_ident(&name);
            if !secret_names.insert(canonical.clone()) {
                return Err(name.span().error(format!("duplicate secret `{canonical}`")));
            }
            secrets.push(SecretDecl { name });
            continue;
        }
        if !body.peek(keyword::credential) {
            return Err(body
                .error("a credentials block accepts only `secret` and `credential` declarations"));
        }
        body.parse::<keyword::credential>()?;
        let name: Ident = body.parse()?;
        body.parse::<Token![=]>()?;
        let source = parse_credential_source(&body, &secret_names)?;
        body.parse::<Token![;]>()?;
        let canonical = canonical_ident(&name);
        if !credential_names.insert(canonical.clone()) {
            return Err(name
                .span()
                .error(format!("duplicate credential `{canonical}`")));
        }
        credentials.push(CredentialDecl { name, source });
    }
    Ok(CredentialsBlock {
        secrets,
        credentials,
    })
}

fn parse_credential_source(
    input: ParseStream<'_>,
    declared_secrets: &HashSet<String>,
) -> syn::Result<CredentialSource> {
    if input.peek(keyword::api_key) {
        input.parse::<keyword::api_key>()?;
        let content;
        parenthesized!(content in input);
        let secret = parse_secret_reference(&content, declared_secrets)?;
        if !content.is_empty() {
            return Err(content.error("an API-key source requires exactly one secret reference"));
        }
        return Ok(CredentialSource::ApiKey { secret });
    }
    if input.peek(keyword::bearer) {
        input.parse::<keyword::bearer>()?;
        let content;
        parenthesized!(content in input);
        let secret = parse_secret_reference(&content, declared_secrets)?;
        if !content.is_empty() {
            return Err(content.error("a bearer source requires exactly one secret reference"));
        }
        return Ok(CredentialSource::Bearer { secret });
    }
    if input.peek(keyword::basic) {
        input.parse::<keyword::basic>()?;
        let content;
        parenthesized!(content in input);
        let username = parse_secret_reference(&content, declared_secrets)?;
        content.parse::<Token![,]>()?;
        let password = parse_secret_reference(&content, declared_secrets)?;
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
        if !content.is_empty() {
            return Err(content.error("Basic credentials require exactly two secret references"));
        }
        return Ok(CredentialSource::Basic { username, password });
    }
    if input.peek(keyword::endpoint) {
        input.parse::<keyword::endpoint>()?;
        return Ok(CredentialSource::Endpoint {
            path: input.parse()?,
        });
    }
    if input.peek(keyword::custom) {
        input.parse::<keyword::custom>()?;
        let provider: Type = input.parse()?;
        if !matches!(provider, Type::Path(_)) {
            return Err(provider
                .span()
                .error("a custom credential provider must be a Rust type path"));
        }
        return Ok(CredentialSource::Custom { provider });
    }
    Err(input
        .error("credential source must be `api_key`, `bearer`, `basic`, `endpoint`, or `custom`"))
}

fn parse_secret_reference(
    input: ParseStream<'_>,
    declared_secrets: &HashSet<String>,
) -> syn::Result<Ident> {
    input.parse::<keyword::secret>()?;
    input.parse::<Token![.]>()?;
    let name: Ident = input.parse()?;
    if !declared_secrets.contains(&canonical_ident(&name)) {
        return Err(name.span().error(format!(
            "unknown or not-yet-declared secret `{}`",
            canonical_ident(&name)
        )));
    }
    Ok(name)
}

fn parse_auth(input: ParseStream<'_>) -> syn::Result<AuthOperation> {
    let key_span = input.span();
    input.parse::<keyword::auth>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(AuthOperation::Off { key_span });
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    let (kind, credential) = if input.peek(keyword::bearer) {
        input.parse::<keyword::bearer>()?;
        (AuthPlacementKind::Bearer, input.parse()?)
    } else if input.peek(keyword::basic) {
        input.parse::<keyword::basic>()?;
        (AuthPlacementKind::Basic, input.parse()?)
    } else if input.peek(keyword::header) {
        input.parse::<keyword::header>()?;
        let name: LitStr = input.parse()?;
        validate_public_header_name(&name)?;
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Header(name), input.parse()?)
    } else if input.peek(keyword::query) {
        input.parse::<keyword::query>()?;
        let name: LitStr = input.parse()?;
        if name.value().is_empty() {
            return Err(name
                .span()
                .error("an authentication query name cannot be empty"));
        }
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Query(name), input.parse()?)
    } else {
        return Err(
            input.error("auth placement must be `bearer`, `basic`, `header`, `query`, or `off`")
        );
    };
    let challenge = if input.peek(keyword::challenge) {
        input.parse::<keyword::challenge>()?;
        Some(if input.peek(keyword::unauthorized) {
            input.parse::<keyword::unauthorized>()?;
            AuthChallenge::Unauthorized
        } else if input.peek(keyword::unauthorized_or_forbidden) {
            input.parse::<keyword::unauthorized_or_forbidden>()?;
            AuthChallenge::UnauthorizedOrForbidden
        } else if input.peek(keyword::never) {
            input.parse::<keyword::never>()?;
            AuthChallenge::Never
        } else {
            return Err(input.error(
                "challenge must be `unauthorized`, `unauthorized_or_forbidden`, or `never`",
            ));
        })
    } else {
        None
    };
    input.parse::<Token![;]>()?;
    Ok(AuthOperation::Modify {
        replace,
        placements: vec![AuthPlacement {
            kind,
            credential,
            challenge,
        }],
        clause_spans: vec![key_span],
    })
}

fn parse_host(input: ParseStream<'_>) -> syn::Result<HostOperation> {
    input.parse::<keyword::host>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(HostOperation::Off);
    }
    let values = parse_expression_list(input, "host")?;
    for value in &values {
        if let PublicExpr::String(value) = value {
            validate_static_host_label(value)?;
        }
    }
    input.parse::<Token![;]>()?;
    Ok(HostOperation::Replace(values))
}

fn parse_path(input: ParseStream<'_>) -> syn::Result<PathOperation> {
    input.parse::<keyword::path>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(PathOperation::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    let values = parse_expression_list(input, "path")?;
    for value in &values {
        if let PublicExpr::String(value) = value {
            validate_static_segment(value)?;
        }
    }
    input.parse::<Token![;]>()?;
    Ok(if replace {
        PathOperation::Replace(values)
    } else {
        PathOperation::Append(values)
    })
}

fn parse_expression_list(input: ParseStream<'_>, owner: &str) -> syn::Result<Vec<PublicExpr>> {
    let content;
    bracketed!(content in input);
    if content.is_empty() {
        return Err(content.error(format!(
            "a {owner} declaration requires at least one expression"
        )));
    }
    let mut values = Vec::new();
    while !content.is_empty() {
        values.push(parse_public_expr(&content, 0)?);
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
        if content.is_empty() {
            break;
        }
    }
    Ok(values)
}

fn parse_query(input: ParseStream<'_>) -> syn::Result<QueryOperation> {
    input.parse::<keyword::query>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(QueryOperation::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    let body;
    braced!(body in input);
    let mut entries = Vec::new();
    let mut keys = HashSet::new();
    while !body.is_empty() {
        let entry = if body.peek(keyword::remove) {
            body.parse::<keyword::remove>()?;
            QueryEntry::Remove(body.parse()?)
        } else {
            let key: LitStr = body.parse()?;
            body.parse::<Token![=]>()?;
            QueryEntry::Assign {
                key,
                value: parse_public_expr(&body, 0)?,
            }
        };
        body.parse::<Token![;]>()?;
        let key = match &entry {
            QueryEntry::Assign { key, .. } | QueryEntry::Remove(key) => key,
        };
        if !keys.insert(key.value()) {
            return Err(key
                .span()
                .error(format!("duplicate local query key `{}`", key.value())));
        }
        entries.push(entry);
    }
    Ok(if replace {
        QueryOperation::Replace(entries)
    } else {
        QueryOperation::Modify(entries)
    })
}

fn parse_headers(input: ParseStream<'_>) -> syn::Result<HeaderOperation> {
    input.parse::<keyword::headers>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(HeaderOperation::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    let body;
    braced!(body in input);
    let mut entries = Vec::new();
    let mut names = HashSet::new();
    while !body.is_empty() {
        let entry = if body.peek(keyword::remove) {
            body.parse::<keyword::remove>()?;
            HeaderEntry::Remove(body.parse()?)
        } else {
            let name: LitStr = body.parse()?;
            body.parse::<Token![=]>()?;
            HeaderEntry::Assign {
                name,
                value: parse_public_expr(&body, 0)?,
            }
        };
        body.parse::<Token![;]>()?;
        let name = match &entry {
            HeaderEntry::Assign { name, .. } | HeaderEntry::Remove(name) => name,
        };
        validate_public_header_name(name)?;
        let canonical = name.value().to_ascii_lowercase();
        if !names.insert(canonical) {
            return Err(name.span().error(format!(
                "duplicate local public header name `{}` (header names are case-insensitive)",
                name.value()
            )));
        }
        entries.push(entry);
    }
    Ok(if replace {
        HeaderOperation::Replace(entries)
    } else {
        HeaderOperation::Modify(entries)
    })
}

fn parse_timeout(input: ParseStream<'_>) -> syn::Result<TimeoutOperation> {
    input.parse::<keyword::timeout>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(TimeoutOperation::Off);
    }
    let literal: LitInt = input.parse().map_err(|_| {
        input.error("timeout requires a duration such as `250ms`, `5s`, `10m`, or `1h`")
    })?;
    let value = parse_duration(&literal)?;
    input.parse::<Token![;]>()?;
    Ok(TimeoutOperation::Set(DurationLiteral {
        value,
        span: literal.span(),
    }))
}

fn parse_public_expr(input: ParseStream<'_>, depth: usize) -> syn::Result<PublicExpr> {
    if input.peek(LitStr) {
        return input.parse().map(PublicExpr::String);
    }
    if input.peek(LitInt) {
        let literal: LitInt = input.parse()?;
        validate_integer(&literal, "public expression")?;
        return Ok(PublicExpr::Integer(literal));
    }
    if input.peek(keyword::fmt) {
        let marker: keyword::fmt = input.parse()?;
        if depth >= MAX_FORMAT_DEPTH {
            return Err(marker
                .span()
                .error("nested `fmt[...]` expressions exceed the maximum depth of 64"));
        }
        let content;
        bracketed!(content in input);
        if content.is_empty() {
            return Err(content.error("`fmt[...]` requires at least one expression"));
        }
        let mut values = Vec::new();
        loop {
            values.push(parse_public_expr(&content, depth + 1)?);
            if content.is_empty() {
                break;
            }
            content
                .parse::<Token![,]>()
                .map_err(|_| content.error("`fmt[...]` expressions must be separated by commas"))?;
            if content.is_empty() {
                return Err(content.error("a trailing comma is not accepted in `fmt[...]`"));
            }
        }
        return Ok(PublicExpr::Format {
            span: marker.span(),
            values,
        });
    }

    let namespace: Ident = input.parse().map_err(|_| {
        input.error(
            "public expressions accept only literals, `vars.NAME`, `scope.NAME`, `ep.NAME`, or `fmt[...]`",
        )
    })?;
    input.parse::<Token![.]>().map_err(|_| {
        namespace.span().error(
            "arbitrary Rust expressions are not supported; use `vars.NAME`, `scope.NAME`, or `ep.NAME`",
        )
    })?;
    let name: Ident = input.parse()?;
    if input.peek(Token![.])
        || input.peek(syn::token::Paren)
        || input.peek(syn::token::Bracket)
        || input.peek(Token![!])
    {
        return Err(input
            .error("arbitrary Rust expressions are not supported after a public value reference"));
    }
    if !matches!(namespace.to_string().as_str(), "vars" | "scope" | "ep") {
        return Err(namespace.span().error(
            "unsupported public-expression namespace; secrets and credentials are not expressible",
        ));
    }
    Ok(PublicExpr::Reference { namespace, name })
}

fn is_named_marker(ty: &Type, expected: &str) -> bool {
    matches!(ty, Type::Path(path) if is_unqualified(path, expected))
}

fn is_unqualified(path: &TypePath, expected: &str) -> bool {
    path.qself.is_none()
        && path.path.leading_colon.is_none()
        && path.path.segments.len() == 1
        && path.path.segments[0].ident == expected
}

fn parse_duration(literal: &LitInt) -> syn::Result<Duration> {
    validate_duration_integer(literal, "duration")?;
    let value = literal
        .base10_parse::<u64>()
        .map_err(|_| literal.span().error("duration value is not representable"))?;
    if value == 0 {
        return Err(literal
            .span()
            .error("operation timeouts must be greater than zero"));
    }
    match literal.suffix() {
        "ms" => Ok(Duration::from_millis(value)),
        "s" => Ok(Duration::from_secs(value)),
        "m" => value
            .checked_mul(60)
            .map(Duration::from_secs)
            .ok_or_else(|| {
                literal
                    .span()
                    .error("duration overflows `std::time::Duration`")
            }),
        "h" => value
            .checked_mul(3_600)
            .map(Duration::from_secs)
            .ok_or_else(|| {
                literal
                    .span()
                    .error("duration overflows `std::time::Duration`")
            }),
        _ => Err(literal
            .span()
            .error("duration unit must be one of `ms`, `s`, `m`, or `h`")),
    }
}

fn validate_duration_integer(literal: &LitInt, owner: &str) -> syn::Result<()> {
    let text = literal.to_string().replace('_', "");
    let digits = text.strip_suffix(literal.suffix()).unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(literal
            .span()
            .error(format!("{owner} integer literals must be base-10 values")));
    }
    Ok(())
}

fn validate_integer(literal: &LitInt, owner: &str) -> syn::Result<()> {
    if !literal.suffix().is_empty() {
        return Err(literal.span().error(format!(
            "{owner} integer literals must be unsuffixed base-10 values"
        )));
    }
    let text = literal.to_string().replace('_', "");
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(literal.span().error(format!(
            "{owner} integer literals must be unsuffixed base-10 values"
        )));
    }
    Ok(())
}

fn validate_base(base: &LitStr) -> syn::Result<()> {
    let parsed = Url::parse(&base.value()).map_err(|_| {
        base.span()
            .error("the fixed base must be a valid absolute HTTP or HTTPS URL")
    })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(base.span().error(
            "the fixed base must be absolute HTTP(S), contain a host, and contain no user information, query, or fragment",
        ));
    }
    Ok(())
}

fn validate_snake_source_identifier(name: &Ident, owner: &str) -> syn::Result<()> {
    let canonical = canonical_ident(name);
    let expected = to_snake_case(&canonical);
    if canonical != expected {
        return Err(name.span().error(format!(
            "{owner} source identifier `{canonical}` must be snake_case; use `{expected}`"
        )));
    }
    Ok(())
}

fn validate_contract_id(id: &LitStr) -> syn::Result<()> {
    let value = id.value();
    let valid = (1..=128).contains(&value.len())
        && value.is_ascii()
        && value.split('.').all(|segment| {
            let bytes = segment.as_bytes();
            (1..=63).contains(&bytes.len())
                && bytes.iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
                && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        });
    if !valid {
        return Err(id.span().error(
            "an endpoint ID must be 1-128 ASCII bytes in dot-separated 1-63 byte segments; each segment begins and ends with a lowercase letter or digit and may also contain `_` or `-`",
        ));
    }
    Ok(())
}

fn validate_static_host_label(label: &LitStr) -> syn::Result<()> {
    let value = label.value();
    let bytes = value.as_bytes();
    if !(1..=63).contains(&bytes.len())
        || !bytes.is_ascii()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        || !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(label.span().error(
            "a static host atom must be one 1-63 byte ASCII DNS label beginning and ending with an alphanumeric character",
        ));
    }
    Ok(())
}

fn validate_static_segment(segment: &LitStr) -> syn::Result<()> {
    let value = segment.value();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(segment.span().error(
            "a static path atom must be one non-empty, non-dot segment without `/` or `\\`",
        ));
    }
    Ok(())
}

fn validate_public_header_name(name: &LitStr) -> syn::Result<()> {
    let parsed = HeaderName::from_bytes(name.value().as_bytes()).map_err(|_| {
        name.span()
            .error("public header name is not a valid HTTP header name")
    })?;
    if matches!(
        parsed.as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "te"
            | "trailer"
            | "upgrade"
            | "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
    ) {
        return Err(name
            .span()
            .error("this public header name is reserved by transport, authentication, cookies, or proxy authentication"));
    }
    Ok(())
}

fn insert_unique_value(
    names: &mut HashSet<String>,
    value: &ValueDecl,
    owner: &str,
) -> syn::Result<()> {
    let name = canonical_ident(&value.name);
    if !names.insert(name.clone()) {
        return Err(value
            .name
            .span()
            .error(format!("duplicate {owner} `{name}`")));
    }
    Ok(())
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
        || input.peek(keyword::var)
        || input.peek(keyword::credentials)
        || input.peek(keyword::retry)
        || input.peek(keyword::rate_limit_observer)
        || input.peek(keyword::rate_limits)
        || input.peek(keyword::observe)
}

trait SpanError {
    fn error(self, message: impl std::fmt::Display) -> syn::Error;
}

impl SpanError for proc_macro2::Span {
    fn error(self, message: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, message)
    }
}

#[cfg(test)]
mod tests {
    use quote::{format_ident, quote};

    use super::*;

    fn parse_ok(tokens: TokenStream) -> SourceApi {
        parse(tokens).expect("source AST")
    }

    #[test]
    fn parses_singular_client_root_children_values_and_all_shaping_layers() {
        let api = parse_ok(quote! {
            client FirstApi {
                base "https://example.test:8443/fixed";
                vars {
                    tenant: String;
                    locale: Option<String>;
                    limit: u32 = 50;
                }
                retry reqwest_default;
                host ["api"];
                path ["v1"];
                query { "tenant" = vars.tenant; }
                headers { "X-Locale" = vars.locale; }
                timeout 5s;
            }
            GET root(id: String, verbose: Option<bool>, page: u32 = 10) -> Bytes {
                path [ep.id];
            }
            scope regions(region: String, language: Option<String>) {
                host [scope.region, "api"];
                path ["regions", scope.region];
                scope users(account: String) {
                    path ["users", scope.account];
                    METHOD("PROPFIND") inspect() -> Text;
                }
            }
        });
        assert_eq!(canonical_ident(&api.client.name), "FirstApi");
        assert_eq!(api.client.values.as_ref().unwrap().value.len(), 3);
        assert_eq!(api.children.len(), 2);
    }

    #[test]
    fn singular_api_structure_and_vars_diagnostics_are_focused() {
        let empty = parse(TokenStream::new()).err().expect("missing client");
        assert!(empty.to_string().contains("exactly one `client`"));

        let child_first = parse(quote! {
            GET status() -> Bytes;
            client Api { base "https://example.test"; }
        })
        .err()
        .expect("child before client");
        assert!(child_first.to_string().contains("must be the first"));

        let second_client = parse(quote! {
            client FirstApi { base "https://one.example.test"; }
            client SecondApi { base "https://two.example.test"; }
        })
        .err()
        .expect("second client");
        assert!(second_client.to_string().contains("exactly one `client`"));
        assert!(
            second_client
                .to_string()
                .contains("separate `api!` invocations")
        );

        let api = parse_ok(quote! {
            client Api { base "https://example.test"; }
            GET root() -> Bytes;
            scope first() { GET nested() -> Bytes; }
            scope second() { }
        });
        assert_eq!(api.children.len(), 3);

        let no_vars = parse_ok(quote! {
            client Api { base "https://example.test"; }
        });
        assert!(no_vars.client.values.is_none());

        let empty_vars = parse_ok(quote! {
            client Api { base "https://example.test"; vars { } }
        });
        assert!(empty_vars.client.values.as_ref().unwrap().value.is_empty());

        let values = parse_ok(quote! {
            client Api {
                base "https://example.test";
                vars {
                    required: String;
                    optional: Option<String>;
                    defaulted: usize = crate::DEFAULT_SIZE;
                }
            }
        });
        let values = &values.client.values.as_ref().unwrap().value;
        assert!(matches!(values[0].kind, ValueKind::Required));
        assert!(matches!(values[1].kind, ValueKind::Optional));
        assert!(matches!(
            values[2].kind,
            ValueKind::Defaulted(DefaultExpr::Path(_))
        ));

        let duplicate_value = parse(quote! {
            client Api {
                base "https://example.test";
                vars { value: String; value: String; }
            }
        })
        .err()
        .expect("duplicate value");
        assert!(
            duplicate_value
                .to_string()
                .contains("duplicate client value `value`")
        );

        let legacy_var = parse(quote! {
            client Api { base "https://example.test"; var value: String; }
        })
        .err()
        .expect("legacy var");
        assert!(legacy_var.to_string().contains("`vars { ... }` block"));

        parse(quote! {
            client Api {
                credentials { }
                vars { value: String; }
                base "https://example.test";
            }
        })
        .expect("client clauses are order-independent");

        let duplicate_vars = parse(quote! {
            client Api {
                base "https://example.test";
                vars { }
                vars { }
            }
        })
        .err()
        .expect("duplicate vars");
        assert!(
            duplicate_vars
                .to_string()
                .contains("duplicate `vars` clause")
        );

        for tokens in [
            quote! { client Api { base "https://example.test"; scope nested() { } } },
            quote! { client Api { base "https://example.test"; GET nested() -> Bytes; } },
        ] {
            let error = parse(tokens).err().expect("client child");
            assert!(error.to_string().contains("after the `client` block"));
        }

        let root_config = parse(quote! {
            client Api { base "https://example.test"; }
            timeout 1s;
        })
        .err()
        .expect("root client configuration");
        assert!(
            root_config
                .to_string()
                .contains("inside the `client` block")
        );
    }

    #[test]
    fn rejects_arbitrary_defaults_and_public_expressions() {
        let default_error = parse(quote! {
            client Api {
                base "https://example.test";
                vars { value: String = make_value(); }
            }
        })
        .err()
        .expect("call default");
        assert!(default_error.to_string().contains("Rust const path"));

        let expression_error = parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET root(id: String) -> Bytes { path [ep.id.to_string()]; }
        })
        .err()
        .expect("method expression");
        assert!(
            expression_error
                .to_string()
                .contains("unsupported endpoint clause")
                || expression_error
                    .to_string()
                    .contains("arbitrary Rust expressions")
        );
    }

    #[test]
    fn duration_and_custom_method_boundaries_are_checked() {
        let zero = parse(quote! {
            client Api { base "https://example.test"; timeout 0s; }
        })
        .err()
        .expect("zero timeout");
        assert!(zero.to_string().contains("greater than zero"));

        let non_decimal = parse(quote! {
            client Api { base "https://example.test"; timeout 0x10s; }
        })
        .err()
        .expect("non-decimal timeout");
        assert!(non_decimal.to_string().contains("base-10"));

        let invalid = parse(quote! {
            client Api { base "https://example.test"; }
            METHOD("BAD METHOD") root() -> Bytes;
        })
        .err()
        .expect("invalid method");
        assert!(invalid.to_string().contains("valid HTTP method token"));

        let overflow = parse(quote! {
            client Api {
                base "https://example.test";
                timeout 18446744073709551615h;
            }
        })
        .err()
        .expect("overflowing timeout");
        assert!(overflow.to_string().contains("overflows"));
    }

    fn nested_scopes(depth: usize) -> TokenStream {
        let mut child = quote!(GET leaf() -> Bytes;);
        for index in 0..depth {
            let name = format_ident!("scope_{index}");
            child = quote!(scope #name() { #child });
        }
        quote!(client Api { base "https://example.test"; } #child)
    }

    fn nested_format(depth: usize) -> TokenStream {
        let mut expression = quote!("leaf");
        for _ in 0..depth {
            expression = quote!(fmt[#expression]);
        }
        quote! {
            client Api {
                base "https://example.test";
            }
            GET leaf() -> Bytes { path [#expression]; }
        }
    }

    #[test]
    fn scope_and_format_depth_bounds_accept_64_and_reject_65() {
        parse(nested_scopes(64)).expect("64 nested scopes");
        let scope_error = parse(nested_scopes(65)).err().expect("65 nested scopes");
        assert!(
            scope_error
                .to_string()
                .contains("maximum supported depth of 64")
        );

        parse(nested_format(64)).expect("64 nested formats");
        let format_error = parse(nested_format(65)).err().expect("65 nested formats");
        assert!(format_error.to_string().contains("maximum depth of 64"));
    }

    #[test]
    fn focused_structure_and_local_duplicate_diagnostics_are_stable() {
        let body_order = parse_ok(quote! {
            client Api {
                base "https://example.test";
            }
            POST create(#[body] body: Text, id: String) -> Bytes;
        });
        let SourceChild::Endpoint(endpoint) = &body_order.children[0] else {
            panic!("endpoint")
        };
        assert_eq!(endpoint.body.as_ref().unwrap().position, 0);
        assert_eq!(endpoint.parameters[0].position, 1);

        let duplicate_query = parse(quote! {
            client Api {
                base "https://example.test";
                query { "key" = "one"; remove "key"; }
            }
        })
        .err()
        .expect("duplicate query key");
        assert!(
            duplicate_query
                .to_string()
                .contains("duplicate local query key")
        );

        let duplicate_header = parse(quote! {
            client Api {
                base "https://example.test";
                headers { "X-Key" = "one"; "x-key" = "two"; }
            }
        })
        .err()
        .expect("case-insensitive duplicate header");
        assert!(duplicate_header.to_string().contains("case-insensitive"));
    }

    #[test]
    fn defaults_formats_and_static_atoms_obey_grammar_boundaries() {
        parse(quote! {
            client Api {
                base "https://example.test";
                vars {
                    text: &'static str = "value";
                    negative: i64 = -7;
                    enabled: bool = true;
                    constant: usize = crate::DEFAULT_SIZE;
                }
                host ["a12345678901234567890123456789012345678901234567890123456789012"];
            }
            GET read(id: u32) -> Bytes { path [fmt["id-", ep.id]]; }
        })
        .expect("restricted defaults and 63-byte host label");

        let host = parse(quote! {
            client Api {
                base "https://example.test";
                host ["a123456789012345678901234567890123456789012345678901234567890123"];
            }
        })
        .err()
        .expect("64-byte host label");
        assert!(host.to_string().contains("1-63 byte"));

        let segment = parse(quote! {
            client Api { base "https://example.test"; path ["two/segments"]; }
        })
        .err()
        .expect("static path structure");
        assert!(segment.to_string().contains("static path atom"));

        let commas = parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET read(id: u32) -> Bytes { path [fmt["id-" ep.id]]; }
        })
        .err()
        .expect("missing format comma");
        assert!(commas.to_string().contains("separated by commas"));

        parse(quote! {
            client Api {
                base "https://example.test";
                credentials { }
            }
        })
        .expect("an empty credentials block is structurally valid");

        parse(quote! {
            client Api {
                base "https://example.test";
                rate_limits { }
            }
        })
        .err()
        .expect("removed rate-limit template block");
    }

    #[test]
    fn credentials_parse_all_sources_and_enforce_definition_order() {
        let api = parse_ok(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret api_key: String;
                    secret bearer: String;
                    secret username: String;
                    secret password: String;
                    credential key = api_key(secret.api_key);
                    credential token = bearer(secret.bearer);
                    credential login = basic(secret.username, secret.password);
                    credential acquired = endpoint login;
                    credential custom = custom crate::SessionProvider;
                }
            }
            GET login() -> Bytes;
        });
        let credentials = api.client.credentials.as_ref().expect("credentials block");
        assert_eq!(credentials.value.secrets.len(), 4);
        assert_eq!(credentials.value.credentials.len(), 5);

        let error = parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    credential key = api_key(secret.later);
                    secret later: String;
                }
            }
        })
        .err()
        .expect("forward secret reference");
        assert!(
            error
                .to_string()
                .contains("not-yet-declared secret `later`")
        );

        let error = parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials { secret first: String; }
                credentials { secret second: String; }
            }
        })
        .err()
        .expect("second credentials block");
        assert!(error.to_string().contains("duplicate `credentials` clause"));
    }

    #[test]
    fn authentication_layer_operations_and_challenge_suffixes_are_bounded() {
        parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret token: String;
                    credential session = bearer(secret.token);
                }
                auth replace bearer session challenge unauthorized_or_forbidden;
                auth query "fallback" = session challenge never;
            }
            scope public() { auth off; }
        })
        .expect("bounded auth syntax");

        let off = parse(quote! {
            client AuthApi {
                base "https://example.test";
                auth off;
                auth bearer session;
            }
        })
        .err()
        .expect("auth off exclusivity");
        assert!(off.to_string().contains("`auth off` is exclusive"));

        let late_replace = parse(quote! {
            client AuthApi {
                base "https://example.test";
                auth bearer first;
                auth replace bearer second;
            }
        })
        .err()
        .expect("late replacement");
        assert!(late_replace.to_string().contains("must be the first"));

        parse(quote! {
            client AuthApi {
                base "https://example.test";
                timeout 1s;
                auth bearer session;
            }
        })
        .expect("authentication entries may follow timeout");
    }

    #[test]
    fn secret_shapes_and_custom_provider_paths_have_focused_diagnostics() {
        let optional = parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials { secret token: Option<String>; }
            }
        })
        .err()
        .expect("optional secret");
        assert!(optional.to_string().contains("required `String`"));

        let non_path_provider = parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    credential session = custom (crate::Provider,);
                }
            }
        })
        .err()
        .expect("non-path provider type");
        assert!(
            non_path_provider
                .to_string()
                .contains("provider must be a Rust type path")
        );
    }

    #[test]
    fn lexical_rate_limit_declarations_parse_in_order() {
        let api = parse_ok(quote! {
            client RateApi {
                base "https://example.test";
                vars { tenant: String; }
                timeout 5s;
                rate_limit {
                    500 / 10s;
                    30000 / 10m;
                }
            }
            scope tenants(tenant: String) {
                rate_limit {
                    100 / 1m;
                }
                GET read(item: String) -> Bytes {
                    timeout 2s;
                    rate_limit {
                        cost 2;
                        10 / 1s;
                    }
                }
                GET public() -> Bytes;
            }
        });
        let client = &api.client;
        assert_eq!(
            client
                .layer
                .rate_limit
                .as_ref()
                .unwrap()
                .value
                .windows
                .len(),
            2
        );
        let SourceChild::Scope(scope) = &api.children[0] else {
            panic!("scope")
        };
        assert!(scope.layer.rate_limit.is_some());
        let SourceChild::Endpoint(read) = &scope.children[0] else {
            panic!("read endpoint")
        };
        assert_eq!(read.layer.rate_limit.as_ref().unwrap().value.cost, 2);
        let SourceChild::Endpoint(public) = &scope.children[1] else {
            panic!("public endpoint")
        };
        assert!(public.layer.rate_limit.is_none());
    }

    #[test]
    fn rate_limit_cost_defaults_and_equal_capacity_is_valid() {
        let api = parse_ok(quote! {
            client Api {
                base "https://example.test";
                rate_limit {
                    1 / 1s;
                }
            }
            scope narrow() {
                rate_limit {
                    cost 2;
                    2 / 1s;
                    10 / 1m;
                }
                GET read() -> Bytes;
            }
        });
        assert_eq!(api.client.layer.rate_limit.as_ref().unwrap().value.cost, 1);
        let SourceChild::Scope(scope) = &api.children[0] else {
            panic!("scope")
        };
        let rule = &scope.layer.rate_limit.as_ref().unwrap().value;
        assert_eq!(rule.cost, 2);
        assert_eq!(rule.windows.len(), 2);
        assert_eq!(rule.windows[0].capacity, 2);
        assert_eq!(rule.windows[1].capacity, 10);
    }

    #[test]
    fn rate_limit_syntax_rejects_invalid_structure_values_dimensions_and_order() {
        let duplicate_rule = parse(quote! {
            client Api {
                base "https://example.test";
                rate_limit { 1 / 1s; }
                rate_limit { 1 / 1s; }
            }
        })
        .err()
        .expect("duplicate direct rule");
        assert!(
            duplicate_rule
                .to_string()
                .contains("duplicate `rate_limit` clause")
        );

        for (tokens, expected) in [
            (
                quote! { client Api { base "https://example.test"; rate_limit { cost 0; 1 / 1s; } } },
                "cost must be greater than zero",
            ),
            (
                quote! { client Api { base "https://example.test"; rate_limit { 0 / 1s; } } },
                "capacity must be greater than zero",
            ),
            (
                quote! { client Api { base "https://example.test"; rate_limit { 1 / 0s; } } },
                "duration must be greater than zero",
            ),
            (
                quote! { client Api { base "https://example.test"; rate_limit { 1 / 0x10s; } } },
                "duration integer literals must be base-10",
            ),
            (
                quote! { client Api { base "https://example.test"; rate_limit { cost 2; 1 / 1s; } } },
                "must not exceed any window capacity",
            ),
            (
                quote! { client Api { base "https://example.test"; rate_limit { cost 2; 100 / 1s; 1 / 1m; } } },
                "must not exceed any window capacity",
            ),
        ] {
            let error = parse(tokens).err().expect("invalid rate syntax");
            assert!(
                error.to_string().contains(expected),
                "expected `{expected}`, got `{error}`"
            );
        }

        let old_syntax = parse(quote! {
            client Api {
                base "https://example.test";
                rate_limits { quota { 1 / 1s; } }
            }
        })
        .err()
        .expect("removed template syntax");
        assert!(old_syntax.to_string().contains("was removed"));

        let old_observer = parse(quote! {
            client Api {
                base "https://example.test";
                observe crate::Observer;
            }
        })
        .err()
        .expect("removed observer syntax");
        assert!(old_observer.to_string().contains("runtime builder"));

        parse(quote! {
            client Api {
                base "https://example.test";
            }
            GET read() -> Bytes {
                rate_limit { 1 / 1s; }
                timeout 1s;
            }
        })
        .expect("endpoint clauses are order-independent");
    }

    #[test]
    fn pagination_struct_forms_parse_with_restricted_initializers() {
        let api = parse_ok(quote! {
            client Api {
                base "https://example.test";
            }
            GET offset(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                paginate concord_core::advanced::OffsetController {
                    offset: bind ep.offset,
                    page_size: bind ep.limit,
                }
            }
            GET page(page: u64, per_page: u32) -> PageSpec<Vec<u32>> {
                paginate PageController {
                    page: bind ep.page,
                    page_size: bind ep.per_page,
                }
            }
            GET cursor(cursor: Option<String>) -> PageSpec<Vec<u32>> {
                paginate CursorController<String> { cursor: bind ep.cursor }
            }
            scope regions(region: String) {
                GET custom(cursor: Option<String>, maximum: u32) -> PageSpec<Vec<u32>> {
                    paginate crate::pagination::VendorCursor {
                        cursor: bind ep.cursor,
                        region: scope.region,
                        empty_pages: 0,
                        enabled: true,
                        label: "vendor",
                        maximum: vars.maximum,
                        policy: crate::pagination::DEFAULT_POLICY,
                    }
                }
            }
        });
        assert_eq!(api.children.len(), 4);
    }

    #[test]
    fn pagination_rejects_old_forms_and_unsafe_initializers() {
        let cases = [
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                        paginate OffsetController
                    }
                },
                "struct field block",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                        paginate OffsetController { offset = ep.offset }
                    }
                },
                "field: bind ep.field",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                        paginate OffsetController {
                            offset: bind ep.offset,
                            offset: bind ep.limit,
                        }
                    }
                },
                "duplicate pagination controller field",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                        paginate OffsetController {
                            offset: bind ep.offset,
                            page_size: bind ep.offset,
                        }
                    }
                },
                "duplicate pagination binding destination",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                        paginate OffsetController { offset: bind vars.offset }
                    }
                },
                "accepts only direct `ep.FIELD` or `scope.FIELD`",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64, limit: u32) -> PageSpec<Vec<u32>> {
                        paginate OffsetController { offset: bind ep.state.offset }
                    }
                },
                "must be direct",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64) -> PageSpec<Vec<u32>> {
                        paginate OffsetController { offset }
                    }
                },
                "field shorthand",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read(offset: u64) -> PageSpec<Vec<u32>> {
                        paginate OffsetController { .. }
                    }
                },
                "rest syntax",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read() -> PageSpec<Vec<u32>> {
                        paginate crate::Controller { value: crate::make() }
                    }
                },
                "calls, methods, macros",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET read() -> PageSpec<Vec<u32>> {
                        paginate custom crate::factory();
                    }
                },
                "old `paginate custom factory(...)` syntax",
            ),
        ];
        for (tokens, expected) in cases {
            let error = parse(tokens).err().expect("invalid pagination");
            assert!(
                error.to_string().contains(expected),
                "expected `{expected}`, got `{error}`"
            );
        }
    }

    fn clause_permutations(clauses: &[TokenStream]) -> Vec<TokenStream> {
        fn visit(
            clauses: &[TokenStream],
            used: &mut [bool],
            current: &mut Vec<TokenStream>,
            output: &mut Vec<TokenStream>,
        ) {
            if current.len() == clauses.len() {
                output.push(quote!(#(#current)*));
                return;
            }
            for index in 0..clauses.len() {
                if used[index] {
                    continue;
                }
                used[index] = true;
                current.push(clauses[index].clone());
                visit(clauses, used, current, output);
                current.pop();
                used[index] = false;
            }
        }

        let mut output = Vec::new();
        visit(
            clauses,
            &mut vec![false; clauses.len()],
            &mut Vec::new(),
            &mut output,
        );
        output
    }

    #[test]
    fn named_client_scope_and_endpoint_entries_are_order_independent() {
        let client_clauses = [
            quote!(base "https://example.test";),
            quote!(vars { tenant: String; }),
            quote!(retry reqwest_default;),
            quote!(timeout 3s;),
            quote!(headers { "X-Static" = "one"; }),
        ];
        for clauses in clause_permutations(&client_clauses) {
            parse(quote! {
                client Api { #clauses }
                GET read() -> Bytes;
            })
            .expect("client clause permutation");
        }

        let endpoint_clauses = [
            quote!(id "api.items.read";),
            quote!(path ["items"];),
            quote!(query { "kind" = "all"; }),
            quote!(headers { "X-Static" = "one"; }),
            quote!(timeout 2s;),
        ];
        for clauses in clause_permutations(&endpoint_clauses) {
            parse(quote! {
                client Api { base "https://example.test"; }
                GET read() -> Bytes { #clauses }
            })
            .expect("endpoint clause permutation");
        }

        let scope_entries = [
            quote!(path ["items"];),
            quote!(timeout 2s;),
            quote!(GET first() -> Bytes;),
            quote!(scope nested() { GET second() -> Bytes; }),
        ];
        for entries in clause_permutations(&scope_entries) {
            parse(quote! {
                client Api { base "https://example.test"; }
                scope items() { #entries }
            })
            .expect("scope clause/child permutation");
        }

        let auth = parse_ok(quote! {
            client Api { base "https://example.test"; }
            GET read() -> Bytes {
                auth bearer first;
                timeout 2s;
                auth query "fallback" = second;
                path ["items"];
            }
        });
        let SourceChild::Endpoint(endpoint) = &auth.children[0] else {
            panic!("endpoint")
        };
        let Some(AuthOperation::Modify { placements, .. }) = &endpoint.layer.auth else {
            panic!("auth placements")
        };
        assert_eq!(canonical_ident(&placements[0].credential), "first");
        assert_eq!(canonical_ident(&placements[1].credential), "second");
    }

    #[test]
    fn duplicate_named_clauses_report_duplicate_and_first_spans() {
        let error = parse(quote! {
            client Api { base "https://example.test"; }
            GET read() -> Bytes {
                timeout 1s;
                path ["items"];
                timeout 2s;
            }
        })
        .err()
        .expect("duplicate timeout");
        assert!(error.to_string().contains("duplicate `timeout` clause"));
        let diagnostic_tokens = error.into_compile_error().to_string();
        assert_eq!(diagnostic_tokens.matches("compile_error").count(), 2);
        assert!(diagnostic_tokens.contains("first `timeout` clause"));
    }

    #[test]
    fn source_names_body_roles_optionality_and_stable_ids_follow_v2_syntax() {
        let api = parse_ok(quote! {
            client HTTPApi { base "https://example.test"; }
            scope http_status() {
                GET get_http_status(
                    first: Option<String>,
                    #[body] payload: Text,
                    second: std::option::Option<u32>,
                    alias_value: crate::MaybeString,
                ) -> Bytes {
                    id "http.status_v2.get";
                }
                GET r#type() -> Bytes;
            }
        });
        let SourceChild::Scope(scope) = &api.children[0] else {
            panic!("scope")
        };
        let SourceChild::Endpoint(endpoint) = &scope.children[0] else {
            panic!("endpoint")
        };
        assert!(matches!(endpoint.parameters[0].kind, ValueKind::Optional));
        assert_eq!(endpoint.body.as_ref().unwrap().position, 1);
        assert!(matches!(endpoint.parameters[1].kind, ValueKind::Optional));
        assert!(matches!(endpoint.parameters[2].kind, ValueKind::Required));
        assert_eq!(
            endpoint.contract_id.as_ref().unwrap().value.value(),
            "http.status_v2.get"
        );

        for (tokens, expected) in [
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET PascalCase() -> Bytes;
                },
                "must be snake_case",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    scope PascalCase() { }
                },
                "must be snake_case",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    POST inferred(body: Text) -> Bytes;
                },
                "inference by parameter name was removed",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    POST duplicate(#[body] first: Text, #[body] second: Bytes) -> Bytes;
                },
                "two request body parameters",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    POST malformed(#[body(value)] payload: Text) -> Bytes;
                },
                "bare `#[body]`",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    POST defaulted(#[body] payload: Text = crate::DEFAULT) -> Bytes;
                },
                "cannot have a default",
            ),
            (
                quote! {
                    client Api { base "https://example.test"; }
                    GET old_name() as renamed -> Bytes;
                },
                "aliases were removed",
            ),
        ] {
            let error = parse(tokens).err().expect("invalid v2 syntax");
            assert!(
                error.to_string().contains(expected),
                "expected `{expected}`, got `{error}`"
            );
        }
    }

    #[test]
    fn stable_id_format_and_removed_policy_or_extras_syntax_are_focused() {
        for invalid in [
            "",
            ".leading",
            "trailing.",
            "two..dots",
            "Upper.case",
            "-leading",
            "trailing-",
            "contains/slash",
        ] {
            let invalid = LitStr::new(invalid, proc_macro2::Span::call_site());
            let error = parse(quote! {
                client Api { base "https://example.test"; }
                GET read() -> Bytes { id #invalid; }
            })
            .err()
            .expect("invalid stable ID");
            assert!(error.to_string().contains("endpoint ID"));
        }

        let retry = parse(quote! {
            client Api {
                base "https://example.test";
                retry custom crate::retry;
            }
        })
        .err()
        .expect("removed custom retry DSL");
        assert!(retry.to_string().contains("runtime policy"));

        let observer = parse(quote! {
            client Api {
                base "https://example.test";
                rate_limit_observer crate::Observer;
            }
        })
        .err()
        .expect("removed observer DSL");
        assert!(observer.to_string().contains("runtime policy"));

        let oauth = parse(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    credential token = oauth2_client(crate::Config);
                }
            }
        })
        .err()
        .expect("removed OAuth-specific syntax");
        assert!(oauth.to_string().contains("credential source"));
    }
}
