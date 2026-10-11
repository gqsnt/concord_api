//! Rate-limit registry and attachment syntax (§12.1).
//!
//! `owner`/`context` parameters name the owning declaration in every
//! diagnostic (DIAG-01): the client for registry entries (``policy `reads`
//! of client `Api` ``) and the attaching owner for attachments.

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitInt, LitStr, Token, Type, braced, bracketed, parenthesized};

use super::ast::{
    BoundDimension, FormalDimension, PolicyDecl, RateAttachment, RateRegistry, WindowLiteral,
    WindowSetDecl, WindowSource,
};
use super::diagnostics::{Code, coded, related};
use super::keyword;
use super::request::{parse_duration, parse_reference};
use super::values::{AttrSite, parse_attrs};
use crate::semantic::surface::canonical_ident;

pub(crate) fn parse_rate_registry(
    input: ParseStream<'_>,
    owner: &str,
) -> syn::Result<RateRegistry> {
    input.parse::<keyword::rate_limits>()?;
    let body;
    braced!(body in input);
    let mut registry = RateRegistry::default();
    while !body.is_empty() {
        let attrs = parse_attrs(&body, AttrSite::Registry, "a rate-limit definition", owner)?;
        if body.peek(keyword::windows) {
            body.parse::<keyword::windows>()?;
            let name: Ident = body.parse()?;
            let context = format!("window set `{}` of {owner}", canonical_ident(&name));
            let content;
            braced!(content in body);
            let windows = parse_windows(&content, &context)?;
            if windows.is_empty() {
                return Err(coded(
                    name.span(),
                    Code::Rate,
                    format!("{context} requires at least one `allow N per DURATION;`"),
                ));
            }
            registry.window_sets.push(WindowSetDecl {
                attrs,
                name,
                windows,
            });
            continue;
        }
        if body.peek(keyword::policy) {
            registry.policies.push(parse_policy(&body, attrs, owner)?);
            continue;
        }
        if body.fork().parse::<Ident>().is_ok() && body.peek2(syn::token::Paren) {
            return Err(coded(
                body.span(),
                Code::Migrate,
                format!(
                    "rate-limit definitions are `policy NAME(formals) {{ ... }}` or `windows NAME {{ ... }}` (in the `rate_limits` registry of {owner})"
                ),
            ));
        }
        return Err(coded(
            body.span(),
            Code::Rate,
            format!(
                "the `rate_limits` registry of {owner} accepts `windows NAME {{ ... }}` and `policy NAME(formals) {{ ... }}`"
            ),
        ));
    }
    Ok(registry)
}

fn parse_windows(input: ParseStream<'_>, context: &str) -> syn::Result<Vec<WindowLiteral>> {
    let mut windows = Vec::new();
    while !input.is_empty() {
        if !input.peek(keyword::allow) {
            return Err(coded(
                input.span(),
                Code::Rate,
                format!("expected `allow N per DURATION;` in {context}"),
            ));
        }
        windows.push(parse_window(input, context)?);
    }
    Ok(windows)
}

fn parse_window(input: ParseStream<'_>, context: &str) -> syn::Result<WindowLiteral> {
    input.parse::<keyword::allow>()?;
    let capacity_literal: LitInt = input.parse()?;
    let capacity = positive_u32(&capacity_literal, "window capacity", context)?;
    input.parse::<keyword::per>()?;
    let duration_literal: LitInt = input.parse()?;
    let duration = parse_duration(&duration_literal, context)?;
    input.parse::<Token![;]>()?;
    Ok(WindowLiteral {
        capacity,
        capacity_span: capacity_literal.span(),
        duration: super::ast::DurationLiteral {
            value: duration,
            span: duration_literal.span(),
        },
    })
}

fn positive_u32(literal: &LitInt, what: &str, context: &str) -> syn::Result<u32> {
    if !literal.suffix().is_empty() {
        return Err(coded(
            literal.span(),
            Code::Rate,
            format!("{what} in {context} must be an unsuffixed integer; write e.g. `10`"),
        ));
    }
    let value = literal.base10_parse::<u32>().map_err(|_| {
        coded(
            literal.span(),
            Code::Rate,
            format!("{what} in {context} must be a positive u32; write a value in 1..=4294967295"),
        )
    })?;
    if value == 0 {
        return Err(coded(
            literal.span(),
            Code::Rate,
            format!("{what} must be positive in {context}; write a value of at least 1"),
        ));
    }
    Ok(value)
}

