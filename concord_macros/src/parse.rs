use std::collections::HashSet;
use std::time::Duration;

use http::{HeaderName, Method};
use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    GenericArgument, Ident, LitBool, LitInt, LitStr, PathArguments, Token, Type, TypePath, braced,
    bracketed, parenthesized,
};
use url::Url;

use crate::ast::{
    AuthChallenge, AuthOperation, AuthPlacement, AuthPlacementKind, CredentialDecl,
    CredentialSource, CredentialsBlock, DefaultExpr, DurationLiteral, HeaderEntry, HeaderOperation,
    HostOperation, Layer, MethodKind, PaginationDecl, PaginationField, PaginationInitializer,
    PaginationValue, PathOperation, PublicExpr, QueryEntry, QueryOperation, RateLimitDecl,
    RateLimitWindowLiteral, RetrySelection, SecretDecl, SourceApi, SourceChild, SourceClient,
    SourceEndpoint, SourceScope, TimeoutOperation, ValueDecl, ValueKind,
};
use crate::names::canonical_ident;

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
    syn::custom_keyword!(client_id);
    syn::custom_keyword!(client_secret);
    syn::custom_keyword!(credential);
    syn::custom_keyword!(credentials);
    syn::custom_keyword!(cost);
    syn::custom_keyword!(custom);
    syn::custom_keyword!(endpoint);
    syn::custom_keyword!(fmt);
    syn::custom_keyword!(headers);
    syn::custom_keyword!(header);
    syn::custom_keyword!(host);
    syn::custom_keyword!(METHOD);
    syn::custom_keyword!(method);
    syn::custom_keyword!(off);
    syn::custom_keyword!(oauth2_client);
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
    syn::custom_keyword!(token_url);
    syn::custom_keyword!(timeout);
    syn::custom_keyword!(var);
    syn::custom_keyword!(never);
    syn::custom_keyword!(unauthorized);
    syn::custom_keyword!(unauthorized_or_forbidden);
}

pub(crate) fn parse(input: TokenStream) -> syn::Result<SourceApi> {
    syn::parse2(input)
}

impl Parse for SourceApi {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Err(
                input.error("an `api!` invocation requires at least one `client` declaration")
            );
        }

        let mut clients = Vec::new();
        let mut names = HashSet::new();
        while !input.is_empty() {
            let client: SourceClient = input.parse()?;
            let canonical = canonical_ident(&client.name);
            if !names.insert(canonical.clone()) {
                return Err(client
                    .name
                    .span()
                    .error(format!("duplicate client `{canonical}`")));
            }
            clients.push(client);
        }
        Ok(Self { clients })
    }
}

impl Parse for SourceClient {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<keyword::client>()?;
        let name: Ident = input.parse()?;
        let body;
        braced!(body in input);

        if !body.peek(keyword::base) {
            return Err(
                body.error("a client must begin with exactly one fixed `base \"https://...\";`")
            );
        }
        body.parse::<keyword::base>()?;
        let base: LitStr = body.parse()?;
        body.parse::<Token![;]>()?;
        validate_base(&base)?;

        let mut values = Vec::new();
        let mut value_names = HashSet::new();
        while body.peek(keyword::var) {
            body.parse::<keyword::var>()?;
            let value = parse_value(&body)?;
            body.parse::<Token![;]>()?;
            insert_unique_value(&mut value_names, &value, "client value")?;
            values.push(value);
        }

        let credentials = body
            .peek(keyword::credentials)
            .then(|| parse_credentials(&body))
            .transpose()?;

        let retry = if body.peek(keyword::retry) {
            parse_retry(&body)?
        } else {
            RetrySelection::Disabled
        };
        let layer = parse_ordered_layer(&body, true)?;
        let rate_limit_observer = if body.peek(keyword::rate_limit_observer) {
            body.parse::<keyword::rate_limit_observer>()?;
            let path = body.parse()?;
            body.parse::<Token![;]>()?;
            if body.peek(keyword::rate_limit_observer) {
                return Err(body.error(
                    "a client may contain at most one `rate_limit_observer PATH;` declaration",
                ));
            }
            Some(path)
        } else {
            None
        };
        let children = parse_children(&body, 0, "client")?;

