use std::collections::HashSet;
use std::time::Duration;

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitInt, LitStr, Token, braced, bracketed};

use super::ast::{
    DurationLiteral, HeaderEntry, HeaderOperation, HostOperation, PathOperation, PublicExpr,
    QueryEntry, QueryOperation, TimeoutOperation,
};
use super::diagnostics::{
    SpanError, validate_duration_integer, validate_integer, validate_public_header_name,
    validate_static_host_label, validate_static_segment,
};
use super::{MAX_FORMAT_DEPTH, keyword};

pub(super) fn parse_host(input: ParseStream<'_>) -> syn::Result<HostOperation> {
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

pub(super) fn parse_path(input: ParseStream<'_>) -> syn::Result<PathOperation> {
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

pub(super) fn parse_query(input: ParseStream<'_>) -> syn::Result<QueryOperation> {
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

pub(super) fn parse_headers(input: ParseStream<'_>) -> syn::Result<HeaderOperation> {
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

pub(super) fn parse_timeout(input: ParseStream<'_>) -> syn::Result<TimeoutOperation> {
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

#[cfg(test)]
mod tests {
    use quote::quote;

    use crate::syntax::ast::SourceChild;

    #[test]
    fn parses_request_shaping_clauses_independently_of_source_order() {
        let source = super::super::parse(quote! {
            client RequestApi { base "https://example.test"; }
            GET read(id: String) -> Bytes {
                timeout 2s;
                headers { "X-Static" = "one"; }
                query { "id" = ep.id; }
                path ["items", ep.id];
                host ["api"];
            }
        })
        .expect("request syntax");
        let SourceChild::Endpoint(endpoint) = &source.children[0] else {
            panic!("endpoint");
        };

        assert!(endpoint.layer.host.is_some());
        assert!(endpoint.layer.path.is_some());
        assert!(endpoint.layer.query.is_some());
        assert!(endpoint.layer.headers.is_some());
        assert!(endpoint.layer.timeout.is_some());
    }
}