fn parse_cost(input: ParseStream<'_>, context: &str) -> syn::Result<(u32, proc_macro2::Span)> {
    input.parse::<keyword::cost>()?;
    let literal: LitInt = input.parse()?;
    Ok((positive_u32(&literal, "cost", context)?, literal.span()))
}

fn parse_window_source(input: ParseStream<'_>, context: &str) -> syn::Result<WindowSource> {
    if input.peek(keyword::using) {
        input.parse::<keyword::using>()?;
        let name: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        if input.peek(keyword::allow) {
            return Err(coded(
                input.span(),
                Code::Rate,
                format!(
                    "{context} uses either one named window set or inline windows, not a mixture; keep `using SET;` or the `allow` lines"
                ),
            ));
        }
        return Ok(WindowSource::Using(name));
    }
    let windows = parse_windows(input, context)?;
    if windows.is_empty() {
        return Err(coded(
            input.span(),
            Code::Rate,
            format!(
                "{context} requires `using WINDOW_SET;` or at least one `allow N per DURATION;`"
            ),
        ));
    }
    Ok(WindowSource::Inline(windows))
}

fn parse_id(input: ParseStream<'_>, context: &str) -> syn::Result<LitStr> {
    if !input.peek(keyword::id) {
        return Err(coded(
            input.span(),
            Code::Rate,
            format!("every rate policy begins with an explicit `id \"...\";` ({context})"),
        ));
    }
    input.parse::<keyword::id>()?;
    let id: LitStr = input.parse()?;
    input.parse::<Token![;]>()?;
    validate_id_length(&id, context)?;
    Ok(id)
}

fn validate_id_length(id: &LitStr, context: &str) -> syn::Result<()> {
    let value = id.value();
    if value.is_empty() || value.len() > crate::MAX_RATE_LIMIT_ID_BYTES {
        return Err(coded(
            id.span(),
            Code::Rate,
            format!(
                "a policy ID must be 1..={} UTF-8 bytes ({context}); write a stable ID such as `\"service.reads\"`",
                crate::MAX_RATE_LIMIT_ID_BYTES
            ),
        ));
    }
    Ok(())
}

fn parse_policy(
    input: ParseStream<'_>,
    attrs: super::ast::Attrs,
    owner: &str,
) -> syn::Result<PolicyDecl> {
    let span = input.span();
    input.parse::<keyword::policy>()?;
    let name: Ident = input.parse()?;
    let context = format!("policy `{}` of {owner}", canonical_ident(&name));
    let formals_content;
    parenthesized!(formals_content in input);
    let mut formals: Vec<Ident> = Vec::new();
    while !formals_content.is_empty() {
        let formal: Ident = formals_content.parse()?;
        if let Some(existing) = formals
            .iter()
            .find(|existing| canonical_ident(existing) == canonical_ident(&formal))
        {
            let error = coded(
                formal.span(),
                Code::Rate,
                format!(
                    "duplicate policy formal `{}` in {context}; declare each formal once",
                    canonical_ident(&formal)
                ),
            );
            return Err(related(error, existing.span(), "first declared here"));
        }
        formals.push(formal);
        if formals_content.is_empty() {
            break;
        }
        formals_content.parse::<Token![,]>()?;
    }
    let body;
    braced!(body in input);
    let id = parse_id(&body, &context)?;
    let key = parse_formal_key(&body, &context)?;
    let cost = if body.peek(keyword::cost) {
        let cost = parse_cost(&body, &context)?;
        body.parse::<Token![;]>()?;
        Some(cost)
    } else {
        None
    };
    let windows = parse_window_source(&body, &context)?;
    if !body.is_empty() {
        return Err(coded(
            body.span(),
            Code::Rate,
            format!(
                "unexpected tokens in {context}; a policy body is `id; key [...]; cost N;? (using SET; | allow N per D; ...)`"
            ),
        ));
    }
    Ok(PolicyDecl {
        attrs,
        name,
        formals,
        id,
        key,
        cost,
        windows,
        span,
    })
}

