use std::collections::HashSet;

use syn::parse::ParseStream;
use syn::{Ident, LitBool, LitChar, LitFloat, LitInt, LitStr, Token, Type, braced};

use super::ast::{PaginationDecl, PaginationField, PaginationInitializer, PaginationValue};
use super::diagnostics::SpanError;
use super::keyword;
use super::values::parse_stable_default;
use crate::semantic::surface::canonical_ident;

pub(super) fn parse_pagination(input: ParseStream<'_>) -> syn::Result<PaginationDecl> {
    input.parse::<keyword::paginate>()?;
    if input.peek(keyword::with) {
        let old: keyword::with = input.parse()?;
        return Err(old
            .span
            .error("v2.1 replaced pagination keyword `with` with `via`"));
    }
    if !input.peek(keyword::via) {
        return Err(input.error(
            "v1 pagination syntax was removed; pagination requires `paginate via Controller { field -> endpoint.slot; stable = value; }`",
        ));
    }
    input.parse::<keyword::via>()?;
    let controller: Type = input.parse().map_err(|_| {
        input.error("`paginate via` requires one Rust controller type and a binding block")
    })?;
    let body;
    braced!(body in input);
    let mut fields = Vec::new();
    let mut seen_fields = HashSet::new();
    let mut seen_destinations = HashSet::new();
    while !body.is_empty() {
        let name: Ident = body
            .parse()
            .map_err(|_| body.error("a pagination controller field begins with its field name"))?;
        let canonical = canonical_ident(&name);
        if !seen_fields.insert(canonical.clone()) {
            return Err(name.span().error(format!(
                "duplicate pagination controller field `{canonical}`"
            )));
        }
        let initializer = if body.peek(Token![->]) {
            body.parse::<Token![->]>()?;
            let namespace: Ident = body.parse()?;
            if canonical_ident(&namespace) != "endpoint" {
                return Err(namespace.span().error(
                    "a live pagination binding may update only an endpoint-owned slot (`field -> endpoint.name`)",
                ));
            }
            body.parse::<Token![.]>()?;
            let destination: Ident = body.parse()?;
            if body.peek(Token![.])
                || body.peek(syn::token::Paren)
                || body.peek(syn::token::Bracket)
                || body.peek(Token![!])
            {
                return Err(body.error(
                    "a live pagination destination must be one direct `endpoint.NAME` reference",
                ));
            }
            let destination_name = canonical_ident(&destination);
            if !seen_destinations.insert(destination_name.clone()) {
                return Err(destination.span().error(format!(
                    "duplicate live pagination destination `endpoint.{destination_name}`"
                )));
            }
            PaginationInitializer::Live {
                namespace,
                destination,
            }
        } else if body.peek(Token![=]) {
            body.parse::<Token![=]>()?;
            PaginationInitializer::Value(parse_pagination_value(&body)?)
        } else if body.peek(Token![:]) || body.peek(keyword::bind) {
            return Err(body.error(
                "v1 pagination field syntax was removed; use `field -> endpoint.slot;` for live authority or `field = value;` for stable state",
            ));
        } else {
            return Err(body
                .error("pagination fields require `-> endpoint.slot` or `= stable_initializer`"));
        };
        body.parse::<Token![;]>()?;
        fields.push(PaginationField { name, initializer });
    }
    Ok(PaginationDecl { controller, fields })
}

fn parse_pagination_value(input: ParseStream<'_>) -> syn::Result<PaginationValue> {
    if input.peek(Token![default]) {
        let keyword: Token![default] = input.parse()?;
        return Err(keyword.span.error(
            "`default` is not a pagination initializer; use a stable literal, const/static path, or visible total owner reference",
        ));
    }
    if input.peek(LitStr)
        || input.peek(LitChar)
        || input.peek(LitInt)
        || input.peek(LitFloat)
        || input.peek(LitBool)
        || input.peek(Token![-])
    {
        return parse_stable_default(input).map(PaginationValue::Stable);
    }
    let fork = input.fork();
    if fork.parse::<Ident>().is_ok() && fork.peek(Token![.]) {
        let namespace: Ident = input.parse()?;
        match canonical_ident(&namespace).as_str() {
            "vars" => {
                return Err(namespace
                    .span()
                    .error("v1 `vars.NAME` was removed; use `client.NAME`"));
            }
            "scope" => {
                return Err(namespace.span().error(
                    "v1 `scope.NAME` was removed; use the actual visible scope identifier",
                ));
            }
            "ep" => {
                return Err(namespace
                    .span()
                    .error("v1 `ep.NAME` was removed; use `endpoint.NAME`"));
            }
            _ => {}
        }
        input.parse::<Token![.]>()?;
        let name: Ident = input.parse()?;
        if input.peek(Token![.])
            || input.peek(syn::token::Paren)
            || input.peek(syn::token::Bracket)
            || input.peek(Token![!])
        {
            return Err(input
                .error("pagination stable references must be one direct owner/value reference"));
        }
        return Ok(PaginationValue::Reference { namespace, name });
    }
    parse_stable_default(input)
        .map(PaginationValue::Stable)
        .map_err(|_| {
            input.error(
                "pagination initializers accept only direct total owner references, supported literals, or Rust const/static paths",
            )
        })
}
