use std::collections::{HashMap, HashSet};
use std::time::Duration;

use syn::ext::IdentExt;
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitInt, LitStr, Token, braced, bracketed, parenthesized};

use super::ast::{
    Clause, DurationLiteral, RateLimitAttachmentDecl, RateLimitBindingDecl, RateLimitBindingValue,
    RateLimitDecl, RateLimitDefinitionDecl, RateLimitDefinitionDimensionDecl,
    RateLimitDimensionDecl, RateLimitUseDecl, RateLimitWindowLiteral,
};
use super::diagnostics::{
    SpanError, validate_duration_integer, validate_integer, validate_snake_source_identifier,
};
use super::keyword;
use crate::{
    MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
    MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
};

pub(super) fn parse_rate_limits(
    input: ParseStream<'_>,
) -> syn::Result<Vec<RateLimitDefinitionDecl>> {
    input.parse::<keyword::rate_limits>()?;
    let body;
    braced!(body in input);
    let mut definitions = Vec::new();
    let mut names = HashMap::new();
    while !body.is_empty() {
        if body.peek(keyword::rule) {
            return Err(body.error(
                "v1 `rule NAME ... by [...]` syntax was removed; write `NAME(formals...) { key [...]; allow N per D; }`",
            ));
        }
        let name: Ident = body
            .parse()
            .map_err(|_| body.error("a rate-limit profile requires a name"))?;
        validate_snake_source_identifier(&name, "rate-limit profile")?;
        let canonical_name = canonical_ident(&name);
        if let Some(first) = names.insert(canonical_name.clone(), name.span()) {
            let mut error = name.span().error(format!(
                "rate-limit profile `{canonical_name}` is defined more than once"
            ));
            error.combine(first.error("the first profile using this name is here"));
            return Err(error);
        }
        let formals_body;
        parenthesized!(formals_body in body);
        let mut formals = HashSet::new();
        let mut formal_names = Vec::new();
        while !formals_body.is_empty() {
            let formal = formals_body.call(Ident::parse_any)?;
            validate_snake_source_identifier(&formal, "rate-limit formal")?;
            let canonical = canonical_ident(&formal);
            if matches!(canonical.as_str(), "host" | "endpoint") {
                return Err(formal.span().error(
                    "`host` and `endpoint` are built-in key dimensions and cannot be profile formals",
                ));
            }
            if !formals.insert(canonical.clone()) {
                return Err(formal
                    .span()
                    .error(format!("duplicate rate-limit formal `{canonical}`")));
            }
            formal_names.push(formal);
            if formals_body.is_empty() {
                break;
            }
            formals_body.parse::<Token![,]>()?;
        }
        definitions.push(parse_rate_limit_definition(
            &body,
            name,
            &formals,
            formal_names,
        )?);
    }
    Ok(definitions)
}

pub(super) fn parse_rate_limit(input: ParseStream<'_>) -> syn::Result<RateLimitAttachmentDecl> {
    let marker: keyword::rate_limit = input.parse()?;
    if input.peek(Token![use]) {
        return Err(input.error(
            "v1 `rate_limit use NAME` syntax was removed; write `rate_limit NAME(formal = owner.value)`",
        ));
    }
    if input.peek(syn::token::Brace) {
        return parse_inline_rate_limit(input, marker.span()).map(RateLimitAttachmentDecl::Inline);
    }
    if input.peek(Ident::peek_any) {
        return parse_rate_limit_use(input).map(RateLimitAttachmentDecl::Named);
    }
    Err(input.error(
        "a rate-limit attachment must be `rate_limit { ... }` or `rate_limit PROFILE(bindings...)`",
    ))
}

fn parse_rate_limit_use(input: ParseStream<'_>) -> syn::Result<RateLimitUseDecl> {
    let name: Ident = input
        .call(Ident::parse_any)
        .map_err(|_| input.error("a named rate-limit attachment requires a profile name"))?;
    validate_snake_source_identifier(&name, "rate-limit profile")?;
    if !input.peek(syn::token::Paren) {
        return Err(
            input.error("a named rate-limit attachment must be `rate_limit PROFILE(bindings...);`")
        );
    }
    let body;
    parenthesized!(body in input);
    let mut bindings = Vec::new();
    let mut binding_names = HashMap::new();
    while !body.is_empty() {
        let binding_name = body
            .call(Ident::parse_any)
            .map_err(|_| body.error("a named rate-limit binding must be `FORMAL = owner.NAME`"))?;
        let canonical_binding = canonical_ident(&binding_name);
        if let Some(first) = binding_names.insert(canonical_binding.clone(), binding_name.span()) {
            let mut error = binding_name.span().error(format!(
                "duplicate binding for rate-limit dimension `{canonical_binding}`"
            ));
            error.combine(first.error("the first binding for this dimension is here"));
            return Err(error);
        }
        body.parse::<Token![=]>().map_err(|_| {
            body.error("a named rate-limit binding requires `=` before its value reference")
        })?;
        let value = parse_binding_value(&body)?;
        bindings.push(RateLimitBindingDecl {
            name: binding_name,
            value,
        });
        if body.is_empty() {
            break;
        }
        body.parse::<Token![,]>()?;
    }
    let mut cost: Option<Clause<u32>> = None;
    if input.peek(syn::token::Brace) {
        let overrides;
        braced!(overrides in input);
        if !overrides.peek(keyword::cost) {
            return Err(overrides.error("a named rate-limit override block accepts only `cost N;`"));
        }
        let marker: keyword::cost = overrides.parse()?;
        let literal: LitInt = overrides.parse()?;
        cost = Some(Clause {
            key_span: marker.span(),
            value: parse_positive_u32(&literal, "rate-limit cost")?,
        });
        overrides.parse::<Token![;]>()?;
        if !overrides.is_empty() {
            return Err(overrides.error("a named rate-limit override block accepts one `cost N;`"));
        }
    } else {
        input.parse::<Token![;]>()?;
    }
    Ok(RateLimitUseDecl {
        name,
        bindings,
        cost,
    })
}

