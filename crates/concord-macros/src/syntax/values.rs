//! Attributes, ordinary values, parameter lists, bodies and stable
//! initializers (§3.2, §3.5, §4).
//!
//! `what` parameters describe the declaration kind (`a client`, `client
//! value`, `endpoint parameter`, `a request body`); `owner` parameters are
//! the owning client/scope/endpoint label every diagnostic names (DIAG-01).

use std::collections::HashMap;

use proc_macro2::Span;
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{
    Attribute, Expr, ExprLit, Ident, Lit, LitBool, LitChar, LitFloat, LitInt, LitStr, Meta, Token,
    Type, braced, parenthesized,
};

use super::ast::{
    Attrs, BodyDecl, Deprecation, Placement, PlacementFamily, StableValue, ValueDecl,
};
use super::diagnostics::{
    Code, coded, keyword_suggestion, parse_with_keywords, related, validate_integer,
    validate_public_header_name, validate_query_key,
};
use super::keyword;
use crate::semantic::surface::canonical_ident;

/// Where attributes appear; controls which attributes are permitted.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum AttrSite {
    Declaration,
    Value,
    Body,
    Registry,
}

pub(crate) fn parse_attrs(
    input: ParseStream<'_>,
    site: AttrSite,
    what: &str,
    owner: &str,
) -> syn::Result<Attrs> {
    let attributes = input.call(Attribute::parse_outer)?;
    attrs_from(attributes, site, what, owner)
}

/// The subject named by attribute diagnostics: the owner itself for a
/// client/scope/endpoint declaration, otherwise ``{what} of {owner}``.
fn attribute_subject(site: AttrSite, what: &str, owner: &str) -> String {
    if site == AttrSite::Declaration {
        owner.to_owned()
    } else {
        format!("{what} of {owner}")
    }
}

pub(crate) fn attrs_from(
    attributes: Vec<Attribute>,
    site: AttrSite,
    what: &str,
    owner: &str,
) -> syn::Result<Attrs> {
    let subject = attribute_subject(site, what, owner);
    let mut result = Attrs::default();
    for attribute in attributes {
        let path = attribute.path();
        if path.is_ident("doc") {
            result.docs.push(doc_text(&attribute, &subject)?);
            continue;
        }
        if path.is_ident("deprecated") {
            if site == AttrSite::Body {
                return Err(coded(
                    attribute.span(),
                    Code::Owner,
                    format!("a request body cannot be deprecated; deprecate {owner} instead"),
                ));
            }
            if result.deprecated.is_some() {
                return Err(coded(
                    attribute.span(),
                    Code::Owner,
                    format!("duplicate `#[deprecated]` attribute on {subject}; keep one"),
                ));
            }
            result.deprecated = Some(parse_deprecation(&attribute, &subject)?);
            continue;
        }
        if path.is_ident("sensitive") || path.is_ident("default") {
            let label = if path.is_ident("sensitive") {
                "sensitive"
            } else {
                "default"
            };
            if site != AttrSite::Value {
                return Err(coded(
                    attribute.span(),
                    Code::Owner,
                    format!(
                        "`#[{label}]` applies only to ordinary values; {subject} is protected or initialized by its role, so remove the attribute"
                    ),
                ));
            }
            if !matches!(attribute.meta, Meta::Path(_)) {
                return Err(coded(
                    attribute.span(),
                    Code::Owner,
                    format!(
                        "`#[{label}]` must be a bare attribute on {subject}; write `#[{label}]`"
                    ),
                ));
            }
            let slot = if label == "sensitive" {
                &mut result.sensitive
            } else {
                &mut result.default
            };
            if slot.is_some() {
                return Err(coded(
                    attribute.span(),
                    Code::Owner,
                    format!("duplicate `#[{label}]` attribute on {subject}; keep one"),
                ));
            }
            *slot = Some(attribute.span());
            continue;
        }
        if path.is_ident("redact") {
            return Err(coded(
                attribute.span(),
                Code::Migrate,
                format!("`#[redact]` was renamed; write `#[sensitive]` on {subject}"),
            ));
        }
        if path.is_ident("cfg") || path.is_ident("cfg_attr") {
            return Err(coded(
                attribute.span(),
                Code::Owner,
                format!(
                    "child-level `cfg`/`cfg_attr` is not supported inside `api!` (on {subject}); condition the whole invocation or its enclosing module"
                ),
            ));
        }
        let mut candidates = vec!["doc"];
        if site != AttrSite::Body && result.deprecated.is_none() {
            candidates.push("deprecated");
        }
        if site == AttrSite::Value {
            if result.sensitive.is_none() {
                candidates.push("sensitive");
            }
            if result.default.is_none() {
                candidates.push("default");
            }
        }
        let hint = path
            .get_ident()
            .map(|written| crate::suggestions::suggestion(&canonical_ident(written), candidates))
            .unwrap_or_default();
        return Err(coded(
            attribute.span(),
            Code::Owner,
            format!(
                "unsupported attribute on {subject}; supported attributes are docs, `#[deprecated]`, and (ordinary values only) `#[sensitive]` and `#[default]`{hint}"
            ),
        ));
    }
    Ok(result)
}

