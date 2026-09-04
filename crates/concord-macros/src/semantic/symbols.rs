use std::collections::HashMap;

use proc_macro2::Span;
use syn::Ident;

use super::model::{
    Child, ObservationClass, ResolvedDefault, ResolvedValue, Value, ValuePresence, ValueReference,
};
use super::surface::{
    RESERVED_GENERATED_METHODS, canonical_ident, insert_name, value_setter_ident,
};
use super::{Resolver, SpanError};
use crate::syntax::ast::{
    DefaultExpr as SourceDefault, Observation as SourceObservation, ValueDecl,
    ValuePresence as SourcePresence,
};

#[derive(Clone)]
pub(super) struct ScopeFrame {
    pub(super) id: usize,
    pub(super) name: Ident,
    pub(super) values: Vec<Value>,
}

pub(super) fn convert_values(values: Vec<ValueDecl>) -> syn::Result<Vec<Value>> {
    values
        .into_iter()
        .map(|value| {
            let payload_type = value.payload_type;
            let effective_type = if matches!(value.presence, SourcePresence::Optional) {
                syn::parse_quote!(::std::option::Option<#payload_type>)
            } else {
                payload_type.clone()
            };
            Ok(Value {
                name: value.name,
                payload_type,
                effective_type,
                presence: match value.presence {
                    SourcePresence::Required => ValuePresence::Required,
                    SourcePresence::Optional => ValuePresence::Optional,
                },
                initializer: value.initializer.map(resolve_default),
                observation: match value.observation {
                    SourceObservation::Ordinary => ObservationClass::OrdinaryPublic,
                    SourceObservation::Redacted => ObservationClass::RedactedPublic,
                },
                docs: value.docs.into_iter().map(|doc| doc.value()).collect(),
                source_position: value.position,
            })
        })
        .collect()
}

pub(super) fn resolve_default(source: SourceDefault) -> ResolvedDefault {
    match source {
        SourceDefault::String(value) => ResolvedDefault::String(value),
        SourceDefault::Character(value) => ResolvedDefault::Character(value),
        SourceDefault::Integer { negative, literal } => {
            ResolvedDefault::Integer { negative, literal }
        }
        SourceDefault::Boolean(value) => ResolvedDefault::Boolean(value),
        SourceDefault::Float { negative, literal } => ResolvedDefault::Float { negative, literal },
        SourceDefault::Path(value) => ResolvedDefault::Path(value),
        SourceDefault::TraitDefault(span) => ResolvedDefault::TraitDefault(span),
    }
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
                    format!("with_{}", canonical_ident(&value.name)),
                    value.name.span(),
                    owner,
                )?;
                insert_name(
                    &mut names,
                    format!("omit_{}", canonical_ident(&value.name)),
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

pub(super) fn validate_endpoint_setters(parameters: &[Value]) -> syn::Result<()> {
    validate_setter_namespace(
        parameters,
        "endpoint intent and request controls",
        false,
        true,
    )
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
                format!("with_{}", canonical_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
            insert_name(
                &mut names,
                format!("omit_{}", canonical_ident(&value.name)),
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
            "client" => {
                let index = find_unique_value(self.client_values, name, "client value")?;
                Ok(ResolvedValue {
                    reference: ValueReference::Client(index),
                    presence: self.client_values[index].presence,
                    observation: self.client_values[index].observation,
                })
            }
            "endpoint" => {
                let Some(endpoint) = endpoint else {
                    return Err(name
                        .span()
                        .error("`endpoint.NAME` is available only while shaping an endpoint"));
                };
                let index = find_unique_value(endpoint, name, "endpoint value")?;
                Ok(ResolvedValue {
                    reference: ValueReference::Endpoint(index),
                    presence: endpoint[index].presence,
                    observation: endpoint[index].observation,
                })
            }
            scope_name => {
                let scope = scopes
                    .iter()
                    .find(|scope| canonical_ident(&scope.name) == scope_name)
                    .ok_or_else(|| {
                        namespace.span().error(format!(
                            "unknown value owner `{scope_name}`; use `client`, `endpoint`, or an exact visible scope identifier"
                        ))
                    })?;
                let parameter =
                    find_unique_value(&scope.values, name, &format!("scope `{scope_name}` value"))?;
                Ok(ResolvedValue {
                    reference: ValueReference::Scope {
                        scope_id: scope.id,
                        parameter,
                    },
                    presence: scope.values[parameter].presence,
                    observation: scope.values[parameter].observation,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::{ToTokens, quote};

    use super::super::model::{ResolvedExpr, ValueReference};
    use super::super::test_support;
    use super::*;

    #[test]
    fn preserves_payload_type_tokens_without_semantic_type_recognition() {
        let api = test_support::resolve_tokens(quote! {
            client ShapeApi {
                base "https://example.test";
                timeout 30s;
                values {
                    option: Option<T>;
                    std_option?: std::option::Option<T>;
                    vector: Vec<T>;
                    option_vector?: Option<Vec<T>>;
                    associated: <T as Trait>::Output;
                    custom_option: crate::custom::Option<T>;
                    core_option: core::option::Option<T>;
                    std_vector: std::vec::Vec<T>;
                    alloc_vector: alloc::vec::Vec<T>;
                }
            }
        });
        let values = &api.client.values;

        assert!(matches!(values[0].presence, ValuePresence::Required));
        assert!(matches!(values[1].presence, ValuePresence::Optional));
        assert!(matches!(values[2].presence, ValuePresence::Required));
        assert!(matches!(values[3].presence, ValuePresence::Optional));
        assert_eq!(
            values[0].payload_type.to_token_stream().to_string(),
            "Option < T >"
        );
        assert_eq!(
            values[2].payload_type.to_token_stream().to_string(),
            "Vec < T >"
        );
        assert_eq!(
            values[3].payload_type.to_token_stream().to_string(),
            "Option < Vec < T > >"
        );
    }

    #[test]
    fn function_pointer_types_remain_opaque_scalars_with_complete_syntax() {
        let api = test_support::resolve_tokens(quote! {
            client FunctionPointerApi {
                base "https://example.test";
                timeout 30s;
                GET process(callback: fn(u32) -> bool) -> Bytes via Binary;
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "process");
        let callback = &endpoint.parameters[0];

        assert!(matches!(callback.presence, ValuePresence::Required));
        assert_eq!(
            callback.payload_type.to_token_stream().to_string(),
            quote!(fn(u32) -> bool).to_string()
        );
        assert!(
            crate::emit::generate(&api, &quote!(::concord))
                .to_string()
                .contains(&quote!(fn(u32) -> bool).to_string())
        );
    }

    #[test]
    fn resolves_syntactic_shapes_and_lexical_symbol_owners_once() {
        let api = test_support::resolve_tokens(quote! {
            client SymbolsApi {
                base "https://example.test";
                timeout 30s;
                values { tenant: String; }
                scope resources(tags: String) {
                GET read(optional?: u64) -> Bytes via Binary {
                    query {
                        "client" = client.tenant;
                        "scope" = resources.tags;
                        "endpoint" = endpoint.optional;
                    }
                }
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");

        assert!(matches!(
            endpoint.parameters[0].presence,
            ValuePresence::Optional
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
