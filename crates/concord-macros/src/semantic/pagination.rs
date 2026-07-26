use std::collections::HashSet;

use super::model::{
    Pagination, PaginationField, PaginationInitializer, PaginationValue, Value, ValueReference,
};
use super::surface::canonical_ident;
use super::symbols::{ScopeFrame, resolve_default};
use super::{Resolver, SpanError};
use crate::syntax::ast::{
    PaginationDecl, PaginationInitializer as SourcePaginationInitializer,
    PaginationValue as SourcePaginationValue,
};

pub(super) struct PaginationBoundValue {
    pub(super) reference: ValueReference,
    pub(super) public_name: String,
    pub(super) span: proc_macro2::Span,
}

pub(super) fn pagination_bound_values(
    pagination: Option<&Pagination>,
    client: &[Value],
    scopes: &[ScopeFrame],
    endpoint: &[Value],
) -> Vec<PaginationBoundValue> {
    let mut values = Vec::new();
    if let Some(pagination) = pagination {
        for field in &pagination.fields {
            if let PaginationInitializer::Bound { value, span } = &field.initializer {
                if !values
                    .iter()
                    .any(|existing: &PaginationBoundValue| existing.reference == value.reference)
                {
                    values.push(PaginationBoundValue {
                        reference: value.reference,
                        public_name: public_value_name(value.reference, client, scopes, endpoint),
                        span: *span,
                    });
                }
            }
        }
    }
    values
}

fn public_value_name(
    reference: ValueReference,
    client: &[Value],
    scopes: &[ScopeFrame],
    endpoint: &[Value],
) -> String {
    match reference {
        ValueReference::Client(index) => {
            format!("vars.{}", canonical_ident(&client[index].name))
        }
        ValueReference::Scope {
            scope_id,
            parameter,
        } => {
            let scope = scopes
                .iter()
                .find(|scope| scope.id == scope_id)
                .expect("resolved pagination scope must remain visible");
            format!("scope.{}", canonical_ident(&scope.values[parameter].name))
        }
        ValueReference::Endpoint(index) => {
            format!("ep.{}", canonical_ident(&endpoint[index].name))
        }
    }
}

impl Resolver<'_> {
    pub(super) fn resolve_pagination(
        &self,
        pagination: Option<PaginationDecl>,
        scopes: &[ScopeFrame],
        endpoint: &[Value],
    ) -> syn::Result<Option<Pagination>> {
        let Some(pagination) = pagination else {
            return Ok(None);
        };
        let PaginationDecl { controller, fields } = pagination;
        let mut bound_destinations = HashSet::new();
        let fields = fields
            .into_iter()
            .map(|field| {
                let initializer = match field.initializer {
                    SourcePaginationInitializer::Bind {
                        namespace,
                        destination,
                    } => {
                        let span = namespace.span();
                        let value = self.resolve_value_reference(
                            &namespace,
                            &destination,
                            scopes,
                            Some(endpoint),
                        )?;
                        if !bound_destinations.insert(value.reference) {
                            return Err(destination.span().error(
                                "duplicate pagination binding destination after resolution",
                            ));
                        }
                        PaginationInitializer::Bound { value, span }
                    }
                    SourcePaginationInitializer::Value(value) => {
                        let value = match value {
                            SourcePaginationValue::Reference { namespace, name } => {
                                PaginationValue::Reference(self.resolve_value_reference(
                                    &namespace,
                                    &name,
                                    scopes,
                                    Some(endpoint),
                                )?)
                            }
                            SourcePaginationValue::Stable(value) => {
                                PaginationValue::Stable(resolve_default(value))
                            }
                        };
                        PaginationInitializer::Value(value)
                    }
                };
                Ok(PaginationField {
                    name: field.name,
                    initializer,
                })
            })
            .collect::<syn::Result<Vec<_>>>()?;
        Ok(Some(Pagination { controller, fields }))
    }
}

pub(super) fn validate_pagination_binding_destinations(
    pagination_values: &[PaginationBoundValue],
) -> syn::Result<()> {
    if let Some(value) = pagination_values
        .iter()
        .find(|value| matches!(value.reference, ValueReference::Client(_)))
    {
        return Err(value
            .span
            .error("`bind` accepts only direct `ep.FIELD` or `scope.FIELD` destinations"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::model::{ResolvedValue, ValueReference};
    use super::super::test_support;
    use super::*;

    #[test]
    fn resolves_controller_fields_to_owned_normalized_references() {
        let api = test_support::resolve_tokens(quote! {
            client PaginationApi {
                base "https://example.test";
                vars { tenant: String; }
            }
            GET list(cursor: Option<String>) -> PageSpec<Vec<u32>> {
                paginate crate::Controller {
                    cursor: bind ep.cursor,
                    tenant: vars.tenant,
                    attempts: 0,
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "list");
        let pagination = endpoint.pagination.as_ref().expect("pagination");

        assert_eq!(pagination.fields.len(), 3);
        assert!(matches!(
            pagination.fields[0].initializer,
            PaginationInitializer::Bound {
                value: ResolvedValue {
                    reference: ValueReference::Endpoint(0),
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            pagination.fields[1].initializer,
            PaginationInitializer::Value(PaginationValue::Reference(ResolvedValue {
                reference: ValueReference::Client(0),
                ..
            }))
        ));
        assert!(matches!(
            pagination.fields[2].initializer,
            PaginationInitializer::Value(PaginationValue::Stable(_))
        ));
    }

    #[test]
    fn client_binding_without_a_rate_dependency_remains_rejected() {
        let error = test_support::try_resolve_tokens(quote! {
            client PaginationApi {
                base "https://example.test";
                vars { cursor: String; }
            }
            GET list() -> PageSpec<Vec<u32>> {
                paginate crate::Controller {
                    cursor: bind vars.cursor,
                }
            }
        })
        .err()
        .expect("client pagination bindings remain unsupported")
        .to_string();

        assert!(
            error.contains("`bind` accepts only direct `ep.FIELD` or `scope.FIELD` destinations")
        );
    }
}
