use std::collections::HashSet;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::Index;

use crate::model::{
    Api, CodecKind, Endpoint, MethodKind, Parameter, PathAtom, ResponseKind, Scope, to_snake_case,
};

const GENERATED_CONTRACT_VERSION: u32 = 4;

pub(crate) fn generate(api: &Api, core_path: &TokenStream) -> TokenStream {
    let client = &api.client;
    let scope = &client.scope;
    let client_name = &client.name;
    let builder_name = format_ident!("{client_name}Builder");
    let endpoint_capability_name = format_ident!("{client_name}Endpoint");
    let scope_name = &scope.name;
    let support_name = format_ident!("{}", to_snake_case(&client_name.to_string()));
    let scope_module_name = format_ident!("{}", to_snake_case(&scope_name.to_string()));
    let scope_method_name = &scope_module_name;
    let facade_name = format_ident!("{}Facade", to_pascal_case(&scope_name.to_string()));
    let base = &client.base;
    let contract_version = GENERATED_CONTRACT_VERSION;
    let json_feature_assertion = if scope.endpoints.iter().any(endpoint_uses_json) {
        quote!(
            const _: () = __core::__private::assert_json_feature();
        )
    } else {
        quote!()
    };
    let client_doc = format!("Generated Concord client `{client_name}`.");
    let builder_doc = format!("Canonical fallible builder for [`{client_name}`].");
    let support_doc = format!("Public generated API modules for [`{client_name}`].");
    let endpoint_capability_doc = format!(
        "Sealed capability implemented only by explicit endpoints generated for [`{client_name}`]."
    );
    let facade_doc = format!("Immutable generated facade for the `{}` scope.", scope.name);
    let scope_method_doc = format!(
        "Creates an immutable `{}` scope facade without starting a request.",
        scope.name
    );

    let scope_names = scope
        .parameters
        .iter()
        .map(|parameter| &parameter.name)
        .collect::<Vec<_>>();
    let scope_types = scope
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect::<Vec<_>>();
    let endpoint_items = scope
        .endpoints
        .iter()
        .map(|endpoint| {
            generate_endpoint(
                client_name,
                scope,
                endpoint,
                core_path,
                &endpoint_capability_name,
            )
        })
        .collect::<Vec<_>>();
    let facade_methods = scope
        .endpoints
        .iter()
        .map(|endpoint| {
            generate_facade_method(client_name, scope, endpoint, core_path, &scope_module_name)
        })
        .collect::<Vec<_>>();

    quote! {
        #[doc = #client_doc]
        #[derive(Clone)]
        pub struct #client_name {
            __concord_core: #core_path::__private::ClientCore,
        }

        #[doc = #builder_doc]
        pub struct #builder_name {
            __concord_http: ::std::option::Option<#core_path::advanced::HttpConfigBuilder>,
            __concord_runtime: ::std::option::Option<#core_path::advanced::RuntimeConfigBuilder>,
            __concord_http_configured: bool,
            __concord_runtime_configured: bool,
            __concord_duplicate_http: bool,
            __concord_duplicate_runtime: bool,
        }

        #[doc = #support_doc]
        pub mod #support_name {
            mod __concord_generated {
                pub(super) use #core_path as __core;

                const _: () = __core::__private::assert_generated_contract(#contract_version);
                #json_feature_assertion
            }

            mod __concord_endpoint_seal {
                pub trait Sealed {}
            }

            #[doc(hidden)]
            #[doc = #endpoint_capability_doc]
            #[allow(private_bounds)]
            pub trait #endpoint_capability_name: __concord_endpoint_seal::Sealed {}

            /// Generated immutable scope facades.
            pub mod facades {
                #[doc = #facade_doc]
                #[derive(Clone)]
                pub struct #facade_name {
                    pub(super) __concord_client: super::super::#client_name,
                    pub(super) __concord_scope: (#(::std::sync::Arc<#scope_types>,)*),
                }
            }

            /// Generated explicit endpoint values.
            pub mod endpoints {
                pub mod #scope_module_name {
                    #(#endpoint_items)*
                }
            }

            impl super::#client_name {
                /// Builds a client with the canonical default configuration.
                pub fn new() -> ::std::result::Result<
                    Self,
                    #core_path::prelude::ClientBuildError,
                > {
                    Self::builder().build()
                }

                #[must_use]
                /// Creates the canonical generated client builder.
                pub fn builder() -> super::#builder_name {
                    super::#builder_name::new()
                }

                #[must_use]
                #[doc = #scope_method_doc]
                pub fn #scope_method_name(
                    &self,
                    #(#scope_names: #scope_types),*
                ) -> facades::#facade_name {
                    facades::#facade_name {
                        __concord_client: self.clone(),
                        __concord_scope: (#(::std::sync::Arc::new(#scope_names),)*),
                    }
                }

                #[must_use]
                /// Creates a side-effect-free pending request from an endpoint generated for this client.
                pub fn request<E>(&self, endpoint: E) -> #core_path::Request<E>
                where
                    E: #endpoint_capability_name,
                {
                    __concord_generated::__core::__private::request(
                        &self.__concord_core,
                        endpoint,
                    )
                }
            }

            impl super::#builder_name {
                #[must_use]
                /// Creates an empty canonical generated client builder.
                pub fn new() -> Self {
                    Self {
                        __concord_http: ::std::option::Option::Some(
                            __concord_generated::__core::__private::default_http_config(),
                        ),
                        __concord_runtime: ::std::option::Option::Some(
                            __concord_generated::__core::__private::default_runtime_config(),
                        ),
                        __concord_http_configured: false,
                        __concord_runtime_configured: false,
                        __concord_duplicate_http: false,
                        __concord_duplicate_runtime: false,
                    }
                }

                #[must_use]
                /// Applies the HTTP configuration closure immediately.
                pub fn http<F>(mut self, configure: F) -> Self
                where
                    F: ::std::ops::FnOnce(
                        #core_path::advanced::HttpConfigBuilder,
                    ) -> #core_path::advanced::HttpConfigBuilder,
                {
                    if self.__concord_http_configured {
                        self.__concord_duplicate_http = true;
                        return self;
                    }
                    self.__concord_http_configured = true;
                    let Some(http) = self.__concord_http.take() else {
                        unreachable!("generated HTTP builder state is internally consistent");
                    };
                    self.__concord_http = ::std::option::Option::Some(configure(http));
                    self
                }

                #[must_use]
                /// Applies the runtime configuration closure immediately.
                pub fn runtime<F>(mut self, configure: F) -> Self
                where
                    F: ::std::ops::FnOnce(
                        #core_path::advanced::RuntimeConfigBuilder,
                    ) -> #core_path::advanced::RuntimeConfigBuilder,
                {
                    if self.__concord_runtime_configured {
                        self.__concord_duplicate_runtime = true;
                        return self;
                    }
                    self.__concord_runtime_configured = true;
                    let Some(runtime) = self.__concord_runtime.take() else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    self.__concord_runtime = ::std::option::Option::Some(configure(runtime));
                    self
                }

                /// Resolves configuration and builds the generated client.
                pub fn build(
                    self,
                ) -> ::std::result::Result<
                    super::#client_name,
                    #core_path::prelude::ClientBuildError,
                > {
                    if self.__concord_duplicate_http {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::duplicate_http_configuration_error(),
                        );
                    }
                    if self.__concord_duplicate_runtime {
                        return ::std::result::Result::Err(
                            __concord_generated::__core::__private::duplicate_runtime_configuration_error(),
                        );
                    }
                    let Some(http) = self.__concord_http else {
                        unreachable!("generated HTTP builder state is internally consistent");
                    };
                    let Some(runtime) = self.__concord_runtime else {
                        unreachable!("generated runtime builder state is internally consistent");
                    };
                    let core = __concord_generated::__core::__private::build_client_core(
                        #base,
                        http,
                        runtime,
                    )?;
                    ::std::result::Result::Ok(super::#client_name {
                        __concord_core: core,
                    })
                }
            }

            impl ::std::default::Default for super::#builder_name {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl facades::#facade_name {
                #(#facade_methods)*
            }
        }
    }
}

