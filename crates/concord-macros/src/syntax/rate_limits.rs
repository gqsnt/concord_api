use std::time::Duration;

use syn::parse::ParseStream;
use syn::{LitInt, Token, braced};

use super::ast::{DurationLiteral, RateLimitDecl, RateLimitWindowLiteral};
use super::diagnostics::{SpanError, validate_duration_integer, validate_integer};
use super::keyword;
use crate::MAX_RATE_LIMIT_WINDOWS_PER_RULE;

pub(super) fn parse_rate_limit(input: ParseStream<'_>) -> syn::Result<RateLimitDecl> {
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
            return Err(body.error(
                "`by [...]` dimensions were removed; normal generated rules use declaration-scoped global buckets",
            ));
        }
        if body.peek(keyword::cost) {
            return Err(
                body.error("a rate-limit rule permits at most one cost clause before windows")
            );
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

#[cfg(test)]
mod tests {
    use quote::quote;

    use crate::MAX_RATE_LIMIT_WINDOWS_PER_RULE;
    use crate::syntax::ast::SourceChild;

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