fn parse_formal_key(input: ParseStream<'_>, context: &str) -> syn::Result<Vec<FormalDimension>> {
    if !input.peek(keyword::key) {
        return Err(coded(
            input.span(),
            Code::Rate,
            format!("{context} requires `key [origin, \"label\" = formal, ...];` after its `id`"),
        ));
    }
    input.parse::<keyword::key>()?;
    let content;
    bracketed!(content in input);
    let mut dimensions = Vec::new();
    while !content.is_empty() {
        dimensions.push(if content.peek(keyword::origin) {
            let token: keyword::origin = content.parse()?;
            FormalDimension::Origin(token.span())
        } else if content.peek(keyword::endpoint) {
            let token: keyword::endpoint = content.parse()?;
            FormalDimension::Endpoint(token.span())
        } else if content.peek(LitStr) {
            let label: LitStr = content.parse()?;
            validate_label(&label, context)?;
            content.parse::<Token![=]>()?;
            let formal: Ident = content.parse()?;
            let via = parse_optional_via(&content)?;
            FormalDimension::Custom { label, formal, via }
        } else if let Ok(bare) = content.fork().parse::<Ident>() {
            return Err(coded(
                bare.span(),
                Code::Migrate,
                format!(
                    "rate key labels are explicit in v3: write `\"{0}\" = {0}` (the label is state-affecting and independent of the Rust name; {context})",
                    canonical_ident(&bare)
                ),
            ));
        } else {
            return Err(coded(
                content.span(),
                Code::Rate,
                format!(
                    "a key dimension is `origin`, `endpoint` or `\"label\" = formal (via Encoder)?` (in {context})"
                ),
            ));
        });
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    input.parse::<Token![;]>()?;
    Ok(dimensions)
}

fn parse_optional_via(input: ParseStream<'_>) -> syn::Result<Option<Type>> {
    if input.peek(keyword::via) {
        input.parse::<keyword::via>()?;
        Ok(Some(input.parse()?))
    } else {
        Ok(None)
    }
}

fn validate_label(label: &LitStr, context: &str) -> syn::Result<()> {
    let value = label.value();
    if value.is_empty() || value.len() > crate::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES {
        return Err(coded(
            label.span(),
            Code::Rate,
            format!(
                "a custom key label of {context} must be 1..={} UTF-8 bytes; write a short label such as `\"tenant\"`",
                crate::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES
            ),
        ));
    }
    if value == "origin" || value == "endpoint" {
        return Err(coded(
            label.span(),
            Code::Rate,
            format!(
                "`origin` and `endpoint` are reserved built-in dimension labels ({context}); write the bare `{value}` dimension or choose another label"
            ),
        ));
    }
    Ok(())
}

fn parse_bound_key(input: ParseStream<'_>, context: &str) -> syn::Result<Vec<BoundDimension>> {
    input.parse::<keyword::key>()?;
    let content;
    bracketed!(content in input);
    let mut dimensions = Vec::new();
    while !content.is_empty() {
        dimensions.push(if content.peek(keyword::origin) {
            let token: keyword::origin = content.parse()?;
            BoundDimension::Origin(token.span())
        } else if content.peek(keyword::endpoint) && !content.peek2(Token![.]) {
            let token: keyword::endpoint = content.parse()?;
            BoundDimension::Endpoint(token.span())
        } else if content.peek(LitStr) {
            let label: LitStr = content.parse()?;
            validate_label(&label, context)?;
            content.parse::<Token![=]>()?;
            let reference = parse_reference(&content, context)?;
            let via = parse_optional_via(&content)?;
            BoundDimension::Custom {
                label,
                reference,
                via,
            }
        } else if content.fork().parse::<Ident>().is_ok() {
            let reference = parse_reference(&content, context)?;
            return Err(coded(
                reference.span,
                Code::Migrate,
                format!(
                    "rate key labels are explicit in v3: write `\"{0}\" = {0}` to preserve the legacy label ({context})",
                    reference.display()
                ),
            ));
        } else {
            return Err(coded(
                content.span(),
                Code::Rate,
                format!(
                    "a key dimension is `origin`, `endpoint` or `\"label\" = value (via Encoder)?` (in {context})"
                ),
            ));
        });
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    Ok(dimensions)
}

/// `rate_limit NAME(bindings) (; | { cost N; })`, `rate_limit { ... }`, or
/// `rate_limit "id" key [...] (cost N)? using SET;`.
pub(crate) fn parse_rate_attachment(
    input: ParseStream<'_>,
    owner: &str,
) -> syn::Result<RateAttachment> {
    input.parse::<keyword::rate_limit>()?;
    if input.peek(syn::token::Brace) {
        let context = format!("the inline rate policy of {owner}");
        let body;
        braced!(body in input);
        let id = parse_id(&body, &context)?;
        if !body.peek(keyword::key) {
            return Err(coded(
                body.span(),
                Code::Rate,
                format!("{context} requires `key [...];` after its `id`"),
            ));
        }
        let key = parse_bound_key(&body, &context)?;
        body.parse::<Token![;]>()?;
        let cost = if body.peek(keyword::cost) {
            let cost = parse_cost(&body, &context)?;
            body.parse::<Token![;]>()?;
            Some(cost)
        } else {
            None
        };
        let windows = parse_window_source(&body, &context)?;
        if !body.is_empty() {
            return Err(coded(
                body.span(),
                Code::Rate,
                format!(
                    "unexpected tokens in {context}; an inline policy is `{{ id; key [...]; cost N;? (using SET; | allow N per D; ...) }}`"
                ),
            ));
        }
        return Ok(RateAttachment::Inline {
            id,
            key,
            cost,
            windows,
        });
    }
    if input.peek(LitStr) {
        let context = format!("the short rate attachment of {owner}");
        let id: LitStr = input.parse()?;
        validate_id_length(&id, &context)?;
        if !input.peek(keyword::key) {
            return Err(coded(
                input.span(),
                Code::Rate,
                format!(
                    "the short attachment form of {owner} is `rate_limit \"id\" key [...] (cost N)? using SET;`"
                ),
            ));
        }
        let key = parse_bound_key(input, &context)?;
        let cost = if input.peek(keyword::cost) {
            Some(parse_cost(input, &context)?)
        } else {
            None
        };
        if !input.peek(keyword::using) {
            return Err(coded(
                input.span(),
                Code::Rate,
                format!("the short attachment form of {owner} ends with `using WINDOW_SET;`"),
            ));
        }
        input.parse::<keyword::using>()?;
        let set: Ident = input.parse()?;
        input.parse::<Token![;]>()?;
        return Ok(RateAttachment::Inline {
            id,
            key,
            cost,
            windows: WindowSource::Using(set),
        });
    }
    if input.peek(keyword::off) {
        return Err(coded(
            input.span(),
            Code::Rate,
            format!(
                "there is no `rate_limit off` in {owner}: rate attachments are additive and no descendant can disable an ancestor constraint (RATE-01); move the attachment to the owners it should cover instead"
            ),
        ));
    }
    let name: Ident = input.parse()?;
    let context = format!(
        "the attachment of policy `{}` in {owner}",
        canonical_ident(&name)
    );
    let content;
    parenthesized!(content in input);
    let mut bindings: Vec<(Ident, super::ast::Reference)> = Vec::new();
    while !content.is_empty() {
        let formal: Ident = content.parse()?;
        if !content.peek(Token![=]) {
            return Err(coded(
                formal.span(),
                Code::Rate,
                format!(
                    "policy bindings are named and exact: `formal = value` ({context}); write `{0} = {0}`",
                    canonical_ident(&formal)
                ),
            ));
        }
        content.parse::<Token![=]>()?;
        let reference = parse_reference(&content, owner)?;
        if let Some((existing, _)) = bindings
            .iter()
            .find(|(existing, _)| canonical_ident(existing) == canonical_ident(&formal))
        {
            let error = coded(
                formal.span(),
                Code::Rate,
                format!(
                    "duplicate binding for policy formal `{}` in {context}; bind each formal once",
                    canonical_ident(&formal)
                ),
            );
            return Err(related(error, existing.span(), "first binding here"));
        }
        bindings.push((formal, reference));
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    let cost = if input.peek(syn::token::Brace) {
        let body;
        braced!(body in input);
        let cost = parse_cost(&body, &context)?;
        body.parse::<Token![;]>()?;
        if !body.is_empty() {
            return Err(coded(
                body.span(),
                Code::Rate,
                format!("a named attachment block accepts only `cost N;` ({context})"),
            ));
        }
        Some(cost)
    } else {
        input.parse::<Token![;]>()?;
        None
    };
    Ok(RateAttachment::Named {
        name,
        bindings,
        cost,
    })
}
