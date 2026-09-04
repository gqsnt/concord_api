use std::time::Duration;

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitBool, LitChar, LitInt, LitStr, Token, Type, braced, bracketed};

use super::ast::{
    DurationLiteral, HeaderEntry, HeaderOperation, PathOperation, PublicExpr, QueryEmission,
    QueryEntry, QueryOperation, SubdomainOperation, TimeoutOperation,
};
use super::diagnostics::{
    SpanError, validate_duration_integer, validate_integer, validate_public_header_name,
    validate_static_host_label, validate_static_segment,
};
use super::{MAX_FORMAT_DEPTH, keyword};

pub(super) fn parse_subdomain(input: ParseStream<'_>) -> syn::Result<SubdomainOperation> {
    input.parse::<keyword::subdomain>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(SubdomainOperation::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    let values = parse_expression_list(input, "subdomain")?;
    for value in &values {
        if let PublicExpr::String(value) = value {
            validate_static_host_label(value)?;
        }
    }
    input.parse::<Token![;]>()?;
    Ok(if replace {
        SubdomainOperation::Replace(values)
    } else {
        SubdomainOperation::Prepend(values)
    })
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
    while !body.is_empty() {
        let entry = if body.peek(keyword::remove) {
            body.parse::<keyword::remove>()?;
            QueryEntry::Remove(body.parse()?)
        } else {
            let when_present = body.peek(keyword::when_present);
            let append = body.peek(keyword::append);
            if when_present {
                body.parse::<keyword::when_present>()?;
            } else if append {
                body.parse::<keyword::append>()?;
            }
            let key: LitStr = body.parse()?;
            if body.peek(Token![+=]) {
                return Err(body.error("v2.1 removed `+=`; write `append \"K\" = value;`"));
            }
            body.parse::<Token![=]>()?;
            let emission = parse_query_emission(&body)?;
            if when_present {
                QueryEntry::WhenPresent { key, emission }
            } else if append {
                QueryEntry::Append { key, emission }
            } else {
                QueryEntry::Assign { key, emission }
            }
        };
        body.parse::<Token![;]>()?;
        entries.push(entry);
    }
    Ok(if replace {
        QueryOperation::Replace(entries)
    } else {
        QueryOperation::Modify(entries)
    })
}

fn parse_query_emission(input: ParseStream<'_>) -> syn::Result<QueryEmission> {
    if input.peek(keyword::each) {
        let marker: keyword::each = input.parse()?;
        let namespace: Ident = input.parse().map_err(|_| {
            input.error("`each` requires one direct owner reference such as `endpoint.tags`")
        })?;
        input.parse::<Token![.]>().map_err(|_| {
            input.error("`each` requires one direct owner reference such as `endpoint.tags`")
        })?;
        let name: Ident = input.parse()?;
        if input.peek(Token![.])
            || input.peek(syn::token::Paren)
            || input.peek(syn::token::Bracket)
            || input.peek(Token![!])
        {
            return Err(input.error("`each` accepts only a direct owner/value reference"));
        }
        let adapter = if input.peek(keyword::via) {
            input.parse::<keyword::via>()?;
            Some(Box::new(input.parse::<Type>()?))
        } else {
            None
        };
        return Ok(QueryEmission::Each {
            namespace,
            name,
            adapter,
            span: marker.span(),
        });
    }
    Ok(QueryEmission::One(parse_public_expr(input, 0)?))
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
    while !body.is_empty() {
        let entry = if body.peek(keyword::remove) {
            body.parse::<keyword::remove>()?;
            HeaderEntry::Remove(body.parse()?)
        } else {
            let when_present = body.peek(keyword::when_present);
            let append = body.peek(keyword::append);
            if when_present {
                body.parse::<keyword::when_present>()?;
            } else if append {
                body.parse::<keyword::append>()?;
            }
            let name: LitStr = body.parse()?;
            if body.peek(Token![+=]) {
                return Err(body.error("v2.1 removed `+=`; write `append \"K\" = value;`"));
            }
            body.parse::<Token![=]>()?;
            let emission = parse_query_emission(&body)?;
            if when_present {
                HeaderEntry::WhenPresent { name, emission }
            } else if append {
                HeaderEntry::Append { name, emission }
            } else {
                HeaderEntry::Assign { name, emission }
            }
        };
        body.parse::<Token![;]>()?;
        let name = match &entry {
            HeaderEntry::Assign { name, .. }
            | HeaderEntry::WhenPresent { name, .. }
            | HeaderEntry::Append { name, .. }
            | HeaderEntry::Remove(name) => name,
        };
        validate_public_header_name(name)?;
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
        return Err(input.error("v2.1 removed `timeout off`; declare one positive finite duration"));
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

pub(super) fn parse_total_public_expr(input: ParseStream<'_>) -> syn::Result<PublicExpr> {
    parse_public_expr(input, 0)
}

fn parse_public_expr(input: ParseStream<'_>, depth: usize) -> syn::Result<PublicExpr> {
    if input.peek(syn::token::Bracket) {
        return Err(input.error(
            "bracketed interpolation is not DSL syntax; concatenate public atoms with `concat [...]`",
        ));
    }
    let mut expression = if input.peek(LitStr) {
        PublicExpr::String(input.parse()?)
    } else if input.peek(LitChar) {
        PublicExpr::Character(input.parse()?)
    } else if input.peek(LitBool) {
        PublicExpr::Boolean(input.parse()?)
    } else if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let literal: LitInt = input
            .parse()
            .map_err(|_| input.error("a public negative atom must be a signed integer literal"))?;
        validate_integer(&literal, "public expression")?;
        PublicExpr::Integer {
            negative: true,
            literal,
        }
    } else if input.peek(LitInt) {
        let literal: LitInt = input.parse()?;
        validate_integer(&literal, "public expression")?;
        PublicExpr::Integer {
            negative: false,
            literal,
        }
    } else if input.peek(keyword::fmt) {
        return Err(input.error("v1 `fmt[...]` was removed; write `concat [...]`"));
    } else if input.peek(keyword::concat) {
        let marker: keyword::concat = input.parse()?;
        if depth >= MAX_FORMAT_DEPTH {
            return Err(marker
                .span()
                .error("nested `concat [...]` expressions exceed the maximum depth of 64"));
        }
        let content;
        bracketed!(content in input);
        if content.is_empty() {
            return Err(content.error("`concat [...]` requires at least one expression"));
        }
        let mut values = Vec::new();
        while !content.is_empty() {
            values.push(parse_public_expr(&content, depth + 1)?);
            if content.is_empty() {
                break;
            }
            content
                .parse::<Token![,]>()
                .map_err(|_| content.error("`concat [...]` atoms must be separated by commas"))?;
        }
        return Ok(PublicExpr::Concat {
            span: marker.span(),
            values,
        });
    } else {
        let namespace: Ident = input.parse().map_err(|_| {
            input.error(
                "public expressions accept only literals, owner-qualified references, or `concat [...]`",
            )
        })?;
        input.parse::<Token![.]>().map_err(|_| {
            namespace.span().error(
                "arbitrary Rust expressions are not supported; use `client.NAME`, `endpoint.NAME`, or a visible named scope owner",
            )
        })?;
        let name: Ident = input.parse()?;
        if input.peek(Token![.])
            || input.peek(syn::token::Paren)
            || input.peek(syn::token::Bracket)
            || input.peek(Token![!])
        {
            return Err(input.error(
                "arbitrary Rust expressions are not supported after a public value reference",
            ));
        }
        match namespace.to_string().as_str() {
            "vars" => {
                return Err(namespace
                    .span()
                    .error("v1 `vars.NAME` was removed; use `client.NAME`"));
            }
            "scope" => return Err(namespace.span().error(
                "v1 `scope.NAME` was removed; use the actual visible scope identifier, for example `regional.NAME`",
            )),
            "ep" => {
                return Err(namespace
                    .span()
                    .error("v1 `ep.NAME` was removed; use `endpoint.NAME`"));
            }
            "secret" => return Err(namespace.span().error(
                "secrets are private credential sources and cannot appear in public expressions",
            )),
            "credential" | "credentials" => return Err(namespace.span().error(
                "credentials are private and cannot appear in public expressions",
            )),
            _ => {}
        }
        PublicExpr::Reference { namespace, name }
    };

    if input.peek(keyword::via) {
        input.parse::<keyword::via>()?;
        expression = PublicExpr::Adapted {
            value: Box::new(expression),
            adapter: Box::new(input.parse()?),
        };
    }
    Ok(expression)
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
            client RequestApi {
                timeout 30s; base "https://example.test";
            GET read(id: String) -> Bytes via Binary {
                timeout 2s;
                headers { "X-Static" = "one"; }
                query { "id" = endpoint.id; }
                path ["items", endpoint.id];
                subdomain ["api"];
            }
        }})
        .expect("request syntax");
        let SourceChild::Endpoint(endpoint) = &source.client.children[0] else {
            panic!("endpoint");
        };

        assert!(endpoint.attachment_site.subdomain.is_some());
        assert!(endpoint.attachment_site.path.is_some());
        assert!(endpoint.attachment_site.query.is_some());
        assert!(endpoint.attachment_site.headers.is_some());
        assert!(endpoint.attachment_site.timeout.is_some());
    }

    #[test]
    fn rejects_bracketed_interpolation_with_fmt_replacement() {
        let error = super::super::parse(quote! {
            client RequestApi {
                timeout 30s; base "https://example.test";
            GET read(id: String) -> Bytes via Binary {
                path [["items-", endpoint.id]];
            }
        }})
        .err()
        .expect("bracketed interpolation must fail");

        assert!(
            error
                .to_string()
                .contains("concatenate public atoms with `concat [...]`")
        );
    }
}
