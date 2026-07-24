use super::*;

pub(super) fn generate_client_child_methods(
    client: &Client,
    core_path: &TokenStream,
) -> TokenStream {
    let methods = client
        .children
        .iter()
        .map(|child| match child {
            Child::Scope(scope) => generate_scope_entry(client, &[], scope, true, core_path),
            Child::Endpoint(endpoint) => {
                generate_endpoint_method(client, &[], endpoint, true, core_path)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#methods)*)
}

pub(super) fn generate_scope_tree(
    client: &Client,
    children: &[Child],
    ancestors: &[&Scope],
    core_path: &TokenStream,
    capabilities: Capabilities,
) -> TokenStream {
    let items = children
        .iter()
        .map(|child| match child {
            Child::Scope(scope) => {
                generate_scope_module(client, scope, ancestors, core_path, capabilities)
            }
            Child::Endpoint(endpoint) => {
                generate_endpoint(client, ancestors, endpoint, core_path, capabilities)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#items)*)
}

pub(super) fn generate_scope_module(
    client: &Client,
    scope: &Scope,
    ancestors: &[&Scope],
    core_path: &TokenStream,
    capabilities: Capabilities,
) -> TokenStream {
    let mut visible = ancestors.to_vec();
    visible.push(scope);
    let module_name = &scope.surface.module_name;
    let facade_name = &scope.surface.facade_name;
    let depth = visible.len();
    let invocation_path = super_chain(depth + 1);
    let support_path = super_chain(depth);
    let client_name = &client.name;
    let scope_values = value_category_summary(&scope.parameters, "local scope");
    let facade_doc = format!(
        "Immutable facade snapshot for the `{}` scope. {scope_values} Optional and defaulted local values use consuming setters; defaults are retained for reset without re-evaluation.",
        scope.name
    );

    let fields = facade_scope_fields(&visible);
    let constructor_parameters =
        facade_constructor_parameters(&visible, &invocation_path, client_name);
    let constructor_fields = facade_constructor_fields(&visible);
    let setters = scope
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_required())
        .map(|(index, value)| generate_facade_setters(scope.id, index, value))
        .collect::<Vec<_>>();
    let child_methods = scope
        .children
        .iter()
        .map(|child| match child {
            Child::Scope(child_scope) => {
                generate_scope_entry(client, &visible, child_scope, false, core_path)
            }
            Child::Endpoint(endpoint) => {
                generate_endpoint_method(client, &visible, endpoint, false, core_path)
            }
        })
        .collect::<Vec<_>>();
    let nested = generate_scope_tree(client, &scope.children, &visible, core_path, capabilities);

    quote! {
        pub mod #module_name {
            #[allow(unused_imports)]
            use super::*;

            #[doc = #facade_doc]
            #[derive(Clone)]
            pub struct #facade_name {
                __concord_client: #invocation_path::#client_name,
                #fields
            }

            impl #facade_name {
                #[doc(hidden)]
                pub(in #support_path) fn __concord_new(
                    #constructor_parameters
                ) -> Self {
                    Self {
                        #constructor_fields
                    }
                }

                #(#setters)*
                #(#child_methods)*
            }

            #nested
        }
    }
}

pub(super) fn generate_scope_entry(
    client: &Client,
    ancestors: &[&Scope],
    scope: &Scope,
    on_client: bool,
    _core_path: &TokenStream,
) -> TokenStream {
    let method = &scope.surface.method_name;
    let facade = &scope.surface.facade_name;
    let module = &scope.surface.module_name;
    let required = scope
        .parameters
        .iter()
        .filter(|value| value.is_required())
        .collect::<Vec<_>>();
    let parameters = required
        .iter()
        .map(|value| {
            let name = &value.name;
            let ty = &value.ty;
            quote!(#name: #ty,)
        })
        .collect::<Vec<_>>();
    let mut visible = ancestors.to_vec();
    visible.push(scope);
    let init = initialize_scope_values(scope, &required);
    let local_arguments = scope_constructor_local_arguments(scope);
    let existing_arguments = if on_client {
        quote!()
    } else {
        let arguments = facade_scope_argument_clones(ancestors);
        quote!(#arguments)
    };
    let client_argument = if on_client {
        quote!(self.clone(),)
    } else {
        quote!(self.__concord_client.clone(),)
    };
    let return_path = quote!(#module::#facade);
    let constructor_path = return_path.clone();
    let doc = format!(
        "Creates an immutable `{}` facade snapshot. {} Selected scope defaults are evaluated once here.",
        scope.name,
        value_category_summary(&scope.parameters, "local scope")
    );
    let _ = client;
    quote! {
        #[must_use]
        #[doc = #doc]
        pub fn #method(&self, #(#parameters)*) -> #return_path {
            #init
            #constructor_path::__concord_new(
                #client_argument
                #existing_arguments
                #local_arguments
            )
        }
    }
}

pub(super) fn initialize_scope_values(scope: &Scope, required: &[&Value]) -> TokenStream {
    let required_names = required
        .iter()
        .map(|value| (canonical_ident(&value.name), &value.name))
        .collect::<HashMap<_, _>>();
    let statements = scope
        .parameters
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = scope_value_field(scope.id, index, value);
            let ty = &value.ty;
            if value.is_required() {
                let name = required_names
                    .get(&canonical_ident(&value.name))
                    .expect("required scope binding");
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#name);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let default_field = scope_default_field(scope.id, index, value);
                let default = default_expr(value.default().expect("defaulted scope value"));
                quote! {
                    let #default_field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#default_field);
                }
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#statements)*)
}

pub(super) fn facade_scope_fields(scopes: &[&Scope]) -> TokenStream {
    let fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let ty = &value.ty;
                    let mut values = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default: ::std::sync::Arc<#ty>,));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

pub(super) fn facade_constructor_parameters(
    scopes: &[&Scope],
    invocation_path: &TokenStream,
    client_name: &Ident,
) -> TokenStream {
    let fields = facade_scope_fields_as_parameters(scopes);
    quote! {
        __concord_client: #invocation_path::#client_name,
        #fields
    }
}

pub(super) fn facade_scope_fields_as_parameters(scopes: &[&Scope]) -> TokenStream {
    let parameters = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let ty = &value.ty;
                    let mut values = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default: ::std::sync::Arc<#ty>,));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#parameters)*)
}

pub(super) fn facade_constructor_fields(scopes: &[&Scope]) -> TokenStream {
    let fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let mut values = vec![quote!(#field,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(#default,));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote! {
        __concord_client,
        #(#fields)*
    }
}

pub(super) fn facade_scope_argument_clones(scopes: &[&Scope]) -> TokenStream {
    let values = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let mut values = vec![quote!(::std::sync::Arc::clone(&self.#field),)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        values.push(quote!(::std::sync::Arc::clone(&self.#default),));
                    }
                    values
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#values)*)
}

pub(super) fn scope_constructor_local_arguments(scope: &Scope) -> TokenStream {
    let values = scope
        .parameters
        .iter()
        .enumerate()
        .flat_map(|(index, value)| {
            let field = scope_value_field(scope.id, index, value);
            let mut values = vec![quote!(#field,)];
            if value.default().is_some() {
                let default = scope_default_field(scope.id, index, value);
                values.push(quote!(#default,));
            }
            values
        })
        .collect::<Vec<_>>();
    quote!(#(#values)*)
}

pub(super) fn generate_facade_setters(scope_id: usize, index: usize, value: &Value) -> TokenStream {
    let method = &value.name;
    let field = scope_value_field(scope_id, index, value);
    let setter_type = value.setter_type();
    if value.is_optional() {
        let clear = helper_ident("clear", method);
        let setter_doc = format!(
            "Returns a new facade selecting `Some(value)` for optional scope value `{}`.",
            canonical_ident(&value.name)
        );
        let clear_doc = format!(
            "Returns a new facade selecting `None` for optional scope value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #setter_type) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::Some(value));
                self
            }

            #[doc = #clear_doc]
            #[must_use]
            pub fn #clear(mut self) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::None);
                self
            }
        }
    } else {
        let reset = helper_ident("reset", method);
        let default = scope_default_field(scope_id, index, value);
        let setter_doc = format!(
            "Returns a new facade overriding defaulted scope value `{}`.",
            canonical_ident(&value.name)
        );
        let reset_doc = format!(
            "Returns a new facade restoring the retained `{}` default without re-evaluation.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #setter_type) -> Self {
                self.#field = ::std::sync::Arc::new(value);
                self
            }

            #[doc = #reset_doc]
            #[must_use]
            pub fn #reset(mut self) -> Self {
                self.#field = ::std::sync::Arc::clone(&self.#default);
                self
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;

    #[test]
    fn emits_scope_module_facade_and_child_method() {
        let api = test_support::api();
        assert_eq!(test_support::scope(&api).name, "resources");
        let output = generate_scope_tree(
            &api.client,
            &api.client.children,
            &[],
            &test_support::core_path(),
            Capabilities::default(),
        )
        .to_string();

        assert!(output.contains("pub mod resources"));
        assert!(output.contains("pub struct Resources"));
        assert!(output.contains("pub fn list"));
    }
}