fn doc_text(attribute: &Attribute, subject: &str) -> syn::Result<LitStr> {
    let Meta::NameValue(meta) = &attribute.meta else {
        return Err(coded(
            attribute.span(),
            Code::Owner,
            format!(
                "malformed Rust doc attribute on {subject}; write `/// text` or `#[doc = \"text\"]`"
            ),
        ));
    };
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &meta.value
    else {
        return Err(coded(
            attribute.span(),
            Code::Owner,
            format!(
                "Rust doc attributes on {subject} require literal string text; write `#[doc = \"text\"]`"
            ),
        ));
    };
    Ok(value.clone())
}

fn parse_deprecation(attribute: &Attribute, subject: &str) -> syn::Result<Deprecation> {
    let mut note = None;
    let mut since = None;
    match &attribute.meta {
        Meta::Path(_) => {}
        Meta::List(_) => {
            attribute.parse_nested_meta(|meta| {
                let slot = if meta.path.is_ident("note") {
                    &mut note
                } else if meta.path.is_ident("since") {
                    &mut since
                } else {
                    let candidates = [("note", note.is_none()), ("since", since.is_none())];
                    let hint = meta.path.get_ident().map(|written| {
                        crate::suggestions::suggestion(&canonical_ident(written), candidates.into_iter().filter_map(|(name, available)| available.then_some(name)))
                    }).unwrap_or_default();
                    return Err(meta.error(format!(
                        "[C3-OWNER] `#[deprecated]` on {subject} accepts only `note = \"...\"` and `since = \"...\"`{hint}"
                    )));
                };
                if slot.is_some() {
                    return Err(meta.error(format!(
                        "[C3-OWNER] duplicate deprecation field on {subject}; write each of `note` and `since` at most once"
                    )));
                }
                *slot = Some(meta.value()?.parse::<LitStr>()?);
                Ok(())
            })?;
        }
        Meta::NameValue(value) => {
            let Expr::Lit(ExprLit {
                lit: Lit::Str(text),
                ..
            }) = &value.value
            else {
                return Err(coded(
                    attribute.span(),
                    Code::Owner,
                    format!(
                        "`#[deprecated = ...]` on {subject} requires a string; write `#[deprecated = \"note\"]`"
                    ),
                ));
            };
            note = Some(text.clone());
        }
    }
    Ok(Deprecation {
        note,
        since,
        span: attribute.span(),
    })
}