fn parse_binding_value(input: ParseStream<'_>) -> syn::Result<RateLimitBindingValue> {
    let namespace = input.call(Ident::parse_any).map_err(|_| {
        input.error("a named rate-limit binding value must be exactly one total owner reference")
    })?;
    let namespace_name = canonical_ident(&namespace);
    match namespace_name.as_str() {
        "vars" => {
            return Err(namespace
                .span()
                .error("v1 `vars.NAME` was removed; use `client.NAME`"));
        }
        "scope" => {
            return Err(namespace
                .span()
                .error("v1 `scope.NAME` was removed; use the actual scope identifier"));
        }
        "ep" => {
            return Err(namespace
                .span()
                .error("v1 `ep.NAME` was removed; use `endpoint.NAME`"));
        }
        _ => {}
    }
    input.parse::<Token![.]>().map_err(|_| {
        namespace
            .span()
            .error("a named rate-limit binding value must be exactly one total owner reference")
    })?;
    let name = input.call(Ident::parse_any).map_err(|_| {
        input.error("a named rate-limit binding value requires a name after its namespace")
    })?;
    Ok(RateLimitBindingValue {
        span: name.span(),
        namespace,
        name,
    })
}

fn parse_inline_rate_limit(
    input: ParseStream<'_>,
    attachment_span: proc_macro2::Span,
) -> syn::Result<RateLimitDecl> {
    let body;
    braced!(body in input);

    let mut id: Option<Clause<LitStr>> = None;
    let mut dimensions = None;
    let mut cost: Option<Clause<u32>> = None;
    let mut windows = Vec::new();
    while !body.is_empty() {
        if body.peek(keyword::id) {
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `key`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::id = body.parse()?;
            if let Some(first) = &id {
                let mut error = marker
                    .span()
                    .error("an anonymous rate-limit attachment permits at most one `id` clause");
                error.combine(first.key_span.error("the first `id` clause is here"));
                return Err(error);
            }
            let value: LitStr = body
                .parse()
                .map_err(|_| body.error("a rate-limit `id` clause requires a string literal"))?;
            validate_rate_limit_id(&value)?;
            body.parse::<Token![;]>()?;
            id = Some(Clause {
                key_span: marker.span(),
                value,
            });
            continue;
        }
        if body.peek(keyword::by) {
            return Err(body.error("v1 `by [...]` was removed; write `key [...]`"));
        }
        if body.peek(keyword::key) {
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `key`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::key = body.parse()?;
            if dimensions.is_some() {
                return Err(marker
                    .span()
                    .error("an anonymous rate-limit attachment permits at most one `key` clause"));
            }
            let dimension_body;
            bracketed!(dimension_body in body);
            if dimension_body.is_empty() {
                return Err(marker.span().error(
                    "omit `key` for a declaration-global bucket; `key [...]` requires at least one dimension",
                ));
            }
            let mut parsed = Vec::new();
            let mut names = HashSet::new();
            while !dimension_body.is_empty() {
                let dimension = parse_rate_limit_dimension(&dimension_body)?;
                if parsed.len() >= MAX_RATE_LIMIT_KEY_PARTS {
                    return Err(dimension.span().error(format!(
                        "a rate-limit key permits at most {MAX_RATE_LIMIT_KEY_PARTS} dimensions"
                    )));
                }
                let name = dimension.public_name();
                validate_key_part_name(&name, dimension.span())?;
                if !names.insert(name.clone()) {
                    return Err(dimension
                        .span()
                        .error(format!("duplicate rate-limit dimension `{name}`")));
                }
                parsed.push(dimension);
                if dimension_body.is_empty() {
                    break;
                }
                dimension_body.parse::<Token![,]>().map_err(|_| {
                    dimension_body.error("rate-limit dimensions must be separated by commas")
                })?;
            }
            body.parse::<Token![;]>()?;
            dimensions = Some(parsed);
            continue;
        }
        if body.peek(keyword::cost) {
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `key`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::cost = body.parse()?;
            if cost.is_some() {
                return Err(marker.span().error(
                    "an anonymous rate-limit attachment permits at most one `cost` clause",
                ));
            }
            let literal: LitInt = body.parse().map_err(|_| {
                body.error("rate-limit cost requires a positive unsuffixed base-10 integer")
            })?;
            cost = Some(Clause {
                key_span: marker.span(),
                value: parse_positive_u32(&literal, "rate-limit cost")?,
            });
            body.parse::<Token![;]>()?;
            continue;
        }
        if !body.peek(keyword::allow) {
            return Err(
                body.error("a v2 rate-limit window is written `allow CAPACITY per DURATION;`")
            );
        }
        body.parse::<keyword::allow>()?;
        let capacity_literal: LitInt = body.parse().map_err(|_| {
            body.error("an anonymous rate-limit attachment requires `allow CAPACITY per DURATION;`")
        })?;
        if windows.len() >= MAX_RATE_LIMIT_WINDOWS_PER_POLICY {
            return Err(capacity_literal.span().error(format!(
                "an anonymous rate-limit attachment permits at most {MAX_RATE_LIMIT_WINDOWS_PER_POLICY} windows"
            )));
        }
        let capacity = parse_positive_u32(&capacity_literal, "rate-limit window capacity")?;
        body.parse::<keyword::per>()?;
        let duration_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit window requires a positive duration after `per`")
        })?;
        let duration = parse_rate_duration(&duration_literal)?;
        body.parse::<Token![;]>()?;
        windows.push(RateLimitWindowLiteral {
            capacity,
            capacity_span: capacity_literal.span(),
            duration: DurationLiteral {
                value: duration,
                span: duration_literal.span(),
            },
        });
    }
    if windows.is_empty() {
        return Err(attachment_span.error(
            "an anonymous rate-limit attachment requires at least one positive window; add `allow 100 per 1m;`",
        ));
    }
    let cost_value = cost.as_ref().map_or(1, |cost| cost.value);
    if let Some(window) = windows.iter().find(|window| cost_value > window.capacity) {
        return Err(window
            .duration
            .span
            .error("rate-limit cost must not exceed any window capacity"));
    }
    Ok(RateLimitDecl {
        id,
        dimensions: dimensions.unwrap_or_default(),
        cost: cost_value,
        cost_span: cost.map(|cost| cost.key_span),
        windows,
    })
}

