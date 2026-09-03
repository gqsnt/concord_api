use super::*;

pub(super) fn generate_client_child_methods(
    client: &Client,
    facade_path: &TokenStream,
) -> TokenStream {
    let methods = client
        .children
        .iter()
        .map(|child| match child {
            Child::Scope(scope) => generate_scope_entry(client, &[], scope, true, facade_path),
            Child::Endpoint(endpoint) => generate_endpoint_method(client, &[], endpoint, true),
        })
        .collect::<Vec<_>>();
    quote!(#(#methods)*)
}

pub(super) fn generate_scope_tree(
    client: &Client,
    children: &[Child],
    ancestors: &[&Scope],
    facade_path: &TokenStream,
) -> TokenStream {
    let items = children
        .iter()
        .map(|child| match child {
            Child::Scope(scope) => generate_scope_module(client, scope, ancestors, facade_path),
            Child::Endpoint(endpoint) => {
                generate_endpoint(client, ancestors, endpoint, facade_path)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#items)*)
}

pub(super) fn generate_scope_module(
    client: &Client,
    scope: &Scope,
    ancestors: &[&Scope],
    facade_path: &TokenStream,
) -> TokenStream {
    let mut visible = ancestors.to_vec();
    visible.push(scope);
    let module_name = &scope.surface.module_name;
    let facade_name = &scope.surface.facade_name;
    let depth = visible.len();
    let invocation_path = super_chain(depth + 1);
    let support_path = super_chain(depth);
    let client_name = &client.name;
    let scope_values = value_category_summary(&scope.parameters, "Scope");
    let facade_doc = format!(
        "Immutable `{}` scope selection. {scope_values} Optional and defaulted inputs use consuming `set_*` controls. Optional values also expose `with_*` and `clear_*`; defaulted values expose `reset_*`, which restores the retained default without re-evaluation. A `#[default]` declaration invokes application-owned `Default::default()` when the scope snapshot is constructed.",
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
                generate_scope_entry(client, &visible, child_scope, false, facade_path)
            }
            Child::Endpoint(endpoint) => {
                generate_endpoint_method(client, &visible, endpoint, false)
            }
        })
        .collect::<Vec<_>>();
    let nested = generate_scope_tree(client, &scope.children, &visible, facade_path);
    let unbound = generate_scope_unbound(client, scope, ancestors);

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

            #unbound
            #nested
        }
    }
}

pub(super) fn generate_scope_entry(
    client: &Client,
    ancestors: &[&Scope],
    scope: &Scope,
    on_client: bool,
    _facade_path: &TokenStream,
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
            let ty = &value.effective_type;
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
        "Creates an immutable `{}` scope selection. {} Scope defaults are evaluated once here.",
        scope.name,
        value_category_summary(&scope.parameters, "Scope")
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
            let ty = &value.effective_type;
            if value.is_required() {
                let name = required_names
                    .get(&canonical_ident(&value.name))
                    .expect("required scope binding");
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#name);)
            } else if value.has_default() {
                let default_field = scope_default_field(scope.id, index, value);
                let default = retained_default_expr(value);
                quote! {
                    let #default_field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#default_field);
                }
            } else {
                debug_assert!(value.is_optional());
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
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
                    let ty = &value.effective_type;
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
                    let ty = &value.effective_type;
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
    let setter = value_setter_ident(&value.name);
    let field = scope_value_field(scope_id, index, value);
    let setter_type = value.setter_argument_type();
    let selected_value = quote!(value);
    let with = value.accepts_absence().then(|| {
        let method = helper_ident("with", &value.name);
        let payload_type = value.with_argument_type();
        quote! {
            #[must_use]
            pub fn #method(mut self, value: #payload_type) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::Some(value));
                self
            }
        }
    });
    let setter_doc = if value.accepts_absence() && value.has_default() {
        format!(
            "Returns a new facade selecting a present value for defaulted absence-capable scope value `{}`.",
            canonical_ident(&value.name)
        )
    } else if value.accepts_absence() {
        format!(
            "Returns a new facade selecting a present value for absence-capable scope value `{}`.",
            canonical_ident(&value.name)
        )
    } else {
        format!(
            "Returns a new facade overriding defaulted scope value `{}`.",
            canonical_ident(&value.name)
        )
    };
    let clear = value.accepts_absence().then(|| {
        let clear = helper_ident("clear", &value.name);
        let clear_doc = format!(
            "Returns a new facade explicitly selecting absence for absence-capable scope value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #clear_doc]
            #[must_use]
            pub fn #clear(mut self) -> Self {
                self.#field = ::std::sync::Arc::new(::std::option::Option::None);
                self
            }
        }
    });
    let reset = value.has_default().then(|| {
        let reset = helper_ident("reset", &value.name);
        let default = scope_default_field(scope_id, index, value);
        let reset_doc = format!(
            "Returns a new facade restoring the retained `{}` default without re-evaluation.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #reset_doc]
            #[must_use]
            pub fn #reset(mut self) -> Self {
                self.#field = ::std::sync::Arc::clone(&self.#default);
                self
            }
        }
    });
    quote! {
        #[doc = #setter_doc]
        #[must_use]
        pub fn #setter(mut self, value: #setter_type) -> Self {
            self.#field = ::std::sync::Arc::new(#selected_value);
            self
        }

        #with
        #clear
        #reset
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
            &test_support::facade_path(),
        )
        .to_string();

        assert!(output.contains("pub mod resources"));
        assert!(output.contains("pub struct Resources"));
        assert!(output.contains("pub fn list"));
    }
}
