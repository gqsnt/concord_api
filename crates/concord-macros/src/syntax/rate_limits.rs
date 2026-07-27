use std::collections::{HashMap, HashSet};
use std::time::Duration;

use syn::ext::IdentExt;
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitInt, LitStr, Token, braced, bracketed};

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
    MAX_RATE_LIMIT_WINDOWS_PER_RULE,
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
        let _marker: keyword::rule = body.parse().map_err(|_| {
            body.error("a `rate_limits` block accepts only `rule NAME { ... }` definitions")
        })?;
        let name: Ident = body
            .parse()
            .map_err(|_| body.error("a reusable rate-limit rule requires a name"))?;
        validate_snake_source_identifier(&name, "rate-limit rule")?;
        let canonical_name = canonical_ident(&name);
        if let Some(first) = names.insert(canonical_name.clone(), name.span()) {
            let mut error = name.span().error(format!(
                "rate-limit rule `{canonical_name}` is defined more than once"
            ));
            error.combine(first.error("the first definition using this rule name is here"));
            return Err(error);
        }
        definitions.push(parse_rate_limit_definition(&body, name)?);
    }
    Ok(definitions)
}

pub(super) fn parse_rate_limit(input: ParseStream<'_>) -> syn::Result<RateLimitAttachmentDecl> {
    input.parse::<keyword::rate_limit>()?;
    if input.peek(Token![use]) {
        return parse_rate_limit_use(input).map(RateLimitAttachmentDecl::Named);
    }
    if input.peek(syn::token::Brace) {
        return parse_inline_rate_limit(input).map(RateLimitAttachmentDecl::Inline);
    }
    Err(input
        .error("a rate-limit attachment must be `rate_limit { ... }` or `rate_limit use RULE ...`"))
}

fn parse_rate_limit_use(input: ParseStream<'_>) -> syn::Result<RateLimitUseDecl> {
    input.parse::<Token![use]>()?;
    let name: Ident = input
        .parse()
        .map_err(|_| input.error("`rate_limit use` requires a reusable rule name"))?;
    validate_snake_source_identifier(&name, "rate-limit rule")?;
    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        return Ok(RateLimitUseDecl {
            name,
            bindings: Vec::new(),
            cost: None,
        });
    }
    if !input.peek(syn::token::Brace) {
        return Err(input.error("a named rate-limit use requires `;` or a `{ ... }` binding body"));
    }
    let body;
    braced!(body in input);
    let mut bindings = Vec::new();
    let mut binding_names = HashMap::new();
    let mut cost: Option<Clause<u32>> = None;
    while !body.is_empty() {
        let is_cost = if body.peek(keyword::cost) {
            let fork = body.fork();
            fork.parse::<keyword::cost>()?;
            !fork.peek(Token![=])
        } else {
            false
        };
        if is_cost {
            let marker: keyword::cost = body.parse()?;
            if let Some(first) = &cost {
                let mut error = marker
                    .span()
                    .error("a named rate-limit use permits at most one `cost` override");
                error.combine(first.key_span.error("the first `cost` override is here"));
                return Err(error);
            }
            let literal: LitInt = body.parse().map_err(|_| {
                body.error("a named rate-limit cost requires a positive unsuffixed base-10 integer")
            })?;
            let value = parse_positive_u32(&literal, "rate-limit cost")?;
            body.parse::<Token![;]>()?;
            cost = Some(Clause {
                key_span: marker.span(),
                value,
            });
            continue;
        }

        let binding_name = body.call(Ident::parse_any).map_err(|_| {
            body.error("a named rate-limit binding must be `FORMAL = vars.NAME`, `scope.NAME`, or `ep.NAME`")
        })?;
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
        body.parse::<Token![;]>()?;
        bindings.push(RateLimitBindingDecl {
            name: binding_name,
            value,
        });
    }
    Ok(RateLimitUseDecl {
        name,
        bindings,
        cost,
    })
}