fn parse_rate_limit_definition(
    input: ParseStream<'_>,
    name: Ident,
    formals: &HashSet<String>,
    formal_names: Vec<Ident>,
) -> syn::Result<RateLimitDefinitionDecl> {
    let body;
    braced!(body in input);

    let mut id: Option<Clause<LitStr>> = None;
    let mut dimensions = None;
    let mut cost: Option<Clause<u32>> = None;
    let mut windows = Vec::new();
    while !body.is_empty() {
        if body.peek(keyword::id) {
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `key`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::id = body.parse()?;
            if let Some(first) = &id {
                let mut error = marker
                    .span()
                    .error("a rate-limit profile permits at most one `id` clause");
                error.combine(first.key_span.error("the first `id` clause is here"));
                return Err(error);
            }
            let value: LitStr = body.parse().map_err(|_| {
                body.error("a reusable rate-limit `id` clause requires a string literal")
            })?;
            validate_rate_limit_id(&value)?;
            body.parse::<Token![;]>()?;
            id = Some(Clause {
                key_span: marker.span(),
                value,
            });
            continue;
        }
        if body.peek(keyword::by) {
            return Err(body.error("v1 `by [...]` was removed; write `key [...]`"));
        }
        if body.peek(keyword::key) {
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `key`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::key = body.parse()?;
            if dimensions.is_some() {
                return Err(marker
                    .span()
                    .error("a rate-limit profile permits at most one `key` clause"));
            }
            let dimension_body;
            bracketed!(dimension_body in body);
            if dimension_body.is_empty() {
                return Err(marker.span().error(
                    "omit `key` for a definition-global bucket; `key [...]` requires at least one dimension",
                ));
            }
            let mut parsed = Vec::new();
            let mut names = HashSet::new();
            while !dimension_body.is_empty() {
                let dimension = parse_definition_dimension(&dimension_body, formals)?;
                if parsed.len() >= MAX_RATE_LIMIT_KEY_PARTS {
                    return Err(dimension.span().error(format!(
                        "a rate-limit key permits at most {MAX_RATE_LIMIT_KEY_PARTS} dimensions"
                    )));
                }
                let public_name = dimension.public_name();
                validate_key_part_name(&public_name, dimension.span())?;
                if !names.insert(public_name.clone()) {
                    return Err(dimension.span().error(format!(
                        "duplicate reusable rate-limit dimension `{public_name}`"
                    )));
                }
                parsed.push(dimension);
                if dimension_body.is_empty() {
                    break;
                }
                dimension_body.parse::<Token![,]>().map_err(|_| {
                    dimension_body
                        .error("reusable rate-limit dimensions must be separated by commas")
                })?;
            }
            body.parse::<Token![;]>()?;
            dimensions = Some(parsed);
            continue;
        }
        if body.peek(keyword::cost) {
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `key`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::cost = body.parse()?;
            if let Some(first) = &cost {
                let mut error = marker
                    .span()
                    .error("a rate-limit profile permits at most one `cost` clause");
                error.combine(first.key_span.error("the first `cost` clause is here"));
                return Err(error);
            }
            let literal: LitInt = body.parse().map_err(|_| {
                body.error("rate-limit cost requires a positive unsuffixed base-10 integer")
            })?;
            cost = Some(Clause {
                key_span: marker.span(),
                value: parse_positive_u32(&literal, "rate-limit cost")?,
            });
            body.parse::<Token![;]>()?;
            continue;
        }
        if !body.peek(keyword::allow) {
            return Err(
                body.error("a v2 rate-limit window is written `allow CAPACITY per DURATION;`")
            );
        }
        body.parse::<keyword::allow>()?;
        let capacity_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit profile requires `allow CAPACITY per DURATION;` windows")
        })?;
        if windows.len() >= MAX_RATE_LIMIT_WINDOWS_PER_POLICY {
            return Err(capacity_literal.span().error(format!(
                "a rate-limit profile permits at most {MAX_RATE_LIMIT_WINDOWS_PER_POLICY} windows"
            )));
        }
        let capacity = parse_positive_u32(&capacity_literal, "rate-limit window capacity")?;
        body.parse::<keyword::per>()?;
        let duration_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit window requires a positive duration after `per`")
        })?;
        let duration = parse_rate_duration(&duration_literal)?;
        body.parse::<Token![;]>()?;
        windows.push(RateLimitWindowLiteral {
            capacity,
            capacity_span: capacity_literal.span(),
            duration: DurationLiteral {
                value: duration,
                span: duration_literal.span(),
            },
        });
    }
    if windows.is_empty() {
        return Err(name.span().error(
            "a rate-limit profile requires at least one positive window; add `allow 100 per 1m;`",
        ));
    }
    let cost_value = cost.as_ref().map_or(1, |cost| cost.value);
    if let Some(window) = windows.iter().find(|window| cost_value > window.capacity) {
        return Err(window
            .capacity_span
            .error("rate-limit cost must not exceed any window capacity"));
    }
    let dimensions = dimensions.unwrap_or_default();
    for formal in formals {
        if !dimensions.iter().any(|dimension| {
            matches!(dimension, RateLimitDefinitionDimensionDecl::Formal { name } if canonical_ident(name) == *formal)
        }) {
            return Err(name.span().error(format!(
                "rate-limit profile formal `{formal}` must appear exactly once in its `key [...]` schema"
            )));
        }
    }
    Ok(RateLimitDefinitionDecl {
        name,
        formals: formal_names,
        id,
        dimensions,
        cost: cost_value,
        cost_span: cost.map(|cost| cost.key_span),
        windows,
    })
}

