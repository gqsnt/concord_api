use std::collections::{HashMap, HashSet};

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Attribute, Ident, LitBool, LitInt, LitStr, Meta, Token, Type, braced};

use super::ast::{BodyDecl, DefaultExpr, ValueDecl, ValueKind, syntactic_option_inner};
use super::diagnostics::{SpanError, insert_unique_value, validate_integer};
use super::keyword;
use crate::semantic::surface::canonical_ident;

pub(super) fn parse_vars(input: ParseStream<'_>) -> syn::Result<Vec<ValueDecl>> {
    input.parse::<keyword::vars>()?;
    let body;
    braced!(body in input);
    let mut values = Vec::new();
    let mut names = HashSet::new();
    while !body.is_empty() {
        let value = parse_value(&body, values.len())?;
        body.parse::<Token![;]>()?;
        insert_unique_value(&mut names, &value, "client value")?;
        values.push(value);
    }
    Ok(values)
}

pub(super) fn parse_parameters(
    input: ParseStream<'_>,
    owner: &str,
    body_is_special: bool,
) -> syn::Result<(Vec<ValueDecl>, Option<BodyDecl>)> {
    let mut parameters = Vec::new();
    let mut body = None;
    let mut names = HashMap::new();
    let mut position = 0;

    while !input.is_empty() {
        let attributes = input.call(Attribute::parse_outer)?;
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let canonical = canonical_ident(&name);
        if let Some(first_span) = names.insert(canonical.clone(), name.span()) {
            let mut error = name
                .span()
                .error(format!("duplicate {owner} parameter `{canonical}`"));
            error.combine(first_span.error("the first parameter is here"));
            return Err(error);
        }

        let is_body = if attributes.is_empty() {
            false
        } else if !body_is_special {
            return Err(attributes[0]
                .span()
                .error("scope parameters do not accept attributes; `#[body]` is endpoint-only"));
        } else {
            for attribute in &attributes {
                if !attribute.path().is_ident("body") || !matches!(&attribute.meta, Meta::Path(_)) {
                    return Err(attribute.span().error(
                        "endpoint parameters accept only the bare `#[body]` role attribute",
                    ));
                }
            }
            if attributes.len() != 1 {
                return Err(attributes[1]
                    .span()
                    .error("a request body parameter accepts exactly one `#[body]` attribute"));
            }
            true
        };

        if is_body {
            if input.peek(Token![=]) {
                return Err(input.error("a request body parameter cannot have a default"));
            }
            if let Some(first) = &body {
                let first: &BodyDecl = first;
                let mut error = name.span().error(format!(
                    "endpoint declares two request body parameters: `{}` and `{canonical}`",
                    canonical_ident(&first.name)
                ));
                error.combine(first.name.span().error("the first request body is here"));
                return Err(error);
            }
            body = Some(BodyDecl { name, ty, position });
        } else {
            let kind = parse_value_kind(input, &ty)?;
            parameters.push(ValueDecl {
                name,
                ty,
                kind,
                position,
            });
        }
        position += 1;

        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok((parameters, body))
}

fn parse_value(input: ParseStream<'_>, position: usize) -> syn::Result<ValueDecl> {
    let name: Ident = input.parse()?;
    input.parse::<Token![:]>()?;
    let ty: Type = input.parse()?;
    let kind = parse_value_kind(input, &ty)?;
    Ok(ValueDecl {
        name,
        ty,
        kind,
        position,
    })
}

fn parse_value_kind(input: ParseStream<'_>, ty: &Type) -> syn::Result<ValueKind> {
    let optional = syntactic_option_inner(ty)?;
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

pub(super) fn parse_default(input: ParseStream<'_>) -> syn::Result<DefaultExpr> {
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

#[cfg(test)]
mod tests {
    use quote::quote;

    use crate::syntax::ast::SourceChild;

    #[test]
    fn parses_required_optional_defaulted_and_explicit_body_roles() {
        let source = super::super::parse(quote! {
            client ValuesApi {
                base "https://example.test";
                vars {
                    required: String;
                    optional: Option<u32>;
                    defaulted: bool = true;
                }
            }
            POST upload(first: String, #[body] payload: Body, later: Option<bool>) -> Response;
        })
        .expect("values syntax");
        let SourceChild::Endpoint(endpoint) = &source.children[0] else {
            panic!("endpoint");
        };

        assert_eq!(source.client.values.as_ref().unwrap().value.len(), 3);
        assert_eq!(endpoint.parameters.len(), 2);
        assert_eq!(endpoint.body.as_ref().unwrap().name, "payload");
        assert_eq!(endpoint.body.as_ref().unwrap().position, 1);
    }
}
