//! Endpoint specifications, named requests, unbound intents and terminals.

use proc_macro2::TokenStream;
use quote::quote;

use super::head::{auth_placement, emission_readers, ops_table, path_segment, rate_declaration};
use super::owner::{stable_tokens, value_controls, value_fields, value_initializers};
use super::{Access, Context, docs, lit, located_at, supers, value_field_expr};
use crate::semantic::{InitValue, LocalAuth, LocalPath, OwnerKind, Positional};

pub(crate) fn items(context: &Context<'_>, index: usize) -> TokenStream {
    let model = context.model;
    let f = context.f;
    let p = context.p();
    let endpoint = &model.endpoints[index];
    let owner = &model.owners[endpoint.owner];
    let surface = &endpoint.surface;
    let spec = &surface.spec_type;
    let local = &surface.local_type;
    let view = syn::Ident::new(&format!("{spec}View"), spec.span());
    let request = &surface.request_type;
    let intent = &surface.endpoint_type;
    let method = &surface.method;
    let access = Access::Endpoint(index);
    let clone = endpoint.is_clone();
    let client = &model.client.name;
    let client_path = supers(owner.depth + 1);
    let root_module = supers(owner.depth);
    let output = context.rust_type(&endpoint.output);
    let response = context.rust_type(&endpoint.response);
    let id = lit(&endpoint.id);

    // ---- storage ----
    let value_storage = endpoint
        .values
        .iter()
        .map(|value| value_fields(context, value, true));
    let body_storage = endpoint.body.as_ref().map(|body| {
        let ty = context.rust_type(&body.ty);
        if body.optional {
            quote!(__concord_body: ::std::option::Option<#ty>,)
        } else {
            quote!(__concord_body: #ty,)
        }
    });
    let derive_clone = clone.then(|| quote!(#[derive(Clone)]));
    let arguments = endpoint
        .positional
        .iter()
        .map(|positional| match positional {
            Positional::Value(value) => {
                let value = &endpoint.values[*value];
                let field = &value.field;
                let ty = context.rust_type(&value.ty);
                quote!(#field: #ty)
            }
            Positional::Body => {
                let ty = context.rust_type(&endpoint.body.as_ref().expect("body").ty);
                quote!(__concord_body: #ty)
            }
        });
    let (statements, fields) = value_initializers(context, &endpoint.values);
    let body_init = endpoint.body.as_ref().map(|body| {
        if body.optional {
            quote!(__concord_body: ::std::option::Option::None,)
        } else {
            quote!(__concord_body,)
        }
    });

    // ---- head ----
    let ops = ops_table(context, &endpoint.program);
    let (presence, emit) = emission_readers(context, access, &endpoint.program);
    let owner_ref = quote!(&*self.__owner);
    let path = match &endpoint.path {
        None => quote!(#p::OwnerProjection::push_path(#owner_ref, &mut __concord_builder)?;),
        Some(LocalPath::Append(template)) => {
            let segments = template
                .segments
                .iter()
                .map(|segment| path_segment(context, access, segment));
            quote!(#p::OwnerProjection::push_path(#owner_ref, &mut __concord_builder)?; #(#segments)*)
        }
        Some(LocalPath::Replace(template)) => {
            let segments = template
                .segments
                .iter()
                .map(|segment| path_segment(context, access, segment));
            quote!(#(#segments)*)
        }
        Some(LocalPath::Off) => TokenStream::new(),
    };
    let program = if endpoint.program.ops.is_empty() {
        quote! {
            #p::OwnerProjection::visit_program(#owner_ref, &mut |__concord_segments| {
                #p::apply_head_program(&mut __concord_builder, __concord_segments)
            })?;
        }
    } else {
        quote! {
            #p::OwnerProjection::visit_program(#owner_ref, &mut |__concord_inherited| {
                #p::visit_extended_program(
                    __concord_inherited,
                    #p::ProgramSegment { ops: Self::__OPS, emissions: self },
                    |__concord_segments| #p::apply_head_program(&mut __concord_builder, __concord_segments),
                )
            })?;
        }
    };
    let auth = match &endpoint.auth {
        None => quote!(#p::OwnerProjection::push_auth(#owner_ref, &mut __concord_builder)?;),
        Some(LocalAuth::Append(placements)) => {
            let placements = placements
                .iter()
                .map(|placement| auth_placement(context, access, placement));
            quote!(#p::OwnerProjection::push_auth(#owner_ref, &mut __concord_builder)?; { let __concord_builder = &mut __concord_builder; #(#placements)* })
        }
        Some(LocalAuth::Replace(placements)) => {
            let placements = placements
                .iter()
                .map(|placement| auth_placement(context, access, placement));
            quote!({ let __concord_builder = &mut __concord_builder; #(#placements)* })
        }
        Some(LocalAuth::Off) => TokenStream::new(),
    };
    let rates = {
        let local = endpoint
            .rates
            .iter()
            .map(|rate| rate_declaration(context, access, rate));
        quote!(#p::OwnerProjection::push_rate_limits(#owner_ref, &mut __concord_builder)?; { let __concord_builder = &mut __concord_builder; #(#local)* })
    };

    // ---- EndpointSpec ----
    let (input, body_spec, into_body) = match &endpoint.body {
        None => (quote!(()), quote!(#p::EmptyBody), quote!(())),
        Some(body) => {
            let ty = context.rust_type(&body.ty);
            let representation = context.rust_type(&body.representation);
            if body.optional {
                // DIAG-01: the optional-body adapter's `RequestRepresentation`
                // obligation is reported at the selected representation.
                let span = syn::spanned::Spanned::span(&representation);
                let open = located_at(quote!(#p::OptionalRequestRepresentation<), span);
                let close = located_at(quote!(>), span);
                let adapter = quote!(#open #representation #close);
                (
                    quote!(::std::option::Option<#ty>),
                    adapter,
                    quote!(self.__local.__concord_body),
                )
            } else {
                (
                    quote!(#ty),
                    quote!(#representation),
                    quote!(self.__local.__concord_body),
                )
            }
        }
    };
    // DIAG-01 / OUTCOME-01: the head-absence obligations (a sealed
    // `OptionalOutcome` result and a `Buffered` present driver) are reported
    // at the `absent on` clause, not across the whole invocation.
    let relocate = |tokens: TokenStream| {
        if endpoint.absence.is_some() {
            located_at(tokens, endpoint.absence_span)
        } else {
            tokens
        }
    };
    let response_spec = if endpoint.absence.is_some() {
        relocate(quote!(#p::HeadAbsence<#response>))
    } else {
        quote!(#response)
    };
    // Without `absent on`, the static `ACCEPT` check's obligation is already
    // reported at the selected representation type (its own tokens).
    let response_accept =
        relocate(quote!(<#response_spec as #p::ResponseRepresentation<#output>>::ACCEPT));
    let absence = endpoint.absence.as_ref().map(|codes| {
        // The result and representation keep their own spans, so a
        // non-`Buffered` driver is reported at the `via` type and a
        // non-`Option` result at the result type.
        let constructor = relocate(quote!(#p::expected_absence));
        let codes = relocate(quote!((&[#(#codes),*])));
        quote! {
            fn expected_absence(&self) -> ::std::option::Option<#p::ExpectedAbsence<Self::Output, Self::Response>> {
                ::std::option::Option::Some(#constructor::<#output, #response> #codes)
            }
        }
    });

    // ---- pagination ----
    let checkpoint = super::checkpoint::items(context, index);
    let pageable = endpoint.pagination.as_ref().map(|pagination| {
        let controller = context.rust_type(&pagination.controller);
        let init_type = context.rust_path(&pagination.init_type);
        let init_fields = pagination.fields.iter().map(|(field, value)| {
            let expr = match value {
                InitValue::Stable(stable) => stable_tokens(context, stable),
                InitValue::Value(key) => {
                    let field = value_field_expr(model, access, *key);
                    if model.value(*key).optional {
                        quote!(#field.as_deref().cloned())
                    } else {
                        quote!(::std::clone::Clone::clone(&*#field))
                    }
                }
            };
            quote!(#field: #expr)
        });
        let cursor_types = pagination.cursor.iter().map(|index| {
            let value = &endpoint.values[*index];
            let ty = context.rust_type(&value.ty);
            if value.optional {
                quote!(::std::option::Option<#ty>)
            } else {
                quote!(#ty)
            }
        });
        let cursor_names: Vec<syn::Ident> = pagination
            .cursor
            .iter()
            .map(|index| {
                syn::Ident::new(&format!("__concord_cursor_{index}"), proc_macro2::Span::mixed_site())
            })
            .collect();
        let assigns = pagination
            .cursor
            .iter()
            .zip(&cursor_names)
            .map(|(index, name)| {
                let value = &endpoint.values[*index];
                let field = &value.field;
                if value.optional {
                    quote!(self.__local.#field = #name.map(::std::sync::Arc::new);)
                } else {
                    quote!(self.__local.#field = ::std::sync::Arc::new(#name);)
                }
            });
        // DIAG-01: a cursor/destination type mismatch is reported at the
        // `cursor -> ...` mapping; the destination types keep their spans.
        let cursor_span = pagination.cursor_span;
        let assign = located_at(quote!(= __concord_cursor;), cursor_span);
        let binding = if pagination.tuple {
            let pattern = located_at(quote!(let (#(#cursor_names),*):), cursor_span);
            quote!(#pattern (#(#cursor_types),*) #assign)
        } else {
            let name = &cursor_names[0];
            let ty = cursor_types.clone().next().expect("one cursor");
            let pattern = located_at(quote!(let #name:), cursor_span);
            quote!(#pattern #ty #assign)
        };
        let page_clone = {
            let values = endpoint.values.iter().map(|value| {
                let field = &value.field;
                let initial = value.is_initialized().then(|| {
                    let initial = &value.initial_field;
                    quote!(#initial: ::std::clone::Clone::clone(&self.__local.#initial),)
                });
                quote!(#field: ::std::clone::Clone::clone(&self.__local.#field), #initial)
            });
            let body = endpoint
                .body
                .as_ref()
                .map(|_| quote!(__concord_body: ::std::clone::Clone::clone(&self.__local.__concord_body),));
            quote! {
                Self {
                    __owner: ::std::clone::Clone::clone(&self.__owner),
                    __local: #local { #(#values)* #body },
                }
            }
        };
        let body_present = match &endpoint.body {
            None => quote!(false),
            Some(body) if body.optional => quote!(self.__local.__concord_body.is_some()),
            Some(_) => quote!(true),
        };
        let span = pagination.span;
        let init_assert = quote::quote_spanned!(span=> {
            let __concord_init: #p::ControllerInit<Self> = #init_type { #(#init_fields),* };
            __concord_init
        });
        // DIAG-01: the controller's projections (`ControllerInit<Self>`,
        // `ControllerCursor<Self>`) are reported at the `paginate` clause;
        // the authored controller type keeps its own span.
        let head = located_at(quote!(impl #p::PageableEndpoint for #spec), span);
        let controller_head = located_at(quote!(type Controller =), span);
        let controller_end = located_at(quote!(;), span);
        let init_signature = located_at(
            quote!(fn controller_init(&self) -> #p::ControllerInit<Self>),
            span,
        );
        let cursor_signature = located_at(
            quote!(fn apply_cursor(&mut self, __concord_cursor: #p::ControllerCursor<Self>)),
            span,
        );
        let checkpoint_methods = located_at(super::checkpoint::methods(context, index), pagination.span);
        quote! {
            #head {
                #controller_head #controller #controller_end

                #checkpoint_methods

                #init_signature {
                    #init_assert
                }

                #cursor_signature {
                    #binding
                    #(#assigns)*
                }

                fn page_clone(&self) -> Self {
                    #page_clone
                }

                fn page_body_present(&self) -> bool {
                    #body_present
                }
            }
        }
    });

    // ---- request ----
    let owner_display = endpoint.display.clone();
    let controls = value_controls(context, &endpoint.values, &owner_display);
    let body_controls = body_controls(context, index);
    // Required-only endpoints have no value mutation surface. Avoid emitting
    // two unused generic helpers for every such endpoint in a large contract.
    let has_value_controls = !controls.is_empty() || !body_controls.is_empty();
    let request_update = has_value_controls.then(|| {
        quote! {
            fn __with(self, __concord_update: impl ::std::ops::FnOnce(&mut #local)) -> Self {
                Self {
                    __inner: #p::map_endpoint(self.__inner, |mut __concord_spec| {
                        __concord_update(&mut __concord_spec.__local);
                        __concord_spec
                    }),
                }
            }
        }
    });
    let intent_update = has_value_controls.then(|| {
        quote! {
            fn __with(mut self, __concord_update: impl ::std::ops::FnOnce(&mut #local)) -> Self {
                __concord_update(&mut self.__local);
                self
            }
        }
    });
    let request_doc = docs::request(model, index);
    let deprecated = docs::deprecated_attr(&endpoint.meta);
    let terminals = terminals(context, index);
    let request_clone = clone.then(|| {
        quote! {
            impl ::std::clone::Clone for #request {
                fn clone(&self) -> Self {
                    Self { __inner: ::std::clone::Clone::clone(&self.__inner) }
                }
            }
        }
    });
    let intent_clone = clone.then(|| quote!(#[derive(Clone)]));
    let request_name = lit(&request.to_string());
    let intent_name = lit(&intent.to_string());

    // ---- constructors ----
    let handle = if owner.kind == OwnerKind::Client {
        quote!(super::#client)
    } else {
        let handle = &owner.surface.handle;
        quote!(#handle)
    };
    let constructor_arguments: Vec<TokenStream> = endpoint
        .positional
        .iter()
        .map(|positional| match positional {
            Positional::Value(value) => {
                let value = &endpoint.values[*value];
                let name = &value.name;
                let ty = context.rust_type(&value.ty);
                quote!(#name: impl #f::IntoValue<#ty>)
            }
            Positional::Body => {
                let body = endpoint.body.as_ref().expect("body");
                let name = &body.name;
                let ty = context.rust_type(&body.ty);
                quote!(#name: #ty)
            }
        })
        .collect();
    let constructor_values: Vec<TokenStream> = endpoint
        .positional
        .iter()
        .map(|positional| match positional {
            Positional::Value(value) => {
                let value = &endpoint.values[*value];
                let name = &value.name;
                let ty = context.rust_type(&value.ty);
                quote!(#f::IntoValue::<#ty>::into_value(#name))
            }
            Positional::Body => {
                let name = &endpoint.body.as_ref().expect("body").name;
                quote!(#name)
            }
        })
        .collect();
    let constructor_doc = docs::constructor(model, index);
    let intent_doc = docs::intent(model, index);
    // Planning an unbound intent only borrows the joined owner projection, so
    // a scope owner's state is assembled on the stack instead of in a new
    // `Arc` (V3-O07 attribution); binding still creates the shared owner.
    let (owner_field, owner_from_handle, owner_from_intent) = if owner.kind == OwnerKind::Client {
        (
            quote!(::std::sync::Arc<__State>),
            quote!(::std::sync::Arc::clone(&self.__concord)),
            quote!(self.__owner.__bind_owned(__concord_client)),
        )
    } else {
        (
            quote!(__State),
            quote!(::std::clone::Clone::clone(&*self.__concord)),
            quote!(self.__owner.__bind_owned_state(__concord_client)),
        )
    };
    let plan_owner = if owner.kind == OwnerKind::Client {
        quote!(let __concord_owner = self.__owner.__bind(__concord_client);)
    } else {
        quote!(let __concord_owner = self.__owner.__bind_state(__concord_client);)
    };

    quote! {
        #[doc(hidden)]
        #derive_clone
        pub struct #local {
            #(#value_storage)*
            #body_storage
        }

        impl #local {
            #[allow(clippy::too_many_arguments)]
            fn __new(#(#arguments),*) -> Self {
                #statements
                Self { #fields #body_init }
            }
        }

        #[doc(hidden)]
        #derive_clone
        pub struct #spec {
            __owner: #owner_field,
            __local: #local,
        }

        #[doc(hidden)]
        pub struct #view<'a> {
            __owner: &'a __State,
            __local: &'a #local,
        }

        impl #view<'_> {
            const __OPS: &'static [#p::PatchOp<'static>] = #ops;

            fn prepare_head(
                &self,
                mut __concord_builder: #p::HeadBuilder,
            ) -> ::std::result::Result<#p::PreparedHead, #f::ApiError> {
                #p::OwnerProjection::push_host(#owner_ref, &mut __concord_builder)?;
                #path
                #program
                #auth
                #rates
                __concord_builder.finish()
            }
        }

        impl #p::HeadEmissions for #view<'_> {
            fn emission_present(&self, __concord_emission: u16) -> bool {
                #presence
            }

            fn emit(
                &self,
                __concord_emission: u16,
                __concord_sink: &mut #p::OccurrenceSink<'_>,
            ) -> ::std::result::Result<(), #f::AtomError> {
                #emit
            }
        }

        impl #spec {
            fn __description() -> &'static #f::endpoint::EndpointDescription {
                static DESCRIPTION: ::std::sync::LazyLock<#f::endpoint::EndpointDescription> =
                    ::std::sync::LazyLock::new(|| {
                        #p::endpoint_description(#client_path #client::contract(), #id)
                    });
                &DESCRIPTION
            }
        }

        // §9.5: the static `ACCEPT` constant is validated as an HTTP Accept
        // value once, before any client can be built: an invalid value is a
        // compile error, and no representation code runs to discover it.
        const _: () = #p::assert_response_accept(#response_accept);

        impl #p::EndpointSpec for #spec {
            type Input = #input;
            type Body = #body_spec;
            type Output = #output;
            type Response = #response_spec;

            fn request_meta(&self) -> #p::RequestMeta {
                #p::RequestMeta::from_description(Self::__description())
            }

            fn prepare_head(
                &self,
                __concord_builder: #p::HeadBuilder,
            ) -> ::std::result::Result<#p::PreparedHead, #f::ApiError> {
                #view { __owner: &self.__owner, __local: &self.__local }.prepare_head(__concord_builder)
            }

            #absence

            fn into_body(self) -> Self::Input {
                #into_body
            }
        }

        #checkpoint
        #pageable

        #[doc = #request_doc]
        #deprecated
        #[must_use = "a request does nothing until it is awaited or a terminal is selected"]
        pub struct #request {
            __inner: #f::endpoint::Request<#spec>,
        }

        impl #request {
            #request_update

            #controls
            #body_controls
            #terminals
        }

        impl ::std::future::IntoFuture for #request {
            type Output = ::std::result::Result<#output, #f::ApiError>;
            type IntoFuture = <#f::endpoint::Request<#spec> as ::std::future::IntoFuture>::IntoFuture;

            fn into_future(self) -> Self::IntoFuture {
                ::std::future::IntoFuture::into_future(self.__inner)
            }
        }

        impl #f::RequestControls for #request {
            fn with_options(self, __concord_options: &#f::CallOptions) -> Self {
                Self::with_options(self, __concord_options)
            }

            fn control_summary(&self) -> #f::ControlSummary {
                Self::control_summary(self)
            }

            fn cancel_with(self, __concord_token: #f::CancellationToken) -> ::std::result::Result<Self, #f::ControlError> {
                Self::cancel_with(self, __concord_token)
            }
        }

        #request_clone

        impl ::std::fmt::Debug for #request {
            fn fmt(&self, __concord_formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                __concord_formatter
                    .debug_struct(#request_name)
                    .field("endpoint", &#id)
                    .field("controls", &self.control_summary())
                    .finish_non_exhaustive()
            }
        }

        #[doc = #intent_doc]
        #deprecated
        #intent_clone
        pub struct #intent {
            __owner: Unbound,
            __local: #local,
        }

        impl #intent {
            #intent_update

            /// Returns this endpoint's value-free effective description.
            #[must_use]
            pub fn description(&self) -> &'static #f::endpoint::EndpointDescription {
                #spec::__description()
            }

            #controls
            #body_controls
        }

        impl ::std::fmt::Debug for #intent {
            fn fmt(&self, __concord_formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                __concord_formatter
                    .debug_struct(#intent_name)
                    .field("endpoint", &#id)
                    .finish_non_exhaustive()
            }
        }

        impl #root_module __concord_seal::Sealed for #intent {}

        impl #root_module GeneratedEndpoint for #intent {
            type Request = #request;

            fn __concord_bind(self, __concord_client: &#client_path #client) -> #request {
                // The bound owner chain ends at `client`'s root state, so its
                // core is the client's core; the consumed intent's snapshot
                // moves into the projection without reference-count churn.
                #request {
                    __inner: #p::request(
                        #p::OwnerProjection::core(&*__concord_client.__concord),
                        #spec { __owner: #owner_from_intent, __local: self.__local },
                    ),
                }
            }

            fn __concord_plan(
                &self,
                __concord_client: &#client_path #client,
            ) -> ::std::result::Result<#f::endpoint::RequestPlan, #f::ApiError> {
                #plan_owner
                let __concord_view = #view { __owner: &__concord_owner, __local: &self.__local };
                #p::plan_unbound_request(
                    #p::OwnerProjection::core(&*__concord_view.__owner),
                    #spec::__description(),
                    #response_accept,
                    |__concord_builder| __concord_view.prepare_head(__concord_builder),
                )
            }
        }

        impl #handle {
            #[doc = #constructor_doc]
            #deprecated
            pub fn #method(&self, #(#constructor_arguments),*) -> #request {
                #request {
                    __inner: #p::request(
                        #p::OwnerProjection::core(&*self.__concord),
                        #spec {
                            __owner: #owner_from_handle,
                            __local: #local::__new(#(#constructor_values),*),
                        },
                    ),
                }
            }
        }

        impl Unbound {
            #[doc = #intent_doc]
            #deprecated
            pub fn #method(&self, #(#constructor_arguments),*) -> #intent {
                #intent {
                    __owner: ::std::clone::Clone::clone(self),
                    __local: #local::__new(#(#constructor_values),*),
                }
            }
        }
    }
}

fn body_controls(context: &Context<'_>, index: usize) -> TokenStream {
    let endpoint = &context.model.endpoints[index];
    let Some(body) = &endpoint.body else {
        return TokenStream::new();
    };
    if !body.optional {
        return TokenStream::new();
    }
    let ty = context.rust_type(&body.ty);
    let controls = &body.controls;
    let set = controls.set.as_ref().expect("optional body set");
    let replace = controls.replace.as_ref().expect("optional body replace");
    let set_if_some = controls
        .set_if_some
        .as_ref()
        .expect("optional body set_if_some");
    let omit = controls.omit.as_ref().expect("optional body omit");
    let name = &body.logical;
    let set_doc = format!("Selects the exact present request body `{name}` (no conversion).");
    let replace_doc = format!(
        "Replaces the outer body slot `{name}`: `Some` selects a present body; `None` selects an absent body (no representation call, no body bytes, no body media header)."
    );
    let if_some_doc =
        format!("Selects `{name}` when `Some`; `None` leaves the current selection unchanged.");
    let omit_doc = format!("Selects an absent request body `{name}`.");
    quote! {
        #[doc = #set_doc]
        #[must_use]
        pub fn #set(self, __concord_value: #ty) -> Self {
            self.__with(move |__concord_local| __concord_local.__concord_body = ::std::option::Option::Some(__concord_value))
        }

        #[doc = #replace_doc]
        #[must_use]
        pub fn #replace(self, __concord_value: ::std::option::Option<#ty>) -> Self {
            self.__with(move |__concord_local| __concord_local.__concord_body = __concord_value)
        }

        #[doc = #if_some_doc]
        #[must_use]
        pub fn #set_if_some(self, __concord_value: ::std::option::Option<#ty>) -> Self {
            match __concord_value {
                ::std::option::Option::Some(__concord_value) => self.__with(move |__concord_local| __concord_local.__concord_body = ::std::option::Option::Some(__concord_value)),
                ::std::option::Option::None => self,
            }
        }

        #[doc = #omit_doc]
        #[must_use]
        pub fn #omit(self) -> Self {
            self.__with(|__concord_local| __concord_local.__concord_body = ::std::option::Option::None)
        }
    }
}

fn terminals(context: &Context<'_>, index: usize) -> TokenStream {
    let model = context.model;
    let f = context.f;
    let p = context.p();
    let endpoint = &model.endpoints[index];
    let spec = &endpoint.surface.spec_type;
    let output = context.rust_type(&endpoint.output);
    let accepted_bytes = endpoint.expose_accepted_bytes.then(|| {
        quote! {
            /// Selects the explicit accepted-bytes terminal: origin, auth, rate,
            /// effect, status, deadline and `Accept` negotiation are retained, but
            /// representation-specific accepted-head validation and conversion are
            /// bypassed. Resolves to complete bounded bytes.
            pub fn accepted_bytes(self) -> #f::endpoint::AcceptedBytesRequest {
                #p::accepted_bytes_request(self.__inner)
            }
        }
    });
    let pages = endpoint.pagination.as_ref().map(|pagination| {
        // DIAG-01: the page-capability obligations of the traversal terminals
        // (`PaginationController`, `PageRequestRepresentation`) are reported
        // at the `paginate` clause, not across the whole invocation.
        let terminals = quote! {
            /// Starts an inert, application-controlled page traversal of at most
            /// `maximum_pages` pages. Controls applied to this request are carried
            /// into the traversal; ordinary `.await` never paginates.
            pub fn pages(self, __concord_maximum_pages: u32) -> #f::pagination::Pages<#p::PageItem<#spec>> {
                (#p::pages_request)(self.__inner, __concord_maximum_pages)
            }

            /// Resumes a durable traversal position with this request's current
            /// controls. Public inputs must match the checkpoint; the saved
            /// controller supplies the next cursor.
            ///
            /// # Errors
            /// Returns a value-free checkpoint error when the endpoint,
            /// controller or public inputs differ, or the saved state is invalid.
            pub fn resume_pages<__ConcordController>(
                self,
                __concord_checkpoint: &#f::pagination::Checkpoint,
                __concord_maximum_pages: u32,
            ) -> ::std::result::Result<#f::pagination::Pages<#p::PageItem<#spec>>, #f::pagination::CheckpointError>
            where
                #spec: #p::PageableEndpoint<Controller = __ConcordController>,
                __ConcordController: #f::pagination::ResumableController<#output>,
            {
                (#p::resume_pages_request)(self.__inner, __concord_checkpoint, __concord_maximum_pages)
            }

            /// Collects the items of at most `maximum_pages` pages together with a
            /// truthful traversal summary; a page bound is not exhaustion.
            pub fn collect_prefix(self, __concord_maximum_pages: u32) -> #f::pagination::CollectPrefix<#p::PageItem<#spec>> {
                (#p::collect_prefix_request)(self.__inner, __concord_maximum_pages)
            }

            /// Collects every item; fails unless the traversal is exhausted within
            /// `maximum_pages` pages.
            pub fn collect_all(
                self,
                __concord_maximum_pages: ::std::num::NonZeroU32,
            ) -> #f::pagination::CollectAll<#p::PageItem<#spec>> {
                (#p::collect_all_request)(self.__inner, __concord_maximum_pages)
            }
        };
        located_at(terminals, pagination.span)
    });
    let storage = endpoint.storage.iter().map(|credential| {
        let credential = &model.client.credentials[*credential];
        let method = &credential.store_terminal;
        let field = &credential.field;
        let depth = model.owners[endpoint.owner].depth;
        let parents = (0..depth).map(|_| quote!(.__parent));
        let doc = format!(
            "Executes this source endpoint through the managed executor and requests local installation of the returned material into credential `{}`. Resolves to `StoreOutcome::Installed` or `StoreOutcome::Superseded` (a newer explicit intent, clear or replace won). Ordinary `.await` returns the material without storing it.",
            credential.logical
        );
        quote! {
            #[doc = #doc]
            pub fn #method(self) -> #f::auth::StoreRequest {
                let __concord_credential = #p::inspect_endpoint(&self.__inner, |__concord_spec| {
                    ::std::clone::Clone::clone(&__concord_spec.__owner #(#parents)* .#field)
                });
                #p::store_request(self.__inner, &__concord_credential)
            }
        }
    });
    let description_doc = "Returns this endpoint's value-free effective description.";
    quote! {
        /// Replaces the selected finite timeout preference. Caps are retained:
        /// a 5-second cap followed by `timeout(60s)` still limits the operation
        /// to 5 seconds.
        ///
        /// # Errors
        ///
        /// Zero or unrepresentable durations are rejected before execution.
        pub fn timeout(self, __concord_duration: ::std::time::Duration) -> ::std::result::Result<Self, #f::ControlError> {
            ::std::result::Result::Ok(Self { __inner: self.__inner.timeout(__concord_duration)? })
        }

        /// Restores the effective declared (or default) timeout. Caps are retained.
        #[must_use]
        pub fn reset_timeout(self) -> Self {
            Self { __inner: self.__inner.reset_timeout() }
        }

        /// Attaches one more cancellation signal (at most eight distinct signals,
        /// deduplicated by identity).
        ///
        /// # Errors
        ///
        /// A ninth distinct signal is rejected; existing signals are kept.
        pub fn cancel_with(self, __concord_token: #f::CancellationToken) -> ::std::result::Result<Self, #f::ControlError> {
            ::std::result::Result::Ok(Self { __inner: self.__inner.cancel_with(__concord_token)? })
        }

        /// Intersects additional caps (componentwise minimum); caps only narrow
        /// execution and never grant a retry.
        #[must_use]
        pub fn with_options(self, __concord_options: &#f::CallOptions) -> Self {
            Self { __inner: self.__inner.with_options(__concord_options) }
        }

        /// Returns the value-free effective control summary.
        #[must_use]
        pub fn control_summary(&self) -> #f::ControlSummary {
            self.__inner.control_summary()
        }

        #[doc = #description_doc]
        #[must_use]
        pub fn description(&self) -> &'static #f::endpoint::EndpointDescription {
            #spec::__description()
        }

        /// Builds the inert, credential-free, redaction-aware plan: surviving
        /// public atoms are encoded, but no body is prepared, no credential is
        /// obtained, no quota is reserved and nothing is sent.
        ///
        /// # Errors
        ///
        /// Public shaping, encoding or structural-limit failures.
        pub fn plan(&self) -> ::std::result::Result<#f::endpoint::RequestPlan, #f::ApiError> {
            #p::request_plan(&self.__inner, #spec::__description())
        }

        /// Selects the response terminal: resolves to the declared output
        /// together with status, headers, URL and execution metadata.
        pub fn response(self) -> #f::endpoint::ResponseRequest<#output> {
            #p::response_request(self.__inner)
        }

        #accepted_bytes
        #pages
        #(#storage)*
    }
}
