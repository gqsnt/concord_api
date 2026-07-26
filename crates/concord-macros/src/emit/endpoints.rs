use super::*;

pub(super) fn generate_endpoint_method(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    on_client: bool,
    core_path: &TokenStream,
) -> TokenStream {
    let method = &endpoint.surface.method_name;
    let endpoint_type = &endpoint.surface.endpoint_type_name;
    let request_type = &endpoint.surface.request_type_name;
    let invocation_path = if on_client {
        quote!(super)
    } else {
        super_chain(scopes.len() + 1)
    };
    let specification_module = specification_module_name(client);
    let body_type = endpoint.body.as_ref().map(|_| {
        let body_specification = body_specification_name(endpoint);
        quote!(
            <#invocation_path::#specification_module::#body_specification
                as #core_path::body::RequestBodySpec>::Input
        )
    });
    let parameters = endpoint
        .surface
        .required_arguments
        .iter()
        .map(|argument| match argument {
            RequiredArgument::Parameter(index) => {
                let value = &endpoint.parameters[*index];
                let name = &value.name;
                let ty = &value.ty;
                quote!(#name: #ty,)
            }
            RequiredArgument::Body => {
                let name = &endpoint.body.as_ref().expect("body argument").name;
                let ty = body_type.as_ref().expect("body input type");
                quote!(#name: #ty,)
            }
        })
        .collect::<Vec<_>>();
    let scope_arguments = if on_client {
        quote!()
    } else {
        facade_scope_argument_clones(scopes)
    };
    let endpoint_arguments = endpoint
        .surface
        .required_arguments
        .iter()
        .map(|argument| {
            let name = match argument {
                RequiredArgument::Parameter(index) => &endpoint.parameters[*index].name,
                RequiredArgument::Body => &endpoint.body.as_ref().expect("body argument").name,
            };
            quote!(#name,)
        })
        .collect::<Vec<_>>();
    let client_expression = if on_client {
        quote!(self)
    } else {
        quote!(&self.__concord_client)
    };
    let endpoint_doc = endpoint_method_doc(endpoint);
    quote! {
        #[must_use]
        #[doc = #endpoint_doc]
        pub fn #method(
            &self,
            #(#parameters)*
        ) -> #request_type {
            let endpoint = #endpoint_type::__concord_from_facade(
                #scope_arguments
                #(#endpoint_arguments)*
            );
            (#client_expression).request(endpoint)
        }
    }
}

pub(super) fn generate_endpoint(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    core_path: &TokenStream,
) -> TokenStream {
    let endpoint_name = &endpoint.surface.endpoint_type_name;
    let request_name = &endpoint.surface.request_type_name;
    let client_name = &client.name;
    let values_type = &client.surface.values_type_name;
    let credentials_store = credentials_store_name(client);
    let depth = scopes.len();
    let support_path = module_ancestor_path(depth);
    let invocation_path = super_chain(depth + 1);
    let generated_core = quote!(#support_path::__concord_generated::__core);
    let specification_module = specification_module_name(client);
    let response_specification = response_specification_name(endpoint);
    let response = quote!(#invocation_path::#specification_module::#response_specification);
    let (description_static, description_declaration) =
        generate_endpoint_description(endpoint, &response, core_path);
    let endpoint_doc = endpoint_type_doc(endpoint);
    let constructor_doc = endpoint_constructor_doc(endpoint);

    let scope_fields = endpoint_scope_fields(scopes);
    let endpoint_fields = endpoint_value_fields(endpoint);
    let body_type = endpoint.body.as_ref().map_or_else(
        || quote!(#core_path::__private::EmptyBody),
        |_| {
            let body_specification = body_specification_name(endpoint);
            quote!(#invocation_path::#specification_module::#body_specification)
        },
    );
    let body_input = request_body_input_type(&body_type, core_path);
    let body_storage = Ident::new("BodyInput", Span::mixed_site());
    let (struct_generics, body_field) = match endpoint.surface.clone_eligibility {
        CloneEligibility::Always => (quote!(), quote!(__concord_body: #body_input,)),
        CloneEligibility::BodyInputBound => (
            quote!(<#body_storage = #body_input>),
            quote!(__concord_body: #body_storage,),
        ),
    };
    let client_values_field = quote!(
        __concord_client_values: ::std::option::Option<::std::sync::Arc<#invocation_path::#values_type>>,
    );
    let credentials_field = quote!(
        __concord_credentials: ::std::option::Option<::std::sync::Arc<#invocation_path::#credentials_store>>,
    );

    let (scope_bindings, endpoint_bindings, body_binding) =
        explicit_parameter_bindings(scopes, endpoint);
    let explicit_parameters = explicit_constructor_parameters(
        scopes,
        endpoint,
        &scope_bindings,
        &endpoint_bindings,
        body_binding.as_ref(),
        &body_type,
        core_path,
    );
    let explicit_initialization = initialize_explicit_endpoint(
        scopes,
        endpoint,
        &scope_bindings,
        &endpoint_bindings,
        body_binding.as_ref(),
        &generated_core,
        core_path,
    );
    let facade_parameters =
        endpoint_facade_constructor_parameters(scopes, endpoint, &body_type, core_path);
    let facade_initialization =
        initialize_facade_endpoint(scopes, endpoint, &generated_core, core_path);
    let inherent_setters = generate_explicit_endpoint_setters(scopes, endpoint);
    let clone_fields = endpoint_clone_field_initializers(scopes, endpoint);
    let clone_attribute = matches!(endpoint.surface.clone_eligibility, CloneEligibility::Always)
        .then(|| quote!(#[derive(Clone)]));
    let clone_impl = match endpoint.surface.clone_eligibility {
        CloneEligibility::Always => quote!(),
        CloneEligibility::BodyInputBound => quote! {
            impl<#body_storage> ::std::clone::Clone for #endpoint_name<#body_storage>
            where
                #body_storage: ::std::clone::Clone,
            {
                fn clone(&self) -> Self {
                    Self {
                        #clone_fields
                    }
                }
            }
        },
    };
    let setter_impl = match endpoint.surface.clone_eligibility {
        CloneEligibility::Always => quote! {
            impl #endpoint_name {
                #inherent_setters
            }
        },
        CloneEligibility::BodyInputBound => quote! {
            impl<#body_storage> #endpoint_name<#body_storage> {
                #inherent_setters
            }
        },
    };
    let prepare_head_impl = match endpoint.surface.clone_eligibility {
        CloneEligibility::Always => quote!(impl #endpoint_name),
        CloneEligibility::BodyInputBound => {
            quote!(impl<#body_storage> #endpoint_name<#body_storage>)
        }
    };

    let default_impl = (scopes
        .iter()
        .flat_map(|scope| scope.parameters.iter())
        .all(|value| !value.is_required())
        && endpoint.parameters.iter().all(|value| !value.is_required())
        && endpoint.body.is_none())
    .then(|| {
        quote! {
            impl ::std::default::Default for #endpoint_name {
                fn default() -> Self {
                    Self::new()
                }
            }
        }
    });

    let host_statements = host_statements(client, scopes, endpoint);
    let path_statements = path_statements(client, scopes, endpoint);
    let query_statements = query_statements(client, scopes, endpoint);
    let header_statements = header_statements(client, scopes, endpoint);
    let auth_statements = auth_statements(client, endpoint, core_path);
    let rate_limit_statements =
        rate_limit_statements(client, scopes, endpoint, core_path, &invocation_path);
    let surface_where_clause =
        endpoint_where_clause(client, scopes, endpoint, core_path, &invocation_path, false);
    let where_clause =
        endpoint_where_clause(client, scopes, endpoint, core_path, &invocation_path, true);
    let request_wrapper = generate_request_wrapper(
        client,
        endpoint,
        RequestBridge {
            support_path: &support_path,
            generated_core: &generated_core,
        },
        core_path,
        &where_clause,
    );
    let pagination_impl = generate_pagination_impl(
        client,
        scopes,
        endpoint,
        &generated_core,
        core_path,
        &invocation_path,
        &where_clause,
    );
    quote! {
        #description_declaration

        #[doc = #endpoint_doc]
        #clone_attribute
        pub struct #endpoint_name #struct_generics {
            #client_values_field
            #credentials_field
            #scope_fields
            #endpoint_fields
            #body_field
        }

        #clone_impl

        impl #endpoint_name {
            #[must_use]
            #[doc = #constructor_doc]
            pub fn new(#explicit_parameters) -> Self {
                #explicit_initialization
            }

            /// Returns this endpoint's safe static contract description.
            #[must_use]
            pub fn description(
                &self,
            ) -> &'static #core_path::endpoint::EndpointDescription {
                ::std::sync::LazyLock::force(&#description_static)
            }

            #[doc(hidden)]
            pub(in #support_path) fn __concord_from_facade(#facade_parameters) -> Self {
                #facade_initialization
            }
        }

        #setter_impl
        #default_impl

        #[doc(hidden)]
        impl #support_path::__concord_endpoint_seal::Sealed for #endpoint_name {}

        #[doc(hidden)]
        #prepare_head_impl
        #surface_where_clause
        {
            fn __concord_prepare_head(
                &self,
                __concord_client_values:
                    &::std::sync::Arc<#invocation_path::#values_type>,
                __concord_credentials:
                    &::std::sync::Arc<#invocation_path::#credentials_store>,
                mut __concord_builder: #generated_core::__private::HeadBuilder,
            ) -> ::std::result::Result<
                #generated_core::__private::PreparedHead,
                #generated_core::__private::ApiError,
            > {
                #host_statements
                #path_statements
                #query_statements
                #header_statements
                #auth_statements
                #rate_limit_statements
                __concord_builder.finish()
            }
        }

        impl #support_path::Endpoint for #endpoint_name
        #surface_where_clause
        {
            type Request = #request_name;
            #[doc(hidden)]
            type Response = #response;

            fn description(
                &self,
            ) -> &#core_path::endpoint::EndpointDescription {
                ::std::sync::LazyLock::force(&#description_static)
            }

            #[doc(hidden)]
            fn __concord_bind(
                mut self,
                client: &#invocation_path::#client_name,
            ) -> Self::Request {
                self.__concord_client_values = ::std::option::Option::Some(
                    ::std::sync::Arc::clone(&client.__concord_values),
                );
                self.__concord_credentials = ::std::option::Option::Some(
                    ::std::sync::Arc::clone(&client.__concord_credentials),
                );
                #request_name {
                    __concord_inner:
                        #generated_core::__private::request(&client.__concord_core, self),
                }
            }

            #[doc(hidden)]
            fn __concord_plan(
                &self,
                client: &#invocation_path::#client_name,
            ) -> ::std::result::Result<
                #core_path::endpoint::SafeRequestPlan,
                #core_path::ApiError,
            > {
                #generated_core::__private::safe_request_plan(
                    &client.__concord_core,
                    ::std::sync::LazyLock::force(&#description_static),
                    <#response as #core_path::response::ResponseBodySpec>::accept(),
                    |builder| self.__concord_prepare_head(
                        &client.__concord_values,
                        &client.__concord_credentials,
                        builder,
                    ),
                )
            }
        }

        #[doc(hidden)]
        impl #generated_core::__private::EndpointSpec for #endpoint_name
        #where_clause
        {
            type Body = #body_type;
            type Response = #response;

            fn request_meta(&self) -> #generated_core::__private::RequestMeta {
                #generated_core::__private::RequestMeta::from_description(
                    ::std::sync::LazyLock::force(&#description_static),
                )
            }

            fn prepare_head(
                &self,
                __concord_builder: #generated_core::__private::HeadBuilder,
            ) -> ::std::result::Result<
                #generated_core::__private::PreparedHead,
                #generated_core::__private::ApiError,
            > {
                let __concord_client_values = self
                    .__concord_client_values
                    .as_ref()
                    .expect("generated endpoint is bound by the sealed client request path");
                let __concord_credentials = self
                    .__concord_credentials
                    .as_ref()
                    .expect("generated endpoint is bound by the sealed client request path");
                self.__concord_prepare_head(
                    __concord_client_values,
                    __concord_credentials,
                    __concord_builder,
                )
            }

            fn body_metadata(
                &self,
                __concord_context: #generated_core::__private::RequestBodyContext<'_>,
            ) -> ::std::result::Result<
                #generated_core::__private::RequestBodyMetadata,
                #generated_core::body::BodyError,
            > {
                <Self::Body as #generated_core::__private::RequestBodySpec>::metadata(
                    &self.__concord_body,
                    __concord_context,
                )
            }

            fn into_body(
                self,
            ) -> <Self::Body as #generated_core::__private::RequestBodySpec>::Input {
                self.__concord_body
            }
        }

        #[doc(hidden)]
        impl #generated_core::__private::RawResponseEndpoint for #endpoint_name
        #where_clause
        {}

        #pagination_impl
        #request_wrapper
    }
}

pub(super) fn endpoint_scope_fields(scopes: &[&Scope]) -> TokenStream {
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
                    let mut fields = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        fields.push(quote!(#default: ::std::sync::Arc<#ty>,));
                    }
                    fields
                })
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

pub(super) fn endpoint_value_fields(endpoint: &Endpoint) -> TokenStream {
    let fields = endpoint
        .parameters
        .iter()
        .enumerate()
        .flat_map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let ty = &value.ty;
            let mut fields = vec![quote!(#field: ::std::sync::Arc<#ty>,)];
            if value.default().is_some() {
                let default = endpoint_default_field(index, value);
                fields.push(quote!(#default: ::std::sync::Arc<#ty>,));
            }
            fields
        })
        .collect::<Vec<_>>();
    quote!(#(#fields)*)
}

pub(super) fn generate_pagination_impl(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
    generated_core: &TokenStream,
    core_path: &TokenStream,
    invocation_path: &TokenStream,
    where_clause: &TokenStream,
) -> TokenStream {
    let Some(pagination) = &endpoint.pagination else {
        return quote!();
    };
    let endpoint_name = &endpoint.surface.endpoint_type_name;
    let specification_module = specification_module_name(client);
    let response_specification = response_specification_name(endpoint);
    let response = quote!(#invocation_path::#specification_module::#response_specification);
    let page_type = quote!(<#response as #core_path::response::ResponseBodySpec>::Output);
    let controller = &pagination.controller;
    let controller_type = pagination_controller_type(controller, invocation_path, false);
    let controller_fields = pagination
        .fields
        .iter()
        .map(|field| {
            let name = &field.name;
            let value = match &field.initializer {
                PaginationInitializer::Bound { value, .. } => {
                    pagination_owned_value_access(value.reference, client, scopes, endpoint)
                }
                PaginationInitializer::Value(PaginationValue::Reference(value)) => {
                    pagination_owned_value_access(value.reference, client, scopes, endpoint)
                }
                PaginationInitializer::Value(PaginationValue::Stable(value)) => default_expr(value),
            };
            quote!(#name: #value,)
        })
        .collect::<Vec<_>>();
    let controller_constructor =
        pagination_controller_constructor(controller, invocation_path, &controller_fields);
    let binding_statements = pagination
        .fields
        .iter()
        .filter_map(|field| {
            let PaginationInitializer::Bound { value, .. } = &field.initializer else {
                return None;
            };
            let controller_field = &field.name;
            let endpoint_field = match value.reference {
                ValueReference::Client(_) => {
                    unreachable!("semantic validation rejects client pagination bindings")
                }
                ValueReference::Scope {
                    scope_id,
                    parameter,
                } => {
                    let scope = scopes
                        .iter()
                        .find(|scope| scope.id == scope_id)
                        .expect("resolved scope identity appears in the endpoint lexical chain");
                    scope_value_field(scope_id, parameter, &scope.parameters[parameter])
                }
                ValueReference::Endpoint(index) => {
                    endpoint_value_field(index, &endpoint.parameters[index])
                }
            };
            Some(quote! {
                __concord_endpoint.#endpoint_field = ::std::sync::Arc::new(
                    ::std::clone::Clone::clone(&__concord_controller.#controller_field),
                );
            })
        })
        .collect::<Vec<_>>();

    quote! {
        #[doc(hidden)]
        impl #generated_core::__private::PaginatedEndpoint for #endpoint_name
        #where_clause
        {
            type Item = <#page_type as ::std::iter::IntoIterator>::Item;
            type Controller = #controller_type;

            fn initial_controller(
                &self,
            ) -> ::std::result::Result<
                #controller_type,
                #generated_core::pagination::PaginationError,
            > {
                let __concord_endpoint = self;
                let __concord_client_values = __concord_endpoint
                    .__concord_client_values
                    .as_ref()
                    .expect("generated endpoint is bound by the sealed client request path");
                let _ = __concord_client_values;
                ::std::result::Result::Ok(#controller_constructor)
            }

            fn bind_controller(
                __concord_endpoint: &mut Self,
                __concord_controller: &#controller_type,
            ) -> ::std::result::Result<
                (),
                #generated_core::pagination::PaginationError,
            > {
                #(#binding_statements)*
                ::std::result::Result::Ok(())
            }
        }
    }
}

pub(super) fn pagination_controller_constructor(
    controller: &Type,
    invocation_path: &TokenStream,
    fields: &[TokenStream],
) -> TokenStream {
    let constructor = pagination_controller_type(controller, invocation_path, true);
    quote!(#constructor { #(#fields)* })
}

pub(super) fn pagination_controller_type(
    controller: &Type,
    invocation_path: &TokenStream,
    expression_position: bool,
) -> TokenStream {
    let mut controller = controller.clone();
    let unqualified = matches!(
        &controller,
        Type::Path(type_path)
            if type_path.qself.is_none()
                && type_path.path.leading_colon.is_none()
                && type_path.path.segments.len() == 1
    );
    if expression_position {
        if let Type::Path(type_path) = &mut controller {
            if let Some(segment) = type_path.path.segments.last_mut() {
                if let PathArguments::AngleBracketed(arguments) = &mut segment.arguments {
                    arguments.colon2_token.get_or_insert_with(Default::default);
                }
            }
        }
    }
    if unqualified {
        quote!(#invocation_path::#controller)
    } else {
        quote!(#controller)
    }
}

pub(super) fn explicit_constructor_parameters(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    scope_bindings: &[Ident],
    endpoint_bindings: &[Ident],
    body_binding: Option<&Ident>,
    body_specification: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    let mut scope_index = 0;
    let scope_parameters = scopes
        .iter()
        .flat_map(|scope| scope.parameters.iter())
        .filter(|value| value.is_required())
        .map(|value| {
            let binding = &scope_bindings[scope_index];
            scope_index += 1;
            let ty = &value.ty;
            quote!(#binding: #ty,)
        })
        .collect::<Vec<_>>();
    let endpoint_parameters = endpoint
        .surface
        .required_arguments
        .iter()
        .map(|argument| match argument {
            RequiredArgument::Parameter(index) => {
                let value = &endpoint.parameters[*index];
                let binding_index = endpoint
                    .parameters
                    .iter()
                    .take(*index + 1)
                    .filter(|value| value.is_required())
                    .count()
                    - 1;
                let binding = &endpoint_bindings[binding_index];
                let ty = &value.ty;
                quote!(#binding: #ty,)
            }
            RequiredArgument::Body => {
                let binding = body_binding.expect("body binding");
                let ty = request_body_input_type(body_specification, core_path);
                quote!(#binding: #ty,)
            }
        })
        .collect::<Vec<_>>();
    quote!(#(#scope_parameters)* #(#endpoint_parameters)*)
}

pub(super) fn initialize_explicit_endpoint(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    scope_bindings: &[Ident],
    endpoint_bindings: &[Ident],
    body_binding: Option<&Ident>,
    generated_core: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    let mut scope_binding_index = 0;
    let scope_statements = scopes
        .iter()
        .flat_map(|scope| {
            scope.parameters.iter().enumerate().map(move |(index, value)| {
                (scope.id, index, value)
            })
        })
        .map(|(scope_id, index, value)| {
            let field = scope_value_field(scope_id, index, value);
            let ty = &value.ty;
            if value.is_required() {
                let binding = &scope_bindings[scope_binding_index];
                scope_binding_index += 1;
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#binding);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let retained = scope_default_field(scope_id, index, value);
                let default = default_expr(value.default().expect("default"));
                quote! {
                    let #retained: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#retained);
                }
            }
        })
        .collect::<Vec<_>>();
    let endpoint_statements = endpoint
        .parameters
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let ty = &value.ty;
            if value.is_required() {
                let binding_index = endpoint
                    .parameters
                    .iter()
                    .take(index + 1)
                    .filter(|value| value.is_required())
                    .count()
                    - 1;
                let binding = &endpoint_bindings[binding_index];
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#binding);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let retained = endpoint_default_field(index, value);
                let default = default_expr(value.default().expect("default"));
                quote! {
                    let #retained: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#retained);
                }
            }
        })
        .collect::<Vec<_>>();
    let fields = endpoint_all_snapshot_field_initializers(scopes, endpoint);
    let body = body_constructor(
        endpoint.body.as_ref().map(|body| &body.ty),
        body_binding,
        generated_core,
        core_path,
    );
    quote! {
        #(#scope_statements)*
        #(#endpoint_statements)*
        Self {
            __concord_client_values: ::std::option::Option::None,
            __concord_credentials: ::std::option::Option::None,
            #fields
            __concord_body: #body,
        }
    }
}

pub(super) fn endpoint_facade_constructor_parameters(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    body_specification: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    let scope_parameters = facade_scope_fields_as_parameters(scopes);
    let endpoint_parameters = endpoint
        .surface
        .required_arguments
        .iter()
        .map(|argument| match argument {
            RequiredArgument::Parameter(index) => {
                let value = &endpoint.parameters[*index];
                let binding = endpoint_constructor_binding(*index, value);
                let ty = &value.ty;
                quote!(#binding: #ty,)
            }
            RequiredArgument::Body => {
                let ty = request_body_input_type(body_specification, core_path);
                quote!(__concord_body_input: #ty,)
            }
        })
        .collect::<Vec<_>>();
    quote!(#scope_parameters #(#endpoint_parameters)*)
}

pub(super) fn initialize_facade_endpoint(
    scopes: &[&Scope],
    endpoint: &Endpoint,
    generated_core: &TokenStream,
    core_path: &TokenStream,
) -> TokenStream {
    let statements = endpoint
        .parameters
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let ty = &value.ty;
            if value.is_required() {
                let binding = endpoint_constructor_binding(index, value);
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#binding);)
            } else if value.is_optional() {
                quote!(let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(::std::option::Option::None);)
            } else {
                let retained = endpoint_default_field(index, value);
                let default = default_expr(value.default().expect("default"));
                quote! {
                    let #retained: ::std::sync::Arc<#ty> = ::std::sync::Arc::new(#default);
                    let #field: ::std::sync::Arc<#ty> = ::std::sync::Arc::clone(&#retained);
                }
            }
        })
        .collect::<Vec<_>>();
    let scope_fields = scopes
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
    let endpoint_fields = endpoint
        .parameters
        .iter()
        .enumerate()
        .flat_map(|(index, value)| {
            let field = endpoint_value_field(index, value);
            let mut values = vec![quote!(#field,)];
            if value.default().is_some() {
                let default = endpoint_default_field(index, value);
                values.push(quote!(#default,));
            }
            values
        })
        .collect::<Vec<_>>();
    let body_binding = endpoint
        .body
        .as_ref()
        .map(|_| Ident::new("__concord_body_input", Span::call_site()));
    let body = body_constructor(
        endpoint.body.as_ref().map(|body| &body.ty),
        body_binding.as_ref(),
        generated_core,
        core_path,
    );
    quote! {
        #(#statements)*
        Self {
            __concord_client_values: ::std::option::Option::None,
            __concord_credentials: ::std::option::Option::None,
            #(#scope_fields)*
            #(#endpoint_fields)*
            __concord_body: #body,
        }
    }
}

pub(super) fn endpoint_all_snapshot_field_initializers(
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let scope_fields = scopes
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
        .chain(
            endpoint
                .parameters
                .iter()
                .enumerate()
                .flat_map(|(index, value)| {
                    let field = endpoint_value_field(index, value);
                    let mut values = vec![quote!(#field,)];
                    if value.default().is_some() {
                        let default = endpoint_default_field(index, value);
                        values.push(quote!(#default,));
                    }
                    values
                }),
        )
        .collect::<Vec<_>>();
    quote!(#(#scope_fields)*)
}

pub(super) fn endpoint_clone_field_initializers(
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let scope_fields = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .flat_map(move |(index, value)| {
                    let field = scope_value_field(scope.id, index, value);
                    let mut fields = vec![quote!(#field: ::std::sync::Arc::clone(&self.#field),)];
                    if value.default().is_some() {
                        let default = scope_default_field(scope.id, index, value);
                        fields.push(quote!(#default: ::std::sync::Arc::clone(&self.#default),));
                    }
                    fields
                })
        })
        .chain(
            endpoint
                .parameters
                .iter()
                .enumerate()
                .flat_map(|(index, value)| {
                    let field = endpoint_value_field(index, value);
                    let mut fields = vec![quote!(#field: ::std::sync::Arc::clone(&self.#field),)];
                    if value.default().is_some() {
                        let default = endpoint_default_field(index, value);
                        fields.push(quote!(#default: ::std::sync::Arc::clone(&self.#default),));
                    }
                    fields
                }),
        )
        .collect::<Vec<_>>();
    quote! {
        __concord_client_values: ::std::clone::Clone::clone(&self.__concord_client_values),
        __concord_credentials: ::std::clone::Clone::clone(&self.__concord_credentials),
        #(#scope_fields)*
        __concord_body: ::std::clone::Clone::clone(&self.__concord_body),
    }
}

pub(super) fn generate_explicit_endpoint_setters(
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> TokenStream {
    let scope_setters = scopes
        .iter()
        .flat_map(|scope| {
            scope
                .parameters
                .iter()
                .enumerate()
                .filter(|(_, value)| !value.is_required())
                .map(move |(index, value)| {
                    generate_endpoint_snapshot_setters(
                        value,
                        scope_value_field(scope.id, index, value),
                        value
                            .default()
                            .map(|_| scope_default_field(scope.id, index, value)),
                    )
                })
        })
        .collect::<Vec<_>>();
    let endpoint_setters = endpoint
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_required())
        .map(|(index, value)| {
            generate_endpoint_snapshot_setters(
                value,
                endpoint_value_field(index, value),
                value
                    .default()
                    .map(|_| endpoint_default_field(index, value)),
            )
        })
        .collect::<Vec<_>>();
    quote!(#(#scope_setters)* #(#endpoint_setters)*)
}

pub(super) fn generate_endpoint_snapshot_setters(
    value: &Value,
    field: Ident,
    default: Option<Ident>,
) -> TokenStream {
    let method = &value.name;
    let ty = value.setter_type();
    if value.is_optional() {
        let clear = helper_ident("clear", method);
        let setter_doc = format!(
            "Returns a new explicit endpoint snapshot selecting `Some(value)` for optional value `{}`.",
            canonical_ident(&value.name)
        );
        let clear_doc = format!(
            "Returns a new explicit endpoint snapshot selecting `None` for optional value `{}`.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #ty) -> Self {
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
        let default = default.expect("default field");
        let setter_doc = format!(
            "Returns a new explicit endpoint snapshot overriding defaulted value `{}`.",
            canonical_ident(&value.name)
        );
        let reset_doc = format!(
            "Returns a new explicit endpoint snapshot restoring the retained `{}` default without re-evaluation.",
            canonical_ident(&value.name)
        );
        quote! {
            #[doc = #setter_doc]
            #[must_use]
            pub fn #method(mut self, value: #ty) -> Self {
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
    fn emits_explicit_endpoint_value_and_binding_contract() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let output =
            generate_endpoint(&api.client, &[], endpoint, &test_support::core_path()).to_string();
        let endpoint_name = endpoint.surface.endpoint_type_name.to_string();

        assert!(output.contains(&format!("pub struct {endpoint_name}")));
        assert!(output.contains("pub fn description"));
        assert!(output.contains("__concord_bind"));
    }
}