fn parse_definition_dimension(
    input: ParseStream<'_>,
    formals: &HashSet<String>,
) -> syn::Result<RateLimitDefinitionDimensionDecl> {
    if input.peek(LitStr) || input.peek(LitInt) {
        let span = input
            .parse::<proc_macro2::TokenTree>()
            .map_err(|_| input.error("invalid reusable rate-limit dimension"))?
            .span();
        return Err(span.error(
            "reusable `key` dimensions accept only `host`, `endpoint`, or declared bare formal names",
        ));
    }
    if input.peek(keyword::fmt) {
        let marker: keyword::fmt = input.parse()?;
        return Err(marker
            .span()
            .error("reusable `key` dimensions do not accept formatted expressions"));
    }
    if input.peek(keyword::concat) {
        return Err(input.error(
            "reusable rate-limit keys use canonical direct dimensions and do not accept `concat`",
        ));
    }
    if input.peek(keyword::host) {
        let marker: keyword::host = input.parse()?;
        return Ok(RateLimitDefinitionDimensionDecl::Host {
            span: marker.span(),
        });
    }
    if input.peek(keyword::endpoint) && {
        let fork = input.fork();
        let _: keyword::endpoint = fork.parse()?;
        !fork.peek(Token![.])
    } {
        let marker: keyword::endpoint = input.parse()?;
        return Ok(RateLimitDefinitionDimensionDecl::Endpoint {
            span: marker.span(),
        });
    }
    let name = input.call(Ident::parse_any).map_err(|_| {
        input.error(
            "a reusable rate-limit dimension must be `host`, `endpoint`, or a bare formal name",
        )
    })?;
    let canonical_name = canonical_ident(&name);
    if matches!(canonical_name.as_str(), "host" | "endpoint") {
        return Err(name.span().error(format!(
            "reusable rate-limit formal `{canonical_name}` is reserved; use built-in dimension `{canonical_name}` without `r#`"
        )));
    }
    if input.peek(Token![.]) {
        return Err(name.span().error(
            "rate-limit profiles cannot capture lexical values; use a bare formal name and bind it at each attachment site",
        ));
    }
    validate_snake_source_identifier(&name, "rate-limit dimension")?;
    if !formals.contains(&canonical_name) {
        return Err(name.span().error(format!(
            "rate-limit key uses undeclared profile formal `{canonical_name}`"
        )));
    }
    Ok(RateLimitDefinitionDimensionDecl::Formal { name })
}