        if !body.is_empty() {
            return Err(client_tail_error(&body));
        }

        Ok(Self {
            name,
            base,
            values,
            credentials,
            retry,
            layer,
            rate_limit_observer,
            children,
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

    let mut names = HashSet::new();
    for child in &children {
        let name = match child {
            SourceChild::Scope(scope) => &scope.name,
            SourceChild::Endpoint(endpoint) => &endpoint.name,
        };
        let normalized = crate::names::to_snake_case(&canonical_ident(name));
        if !names.insert(normalized.clone()) {
            return Err(name.span().error(format!(
                "generated child name `{normalized}` collides within this {owner} after deterministic Rust normalization"
            )));
        }
    }
    Ok(children)
}

fn parse_scope(input: ParseStream<'_>, depth: usize) -> syn::Result<SourceScope> {
    if depth > 64 {
        return Err(input
            .span()
            .error("scope nesting exceeds the maximum supported depth of 64"));
    }
    input.parse::<keyword::scope>()?;
    let name: Ident = input.parse()?;
    let parameters_content;
    parenthesized!(parameters_content in input);
    let parameters = parse_parameters(&parameters_content, "scope", false)?.0;

    let body;
    braced!(body in input);
    let layer = parse_ordered_layer(&body, true)?;
    let children = parse_children(&body, depth, "scope")?;
    if !body.is_empty() {
        if body.peek(keyword::retry) {
            return Err(body.error("`retry` is client-only and cannot be declared in a scope"));
        }
        if body.peek(keyword::rate_limit_observer) {
            return Err(body.error("rate-limit observers are client-only; use `rate_limit_observer PATH;` on the client"));
        }
        if body.peek(keyword::var)
            || body.peek(keyword::base)
            || body.peek(keyword::credentials)
            || body.peek(keyword::rate_limits)
        {
            return Err(body.error("this declaration is client-only and cannot appear in a scope"));
        }
        if is_layer_start(&body) {
            return Err(body.error(
                "scope configuration must appear once, in normative order, before child scopes and endpoints",
            ));
        }
        return Err(body.error("unsupported scope declaration"));
    }

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
    if !input.peek(syn::token::Paren) {
        return Err(input.error("every endpoint declaration requires parentheses, including `()`"));
    }
    let parameters_content;
    parenthesized!(parameters_content in input);
    let (parameters, body) = parse_parameters(&parameters_content, "endpoint", true)?;

    let alias = if input.peek(Token![as]) {
        input.parse::<Token![as]>()?;
        Some(input.parse()?)
    } else {
        None
    };
    input.parse::<Token![->]>()?;
    let response = input.parse()?;

    let layer = if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        Layer::default()
    } else {
        let clauses;
        braced!(clauses in input);
        let layer = parse_ordered_layer(&clauses, true)?;
        let pagination = clauses
            .peek(keyword::paginate)
            .then(|| parse_pagination(&clauses))
            .transpose()?;
        if pagination.is_some() && body.is_some() {
            return Err(clauses
                .span()
                .error("pagination requires a bodyless endpoint"));
        }
        if clauses.peek(keyword::retry) {
            return Err(
                clauses.error("`retry` is client-only and cannot be declared on an endpoint")
            );
        }
        if !clauses.is_empty() {
            if clauses.peek(keyword::rate_limit_observer) {
                return Err(clauses.error("rate-limit observers are client-only; use `rate_limit_observer PATH;` on the client"));
            }
            if is_layer_start(&clauses) {
                return Err(clauses.error(
                    "endpoint clauses must appear at most once and in `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, `paginate` order",
                ));
            }
            return Err(clauses.error("unsupported endpoint clause"));
        }
        return Ok(SourceEndpoint {
            method,
            name,
            alias,
            parameters,
            body,
            layer,
            response,
            pagination,
        });
    };

    Ok(SourceEndpoint {
        method,
        name,
        alias,
        parameters,
        body,
        layer,
        response,
        pagination: None,
    })
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
) -> syn::Result<(Vec<ValueDecl>, Option<Type>)> {
    let mut parameters = Vec::new();
    let mut body = None;
    let mut names = HashSet::new();

    while !input.is_empty() {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let canonical = canonical_ident(&name);
        if !names.insert(canonical.clone()) {
            return Err(name
                .span()
                .error(format!("duplicate {owner} parameter `{canonical}`")));
        }

        if body_is_special && canonical == "body" {
            if input.peek(Token![=]) {
                return Err(input.error("a request body parameter cannot have a default"));
            }
            body = Some(ty);
        } else {
            let kind = parse_value_kind(input, &ty)?;
            parameters.push(ValueDecl { name, ty, kind });
        }

        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
        if body.is_some() && !input.is_empty() {
            return Err(input.error("the request body must be the final endpoint parameter"));
        }
    }
    Ok((parameters, body))
}

fn parse_value(input: ParseStream<'_>) -> syn::Result<ValueDecl> {
    let name: Ident = input.parse()?;
    input.parse::<Token![:]>()?;
    let ty: Type = input.parse()?;
    let kind = parse_value_kind(input, &ty)?;
    Ok(ValueDecl { name, ty, kind })
}

fn parse_value_kind(input: ParseStream<'_>, ty: &Type) -> syn::Result<ValueKind> {
    let optional = outer_type_argument(ty, "Option")?;
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
        input.parse::<keyword::custom>()?;
        RetrySelection::Custom(input.parse()?)
    } else {
        return Err(input.error("retry must be `off`, `reqwest_default`, or `custom PATH`"));
    };
    input.parse::<Token![;]>()?;
    Ok(selection)
}