pub(crate) fn parse_values(input: ParseStream<'_>, owner: &str) -> syn::Result<Vec<ValueDecl>> {
    input.parse::<keyword::values>()?;
    let body;
    braced!(body in input);
    let mut values = Vec::new();
    let mut names = HashMap::new();
    while !body.is_empty() {
        let attributes = body.call(Attribute::parse_outer)?;
        if body.peek(keyword::body) {
            return Err(coded(
                body.span(),
                Code::Owner,
                format!(
                    "request bodies are endpoint-only parameters and cannot appear in the `values` block of {owner}; declare `body name: T` among an endpoint's parameters"
                ),
            ));
        }
        let value = parse_ordinary_value(&body, attributes, values.len(), "client value", owner)?;
        body.parse::<Token![;]>().map_err(|_| {
            body.error(format!(
                "[C3-OWNER] each root value declaration of {owner} ends with `;`; write `name: Type;`{}",
                keyword_suggestion(&body, if value.placement.is_some() && value.via.is_none() && value.initializer.is_none() { &["via"] } else { &[] })
            ))
        })?;
        insert_name(&mut names, &value.name, "client value", owner)?;
        values.push(value);
    }
    Ok(values)
}

pub(crate) fn parse_parameters(
    input: ParseStream<'_>,
    what: &str,
    allow_body: bool,
    owner: &str,
) -> syn::Result<(Vec<ValueDecl>, Option<BodyDecl>)> {
    let mut parameters = Vec::new();
    let mut body: Option<BodyDecl> = None;
    let mut names = HashMap::new();
    let mut position = 0;
    while !input.is_empty() {
        let attributes = input.call(Attribute::parse_outer)?;
        if input.peek(keyword::body) {
            if !allow_body {
                return Err(coded(
                    input.span(),
                    Code::Owner,
                    format!(
                        "request bodies are endpoint-only parameters and cannot be declared by {owner}; declare `body name: T` among an endpoint's parameters"
                    ),
                ));
            }
            input.parse::<keyword::body>()?;
            let name: Ident = input.parse()?;
            insert_name(&mut names, &name, what, owner)?;
            let optional = parse_optional_marker(input);
            input.parse::<Token![:]>()?;
            let ty: Type = input.parse()?;
            let via = parse_via(input, owner)?;
            if input.peek(Token![=]) {
                return Err(coded(
                    input.span(),
                    Code::Owner,
                    format!(
                        "a request body of {owner} cannot have an initializer; body defaults are not supported, so remove `= ...` (use `body {}?: T` for an optional body)",
                        canonical_ident(&name)
                    ),
                ));
            }
            let attrs = attrs_from(attributes, AttrSite::Body, "a request body", owner)?;
            if let Some(first) = &body {
                let error = coded(
                    name.span(),
                    Code::Owner,
                    format!(
                        "{owner} declares a second request body `{}`; an endpoint has at most one body, so combine the payloads into one body type",
                        canonical_ident(&name)
                    ),
                );
                return Err(related(
                    error,
                    first.name.span(),
                    "the first request body is here",
                ));
            }
            body = Some(BodyDecl {
                attrs,
                name,
                optional,
                ty,
                via,
                position,
            });
        } else {
            let value = parse_ordinary_value(input, attributes, position, what, owner)?;
            insert_name(&mut names, &value.name, what, owner)?;
            parameters.push(value);
        }
        position += 1;
        if input.is_empty() {
            break;
        }
        let via_allowed = if let Some(body) = &body {
            body.position == position - 1 && body.via.is_none()
        } else {
            false
        } || parameters.last().is_some_and(|value| {
            value.position == position - 1
                && value.placement.is_some()
                && value.via.is_none()
                && value.initializer.is_none()
        });
        parse_with_keywords::<Token![,]>(input, if via_allowed { &["via"] } else { &[] })?;
    }
    Ok((parameters, body))
}

fn insert_name(
    names: &mut HashMap<String, Span>,
    name: &Ident,
    what: &str,
    owner: &str,
) -> syn::Result<()> {
    let canonical = canonical_ident(name);
    if let Some(first) = names.insert(canonical.clone(), name.span()) {
        let error = coded(
            name.span(),
            Code::Name,
            format!("duplicate {what} `{canonical}` in {owner}; rename one of them"),
        );
        return Err(related(error, first, "the first declaration is here"));
    }
    Ok(())
}

