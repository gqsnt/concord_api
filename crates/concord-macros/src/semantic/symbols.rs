use std::collections::HashMap;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{GenericArgument, Ident, PathArguments, Type, TypePath};

use super::model::{
    Child, ResolvedDefault, ResolvedValue, ResolvedValueKind, Value, ValueReference, ValueShape,
};
use super::surface::{
    RESERVED_GENERATED_METHODS, canonical_ident, insert_name, value_setter_ident,
};
use super::{Resolver, SpanError};
use crate::syntax::ast::{
    DefaultExpr as SourceDefault, ValueDecl, ValueKind as SourceValueKind, syntactic_option_inner,
};

#[derive(Clone)]
pub(super) struct ScopeFrame {
    pub(super) id: usize,
    pub(super) values: Vec<Value>,
}

pub(super) fn convert_values(values: Vec<ValueDecl>) -> syn::Result<Vec<Value>> {
    values
        .into_iter()
        .map(|value| {
            let shape = value_shape(&value.ty)?;
            Ok(Value {
                name: value.name,
                ty: value.ty,
                kind: resolve_value_kind(value.kind),
                shape,
                source_position: value.position,
            })
        })
        .collect()
}

pub(super) fn resolve_default(source: SourceDefault) -> ResolvedDefault {
    match source {
        SourceDefault::String(value) => ResolvedDefault::String(value),
        SourceDefault::Integer { negative, literal } => {
            ResolvedDefault::Integer { negative, literal }
        }
        SourceDefault::Boolean(value) => ResolvedDefault::Boolean(value),
        SourceDefault::Path(value) => ResolvedDefault::Path(value),
        SourceDefault::TraitDefault(span) => ResolvedDefault::TraitDefault(span),
    }
}

fn resolve_value_kind(source: SourceValueKind) -> ResolvedValueKind {
    match source {
        SourceValueKind::Required => ResolvedValueKind::Required,
        SourceValueKind::Optional => ResolvedValueKind::Optional,
        SourceValueKind::Defaulted(value) => ResolvedValueKind::Defaulted(resolve_default(value)),
    }
}

fn value_shape(ty: &Type) -> syn::Result<ValueShape> {
    if let Some(inner) = syntactic_option_inner(ty)? {
        if let Some(element) = single_outer_argument(&inner, "Vec")? {
            return Ok(ValueShape::OptionVec { inner, element });
        }
        return Ok(ValueShape::Option { inner });
    }
    if let Some(element) = single_outer_argument(ty, "Vec")? {
        return Ok(ValueShape::Vec { element });
    }
    Ok(ValueShape::Scalar)
}

fn single_outer_argument(ty: &Type, expected: &str) -> syn::Result<Option<Type>> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return Ok(None);
    };
    if path.leading_colon.is_some()
        || path.segments.len() != 1
        || path.segments[0].ident != expected
    {
        return Ok(None);
    }
    let PathArguments::AngleBracketed(arguments) = &path.segments[0].arguments else {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    };
    let mut arguments = arguments.args.iter();
    let Some(GenericArgument::Type(inner)) = arguments.next() else {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    };
    if arguments.next().is_some() {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    }
    Ok(Some(inner.clone()))
}

pub(super) fn find_value(values: &[Value], name: &Ident) -> Option<usize> {
    let name = canonical_ident(name);
    values
        .iter()
        .position(|value| canonical_ident(&value.name) == name)
}

pub(super) fn find_unique_value(values: &[Value], name: &Ident, owner: &str) -> syn::Result<usize> {
    find_value(values, name).ok_or_else(|| {
        name.span()
            .error(format!("unknown {owner} `{}`", canonical_ident(name)))
    })
}