fn generate_endpoint(
    client_name: &Ident,
    scope: &Scope,
    endpoint: &Endpoint,
    core_path: &TokenStream,
    endpoint_capability_name: &Ident,
) -> TokenStream {
    let endpoint_name = &endpoint.name;
    let scope_types = scope
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect::<Vec<_>>();
    let endpoint_types = endpoint
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect::<Vec<_>>();
    let scope_bindings = scope
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| parameter_binding("scope", index, &parameter.name))
        .collect::<Vec<_>>();
    let endpoint_bindings = endpoint
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| parameter_binding("endpoint", index, &parameter.name))
        .collect::<Vec<_>>();
    let (scope_public_bindings, endpoint_public_bindings, body_public_binding) =
        explicit_parameter_bindings(
            &scope.parameters,
            &endpoint.parameters,
            endpoint.body.is_some(),
        );
    let body_private_binding = endpoint
        .body
        .as_ref()
        .map(|_| Ident::new("__concord_body_0_body", Span::mixed_site()));
    let body_input_type = endpoint
        .body
        .as_ref()
        .map(|body| codec_input_type(body, core_path));
    let body_codec_type = endpoint
        .body
        .as_ref()
        .map(|body| codec_type(body, core_path));
    let path_statements = scope
        .path
        .iter()
        .chain(endpoint.path.iter())
        .map(|atom| path_statement(atom, &scope.parameters, &endpoint.parameters))
        .collect::<Vec<_>>();
    let endpoint_identity = format!("{client_name}::{}::{endpoint_name}", scope.name);
    let route_shape = route_shape(scope, endpoint);
    let response_name = response_name(&endpoint.response);
    let method_name = method_name(endpoint.method);
    let required_names = scope
        .parameters
        .iter()
        .chain(endpoint.parameters.iter())
        .map(|parameter| format!("`{}`", parameter.name))
        .chain(endpoint.body.as_ref().map(|_| "`body`".to_owned()))
        .collect::<Vec<_>>();
    let required_arguments = if required_names.is_empty() {
        "none".to_owned()
    } else {
        required_names.join(", ")
    };
    let endpoint_doc = format!(
        "`{method_name}` endpoint `{endpoint_identity}` for route `{route_shape}`. Required arguments: {required_arguments}. Direct await uses the `{response_name}` response mode; metadata is available through `Request::response`."
    );
    let constructor_doc = format!(
        "Creates the explicit `{endpoint_identity}` endpoint value without preparing or executing a request."
    );
    let response = response_handler(&endpoint.response, core_path);
    let body_type = body_handler(endpoint.body.as_ref(), core_path);
    let method = method_token(endpoint.method, core_path);
    let body_constructor = match (&body_codec_type, &body_private_binding) {
        (Some(codec), Some(binding)) => {
            quote!(super::super::__concord_generated::__core::__private::CodecBody::<#codec>::new(#binding))
        }
        (None, None) => {
            quote!(super::super::__concord_generated::__core::__private::EmptyBody::new())
        }
        _ => unreachable!("body codec and binding are modeled together"),
    };
    let body_field = quote!(__concord_body: #body_type,);
    let body_public_parameter = match (&body_public_binding, &body_input_type) {
        (Some(binding), Some(ty)) => quote!(#binding: #ty,),
        (None, None) => quote!(),
        _ => unreachable!("body input and public binding are modeled together"),
    };
    let body_private_parameter = match (&body_private_binding, &body_input_type) {
        (Some(binding), Some(ty)) => quote!(#binding: #ty,),
        (None, None) => quote!(),
        _ => unreachable!("body input and private binding are modeled together"),
    };
    let body_public_argument = body_public_binding
        .as_ref()
        .map_or_else(TokenStream::new, |binding| quote!(#binding,));
    let body_private_argument = quote!(__concord_body: #body_constructor,);
    let mut bounds = scope
        .parameters
        .iter()
        .map(|parameter| {
            let ty = &parameter.ty;
            quote!(#ty: ::std::fmt::Display + ::std::marker::Send + ::std::marker::Sync + 'static)
        })
        .chain(endpoint.parameters.iter().map(|parameter| {
            let ty = &parameter.ty;
            quote!(#ty: ::std::fmt::Display + ::std::marker::Send + 'static)
        }))
        .collect::<Vec<_>>();
    if let Some(codec) = &body_codec_type {
        bounds.push(quote!(#codec: #core_path::advanced::EncodeBody));
    }
    if let ResponseKind::Buffered(codec) = &endpoint.response {
        let codec = codec_type(codec, core_path);
        bounds.push(quote!(#codec: #core_path::advanced::DecodeBody));
    }
    let where_clause = if bounds.is_empty() {
        quote!()
    } else {
        quote!(where #(#bounds,)*)
    };
    let default_impl =
        if scope.parameters.is_empty() && endpoint.parameters.is_empty() && endpoint.body.is_none()
        {
            quote! {
                impl ::std::default::Default for #endpoint_name {
                    fn default() -> Self {
                        Self::new()
                    }
                }
            }
        } else {
            quote!()
        };

    quote! {
        #[doc = #endpoint_doc]
        pub struct #endpoint_name {
            __concord_scope: (#(::std::sync::Arc<#scope_types>,)*),
            __concord_endpoint: (#(#endpoint_types,)*),
            #body_field
        }

        impl #endpoint_name {
            #[must_use]
            #[doc = #constructor_doc]
            pub fn new(
                #(#scope_public_bindings: #scope_types,)*
                #(#endpoint_public_bindings: #endpoint_types,)*
                #body_public_parameter
            ) -> Self {
                Self::__concord_from_shared(
                    #(::std::sync::Arc::new(#scope_public_bindings),)*
                    #(#endpoint_public_bindings,)*
                    #body_public_argument
                )
            }

            pub(in super::super) fn __concord_from_shared(
                #(#scope_bindings: ::std::sync::Arc<#scope_types>,)*
                #(#endpoint_bindings: #endpoint_types,)*
                #body_private_parameter
            ) -> Self {
                Self {
                    __concord_scope: (#(#scope_bindings,)*),
                    __concord_endpoint: (#(#endpoint_bindings,)*),
                    #body_private_argument
                }
            }
        }

        #default_impl

        #[doc(hidden)]
        impl super::super::__concord_endpoint_seal::Sealed for #endpoint_name {}

        #[doc(hidden)]
        impl super::super::#endpoint_capability_name for #endpoint_name {}

        #[doc(hidden)]
        impl super::super::__concord_generated::__core::__private::EndpointSpec for #endpoint_name
        #where_clause
        {
            type Body = #body_type;
            type Response = #response;

            fn request_meta(
                &self,
            ) -> super::super::__concord_generated::__core::__private::RequestMeta {
                super::super::__concord_generated::__core::__private::RequestMeta::new(
                    #endpoint_identity,
                    #method,
                    ::std::option::Option::None,
                )
            }

            fn prepare_head_and_body_metadata(
                &self,
                mut __concord_builder: super::super::__concord_generated::__core::__private::HeadBuilder,
            ) -> ::std::result::Result<
                (
                    super::super::__concord_generated::__core::__private::PreparedHead,
                    super::super::__concord_generated::__core::__private::BodyMetadata,
                ),
                super::super::__concord_generated::__core::__private::ApiError,
            > {
                #(#path_statements)*
                ::std::result::Result::Ok((
                    __concord_builder.finish(),
                    <Self::Body as super::super::__concord_generated::__core::__private::BodySpec>::metadata(
                        &self.__concord_body,
                        #endpoint_identity,
                        &#method,
                    )?,
                ))
            }

            fn into_body(self) -> Self::Body {
                self.__concord_body
            }
        }
    }
}

fn generate_facade_method(
    client_name: &Ident,
    scope: &Scope,
    endpoint: &Endpoint,
    core_path: &TokenStream,
    scope_module_name: &Ident,
) -> TokenStream {
    let endpoint_name = &endpoint.name;
    let endpoint_method_name = format_ident!("{}", to_snake_case(&endpoint_name.to_string()));
    let endpoint_names = endpoint
        .parameters
        .iter()
        .map(|parameter| &parameter.name)
        .collect::<Vec<_>>();
    let endpoint_types = endpoint
        .parameters
        .iter()
        .map(|parameter| &parameter.ty)
        .collect::<Vec<_>>();
    let body_type = endpoint
        .body
        .as_ref()
        .map(|body| codec_input_type(body, core_path));
    let body_parameter = body_type
        .as_ref()
        .map_or_else(TokenStream::new, |ty| quote!(body: #ty,));
    let body_argument = body_type
        .as_ref()
        .map_or_else(TokenStream::new, |_| quote!(body,));
    let scope_indices = (0..scope.parameters.len())
        .map(Index::from)
        .collect::<Vec<_>>();
    let endpoint_identity = format!("{client_name}::{}::{}", scope.name, endpoint.name);
    let route_shape = route_shape(scope, endpoint);
    let response_name = response_name(&endpoint.response);
    let method_name = method_name(endpoint.method);
    let method_doc = format!(
        "Creates a pending `{method_name}` request for `{endpoint_identity}` at route `{route_shape}` using the `{response_name}` response mode."
    );

    quote! {
        #[must_use]
        #[doc = #method_doc]
        pub fn #endpoint_method_name(
            &self,
            #(#endpoint_names: #endpoint_types,)*
            #body_parameter
        ) -> #core_path::Request<endpoints::#scope_module_name::#endpoint_name> {
            let endpoint = endpoints::#scope_module_name::#endpoint_name::__concord_from_shared(
                #(::std::sync::Arc::clone(&self.__concord_scope.#scope_indices),)*
                #(#endpoint_names,)*
                #body_argument
            );
            self.__concord_client.request(endpoint)
        }
    }
}

fn explicit_parameter_bindings(
    scope_parameters: &[Parameter],
    endpoint_parameters: &[Parameter],
    has_body: bool,
) -> (Vec<Ident>, Vec<Ident>, Option<Ident>) {
    let body_name = has_body.then(|| Ident::new("body", Span::call_site()));
    let original_names = scope_parameters
        .iter()
        .chain(endpoint_parameters.iter())
        .map(|parameter| parameter.name.to_string())
        .chain(body_name.iter().map(ToString::to_string))
        .collect::<Vec<_>>();
    let reserved = original_names.iter().cloned().collect::<HashSet<_>>();
    let mut used = HashSet::new();

    let mut allocate = |namespace: &str, name: &Ident| {
        let original = name.to_string();
        if original_names
            .iter()
            .filter(|name| name.as_str() == original.as_str())
            .count()
            == 1
            && used.insert(original)
        {
            return name.clone();
        }

        let base = format!("{namespace}_{}", to_snake_case(&name.to_string()));
        let mut candidate = base.clone();
        let mut suffix = 2_u32;
        while reserved.contains(&candidate) || !used.insert(candidate.clone()) {
            candidate = format!("{base}_{suffix}");
            suffix += 1;
        }
        Ident::new(&candidate, name.span())
    };

    let scope = scope_parameters
        .iter()
        .map(|parameter| allocate("scope", &parameter.name))
        .collect();
    let endpoint = endpoint_parameters
        .iter()
        .map(|parameter| allocate("endpoint", &parameter.name))
        .collect();
    let body = body_name.as_ref().map(|name| allocate("request", name));
    (scope, endpoint, body)
}

fn route_shape(scope: &Scope, endpoint: &Endpoint) -> String {
    let segments = scope
        .path
        .iter()
        .chain(endpoint.path.iter())
        .map(|atom| match atom {
            PathAtom::Static(value) => value.value(),
            PathAtom::Scope(name) => format!("{{scope.{name}}}"),
            PathAtom::Endpoint(name) => format!("{{ep.{name}}}"),
        })
        .collect::<Vec<_>>();
    format!("/{}", segments.join("/"))
}

fn response_handler(response: &ResponseKind, core_path: &TokenStream) -> TokenStream {
    match response {
        ResponseKind::Bytes => quote!(#core_path::__private::BytesResponse),
        ResponseKind::NoContent => quote!(#core_path::__private::NoContentResponse),
        ResponseKind::Buffered(codec) => {
            let codec = codec_type(codec, core_path);
            quote!(#core_path::__private::BufferedResponse<#codec>)
        }
    }
}

fn body_handler(body: Option<&CodecKind>, core_path: &TokenStream) -> TokenStream {
    match body {
        Some(codec) => {
            let codec = codec_type(codec, core_path);
            quote!(#core_path::__private::CodecBody<#codec>)
        }
        None => quote!(#core_path::__private::EmptyBody),
    }
}

fn codec_type(codec: &CodecKind, core_path: &TokenStream) -> TokenStream {
    match codec {
        CodecKind::Text => quote!(#core_path::prelude::Text),
        CodecKind::Json(ty) => quote!(#core_path::prelude::Json<#ty>),
        CodecKind::Custom(ty) => quote!(#ty),
    }
}

fn codec_input_type(codec: &CodecKind, core_path: &TokenStream) -> TokenStream {
    match codec {
        CodecKind::Text => quote!(::std::string::String),
        CodecKind::Json(ty) => quote!(#ty),
        CodecKind::Custom(ty) => quote!(<#ty as #core_path::advanced::EncodeBody>::Input),
    }
}

fn response_name(response: &ResponseKind) -> String {
    match response {
        ResponseKind::Bytes => "Bytes".to_owned(),
        ResponseKind::NoContent => "NoContent".to_owned(),
        ResponseKind::Buffered(CodecKind::Text) => "Text".to_owned(),
        ResponseKind::Buffered(CodecKind::Json(ty)) => {
            format!("Json<{}>", ty.to_token_stream())
        }
        ResponseKind::Buffered(CodecKind::Custom(ty)) => ty.to_token_stream().to_string(),
    }
}

const fn method_name(method: MethodKind) -> &'static str {
    match method {
        MethodKind::Get => "GET",
        MethodKind::Post => "POST",
    }
}

fn method_token(method: MethodKind, core_path: &TokenStream) -> TokenStream {
    match method {
        MethodKind::Get => quote!(#core_path::__private::Method::GET),
        MethodKind::Post => quote!(#core_path::__private::Method::POST),
    }
}

fn endpoint_uses_json(endpoint: &Endpoint) -> bool {
    endpoint.body.as_ref().is_some_and(codec_is_json)
        || matches!(
            &endpoint.response,
            ResponseKind::Buffered(codec) if codec_is_json(codec)
        )
}

const fn codec_is_json(codec: &CodecKind) -> bool {
    matches!(codec, CodecKind::Json(_))
}

fn path_statement(
    atom: &PathAtom,
    scope_parameters: &[Parameter],
    endpoint_parameters: &[Parameter],
) -> TokenStream {
    match atom {
        PathAtom::Static(value) => quote!(__concord_builder.push_static(#value)?;),
        PathAtom::Scope(name) => {
            let index = scope_parameters
                .iter()
                .position(|parameter| parameter.name == *name)
                .expect("validated scope path references have a generated binding");
            let index = Index::from(index);
            quote!(__concord_builder.push_value(&*self.__concord_scope.#index)?;)
        }
        PathAtom::Endpoint(name) => {
            let index = endpoint_parameters
                .iter()
                .position(|parameter| parameter.name == *name)
                .expect("validated endpoint path references have a generated binding");
            let index = Index::from(index);
            quote!(__concord_builder.push_value(&self.__concord_endpoint.#index)?;)
        }
    }
}

fn parameter_binding(namespace: &str, index: usize, name: &Ident) -> Ident {
    let normalized = to_snake_case(&name.to_string());
    Ident::new(
        &format!("__concord_{namespace}_{index}_{normalized}"),
        Span::mixed_site(),
    )
}

fn to_pascal_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            let Some(first) = characters.next() else {
                return String::new();
            };
            first.to_uppercase().chain(characters).collect::<String>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::{explicit_parameter_bindings, to_pascal_case};
    use crate::model::{Parameter, to_snake_case};

    #[test]
    fn generated_names_are_deterministic() {
        assert_eq!(to_snake_case("ProofApi"), "proof_api");
        assert_eq!(to_snake_case("ById"), "by_id");
        assert_eq!(to_snake_case("HTTPStatus"), "http_status");
        assert_eq!(to_pascal_case("resources"), "Resources");
        assert_eq!(to_pascal_case("user_resources"), "UserResources");
    }

    #[test]
    fn explicit_constructor_bindings_preserve_dsl_names_and_qualify_overlap() {
        let scope = [Parameter {
            name: parse_quote!(region),
            ty: parse_quote!(String),
        }];
        let endpoint = [Parameter {
            name: parse_quote!(user_id),
            ty: parse_quote!(String),
        }];
        let (scope_names, endpoint_names, body_name) =
            explicit_parameter_bindings(&scope, &endpoint, false);
        assert_eq!(scope_names[0], "region");
        assert_eq!(endpoint_names[0], "user_id");
        assert!(body_name.is_none());

        let scope = [Parameter {
            name: parse_quote!(id),
            ty: parse_quote!(String),
        }];
        let endpoint = [Parameter {
            name: parse_quote!(id),
            ty: parse_quote!(String),
        }];
        let (scope_names, endpoint_names, _) =
            explicit_parameter_bindings(&scope, &endpoint, false);
        assert_eq!(scope_names[0], "scope_id");
        assert_eq!(endpoint_names[0], "endpoint_id");

        let scope = [Parameter {
            name: parse_quote!(body),
            ty: parse_quote!(String),
        }];
        let (scope_names, endpoint_names, body_name) =
            explicit_parameter_bindings(&scope, &[], true);
        assert_eq!(scope_names[0], "scope_body");
        assert!(endpoint_names.is_empty());
        assert_eq!(body_name.expect("body binding"), "request_body");
    }
}
