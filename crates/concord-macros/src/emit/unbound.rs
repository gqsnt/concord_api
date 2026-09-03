use super::*;

const UNBOUND_TYPE: &str = "Unbound";

pub(super) fn generate_unbound_tree(client: &Client) -> TokenStream {
    let root_type = Ident::new(UNBOUND_TYPE, Span::mixed_site());
    let methods = unbound_child_methods(client, &client.children, &[]);
    quote! {
        /// Inert root of the declaration-shaped unbound endpoint tree.
        #[derive(Clone, Copy, Default)]
        pub struct #root_type;

        impl #root_type {
            #(#methods)*
        }
    }
}

pub(super) fn generate_scope_unbound(
    client: &Client,
    scope: &Scope,
    ancestors: &[&Scope],
) -> TokenStream {
    let unbound_type = Ident::new(UNBOUND_TYPE, Span::mixed_site());
    let mut visible = ancestors.to_vec();
    visible.push(scope);
    let fields = facade_scope_fields(&visible);
    let constructor_parameters = facade_scope_fields_as_parameters(&visible);
    let constructor_fields = unbound_scope_constructor_fields(&visible);
    let super_scope_path = super_chain(visible.len());
    let setters = scope
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_required())
        .map(|(index, value)| generate_facade_setters(scope.id, index, value))
        .collect::<Vec<_>>();
    let child_methods = unbound_child_methods(client, &scope.children, &visible);

    quote! {
        /// Inert unbound selection of this lexical scope.
        #[derive(Clone)]
        pub struct #unbound_type {
            #fields
        }

        impl #unbound_type {
            #[doc(hidden)]
            pub(in #super_scope_path) fn __concord_new(
                #constructor_parameters
            ) -> Self {
                Self {
                    #constructor_fields
                }
            }

            #(#setters)*
            #(#child_methods)*
        }
    }
}

fn unbound_child_methods(
    client: &Client,
    children: &[Child],
    ancestors: &[&Scope],
) -> Vec<TokenStream> {
    children
        .iter()
        .map(|child| match child {
            Child::Scope(scope) => unbound_scope_entry(client, ancestors, scope),
            Child::Endpoint(endpoint) => unbound_endpoint_entry(ancestors, endpoint),
        })
        .collect()
}

fn unbound_scope_entry(_client: &Client, ancestors: &[&Scope], scope: &Scope) -> TokenStream {
    let method = &scope.surface.method_name;
    let module = &scope.surface.module_name;
    let unbound_type = Ident::new(UNBOUND_TYPE, Span::mixed_site());
    let required = scope
        .parameters
        .iter()
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let parameters = required.iter().map(|value| {
        let name = &value.name;
        let ty = &value.effective_type;
        quote!(#name: #ty,)
    });
    let init = initialize_scope_values(scope, &required);
    let existing = if ancestors.is_empty() {
        quote!()
    } else {
        facade_scope_argument_clones(ancestors)
    };
    let local = scope_constructor_local_arguments(scope);
    quote! {
        /// Selects this scope without a client runtime. Defaults are evaluated
        /// exactly once in the returned immutable snapshot.
        #[must_use]
        pub fn #method(&self, #(#parameters)*) -> #module::#unbound_type {
            #init
            #module::#unbound_type::__concord_new(
                #existing
                #local
            )
        }
    }
}

fn unbound_endpoint_entry(ancestors: &[&Scope], endpoint: &Endpoint) -> TokenStream {
    let method = &endpoint.surface.method_name;
    let endpoint_type = &endpoint.surface.endpoint_type_name;
    let parameters = endpoint
        .surface
        .required_arguments
        .iter()
        .map(|argument| match argument {
            RequiredArgument::Parameter(index) => {
                let value = &endpoint.parameters[*index];
                let name = &value.name;
                let ty = &value.effective_type;
                quote!(#name: #ty,)
            }
            RequiredArgument::Body => {
                let body = endpoint.body.as_ref().expect("body argument");
                let name = &body.name;
                let ty = &body.input_type;
                quote!(#name: #ty,)
            }
        })
        .collect::<Vec<_>>();
    let scope_arguments = if ancestors.is_empty() {
        quote!()
    } else {
        facade_scope_argument_clones(ancestors)
    };
    let endpoint_arguments = endpoint.surface.required_arguments.iter().map(|argument| {
        let name = match argument {
            RequiredArgument::Parameter(index) => &endpoint.parameters[*index].name,
            RequiredArgument::Body => &endpoint.body.as_ref().expect("body argument").name,
        };
        quote!(#name,)
    });
    quote! {
        /// Constructs this endpoint intent without binding transport,
        /// credentials, limiter state, gates, observers, or a client value snapshot.
        #[must_use]
        pub fn #method(&self, #(#parameters)*) -> #endpoint_type {
            #endpoint_type::__concord_from_facade(
                #scope_arguments
                #(#endpoint_arguments)*
            )
        }
    }
}

fn unbound_scope_constructor_fields(scopes: &[&Scope]) -> TokenStream {
    let fields = scopes.iter().flat_map(|scope| {
        scope
            .parameters
            .iter()
            .enumerate()
            .flat_map(move |(index, value)| {
                let field = scope_value_field(scope.id, index, value);
                let mut fields = vec![quote!(#field,)];
                if value.default().is_some() {
                    let default = scope_default_field(scope.id, index, value);
                    fields.push(quote!(#default,));
                }
                fields
            })
    });
    quote!(#(#fields)*)
}
