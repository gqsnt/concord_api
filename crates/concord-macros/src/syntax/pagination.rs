use std::collections::HashSet;

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitBool, LitInt, LitStr, Token, Type, braced};

use super::ast::{PaginationDecl, PaginationField, PaginationInitializer, PaginationValue};
use super::diagnostics::SpanError;
use super::keyword;
use super::values::parse_default;
use crate::semantic::surface::canonical_ident;

pub(super) fn parse_pagination(input: ParseStream<'_>) -> syn::Result<PaginationDecl> {
    input.parse::<keyword::paginate>()?;
    if input.peek(keyword::custom) {
        return Err(input.error(
            "old `paginate custom factory(...)` syntax is no longer supported; use `paginate TYPE { field: bind ep.field, ... }`",
        ));
    }

    let controller_type: Type = input.parse().map_err(|_| {
        input.error("pagination requires one Rust controller type followed by a struct field block")
    })?;
    if !input.peek(syn::token::Brace) {
        return Err(controller_type.span().error(
            "pagination requires a struct field block; use `paginate TYPE { field: bind ep.field, ... }`",
        ));
    }
    let fields_body;
    braced!(fields_body in input);
    let mut fields = Vec::new();
    let mut seen_fields = HashSet::new();
    let mut seen_bindings = HashSet::new();
    while !fields_body.is_empty() {
        if fields_body.peek(Token![..]) {
            return Err(fields_body
                .error("pagination controller construction does not accept struct rest syntax"));
        }
        let name: Ident = fields_body.parse().map_err(|_| {
            fields_body.error("a pagination controller field begins with its field name")
        })?;
        let canonical = canonical_ident(&name);
        if !seen_fields.insert(canonical.clone()) {
            return Err(name.span().error(format!(
                "duplicate pagination controller field `{canonical}`"
            )));
        }
        if fields_body.peek(Token![=]) {
            return Err(fields_body.error(
                "old `field = ep.field` pagination syntax is no longer supported; use `field: bind ep.field`",
            ));
        }
        fields_body.parse::<Token![:]>().map_err(|_| {
            name.span()
                .error("pagination field shorthand is not supported; provide `field: initializer`")
        })?;
        let initializer = if fields_body.peek(keyword::bind) {
            fields_body.parse::<keyword::bind>()?;
            let namespace: Ident = fields_body.parse().map_err(|_| {
                fields_body.error("`bind` requires direct `ep.FIELD` or `scope.FIELD`")
            })?;
            if !matches!(namespace.to_string().as_str(), "ep" | "scope") {
                return Err(namespace
                    .span()
                    .error("`bind` accepts only direct `ep.FIELD` or `scope.FIELD` destinations"));
            }
            fields_body.parse::<Token![.]>().map_err(|_| {
                namespace
                    .span()
                    .error("`bind` requires direct `ep.FIELD` or `scope.FIELD`")
            })?;
            let destination: Ident = fields_body.parse()?;
            if fields_body.peek(Token![.])
                || fields_body.peek(syn::token::Paren)
                || fields_body.peek(syn::token::Bracket)
                || fields_body.peek(Token![!])
            {
                return Err(fields_body.error(
                    "pagination bindings must be direct `ep.FIELD` or `scope.FIELD` destinations",
                ));
            }
            let binding_key = format!(
                "{}.{}",
                canonical_ident(&namespace),
                canonical_ident(&destination)
            );
            if !seen_bindings.insert(binding_key.clone()) {
                return Err(destination.span().error(format!(
                    "duplicate pagination binding destination `{binding_key}`"
                )));
            }
            PaginationInitializer::Bind {
                namespace,
                destination,
            }
        } else {
            PaginationInitializer::Value(parse_pagination_value(&fields_body)?)
        };
        fields.push(PaginationField { name, initializer });
        if fields_body.is_empty() {
            break;
        }
        fields_body.parse::<Token![,]>().map_err(|_| {
            fields_body.error("pagination controller fields must be comma-separated")
        })?;
    }
    Ok(PaginationDecl {
        controller: controller_type,
        fields,
    })
}

fn parse_pagination_value(input: ParseStream<'_>) -> syn::Result<PaginationValue> {
    if input.peek(LitStr) || input.peek(LitInt) || input.peek(LitBool) || input.peek(Token![-]) {
        return parse_default(input).map(PaginationValue::Stable);
    }
    let fork = input.fork();
    if let Ok(namespace) = fork.parse::<Ident>() {
        if matches!(namespace.to_string().as_str(), "vars" | "scope" | "ep") && fork.peek(Token![.])
        {
            let namespace: Ident = input.parse()?;
            input.parse::<Token![.]>()?;
            let name: Ident = input.parse()?;
            if input.peek(Token![.])
                || input.peek(syn::token::Paren)
                || input.peek(syn::token::Bracket)
                || input.peek(Token![!])
            {
                return Err(input.error(
                    "pagination value references must be direct `vars.FIELD`, `scope.FIELD`, or `ep.FIELD`",
                ));
            }
            return Ok(PaginationValue::Reference { namespace, name });
        }
    }
    parse_default(input).map(PaginationValue::Stable).map_err(|_| {
        input.error(
            "pagination initializers accept only direct public value references, string/integer/boolean literals, or Rust const paths; calls, methods, macros, closures, async blocks, blocks, and formatting expressions are not supported",
        )
    })
}

#[cfg(test)]
mod tests {
    use quote::{ToTokens, quote};

    use crate::syntax::ast::SourceChild;

    #[test]
    fn parses_opaque_controller_types_and_ordered_initializers() {
        let source = super::super::parse(quote! {
            client PaginationApi {
                base "https://example.test";
                vars { tenant: String; }
            }
            GET list(cursor: Option<String>) -> Page {
                paginate crate::Controller<String> {
                    cursor: bind ep.cursor,
                    tenant: vars.tenant,
                    attempts: 0,
                }
            }
        })
        .expect("pagination syntax");
        let SourceChild::Endpoint(endpoint) = &source.children[0] else {
            panic!("endpoint");
        };
        let pagination = &endpoint.pagination.as_ref().unwrap().value;

        assert_eq!(pagination.fields.len(), 3);
        assert_eq!(
            pagination.controller.to_token_stream().to_string(),
            "crate :: Controller < String >"
        );
    }
}