fn parse_binding_value(input: ParseStream<'_>) -> syn::Result<RateLimitBindingValue> {
    let namespace = input.call(Ident::parse_any).map_err(|_| {
        input.error(
            "a named rate-limit binding value must be exactly `vars.NAME`, `scope.NAME`, or `ep.NAME`",
        )
    })?;
    let namespace_name = canonical_ident(&namespace);
    if !matches!(namespace_name.as_str(), "vars" | "scope" | "ep") {
        return Err(namespace.span().error(format!(
            "unknown rate-limit binding namespace `{namespace_name}`; use only `vars.*`, `scope.*`, or `ep.*`"
        )));
    }
    input.parse::<Token![.]>().map_err(|_| {
        namespace.span().error(
            "a named rate-limit binding value must be exactly `vars.NAME`, `scope.NAME`, or `ep.NAME`",
        )
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

fn parse_inline_rate_limit(input: ParseStream<'_>) -> syn::Result<RateLimitDecl> {
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
                    "`id`, `by`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::id = body.parse()?;
            if let Some(first) = &id {
                let mut error = marker
                    .span()
                    .error("a rate-limit rule permits at most one `id` clause");
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
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `by`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::by = body.parse()?;
            if dimensions.is_some() {
                return Err(marker
                    .span()
                    .error("a rate-limit rule permits at most one `by` clause"));
            }
            let dimension_body;
            bracketed!(dimension_body in body);
            if dimension_body.is_empty() {
                return Err(marker.span().error(
                    "omit `by` for a declaration-global bucket; `by [...]` requires at least one dimension",
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
                    "`id`, `by`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::cost = body.parse()?;
            if cost.is_some() {
                return Err(marker
                    .span()
                    .error("a rate-limit rule permits at most one `cost` clause"));
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
        let capacity_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit rule requires at least one `CAPACITY / DURATION;` window")
        })?;
        if windows.len() >= MAX_RATE_LIMIT_WINDOWS_PER_RULE {
            return Err(capacity_literal.span().error(format!(
                "a rate-limit rule permits at most {MAX_RATE_LIMIT_WINDOWS_PER_RULE} windows"
            )));
        }
        let capacity = parse_positive_u32(&capacity_literal, "rate-limit window capacity")?;
        body.parse::<Token![/]>()?;
        let duration_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit window requires a positive duration after `/`")
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
        return Err(input.error("a rate-limit declaration requires at least one positive window"));
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
                    "`id`, `by`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::id = body.parse()?;
            if let Some(first) = &id {
                let mut error = marker
                    .span()
                    .error("a reusable rate-limit rule permits at most one `id` clause");
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
            if !windows.is_empty() {
                return Err(body.error(
                    "`id`, `by`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::by = body.parse()?;
            if dimensions.is_some() {
                return Err(marker
                    .span()
                    .error("a reusable rate-limit rule permits at most one `by` clause"));
            }
            let dimension_body;
            bracketed!(dimension_body in body);
            if dimension_body.is_empty() {
                return Err(marker.span().error(
                    "omit `by` for a definition-global bucket; `by [...]` requires at least one dimension",
                ));
            }
            let mut parsed = Vec::new();
            let mut names = HashSet::new();
            while !dimension_body.is_empty() {
                let dimension = parse_definition_dimension(&dimension_body)?;
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
                    "`id`, `by`, and `cost` clauses must appear before rate-limit windows",
                ));
            }
            let marker: keyword::cost = body.parse()?;
            if let Some(first) = &cost {
                let mut error = marker
                    .span()
                    .error("a reusable rate-limit rule permits at most one `cost` clause");
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
        let capacity_literal: LitInt = body.parse().map_err(|_| {
            body.error("a reusable rate-limit rule requires `CAPACITY / DURATION;` windows")
        })?;
        if windows.len() >= MAX_RATE_LIMIT_WINDOWS_PER_RULE {
            return Err(capacity_literal.span().error(format!(
                "a rate-limit rule permits at most {MAX_RATE_LIMIT_WINDOWS_PER_RULE} windows"
            )));
        }
        let capacity = parse_positive_u32(&capacity_literal, "rate-limit window capacity")?;
        body.parse::<Token![/]>()?;
        let duration_literal: LitInt = body.parse().map_err(|_| {
            body.error("a rate-limit window requires a positive duration after `/`")
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
        return Err(name
            .span()
            .error("a reusable rate-limit rule requires at least one positive window"));
    }
    let cost_value = cost.as_ref().map_or(1, |cost| cost.value);
    if let Some(window) = windows.iter().find(|window| cost_value > window.capacity) {
        return Err(window
            .capacity_span
            .error("rate-limit cost must not exceed any window capacity"));
    }
    Ok(RateLimitDefinitionDecl {
        name,
        id,
        dimensions: dimensions.unwrap_or_default(),
        cost: cost_value,
        cost_span: cost.map(|cost| cost.key_span),
        windows,
    })
}

fn parse_definition_dimension(
    input: ParseStream<'_>,
) -> syn::Result<RateLimitDefinitionDimensionDecl> {
    if input.peek(LitStr) || input.peek(LitInt) {
        let span = input
            .parse::<proc_macro2::TokenTree>()
            .map_err(|_| input.error("invalid reusable rate-limit dimension"))?
            .span();
        return Err(span.error(
            "reusable `by` dimensions accept only `host`, `endpoint`, or bare formal names",
        ));
    }
    if input.peek(keyword::fmt) {
        let marker: keyword::fmt = input.parse()?;
        return Err(marker
            .span()
            .error("reusable `by` dimensions do not accept formatted expressions"));
    }
    if input.peek(keyword::host) {
        let marker: keyword::host = input.parse()?;
        return Ok(RateLimitDefinitionDimensionDecl::Host {
            span: marker.span(),
        });
    }
    if input.peek(keyword::endpoint) {
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
    if input.peek(Token![.]) {
        return Err(name.span().error(
            "reusable rate-limit definitions cannot capture lexical values; use a bare formal name and bind it at each attachment",
        ));
    }
    validate_snake_source_identifier(&name, "rate-limit dimension")?;
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
        return Err(span.error("`by` dimensions do not accept literal values"));
    }
    if input.peek(keyword::fmt) {
        let marker: keyword::fmt = input.parse()?;
        return Err(marker
            .span()
            .error("`by` dimensions do not accept formatted expressions"));
    }
    if input.peek(keyword::host) {
        let marker: keyword::host = input.parse()?;
        return Ok(RateLimitDimensionDecl::Host {
            span: marker.span(),
        });
    }
    if input.peek(keyword::endpoint) {
        let marker: keyword::endpoint = input.parse()?;
        return Ok(RateLimitDimensionDecl::Endpoint {
            span: marker.span(),
        });
    }

    let namespace = input.call(Ident::parse_any).map_err(|_| {
        input.error(
            "a rate-limit dimension must be `host`, `endpoint`, `vars.NAME`, `scope.NAME`, or `ep.NAME`",
        )
    })?;
    let namespace_name = canonical_ident(&namespace);
    if !input.peek(Token![.]) {
        return Err(namespace.span().error(format!(
            "unsupported rate-limit dimension `{namespace_name}`; use `endpoint` or a named `scope.*`/`ep.*` value"
        )));
    }
    input.parse::<Token![.]>()?;
    let name = input.call(Ident::parse_any).map_err(|_| {
        input.error("a rate-limit value dimension requires a value name after the namespace")
    })?;
    if !matches!(namespace_name.as_str(), "vars" | "scope" | "ep") {
        return Err(namespace.span().error(format!(
            "unknown rate-limit dimension namespace `{namespace_name}`; use only `vars.*`, `scope.*`, or `ep.*`"
        )));
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
        Layer, RateLimitAttachmentDecl, RateLimitDecl, RateLimitDefinitionDimensionDecl,
        RateLimitDimensionDecl, SourceChild,
    };
    use crate::{
        MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES, MAX_RATE_LIMIT_KEY_PARTS,
        MAX_RATE_LIMIT_WINDOWS_PER_RULE,
    };

    fn parse_error(tokens: proc_macro2::TokenStream) -> String {
        super::super::parse(tokens)
            .err()
            .expect("rate-limit syntax error")
            .to_string()
    }

    fn inline_rate_limit(layer: &Layer) -> &RateLimitDecl {
        match &layer.rate_limits[0].value {
            RateLimitAttachmentDecl::Inline(rule) => rule,
            RateLimitAttachmentDecl::Named(_) => panic!("anonymous direct rate-limit rule"),
        }
    }

    #[test]
    fn parses_reusable_definitions_and_source_ordered_named_uses() {
        let source = super::super::parse(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit use service_reads;
                rate_limits {
                    rule account_reads {
                        id "service.account.read";
                        by [host, endpoint, account];
                        cost 1;
                        100 / 1m;
                        1_000 / 1h;
                    }
                    rule service_reads {
                        10_000 / 1m;
                    }
                }
            }
            scope accounts(account: String) {
                rate_limit use account_reads {
                    cost 2;
                    account = scope.account;
                }
                rate_limit { 5 / 1s; }
                rate_limit use service_reads;
                GET read() -> Bytes;
            }
        })
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
        assert_eq!(source.client.layer.rate_limits.len(), 1);
        let SourceChild::Scope(scope) = &source.children[0] else {
            panic!("accounts scope");
        };
        assert_eq!(scope.layer.rate_limits.len(), 3);
        assert!(matches!(
            scope.layer.rate_limits[0].value,
            RateLimitAttachmentDecl::Named(_)
        ));
        assert!(matches!(
            scope.layer.rate_limits[1].value,
            RateLimitAttachmentDecl::Inline(_)
        ));
        let RateLimitAttachmentDecl::Named(last) = &scope.layer.rate_limits[2].value else {
            panic!("last named attachment");
        };
        assert_eq!(super::canonical_ident(&last.name), "service_reads");
        let RateLimitAttachmentDecl::Named(first) = &scope.layer.rate_limits[0].value else {
            panic!("first named attachment");
        };
        assert_eq!(first.cost.as_ref().map(|cost| cost.value), Some(2));
        assert_eq!(super::canonical_ident(&first.bindings[0].name), "account");
        assert_eq!(
            super::canonical_ident(&first.bindings[0].value.namespace),
            "scope"
        );
    }

    #[test]
    fn reusable_definition_and_binding_diagnostics_are_focused() {
        let invalid_rule_name = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule NotSnakeCase { 1 / 1s; }
                }
            }
        });
        assert!(invalid_rule_name.contains("rate-limit rule source identifier"));

        let duplicate_definition = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { 1 / 1s; }
                    rule shared { 1 / 1s; }
                }
            }
        });
        assert!(duplicate_definition.contains("is defined more than once"));

        let duplicate_formal = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [account, account]; 1 / 1s; }
                }
            }
        });
        assert!(duplicate_formal.contains("duplicate reusable rate-limit dimension `account`"));

        let captured_value = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [scope.account]; 1 / 1s; }
                }
            }
        });
        assert!(captured_value.contains("cannot capture lexical values"));

        let duplicate_binding = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limit use shared {
                    account = vars.first;
                    account = vars.second;
                }
            }
        });
        assert!(duplicate_binding.contains("duplicate binding"));

        let invalid_value = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limit use shared {
                    account = "literal";
                }
            }
        });
        assert!(invalid_value.contains("must be exactly `vars.NAME`"));

        let dimensions = (0..=MAX_RATE_LIMIT_KEY_PARTS)
            .map(|index| format_ident!("dimension_{index}"))
            .collect::<Vec<_>>();
        let too_many_dimensions = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared {
                        by [#(#dimensions),*];
                        1 / 1s;
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
                    base "https://example.test";
                    rate_limits {
                        rule shared {
                            cost #cost;
                            1 / 1s;
                        }
                    }
                }
            });
            assert!(error.contains(expected), "unexpected error: {error}");
        }

        let excessive_cost = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared {
                        cost 2;
                        1 / 1s;
                    }
                }
            }
        });
        assert!(excessive_cost.contains("must not exceed any window capacity"));
    }

    #[test]
    fn reusable_formal_name_enforces_the_runtime_key_part_name_limit() {
        let exact = format_ident!("{}", "a".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES));
        super::super::parse(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [#exact]; 1 / 1s; }
                }
            }
        })
        .expect("exact formal-name byte limit");

        let excess = format_ident!("{}", "a".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES + 1));
        let error = parse_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [#excess]; 1 / 1s; }
                }
            }
        });
        assert!(error.contains("key-part name permits 1-128 UTF-8 bytes"));
    }

    #[test]
    fn parses_ordered_finite_windows_and_optional_cost() {
        let source = super::super::parse(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { 100 / 1m; 5 / 1s; }
            }
            GET read() -> Bytes {
                rate_limit { cost 2; 2 / 250ms; }
            }
        })
        .expect("rate-limit syntax");
        let SourceChild::Endpoint(endpoint) = &source.children[0] else {
            panic!("endpoint");
        };

        assert_eq!(inline_rate_limit(&source.client.layer).windows.len(), 2);
        assert_eq!(inline_rate_limit(&endpoint.layer).cost, 2);
    }

    #[test]
    fn parses_explicit_id_by_and_cost_in_any_pre_window_order() {
        let source = super::super::parse(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit {
                    cost 2;
                    id "riot.application";
                    by [host];
                    2 / 1s;
                }
            }
            GET read() -> Bytes {
                rate_limit {
                    by [endpoint];
                    id "riot.endpoint";
                    cost 1;
                    1 / 1s;
                }
            }
        })
        .expect("explicit rate-limit IDs");
        let client = inline_rate_limit(&source.client.layer);
        let SourceChild::Endpoint(endpoint) = &source.children[0] else {
            panic!("endpoint");
        };
        let endpoint = inline_rate_limit(&endpoint.layer);

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
                base "https://example.test";
                rate_limit {
                    id "riot.first";
                    id "riot.second";
                    1 / 1s;
                }
            }
        });
        assert!(duplicate.contains("at most one `id` clause"));

        let late = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit {
                    1 / 1s;
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
                base "https://example.test";
                rate_limit {
                    by [host, endpoint, vars.tenant];
                    cost 2;
                    2 / 1s;
                }
            }
        })
        .expect("by before cost");
        let rule = inline_rate_limit(&source.client.layer);

        assert_eq!(
            rule.dimensions
                .iter()
                .map(RateLimitDimensionDecl::public_name)
                .collect::<Vec<_>>(),
            ["host", "endpoint", "vars.tenant"]
        );
        assert_eq!(rule.cost, 2);
    }

    #[test]
    fn parses_cost_before_by() {
        let source = super::super::parse(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit {
                    cost 2;
                    by [scope.region, ep.account];
                    2 / 1s;
                }
            }
        })
        .expect("cost before by");
        let rule = inline_rate_limit(&source.client.layer);

        assert_eq!(
            rule.dimensions
                .iter()
                .map(RateLimitDimensionDecl::public_name)
                .collect::<Vec<_>>(),
            ["scope.region", "ep.account"]
        );
        assert_eq!(rule.cost, 2);
    }

    #[test]
    fn parses_global_rule_without_by() {
        let source = super::super::parse(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { 1 / 1s; }
            }
        })
        .expect("declaration-global rule");

        assert!(
            inline_rate_limit(&source.client.layer)
                .dimensions
                .is_empty()
        );
    }

    #[test]
    fn rejects_empty_by() {
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by []; 1 / 1s; }
            }
        });

        assert!(error.contains("omit `by` for a declaration-global bucket"));
    }

    #[test]
    fn rejects_duplicate_by_clause() {
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [host]; by [endpoint]; 1 / 1s; }
            }
        });

        assert!(error.contains("at most one `by` clause"));
    }

    #[test]
    fn rejects_duplicate_dimension() {
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [vars.tenant, vars.tenant]; 1 / 1s; }
            }
        });

        assert!(error.contains("duplicate rate-limit dimension `vars.tenant`"));
    }

    #[test]
    fn rejects_by_after_window() {
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { 1 / 1s; by [host]; }
            }
        });

        assert!(error.contains("must appear before rate-limit windows"));
    }

    #[test]
    fn rejects_cost_after_window() {
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { 1 / 1s; cost 1; }
            }
        });

        assert!(error.contains("must appear before rate-limit windows"));
    }

    #[test]
    fn rejects_unknown_bare_dimension() {
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [path]; 1 / 1s; }
            }
        });

        assert!(error.contains("unsupported rate-limit dimension `path`"));
    }

    #[test]
    fn rejects_unknown_namespace() {
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [secret.token]; 1 / 1s; }
            }
        });

        assert!(error.contains("unknown rate-limit dimension namespace `secret`"));
    }

    #[test]
    fn rejects_literal_and_formatted_dimensions() {
        for (dimension, expected) in [
            (
                quote!("tenant"),
                "`by` dimensions do not accept literal values",
            ),
            (
                quote!(fmt[vars.tenant]),
                "`by` dimensions do not accept formatted expressions",
            ),
        ] {
            let error = parse_error(quote! {
                client RateApi {
                    base "https://example.test";
                    rate_limit { by [#dimension]; 1 / 1s; }
                }
            });
            assert!(error.contains(expected), "unexpected diagnostic: {error}");
        }
    }

    #[test]
    fn rejects_first_excess_dimension() {
        let dimensions = (0..=MAX_RATE_LIMIT_KEY_PARTS).map(|index| {
            let name = format_ident!("value_{index}");
            quote!(vars.#name)
        });
        let error = parse_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [#(#dimensions),*]; 1 / 1s; }
            }
        });

        assert_eq!(
            error,
            format!("a rate-limit key permits at most {MAX_RATE_LIMIT_KEY_PARTS} dimensions")
        );
    }

    #[test]
    fn rejects_the_first_excess_static_window() {
        let windows = (0..=MAX_RATE_LIMIT_WINDOWS_PER_RULE).map(|_| quote! { 1 / 1s; });
        let error = super::super::parse(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { #(#windows)* }
            }
        })
        .err()
        .expect("first excess static window");

        assert!(
            error
                .to_string()
                .contains("a rate-limit rule permits at most 16 windows")
        );
    }
}