fn validate_key_part_name(name: &str, span: proc_macro2::Span) -> syn::Result<()> {
    if name.is_empty() || name.len() > MAX_RATE_LIMIT_KEY_PART_NAME_BYTES {
        return Err(span.error(format!(
            "a rate-limit key-part name permits 1-{MAX_RATE_LIMIT_KEY_PART_NAME_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_rate_limit_id(id: &LitStr) -> syn::Result<()> {
    let value = id.value();
    let valid = (1..=MAX_RATE_LIMIT_ID_BYTES).contains(&value.len())
        && value.is_ascii()
        && value.split('.').all(|segment| {
            let bytes = segment.as_bytes();
            !bytes.is_empty()
                && bytes.iter().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
                && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        });
    if !valid {
        return Err(id.span().error(format!(
            "a generated rate-limit ID must be 1-{MAX_RATE_LIMIT_ID_BYTES} ASCII bytes in dot-separated segments; each segment begins and ends with a lowercase letter or digit and may also contain `_` or `-`"
        )));
    }
    Ok(())
}

fn parse_rate_limit_dimension(input: ParseStream<'_>) -> syn::Result<RateLimitDimensionDecl> {
    if input.peek(LitStr) || input.peek(LitInt) {
        let span = input
            .parse::<proc_macro2::TokenTree>()
            .map_err(|_| input.error("invalid rate-limit dimension"))?
            .span();
        return Err(span.error("rate-limit `key` dimensions do not accept literal values"));
    }
    if input.peek(keyword::fmt) {
        let marker: keyword::fmt = input.parse()?;
        return Err(marker
            .span()
            .error("rate-limit `key` dimensions do not accept formatted expressions"));
    }
    if input.peek(keyword::concat) {
        return Err(input
            .error("rate-limit keys use canonical direct dimensions and do not accept `concat`"));
    }
    if input.peek(keyword::host) {
        let marker: keyword::host = input.parse()?;
        return Ok(RateLimitDimensionDecl::Host {
            span: marker.span(),
        });
    }
    if input.peek(keyword::endpoint) && {
        let fork = input.fork();
        let _: keyword::endpoint = fork.parse()?;
        !fork.peek(Token![.])
    } {
        let marker: keyword::endpoint = input.parse()?;
        return Ok(RateLimitDimensionDecl::Endpoint {
            span: marker.span(),
        });
    }

    let namespace = input.call(Ident::parse_any).map_err(|_| {
        input.error(
            "a rate-limit dimension must be `host`, `endpoint`, or one direct total owner reference",
        )
    })?;
    let namespace_name = canonical_ident(&namespace);
    if !input.peek(Token![.]) {
        return Err(namespace.span().error(format!(
            "unsupported rate-limit dimension `{namespace_name}`; use `host`, `endpoint`, or an owner-qualified value"
        )));
    }
    input.parse::<Token![.]>()?;
    let name = input.call(Ident::parse_any).map_err(|_| {
        input.error("a rate-limit value dimension requires a value name after the namespace")
    })?;
    match namespace_name.as_str() {
        "vars" => {
            return Err(namespace
                .span()
                .error("v1 `vars.NAME` was removed; use `client.NAME`"));
        }
        "scope" => {
            return Err(namespace
                .span()
                .error("v1 `scope.NAME` was removed; use the actual scope identifier"));
        }
        "ep" => {
            return Err(namespace
                .span()
                .error("v1 `ep.NAME` was removed; use `endpoint.NAME`"));
        }
        "secret" => {
            return Err(namespace
                .span()
                .error("secrets and credentials cannot be rate-limit dimensions"));
        }
        _ => {}
    }
    let span = namespace.span();
    Ok(RateLimitDimensionDecl::Value {
        namespace,
        name,
        span,
    })
}

fn canonical_ident(ident: &Ident) -> String {
    let text = ident.to_string();
    text.strip_prefix("r#").unwrap_or(&text).to_owned()
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

#[cfg(test)]
mod tests {
    use quote::{format_ident, quote};
    use syn::LitStr;

    use crate::syntax::ast::{
        AttachmentSite, RateLimitAttachmentDecl, RateLimitDecl, RateLimitDefinitionDimensionDecl,
        RateLimitDimensionDecl, SourceChild,
    };
    use crate::{
        MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
        MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
    };

    fn parse_error(tokens: proc_macro2::TokenStream) -> String {
        super::super::parse(tokens)
            .err()
            .expect("rate-limit syntax error")
            .to_string()
    }

    fn inline_rate_limit(attachment_site: &AttachmentSite) -> &RateLimitDecl {
        match &attachment_site.rate_limits[0].value {
            RateLimitAttachmentDecl::Inline(rule) => rule,
            RateLimitAttachmentDecl::Named(_) => panic!("anonymous rate-limit attachment"),
        }
    }

    #[test]
    fn parses_reusable_definitions_and_source_ordered_named_uses() {
        let source = super::super::parse(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit service_reads();
                rate_limits {
                    account_reads(account) {
                        id "service.account.read";
                        key [host, endpoint, account];
                        cost 1;
                        allow 100 per 1m;
                        allow 1_000 per 1h;
                    }
                    service_reads() {
                        allow 10_000 per 1m;
                    }
                }

            scope accounts(account: String) {
                rate_limit account_reads(account = accounts.account) { cost 2; }
                rate_limit { allow 5 per 1s; }
                rate_limit service_reads();
                GET read() -> Bytes using Bytes;
            }
        }})
        .expect("reusable rate-limit syntax");

        let definitions = &source.client.rate_limits.as_ref().unwrap().value;
        assert_eq!(definitions.len(), 2);
        assert_eq!(
            super::canonical_ident(&definitions[0].name),
            "account_reads"
        );
        assert_eq!(definitions[0].cost, 1);
        assert_eq!(
            definitions[0]
                .dimensions
                .iter()
                .map(RateLimitDefinitionDimensionDecl::public_name)
                .collect::<Vec<_>>(),
            ["host", "endpoint", "account"]
        );
        assert_eq!(source.client.attachment_site.rate_limits.len(), 1);
        let SourceChild::Scope(scope) = &source.client.children[0] else {
            panic!("accounts scope");
        };
        assert_eq!(scope.attachment_site.rate_limits.len(), 3);
        assert!(matches!(
            scope.attachment_site.rate_limits[0].value,
            RateLimitAttachmentDecl::Named(_)
        ));
        assert!(matches!(
            scope.attachment_site.rate_limits[1].value,
            RateLimitAttachmentDecl::Inline(_)
        ));
        let RateLimitAttachmentDecl::Named(last) = &scope.attachment_site.rate_limits[2].value
        else {
            panic!("last named attachment");
        };
        assert_eq!(super::canonical_ident(&last.name), "service_reads");
        let RateLimitAttachmentDecl::Named(first) = &scope.attachment_site.rate_limits[0].value
        else {
            panic!("first named attachment");
        };
        assert_eq!(first.cost.as_ref().map(|cost| cost.value), Some(2));
        assert_eq!(super::canonical_ident(&first.bindings[0].name), "account");
        assert_eq!(
            super::canonical_ident(&first.bindings[0].value.namespace),
            "accounts"
        );
    }

    #[test]
    fn reusable_definition_and_binding_diagnostics_are_focused() {
        let invalid_rule_name = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    NotSnakeCase() { allow 1 per 1s; }
                }
            }
        });
        assert!(invalid_rule_name.contains("rate-limit profile source identifier"));

        let duplicate_definition = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    shared() { allow 1 per 1s; }
                    shared() { allow 1 per 1s; }
                }
            }
        });
        assert!(duplicate_definition.contains("is defined more than once"));

        let duplicate_formal = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    shared(account) { key [account, account]; allow 1 per 1s; }
                }
            }
        });
        assert!(duplicate_formal.contains("duplicate reusable rate-limit dimension `account`"));

        let captured_value = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    shared() { key [scope.account]; allow 1 per 1s; }
                }
            }
        });
        assert!(captured_value.contains("cannot capture lexical values"));

        let duplicate_binding = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limit shared(account = client.first, account = client.second);
            }
        });
        assert!(duplicate_binding.contains("duplicate binding"));

        let invalid_value = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limit shared(account = "literal");
            }
        });
        assert!(invalid_value.contains("must be exactly one total owner reference"));

        let dimensions = (0..=MAX_RATE_LIMIT_KEY_PARTS)
            .map(|index| format_ident!("dimension_{index}"))
            .collect::<Vec<_>>();
        let too_many_dimensions = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    shared(#(#dimensions), *) {
                        key [#(#dimensions),*];
                        allow 1 per 1s;
                    }
                }
            }
        });
        assert!(too_many_dimensions.contains("permits at most 32 dimensions"));

        for (cost, expected) in [
            (quote!(0), "greater than zero"),
            (quote!(4_294_967_296), "not representable as `u32`"),
        ] {
            let error = parse_error(quote! {
                client Api {
                    timeout off;
                    base "https://example.test";
                    rate_limits {
                        shared() {
                            cost #cost;
                            allow 1 per 1s;
                        }
                    }
                }
            });
            assert!(error.contains(expected), "unexpected error: {error}");
        }

        let excessive_cost = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    shared() {
                        cost 2;
                        allow 1 per 1s;
                    }
                }
            }
        });
        assert!(excessive_cost.contains("must not exceed any window capacity"));
    }

    #[test]
    fn raw_builtin_names_are_rejected_as_reusable_formals() {
        for formal in [quote!(r#host), quote!(r#endpoint)] {
            let error = parse_error(quote! {
                client Api {
                    timeout off;
                    base "https://example.test";
                    rate_limits {
                        shared(#formal) {
                            key [#formal];
                            allow 1 per 1s;
                        }
                    }
                }
            });
            assert_eq!(
                error,
                format!(
                    "`host` and `endpoint` are built-in key dimensions and cannot be profile formals"
                )
            );
        }
    }

    #[test]
    fn reusable_formal_name_enforces_the_runtime_key_part_name_limit() {
        let exact = format_ident!("{}", "a".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES));
        super::super::parse(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    shared(#exact) { key [#exact]; allow 1 per 1s; }
                }
            }
        })
        .expect("exact formal-name byte limit");

        let excess = format_ident!("{}", "a".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES + 1));
        let error = parse_error(quote! {
            client Api {
                timeout off;
                base "https://example.test";
                rate_limits {
                    shared(#excess) { key [#excess]; allow 1 per 1s; }
                }
            }
        });
        assert!(error.contains("key-part name permits 1-128 UTF-8 bytes"));
    }

    #[test]
    fn parses_ordered_finite_windows_and_optional_cost() {
        let source = super::super::parse(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { allow 100 per 1m; allow 5 per 1s; }

            GET read() -> Bytes using Bytes {
                rate_limit { cost 2; allow 2 per 250ms; }
            }
        }})
        .expect("rate-limit syntax");
        let SourceChild::Endpoint(endpoint) = &source.client.children[0] else {
            panic!("endpoint");
        };

        assert_eq!(
            inline_rate_limit(&source.client.attachment_site)
                .windows
                .len(),
            2
        );
        assert_eq!(inline_rate_limit(&endpoint.attachment_site).cost, 2);
    }

    #[test]
    fn parses_explicit_id_by_and_cost_in_any_pre_window_order() {
        let source = super::super::parse(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit {
                    cost 2;
                    id "riot.application";
                    key [host];
                    allow 2 per 1s;
                }

            GET read() -> Bytes using Bytes {
                rate_limit {
                    key [endpoint];
                    id "riot.endpoint";
                    cost 1;
                    allow 1 per 1s;
                }
            }
        }})
        .expect("explicit rate-limit IDs");
        let client = inline_rate_limit(&source.client.attachment_site);
        let SourceChild::Endpoint(endpoint) = &source.client.children[0] else {
            panic!("endpoint");
        };
        let endpoint = inline_rate_limit(&endpoint.attachment_site);

        assert_eq!(
            client.id.as_ref().map(|id| id.value.value()).as_deref(),
            Some("riot.application")
        );
        assert_eq!(
            endpoint.id.as_ref().map(|id| id.value.value()).as_deref(),
            Some("riot.endpoint")
        );
    }

    #[test]
    fn validates_explicit_id_grammar_and_exact_byte_limit() {
        let exact = LitStr::new(
            &format!("a{}", "b".repeat(MAX_RATE_LIMIT_ID_BYTES - 1)),
            proc_macro2::Span::call_site(),
        );
        super::validate_rate_limit_id(&exact).expect("exact rate-limit ID byte limit");

        for invalid in [
            "",
            "Uppercase",
            ".leading",
            "trailing.",
            "double..dot",
            "_leading",
            "trailing-",
            "contains space",
            "nonascii-é",
        ] {
            let literal = LitStr::new(invalid, proc_macro2::Span::call_site());
            let error = super::validate_rate_limit_id(&literal)
                .expect_err("invalid generated rate-limit ID");
            assert_eq!(
                error.to_string(),
                "a generated rate-limit ID must be 1-256 ASCII bytes in dot-separated segments; each segment begins and ends with a lowercase letter or digit and may also contain `_` or `-`"
            );
        }

        let first_excess = LitStr::new(
            &format!("a{}", "b".repeat(MAX_RATE_LIMIT_ID_BYTES)),
            proc_macro2::Span::call_site(),
        );
        super::validate_rate_limit_id(&first_excess).expect_err("first excess rate-limit ID byte");
    }

    #[test]
    fn rejects_duplicate_and_late_explicit_id_clauses() {
        let duplicate = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit {
                    id "riot.first";
                    id "riot.second";
                    allow 1 per 1s;
                }
            }
        });
        assert!(duplicate.contains("at most one `id` clause"));

        let late = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit {
                    allow 1 per 1s;
                    id "riot.late";
                }
            }
        });
        assert!(late.contains("must appear before rate-limit windows"));
    }

    #[test]
    fn parses_by_before_cost() {
        let source = super::super::parse(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit {
                    key [host, endpoint, client.tenant];
                    cost 2;
                    allow 2 per 1s;
                }
            }
        })
        .expect("by before cost");
        let rule = inline_rate_limit(&source.client.attachment_site);

        assert_eq!(
            rule.dimensions
                .iter()
                .map(RateLimitDimensionDecl::public_name)
                .collect::<Vec<_>>(),
            ["host", "endpoint", "client.tenant"]
        );
        assert_eq!(rule.cost, 2);
    }

    #[test]
    fn parses_cost_before_by() {
        let source = super::super::parse(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit {
                    cost 2;
                    key [regional.region, endpoint.account];
                    allow 2 per 1s;
                }
            }
        })
        .expect("cost before by");
        let rule = inline_rate_limit(&source.client.attachment_site);

        assert_eq!(
            rule.dimensions
                .iter()
                .map(RateLimitDimensionDecl::public_name)
                .collect::<Vec<_>>(),
            ["regional.region", "endpoint.account"]
        );
        assert_eq!(rule.cost, 2);
    }

    #[test]
    fn parses_global_rule_without_by() {
        let source = super::super::parse(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { allow 1 per 1s; }
            }
        })
        .expect("declaration-global rule");

        assert!(
            inline_rate_limit(&source.client.attachment_site)
                .dimensions
                .is_empty()
        );
    }

    #[test]
    fn rejects_empty_by() {
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { key []; allow 1 per 1s; }
            }
        });

        assert!(error.contains("omit `key` for a declaration-global bucket"));
    }

    #[test]
    fn rejects_duplicate_by_clause() {
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { key [host]; key [endpoint]; allow 1 per 1s; }
            }
        });

        assert!(error.contains("at most one `key` clause"));
    }

    #[test]
    fn rejects_duplicate_dimension() {
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { key [client.tenant, client.tenant]; allow 1 per 1s; }
            }
        });

        assert!(error.contains("duplicate rate-limit dimension `client.tenant`"));
    }

    #[test]
    fn rejects_by_after_window() {
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { allow 1 per 1s; key [host]; }
            }
        });

        assert!(error.contains("must appear before rate-limit windows"));
    }

    #[test]
    fn rejects_cost_after_window() {
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { allow 1 per 1s; cost 1; }
            }
        });

        assert!(error.contains("must appear before rate-limit windows"));
    }

    #[test]
    fn rejects_unknown_bare_dimension() {
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { key [path]; allow 1 per 1s; }
            }
        });

        assert!(error.contains("unsupported rate-limit dimension `path`"));
    }

    #[test]
    fn rejects_unknown_namespace() {
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { key [secret.token]; allow 1 per 1s; }
            }
        });

        assert!(error.contains("secrets and credentials cannot be rate-limit dimensions"));
    }

    #[test]
    fn rejects_literal_and_formatted_dimensions() {
        for (dimension, expected) in [
            (
                quote!("tenant"),
                "rate-limit `key` dimensions do not accept literal values",
            ),
            (
                quote!(concat[client.tenant]),
                "rate-limit keys use canonical direct dimensions and do not accept `concat`",
            ),
        ] {
            let error = parse_error(quote! {
                client RateApi {
                    timeout off;
                    base "https://example.test";
                    rate_limit { key [#dimension]; allow 1 per 1s; }
                }
            });
            assert!(error.contains(expected), "unexpected diagnostic: {error}");
        }
    }

    #[test]
    fn rejects_first_excess_dimension() {
        let dimensions = (0..=MAX_RATE_LIMIT_KEY_PARTS).map(|index| {
            let name = format_ident!("value_{index}");
            quote!(client.#name)
        });
        let error = parse_error(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { key [#(#dimensions),*]; allow 1 per 1s; }
            }
        });

        assert_eq!(
            error,
            format!("a rate-limit key permits at most {MAX_RATE_LIMIT_KEY_PARTS} dimensions")
        );
    }

    #[test]
    fn rejects_the_first_excess_static_window() {
        let windows = (0..=MAX_RATE_LIMIT_WINDOWS_PER_POLICY).map(|_| quote! { allow 1 per 1s; });
        let error = super::super::parse(quote! {
            client RateApi {
                timeout off;
                base "https://example.test";
                rate_limit { #(#windows)* }
            }
        })
        .err()
        .expect("first excess static window");

        assert!(
            error
                .to_string()
                .contains("an anonymous rate-limit attachment permits at most 16 windows")
        );
    }
}