fn parse_optional_marker(input: ParseStream<'_>) -> bool {
    if input.peek(Token![?]) {
        let _ = input.parse::<Token![?]>();
        true
    } else {
        false
    }
}

fn parse_via(input: ParseStream<'_>, owner: &str) -> syn::Result<Option<Type>> {
    if input.peek(keyword::via) {
        input.parse::<keyword::via>()?;
        Ok(Some(input.parse()?))
    } else if input.peek(keyword::using) {
        Err(coded(
            input.span(),
            Code::Migrate,
            format!("v2 `using Representation` was replaced by `via Representation` in {owner}"),
        ))
    } else {
        Ok(None)
    }
}

fn parse_placement(input: ParseStream<'_>, owner: &str) -> syn::Result<Option<Placement>> {
    // `query` / `header` are only placements when followed by an identifier,
    // `(` or `each`; otherwise they are ordinary value names.
    let family = if input.peek(keyword::query) && placement_follows(input) {
        let span = input.span();
        input.parse::<keyword::query>()?;
        (PlacementFamily::Query, span)
    } else if input.peek(keyword::header) && placement_follows(input) {
        let span = input.span();
        input.parse::<keyword::header>()?;
        (PlacementFamily::Header, span)
    } else {
        return Ok(None);
    };
    let (family, span) = family;
    let key = if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        let key: LitStr = content.parse().map_err(|_| {
            content.error(format!(
                "[C3-PLACE] a placement key in {owner} is one string literal; write `query(\"page\") page: u32`"
            ))
        })?;
        if !content.is_empty() {
            return Err(coded(
                content.span(),
                Code::Place,
                format!(
                    "a placement key in {owner} is exactly one string literal; write `header(\"X-Trace\") trace: String`"
                ),
            ));
        }
        match family {
            PlacementFamily::Query => validate_query_key(&key, owner)?,
            PlacementFamily::Header => {
                validate_public_header_name(&key, owner)?;
            }
        }
        Some(key)
    } else {
        if family == PlacementFamily::Header {
            return Err(coded(
                span,
                Code::Place,
                format!(
                    "a direct header placement in {owner} names its field explicitly: write `header(\"Accept-Language\") language?: Language`"
                ),
            ));
        }
        None
    };
    let each = if input.peek(keyword::each) {
        input.parse::<keyword::each>()?;
        true
    } else {
        false
    };
    Ok(Some(Placement {
        family,
        key,
        each,
        span,
    }))
}

fn placement_follows(input: ParseStream<'_>) -> bool {
    let fork = input.fork();
    if fork.parse::<Ident>().is_err() {
        return false;
    }
    fork.peek(syn::token::Paren)
        || fork.peek(keyword::each)
        || (fork.peek(Ident) && !fork.peek2(Token![.]))
}

