use std::collections::HashSet;

use super::model::{Pagination, PaginationField, PaginationInitializer, PaginationValue, Value};
use super::symbols::ScopeFrame;
use super::{Resolver, SpanError};
use crate::syntax::ast::{
    PaginationDecl, PaginationInitializer as SourcePaginationInitializer,
    PaginationValue as SourcePaginationValue,
};

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
                        PaginationInitializer::Bound(value)
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
                            SourcePaginationValue::Stable(value) => PaginationValue::Stable(value),
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
            PaginationInitializer::Bound(ResolvedValue {
                reference: ValueReference::Endpoint(0),
                ..
            })
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
}
