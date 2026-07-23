use std::collections::HashSet;

use http::Method;
use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, braced, parenthesized};

use super::ast::{
    Clause, Layer, MethodKind, RetrySelection, SourceApi, SourceChild, SourceClient,
    SourceEndpoint, SourceScope,
};
use super::credentials::{parse_auth, parse_credentials};
use super::diagnostics::{
    SpanError, insert_clause, merge_auth, validate_base, validate_contract_id,
    validate_snake_source_identifier,
};
use super::keyword;
use super::pagination::parse_pagination;
use super::rate_limits::parse_rate_limit;
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

#[cfg(test)]
mod tests {
    use quote::{format_ident, quote};

    use super::super::ast::{AuthOperation, DefaultExpr, ValueKind};
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