pub(crate) fn parse_ordinary_value(
    input: ParseStream<'_>,
    attributes: Vec<Attribute>,
    position: usize,
    what: &str,
    owner: &str,
) -> syn::Result<ValueDecl> {
    let attrs = attrs_from(attributes, AttrSite::Value, what, owner)?;
    let placement_marker = input.fork();
    let placement = parse_placement(input, owner)?;
    let name: Ident = input.parse()?;
    if name == "endpoint" || name == "client" || canonical_ident(&name).starts_with("__concord") {
        return Err(coded(
            name.span(),
            Code::Name,
            format!(
                "`{}` is a reserved owner name and cannot name a value of {owner}; rename the value",
                canonical_ident(&name)
            ),
        ));
    }
    let optional = parse_optional_marker(input);
    input.parse::<Token![:]>().map_err(|_| {
        let candidates: &[&str] = if placement.is_none() && placement_follows(&placement_marker) {
            if what == "endpoint parameter" {
                &["query", "header", "body"]
            } else {
                &["query", "header"]
            }
        } else if placement.as_ref().is_some_and(|placement| !placement.each) && input.peek(Ident) {
            &["each"]
        } else {
            &[]
        };
        input.error(format!(
            "[C3-OWNER] a value declaration in {owner} is `name: Type` or `name?: Type`{}",
            crate::suggestions::suggestion(&canonical_ident(&name), candidates)
        ))
    })?;
    let ty: Type = input.parse()?;
    let via = if input.peek(keyword::via) {
        let span = input.span();
        input.parse::<keyword::via>()?;
        let adapter: Type = input.parse()?;
        if placement.is_none() {
            return Err(coded(
                span,
                Code::Place,
                format!(
                    "value `{}` of {owner} is unplaced, so it selects its adapter at each explicit use (`{{name via Adapter}}`, `\"k\" = name via Adapter`); `via` on a declaration belongs to a direct query/header placement",
                    canonical_ident(&name)
                ),
            ));
        }
        Some(adapter)
    } else {
        None
    };
    let initializer = if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
        if input.peek(Token![default]) {
            let retired: Token![default] = input.parse()?;
            return Err(coded(
                retired.span,
                Code::Migrate,
                format!(
                    "`= default` was removed; place `#[default]` on the value declaration (`{}` of {owner})",
                    canonical_ident(&name)
                ),
            ));
        }
        Some(parse_stable_value(input, owner)?)
    } else {
        None
    };
    if attrs.default.is_some() && initializer.is_some() {
        return Err(coded(
            name.span(),
            Code::Owner,
            format!(
                "`#[default]` and an explicit `= VALUE` initializer are mutually exclusive on value `{}` of {owner}; keep one",
                canonical_ident(&name)
            ),
        ));
    }
    Ok(ValueDecl {
        attrs,
        placement,
        name,
        optional,
        ty,
        via,
        initializer,
        position,
    })
}

/// `stable_value` (§3.1): literal, unary-negative numeric literal or a
/// constant path. No call, indexing, closure or block.
pub(crate) fn parse_stable_value(input: ParseStream<'_>, owner: &str) -> syn::Result<StableValue> {
    if input.peek(LitStr) {
        return input.parse().map(StableValue::Str);
    }
    if input.peek(LitChar) {
        return input.parse().map(StableValue::Char);
    }
    if input.peek(LitBool) {
        return input.parse::<LitBool>().map(StableValue::Bool);
    }
    let negative = if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        true
    } else {
        false
    };
    if input.peek(LitInt) {
        let literal: LitInt = input.parse()?;
        validate_integer(&literal, "initializer", owner)?;
        return Ok(StableValue::Int { negative, literal });
    }
    if input.peek(LitFloat) {
        let literal: LitFloat = input.parse()?;
        return Ok(StableValue::Float { negative, literal });
    }
    if negative {
        return Err(coded(
            input.span(),
            Code::Owner,
            format!(
                "a negative stable value in {owner} requires a numeric literal; write `-1` or name a Rust constant"
            ),
        ));
    }
    if input.fork().parse::<syn::Path>().is_ok() {
        let path: syn::Path = input.parse()?;
        if input.peek(syn::token::Paren)
            || input.peek(syn::token::Brace)
            || input.peek(syn::token::Bracket)
            || input.peek(Token![.])
            || input.peek(Token![!])
        {
            return Err(coded(
                input.span(),
                Code::Owner,
                format!(
                    "stable initializers accept a literal or a Rust constant path, not a call, method, macro, block, index or collection (in {owner}); name a supporting Rust constant instead"
                ),
            ));
        }
        return Ok(StableValue::Path(path));
    }
    Err(coded(
        input.span(),
        Code::Owner,
        format!(
            "stable initializers accept only string, char, boolean or numeric literals, or a Rust constant path (in {owner}); write a literal such as `= \"en\"` or name a constant"
        ),
    ))
}