fn parse_ordered_layer(input: ParseStream<'_>, include_rate_limit: bool) -> syn::Result<Layer> {
    let host = input
        .peek(keyword::host)
        .then(|| parse_host(input))
        .transpose()?;
    let path = input
        .peek(keyword::path)
        .then(|| parse_path(input))
        .transpose()?;
    let query = input
        .peek(keyword::query)
        .then(|| parse_query(input))
        .transpose()?;
    let headers = input
        .peek(keyword::headers)
        .then(|| parse_headers(input))
        .transpose()?;
    let auth = input
        .peek(keyword::auth)
        .then(|| parse_auth(input))
        .transpose()?;
    let timeout = input
        .peek(keyword::timeout)
        .then(|| parse_timeout(input))
        .transpose()?;
    let rate_limit = if include_rate_limit && input.peek(keyword::rate_limit) {
        let declaration = parse_rate_limit(input)?;
        if input.peek(keyword::rate_limit) {
            return Err(input.error(
                "a lexical layer may contain at most one direct `rate_limit { ... }` declaration",
            ));
        }
        Some(declaration)
    } else {
        None
    };
    Ok(Layer {
        host,
        path,
        query,
        headers,
        auth,
        timeout,
        rate_limit,
    })
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
    if input.peek(keyword::oauth2_client) {
        input.parse::<keyword::oauth2_client>()?;
        return parse_oauth2_source(input, declared_secrets);
    }
    if input.peek(keyword::endpoint) {
        input.parse::<keyword::endpoint>()?;
        return Ok(CredentialSource::Endpoint {
            path: input.parse()?,
        });
    }
    if input.peek(keyword::custom) {
        input.parse::<keyword::custom>()?;
        let material: Type = input.parse()?;
        if !matches!(material, Type::Path(_)) {
            return Err(material
                .span()
                .error("a custom credential material must be a Rust type path"));
        }
        return Ok(CredentialSource::Custom { material });
    }
    Err(input.error(
        "credential source must be `api_key`, `bearer`, `basic`, `oauth2_client`, `endpoint`, or `custom`",
    ))
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

fn parse_oauth2_source(
    input: ParseStream<'_>,
    declared_secrets: &HashSet<String>,
) -> syn::Result<CredentialSource> {
    let body;
    braced!(body in input);
    let mut token_url = None;
    let mut client_id = None;
    let mut client_secret = None;
    let mut scope = None;
    let mut fields = HashSet::new();
    while !body.is_empty() {
        let field: Ident = body.parse()?;
        let field_name = canonical_ident(&field);
        if !fields.insert(field_name.clone()) {
            return Err(field
                .span()
                .error(format!("duplicate OAuth2 field `{field_name}`")));
        }
        body.parse::<Token![:]>()?;
        match field_name.as_str() {
            "token_url" => {
                let literal: LitStr = body.parse()?;
                let parsed = Url::parse(&literal.value()).map_err(|_| {
                    literal
                        .span()
                        .error("OAuth2 token_url must be an absolute HTTPS URL")
                })?;
                if parsed.scheme() != "https"
                    || parsed.host().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                    || parsed.fragment().is_some()
                {
                    return Err(literal.span().error(
                        "OAuth2 token_url must use HTTPS with a host and no user information or fragment",
                    ));
                }
                token_url = Some(literal);
            }
            "client_id" => {
                client_id = Some(parse_secret_reference(&body, declared_secrets)?);
            }
            "client_secret" => {
                client_secret = Some(parse_secret_reference(&body, declared_secrets)?);
            }
            "scope" => scope = Some(body.parse()?),
            _ => {
                return Err(field
                    .span()
                    .error(format!("unknown OAuth2 field `{field_name}`")));
            }
        }
        if body.peek(Token![,]) {
            body.parse::<Token![,]>()?;
        } else if !body.is_empty() {
            return Err(body.error("OAuth2 fields must be separated by commas"));
        }
    }
    Ok(CredentialSource::OAuth2Client {
        token_url: token_url.ok_or_else(|| input.error("OAuth2 requires `token_url`"))?,
        client_id: client_id.ok_or_else(|| input.error("OAuth2 requires `client_id`"))?,
        client_secret: client_secret
            .ok_or_else(|| input.error("OAuth2 requires `client_secret`"))?,
        scope,
    })
}

fn parse_auth(input: ParseStream<'_>) -> syn::Result<AuthOperation> {
    let mut replace = false;
    let mut placements = Vec::new();
    while input.peek(keyword::auth) {
        input.parse::<keyword::auth>()?;
        if input.peek(keyword::off) {
            input.parse::<keyword::off>()?;
            input.parse::<Token![;]>()?;
            if replace || !placements.is_empty() || input.peek(keyword::auth) {
                return Err(input.error("`auth off` is exclusive within its layer"));
            }
            return Ok(AuthOperation::Off);
        }
        let local_replace = input.peek(keyword::replace);
        if local_replace {
            let marker: keyword::replace = input.parse()?;
            if replace || !placements.is_empty() {
                return Err(marker.span().error(
                    "at most one `auth replace` is allowed and it must precede local auth declarations",
                ));
            }
            replace = true;
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
            return Err(input
                .error("auth placement must be `bearer`, `basic`, `header`, `query`, or `off`"));
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
        placements.push(AuthPlacement {
            kind,
            credential,
            challenge,
        });
    }
    Ok(AuthOperation::Modify {
        replace,
        placements,
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

fn single_type_argument(path: &TypePath, marker: &str) -> syn::Result<Type> {
    let segment = &path.path.segments[0];
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return Err(segment
            .span()
            .error(format!("`{marker}` requires exactly one type argument")));
    };
    let mut values = arguments.args.iter();
    let Some(GenericArgument::Type(ty)) = values.next() else {
        return Err(arguments
            .span()
            .error(format!("`{marker}` requires exactly one type argument")));
    };
    if values.next().is_some() {
        return Err(arguments
            .span()
            .error(format!("`{marker}` requires exactly one type argument")));
    }
    Ok(ty.clone())
}

fn outer_type_argument(ty: &Type, marker: &str) -> syn::Result<Option<Type>> {
    let Type::Path(path) = ty else {
        return Ok(None);
    };
    if !is_unqualified(path, marker) {
        return Ok(None);
    }
    single_type_argument(path, marker).map(Some)
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

fn is_layer_start(input: ParseStream<'_>) -> bool {
    input.peek(keyword::host)
        || input.peek(keyword::path)
        || input.peek(keyword::query)
        || input.peek(keyword::headers)
        || input.peek(keyword::auth)
        || input.peek(keyword::timeout)
        || input.peek(keyword::rate_limit)
        || input.peek(keyword::paginate)
}

fn client_tail_error(input: ParseStream<'_>) -> syn::Error {
    if input.peek(keyword::base) {
        return input.error("a client requires exactly one `base` declaration");
    }
    if input.peek(keyword::rate_limit) {
        return input.error(
            "a lexical layer may contain at most one direct `rate_limit { ... }` declaration",
        );
    }
    if input.peek(keyword::var) || input.peek(keyword::retry) || is_layer_start(input) {
        return input.error(
            "client configuration must appear once, in normative order, before every child scope or endpoint",
        );
    }
    if input.peek(keyword::credentials) {
        return input.error("a client may contain at most one `credentials` block, before retry and request configuration");
    }
    if input.peek(keyword::rate_limits) {
        return input.error(
            "`rate_limits { ... }` was removed; declare `rate_limit { ... }` at each lexical owner",
        );
    }
    if input.peek(keyword::observe) {
        return input.error(
            "`observe PATH;` was removed; declare `rate_limit_observer PATH;` on the client",
        );
    }
    input.error("unsupported client declaration")
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
    fn parses_multiple_clients_roots_nested_values_and_all_shaping_layers() {
        let api = parse_ok(quote! {
            client FirstApi {
                base "https://example.test:8443/fixed";
                var tenant: String;
                var locale: Option<String>;
                var limit: u32 = 50;
                retry reqwest_default;
                host ["api"];
                path ["v1"];
                query { "tenant" = vars.tenant; }
                headers { "X-Locale" = vars.locale; }
                timeout 5s;
                GET Root(id: String, verbose: Option<bool>, page: u32 = 10) as root -> Bytes {
                    path [ep.id];
                }
                scope regions(region: String, language: Option<String>) {
                    host [scope.region, "api"];
                    path ["regions", scope.region];
                    scope users(account: String) {
                        path ["users", scope.account];
                        METHOD("PROPFIND") Inspect() -> Text;
                    }
                }
            }
            client SecondApi {
                base "http://example.test";
                CONNECT Tunnel() -> NoContent;
            }
        });
        assert_eq!(api.clients.len(), 2);
        assert_eq!(api.clients[0].children.len(), 2);
    }

    #[test]
    fn rejects_arbitrary_defaults_and_public_expressions() {
        let default_error = parse(quote! {
            client Api {
                base "https://example.test";
                var value: String = make_value();
            }
        })
        .err()
        .expect("call default");
        assert!(default_error.to_string().contains("Rust const path"));

        let expression_error = parse(quote! {
            client Api {
                base "https://example.test";
                GET Root(id: String) -> Bytes { path [ep.id.to_string()]; }
            }
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
            client Api { base "https://example.test"; METHOD("BAD METHOD") Root() -> Bytes; }
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
        let mut child = quote!(GET Leaf() -> Bytes;);
        for index in 0..depth {
            let name = format_ident!("scope_{index}");
            child = quote!(scope #name() { #child });
        }
        quote!(client Api { base "https://example.test"; #child })
    }

    fn nested_format(depth: usize) -> TokenStream {
        let mut expression = quote!("leaf");
        for _ in 0..depth {
            expression = quote!(fmt[#expression]);
        }
        quote! {
            client Api {
                base "https://example.test";
                GET Leaf() -> Bytes { path [#expression]; }
            }
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
        let empty = parse(TokenStream::new()).err().expect("zero clients");
        assert!(empty.to_string().contains("at least one `client`"));

        let duplicate_client = parse(quote! {
            client Api { base "https://one.example.test"; }
            client Api { base "https://two.example.test"; }
        })
        .err()
        .expect("duplicate clients");
        assert!(
            duplicate_client
                .to_string()
                .contains("duplicate client `Api`")
        );

        let body_order = parse(quote! {
            client Api {
                base "https://example.test";
                POST Create(body: Text, id: String) -> Bytes;
            }
        })
        .err()
        .expect("body must be last");
        assert!(body_order.to_string().contains("final endpoint parameter"));

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
                var text: &'static str = "value";
                var negative: i64 = -7;
                var enabled: bool = true;
                var constant: usize = crate::DEFAULT_SIZE;
                host ["a12345678901234567890123456789012345678901234567890123456789012"];
                GET Read(id: u32) -> Bytes { path [fmt["id-", ep.id]]; }
            }
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
                GET Read(id: u32) -> Bytes { path [fmt["id-" ep.id]]; }
            }
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
                    secret client_id: String;
                    secret client_secret: String;
                    credential key = api_key(secret.api_key);
                    credential token = bearer(secret.bearer);
                    credential login = basic(secret.username, secret.password);
                    credential oauth = oauth2_client {
                        token_url: "https://auth.example.test/token",
                        client_id: secret.client_id,
                        client_secret: secret.client_secret,
                        scope: "users.read",
                    };
                    credential acquired = endpoint Login;
                    credential custom = custom crate::SessionToken;
                }
                GET Login() -> Bytes;
            }
        });
        let credentials = api.clients[0]
            .credentials
            .as_ref()
            .expect("credentials block");
        assert_eq!(credentials.secrets.len(), 6);
        assert_eq!(credentials.credentials.len(), 6);

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
        assert!(
            error
                .to_string()
                .contains("at most one `credentials` block")
        );
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
                scope Public() { auth off; }
            }
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
        assert!(late_replace.to_string().contains("must precede"));

        let ordered = parse(quote! {
            client AuthApi {
                base "https://example.test";
                timeout 1s;
                auth bearer session;
            }
        })
        .err()
        .expect("auth after timeout");
        assert!(ordered.to_string().contains("normative order"));
    }

    #[test]
    fn oauth_fields_and_secret_shapes_have_focused_diagnostics() {
        let optional = parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials { secret token: Option<String>; }
            }
        })
        .err()
        .expect("optional secret");
        assert!(optional.to_string().contains("required `String`"));

        let unsafe_url = parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret id: String;
                    secret secret: String;
                    credential oauth = oauth2_client {
                        token_url: "http://auth.example.test/token",
                        client_id: secret.id,
                        client_secret: secret.secret,
                    };
                }
            }
        })
        .err()
        .expect("cleartext OAuth URL");
        assert!(unsafe_url.to_string().contains("must use HTTPS"));

        let duplicate = parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret id: String;
                    secret secret: String;
                    credential oauth = oauth2_client {
                        token_url: "https://auth.example.test/token",
                        token_url: "https://auth.example.test/other",
                        client_id: secret.id,
                        client_secret: secret.secret,
                    };
                }
            }
        })
        .err()
        .expect("duplicate OAuth field");
        assert!(duplicate.to_string().contains("duplicate OAuth2 field"));
    }

    #[test]
    fn lexical_rate_limit_declarations_parse_in_order() {
        let api = parse_ok(quote! {
            client RateApi {
                base "https://example.test";
                var tenant: String;
                timeout 5s;
                rate_limit {
                    500 / 10s;
                    30000 / 10m;
                }
                rate_limit_observer crate::ProviderObserver;
                scope tenants(tenant: String) {
                    rate_limit {
                        100 / 1m;
                    }
                    GET Read(item: String) -> Bytes {
                        timeout 2s;
                        rate_limit {
                            cost 2;
                            10 / 1s;
                        }
                    }
                    GET Public() -> Bytes;
                }
            }
        });
        let client = &api.clients[0];
        assert_eq!(
            client.rate_limit_observer.as_ref().unwrap().segments.len(),
            2
        );
        assert_eq!(client.layer.rate_limit.as_ref().unwrap().windows.len(), 2);
        let SourceChild::Scope(scope) = &client.children[0] else {
            panic!("scope")
        };
        assert!(scope.layer.rate_limit.is_some());
        let SourceChild::Endpoint(read) = &scope.children[0] else {
            panic!("read endpoint")
        };
        assert_eq!(read.layer.rate_limit.as_ref().unwrap().cost, 2);
        let SourceChild::Endpoint(public) = &scope.children[1] else {
            panic!("public endpoint")
        };
        assert!(public.layer.rate_limit.is_none());
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
        assert!(duplicate_rule.to_string().contains("at most one direct"));

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
        assert!(old_observer.to_string().contains("rate_limit_observer"));

        let endpoint_order = parse(quote! {
            client Api {
                base "https://example.test";
                GET Read() -> Bytes {
                    rate_limit { 1 / 1s; }
                    timeout 1s;
                }
            }
        })
        .err()
        .expect("rate limit before timeout");
        assert!(endpoint_order.to_string().contains("endpoint clauses"));
    }

    #[test]
    fn pagination_struct_forms_parse_with_restricted_initializers() {
        let api = parse_ok(quote! {
            client Api {
                base "https://example.test";
                GET Offset(offset: u64, limit: u32) -> Json<Vec<u32>> {
                    paginate concord_core::advanced::OffsetPagination {
                        offset: bind ep.offset,
                        page_size: bind ep.limit,
                    }
                }
                GET Page(page: u64, per_page: u32) -> Json<Vec<u32>> {
                    paginate PagePagination {
                        page: bind ep.page,
                        page_size: bind ep.per_page,
                    }
                }
                GET Cursor(cursor: Option<String>) -> Json<Vec<u32>> {
                    paginate CursorPagination<String> { cursor: bind ep.cursor }
                }
                scope Regions(region: String) {
                    GET Custom(cursor: Option<String>, maximum: u32) -> Json<Vec<u32>> {
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
            }
        });
        assert_eq!(api.clients[0].children.len(), 4);
    }

    #[test]
    fn pagination_rejects_old_forms_and_unsafe_initializers() {
        let cases = [
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64, limit: u32) -> Json<Vec<u32>> {
                            paginate OffsetPagination
                        }
                    }
                },
                "struct field block",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64, limit: u32) -> Json<Vec<u32>> {
                            paginate OffsetPagination { offset = ep.offset }
                        }
                    }
                },
                "field: bind ep.field",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64, limit: u32) -> Json<Vec<u32>> {
                            paginate OffsetPagination {
                                offset: bind ep.offset,
                                offset: bind ep.limit,
                            }
                        }
                    }
                },
                "duplicate pagination controller field",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64, limit: u32) -> Json<Vec<u32>> {
                            paginate OffsetPagination {
                                offset: bind ep.offset,
                                page_size: bind ep.offset,
                            }
                        }
                    }
                },
                "duplicate pagination binding destination",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64, limit: u32) -> Json<Vec<u32>> {
                            paginate OffsetPagination { offset: bind vars.offset }
                        }
                    }
                },
                "accepts only direct `ep.FIELD` or `scope.FIELD`",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64, limit: u32) -> Json<Vec<u32>> {
                            paginate OffsetPagination { offset: bind ep.state.offset }
                        }
                    }
                },
                "must be direct",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64) -> Json<Vec<u32>> {
                            paginate OffsetPagination { offset }
                        }
                    }
                },
                "field shorthand",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read(offset: u64) -> Json<Vec<u32>> {
                            paginate OffsetPagination { .. }
                        }
                    }
                },
                "rest syntax",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read() -> Json<Vec<u32>> {
                            paginate crate::Controller { value: crate::make() }
                        }
                    }
                },
                "calls, methods, macros",
            ),
            (
                quote! {
                    client Api { base "https://example.test";
                        GET Read() -> Json<Vec<u32>> {
                            paginate custom crate::factory();
                        }
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
}