pub(super) fn validate_setter_namespace(
    values: &[Value],
    owner: &str,
    include_required: bool,
    reserve_builder_methods: bool,
) -> syn::Result<()> {
    let mut names = HashMap::new();
    if reserve_builder_methods {
        for name in RESERVED_GENERATED_METHODS {
            names.insert((*name).to_owned(), Span::call_site());
        }
    }
    for value in values {
        if include_required || !value.is_required() {
            insert_name(
                &mut names,
                canonical_ident(&value_setter_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
            if value.accepts_absence() {
                insert_name(
                    &mut names,
                    format!("clear_{}", canonical_ident(&value.name)),
                    value.name.span(),
                    owner,
                )?;
            }
            if value.has_default() {
                insert_name(
                    &mut names,
                    format!("reset_{}", canonical_ident(&value.name)),
                    value.name.span(),
                    owner,
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn validate_endpoint_setters(
    parameters: &[Value],
    scopes: &[ScopeFrame],
) -> syn::Result<()> {
    let mut values = scopes
        .iter()
        .flat_map(|scope| scope.values.iter())
        .filter(|value| !value.is_required())
        .cloned()
        .collect::<Vec<_>>();
    values.extend(
        parameters
            .iter()
            .filter(|value| !value.is_required())
            .cloned(),
    );
    validate_setter_namespace(&values, "explicit endpoint controls", false, true)?;
    validate_setter_namespace(parameters, "endpoint request controls", false, true)
}

pub(super) fn validate_child_methods(
    children: &[Child],
    local_values: &[Value],
    owner: &str,
    reserve_all: bool,
) -> syn::Result<()> {
    let mut names = HashMap::new();
    if reserve_all {
        for reserved in RESERVED_GENERATED_METHODS {
            names.insert((*reserved).to_owned(), Span::call_site());
        }
    }
    for value in local_values.iter().filter(|value| !value.is_required()) {
        insert_name(
            &mut names,
            canonical_ident(&value_setter_ident(&value.name)),
            value.name.span(),
            owner,
        )?;
        if value.accepts_absence() {
            insert_name(
                &mut names,
                format!("clear_{}", canonical_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
        }
        if value.has_default() {
            insert_name(
                &mut names,
                format!("reset_{}", canonical_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
        }
    }
    for child in children {
        let (name, span) = match child {
            Child::Scope(scope) => (
                canonical_ident(&scope.surface.method_name),
                scope.name.span(),
            ),
            Child::Endpoint(endpoint) => (
                canonical_ident(&endpoint.surface.method_name),
                endpoint.name.span(),
            ),
        };
        if RESERVED_GENERATED_METHODS.contains(&name.as_str()) {
            return Err(span.error(format!(
                "generated {owner} method `{name}` collides with a reserved Concord method"
            )));
        }
        insert_name(&mut names, name, span, owner)?;
    }
    Ok(())
}

impl Resolver<'_> {
    pub(super) fn resolve_value_reference(
        &self,
        namespace: &Ident,
        name: &Ident,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
    ) -> syn::Result<ResolvedValue> {
        match namespace.to_string().as_str() {
            "vars" => {
                let index = find_unique_value(self.client_values, name, "client value")?;
                Ok(ResolvedValue {
                    reference: ValueReference::Client(index),
                    shape: self.client_values[index].shape.clone(),
                })
            }
            "scope" => {
                let matches = scopes
                    .iter()
                    .filter_map(|scope| {
                        find_value(&scope.values, name).map(|parameter| (scope, parameter))
                    })
                    .collect::<Vec<_>>();
                if matches.is_empty() {
                    return Err(name.span().error(format!(
                        "unknown visible scope value `{}`",
                        canonical_ident(name)
                    )));
                }
                if matches.len() != 1 {
                    return Err(name.span().error(format!(
                        "ambiguous visible scope value `{}`; qualify the DSL model so the name is unique",
                        canonical_ident(name)
                    )));
                }
                let (scope, parameter) = matches[0];
                Ok(ResolvedValue {
                    reference: ValueReference::Scope {
                        scope_id: scope.id,
                        parameter,
                    },
                    shape: scope.values[parameter].shape.clone(),
                })
            }
            "ep" => {
                let Some(endpoint) = endpoint else {
                    return Err(name
                        .span()
                        .error("`ep.NAME` is available only while shaping an endpoint"));
                };
                let index = find_unique_value(endpoint, name, "endpoint value")?;
                Ok(ResolvedValue {
                    reference: ValueReference::Endpoint(index),
                    shape: endpoint[index].shape.clone(),
                })
            }
            _ => Err(namespace
                .span()
                .error("value references use only `vars.NAME`, `scope.NAME`, or `ep.NAME`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::model::{ResolvedExpr, ValueReference};
    use super::super::test_support;
    use super::*;

    #[test]
    fn resolves_syntactic_shapes_and_lexical_symbol_owners_once() {
        let api = test_support::resolve_tokens(quote! {
            client SymbolsApi {
                base "https://example.test";
                vars { tenant: String; }
            }
            scope resources(tags: Vec<String>) {
                GET read(optional: Option<u64>) -> Bytes {
                    query {
                        "client" = vars.tenant;
                        "scope" = scope.tags;
                        "endpoint" = ep.optional;
                    }
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");

        assert!(matches!(
            endpoint.parameters[0].shape,
            ValueShape::Option { .. }
        ));
        let references = endpoint
            .config
            .query
            .iter()
            .map(|entry| match &entry.value {
                ResolvedExpr::Value(value) => value.reference,
                _ => panic!("resolved reference"),
            })
            .collect::<Vec<_>>();
        assert!(matches!(references[0], ValueReference::Client(0)));
        assert!(matches!(
            references[1],
            ValueReference::Scope {
                scope_id: 0,
                parameter: 0
            }
        ));
        assert!(matches!(references[2], ValueReference::Endpoint(0)));
    }
}
