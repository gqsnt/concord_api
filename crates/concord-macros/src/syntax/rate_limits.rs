use std::collections::HashSet;
use std::time::Duration;

use syn::ext::IdentExt;
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitInt, LitStr, Token, braced, bracketed};

use super::ast::{DurationLiteral, RateLimitDecl, RateLimitDimensionDecl, RateLimitWindowLiteral};
use super::diagnostics::{SpanError, validate_duration_integer, validate_integer};
use super::keyword;
use crate::{MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_WINDOWS_PER_RULE};

pub(super) fn parse_rate_limit(input: ParseStream<'_>) -> syn::Result<RateLimitDecl> {
    input.parse::<keyword::rate_limit>()?;
    if !input.peek(syn::token::Brace) {
        return Err(input.error(
            "rate-limit rules are now direct `rate_limit { ... }` declarations; named attachments, `replace`, and `off` were removed",
        ));
    }
    let body;
    braced!(body in input);

    let mut dimensions = None;
    let mut cost = None;
    let mut windows = Vec::new();
    while !body.is_empty() {
        if body.peek(keyword::by) {
            if !windows.is_empty() {
                return Err(
                    body.error("`by` and `cost` clauses must appear before rate-limit windows")
                );
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
                return Err(
                    body.error("`by` and `cost` clauses must appear before rate-limit windows")
                );
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
            cost = Some(parse_positive_u32(&literal, "rate-limit cost")?);
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
            duration: DurationLiteral {
                value: duration,
                span: duration_literal.span(),
            },
        });
    }
    if windows.is_empty() {
        return Err(input.error("a rate-limit declaration requires at least one positive window"));
    }
    let cost = cost.unwrap_or(1);
    if let Some(window) = windows.iter().find(|window| cost > window.capacity) {
        return Err(window
            .duration
            .span
            .error("rate-limit cost must not exceed any window capacity"));
    }
    Ok(RateLimitDecl {
        dimensions: dimensions.unwrap_or_default(),
        cost,
        windows,
    })
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

    use crate::syntax::ast::{RateLimitDimensionDecl, SourceChild};
    use crate::{MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_WINDOWS_PER_RULE};

    fn parse_error(tokens: proc_macro2::TokenStream) -> String {
        super::super::parse(tokens)
            .err()
            .expect("rate-limit syntax error")
            .to_string()
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

        assert_eq!(
            source
                .client
                .layer
                .rate_limit
                .as_ref()
                .unwrap()
                .value
                .windows
                .len(),
            2
        );
        assert_eq!(endpoint.layer.rate_limit.as_ref().unwrap().value.cost, 2);
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
        let rule = &source.client.layer.rate_limit.as_ref().unwrap().value;

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
        let rule = &source.client.layer.rate_limit.as_ref().unwrap().value;

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
            source
                .client
                .layer
                .rate_limit
                .as_ref()
                .unwrap()
                .value
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
