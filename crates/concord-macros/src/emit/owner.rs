//! Owner modules: state, handles, unbound handles, projections and
//! navigation.

use proc_macro2::TokenStream;
use quote::quote;

use super::head::{
    auth_placement, emission_readers, host_label, ops_table, path_segment, rate_declaration,
};
use super::{Access, Context, docs, endpoint, lit, option_duration, supers};
use crate::semantic::{
    Child, Initializer, LocalAuth, LocalHost, LocalPath, OwnerKind, StableValue, Value,
};

/// The complete module of one owner (recursively including its scopes).
pub(crate) fn module(context: &Context<'_>, owner: usize, root_items: TokenStream) -> TokenStream {
    let context = &Context {
        model: context.model,
        f: context.f,
        rust_depth: context.model.owners[owner].depth + 1,
    };
    let model = context.model;
    let entry = &model.owners[owner];
    let module_name = entry.surface.module_path.last().expect("owner module");
    let doc = docs::owner_module(model, owner);
    let state = state(context, owner);
    let projection = projection(context, owner);
    let handle = if entry.kind == OwnerKind::Scope {
        scope_handle(context, owner)
    } else {
        TokenStream::new()
    };
    let unbound = unbound(context, owner);
    let allow_deprecated = super::internal_deprecation_allowance(model);
    let endpoints = entry.children.iter().filter_map(|child| match child {
        Child::Endpoint(endpoint) => Some(endpoint::items(context, *endpoint)),
        Child::Scope(_) => None,
    });
    let scopes = entry.children.iter().filter_map(|child| match child {
        Child::Scope(scope) => Some(module(context, *scope, TokenStream::new())),
        Child::Endpoint(_) => None,
    });
    quote! {
        #[doc = #doc]
        #[allow(unused_imports, clippy::all, clippy::pedantic, non_camel_case_types, dead_code)]
        #allow_deprecated
        pub mod #module_name {
            use super::*;

            #root_items
            #state
            #projection
            #handle
            #unbound
            #(#endpoints)*
            #(#scopes)*
        }
    }
}

/// Storage type of one value's current selection.
pub(crate) fn slot_type(context: &Context<'_>, value: &Value) -> TokenStream {
    let ty = context.rust_type(&value.ty);
    if value.optional {
        quote!(::std::option::Option<::std::sync::Arc<#ty>>)
    } else {
        quote!(::std::sync::Arc<#ty>)
    }
}

pub(crate) fn stable_tokens(context: &Context<'_>, value: &StableValue) -> TokenStream {
    match value {
        StableValue::Str(literal) => quote!(#literal),
        StableValue::Char(literal) => quote!(#literal),
        StableValue::Bool(literal) => quote!(#literal),
        StableValue::Int { negative, literal } => {
            if *negative {
                quote!(-#literal)
            } else {
                quote!(#literal)
            }
        }
        StableValue::Float { negative, literal } => {
            if *negative {
                quote!(-#literal)
            } else {
                quote!(#literal)
            }
        }
        StableValue::Path(path) => {
            let path = context.rust_path(path);
            quote!(#path)
        }
    }
}

/// Expression evaluating one value's initializer once (`Arc<T>`).
pub(crate) fn initializer_expr(context: &Context<'_>, value: &Value) -> TokenStream {
    let f = context.f;
    let ty = context.rust_type(&value.ty);
    match &value.init {
        Initializer::None => unreachable!("initialized value"),
        Initializer::Default(span) => {
            quote::quote_spanned!(*span=> ::std::sync::Arc::new(<#ty as ::std::default::Default>::default()))
        }
        Initializer::Stable(stable) => {
            let tokens = stable_tokens(context, stable);
            quote!(::std::sync::Arc::new(#f::IntoValue::<#ty>::into_value(#tokens)))
        }
    }
}

/// Field declarations (current + retained initial) for one value.
pub(crate) fn value_fields(
    context: &Context<'_>,
    value: &Value,
    retain_initial: bool,
) -> TokenStream {
    let field = &value.field;
    let slot = slot_type(context, value);
    let initial = (retain_initial && value.is_initialized()).then(|| {
        let initial = &value.initial_field;
        let ty = context.rust_type(&value.ty);
        quote!(#initial: ::std::sync::Arc<#ty>,)
    });
    quote!(#field: #slot, #initial)
}

/// Statements initializing local storage fields from positional arguments
/// (named after the value fields) and declaration initializers; returns the
/// struct field initializers.
pub(crate) fn value_initializers(
    context: &Context<'_>,
    values: &[Value],
) -> (TokenStream, TokenStream) {
    let mut statements = Vec::new();
    let mut fields = Vec::new();
    for value in values {
        let field = &value.field;
        let initial = &value.initial_field;
        if value.is_initialized() {
            let init = initializer_expr(context, value);
            statements.push(quote!(let #initial = #init;));
            if value.optional {
                fields.push(quote!(#field: ::std::option::Option::Some(::std::sync::Arc::clone(&#initial)), #initial,));
            } else {
                fields.push(quote!(#field: ::std::sync::Arc::clone(&#initial), #initial,));
            }
        } else if value.optional {
            fields.push(quote!(#field: ::std::option::Option::None,));
        } else {
            fields.push(quote!(#field: ::std::sync::Arc::new(#field),));
        }
    }
    (quote!(#(#statements)*), quote!(#(#fields)*))
}

/// Value-control methods on a consuming handle whose `__with` helper applies
/// an update to the storage `local`.
pub(crate) fn value_controls(
    context: &Context<'_>,
    values: &[Value],
    owner_display: &str,
) -> TokenStream {
    let f = context.f;
    let methods = values.iter().map(|value| {
        let field = &value.field;
        let initial = &value.initial_field;
        let ty = context.rust_type(&value.ty);
        let controls = &value.controls;
        let deprecated = docs::deprecated_attr(&value.meta);
        let mut out = Vec::new();
        if let Some(set) = &controls.set {
            let doc = docs::control(value, "set", owner_display);
            let assign = if value.optional {
                quote!(__concord_local.#field = ::std::option::Option::Some(__concord_value);)
            } else {
                quote!(__concord_local.#field = __concord_value;)
            };
            out.push(quote! {
                #[doc = #doc]
                #deprecated
                #[must_use]
                pub fn #set(self, __concord_value: impl #f::IntoValue<#ty>) -> Self {
                    let __concord_value = ::std::sync::Arc::new(#f::IntoValue::<#ty>::into_value(__concord_value));
                    self.__with(move |__concord_local| { #assign })
                }
            });
        }
        if let Some(replace) = &controls.replace {
            let doc = docs::control(value, "replace", owner_display);
            out.push(quote! {
                #[doc = #doc]
                #deprecated
                #[must_use]
                pub fn #replace(self, __concord_value: ::std::option::Option<#ty>) -> Self {
                    self.__with(move |__concord_local| __concord_local.#field = __concord_value.map(::std::sync::Arc::new))
                }
            });
        }
        if let Some(set_if_some) = &controls.set_if_some {
            let doc = docs::control(value, "set_if_some", owner_display);
            let assign = if value.optional {
                quote!(__concord_local.#field = ::std::option::Option::Some(::std::sync::Arc::new(__concord_value));)
            } else {
                quote!(__concord_local.#field = ::std::sync::Arc::new(__concord_value);)
            };
            out.push(quote! {
                #[doc = #doc]
                #deprecated
                #[must_use]
                pub fn #set_if_some(self, __concord_value: ::std::option::Option<#ty>) -> Self {
                    match __concord_value {
                        ::std::option::Option::Some(__concord_value) => self.__with(move |__concord_local| { #assign }),
                        ::std::option::Option::None => self,
                    }
                }
            });
        }
        if let Some(omit) = &controls.omit {
            let doc = docs::control(value, "omit", owner_display);
            out.push(quote! {
                #[doc = #doc]
                #deprecated
                #[must_use]
                pub fn #omit(self) -> Self {
                    self.__with(|__concord_local| __concord_local.#field = ::std::option::Option::None)
                }
            });
        }
        if let Some(reset) = &controls.reset {
            let doc = docs::control(value, "reset", owner_display);
            let assign = if value.optional {
                quote!(__concord_local.#field = ::std::option::Option::Some(::std::sync::Arc::clone(&__concord_local.#initial));)
            } else {
                quote!(__concord_local.#field = ::std::sync::Arc::clone(&__concord_local.#initial);)
            };
            out.push(quote! {
                #[doc = #doc]
                #deprecated
                #[must_use]
                pub fn #reset(self) -> Self {
                    self.__with(|__concord_local| { #assign })
                }
            });
        }
        quote!(#(#out)*)
    });
    quote!(#(#methods)*)
}

fn state(context: &Context<'_>, owner: usize) -> TokenStream {
    let model = context.model;
    let p = context.p();
    let f = context.f;
    let entry = &model.owners[owner];
    let ops = ops_table(context, &entry.program);
    let (presence, emit) = emission_readers(context, Access::OwnerState(owner), &entry.program);
    let storage = if entry.kind == OwnerKind::Client {
        let credentials = model.client.credentials.iter().map(|credential| {
            let field = &credential.field;
            let material = super::client::material_type(context, credential);
            quote!(#field: #p::GeneratedCredential<#material>,)
        });
        let values = entry
            .values
            .iter()
            .map(|value| value_fields(context, value, false));
        quote! {
            #[doc(hidden)]
            pub struct __State {
                __core: #p::ClientCore,
                #(#credentials)*
                #(#values)*
            }
        }
    } else {
        let values = entry
            .values
            .iter()
            .map(|value| value_fields(context, value, true));
        let arguments = entry.positional.iter().map(|index| {
            let value = &entry.values[*index];
            let field = &value.field;
            let ty = context.rust_type(&value.ty);
            quote!(#field: #ty)
        });
        let (statements, fields) = value_initializers(context, &entry.values);
        quote! {
            // The scope's own values are stored inline: each value is already
            // an `Arc`, so a separate `Arc<__Local>` layer only cost one more
            // allocation per scope selection (V3-O07 attribution).
            #[doc(hidden)]
            #[derive(Clone)]
            pub struct __State {
                __parent: ::std::sync::Arc<super::__State>,
                __local: __Local,
            }

            #[doc(hidden)]
            #[derive(Clone)]
            pub struct __Local {
                #(#values)*
            }

            impl __Local {
                #[allow(clippy::too_many_arguments)]
                fn __new(#(#arguments),*) -> Self {
                    #statements
                    Self { #fields }
                }
            }
        }
    };
    quote! {
        #storage

        impl __State {
            const __OPS: &'static [#p::PatchOp<'static>] = #ops;
        }

        impl #p::HeadEmissions for __State {
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
    }
}

fn projection(context: &Context<'_>, owner: usize) -> TokenStream {
    let model = context.model;
    let p = context.p();
    let f = context.f;
    let entry = &model.owners[owner];
    let access = Access::OwnerState(owner);
    let is_root = entry.kind == OwnerKind::Client;
    let parent = quote!(&*self.__parent);
    let core = if is_root {
        quote!(&self.__core)
    } else {
        quote!(#p::OwnerProjection::core(#parent))
    };
    let display = lit(&entry.display);
    let timeout = option_duration(entry.effective_timeout);
    let inherit = |call: TokenStream| if is_root { TokenStream::new() } else { call };
    let host = {
        let parent_host =
            inherit(quote!(#p::OwnerProjection::push_host(#parent, __concord_builder)?;));
        match &entry.host {
            None => parent_host,
            Some(LocalHost::Prepend(labels)) => {
                let labels = labels
                    .iter()
                    .map(|label| host_label(context, access, label));
                quote!(#(#labels)* #parent_host)
            }
            Some(LocalHost::Replace(labels)) => {
                let labels = labels
                    .iter()
                    .map(|label| host_label(context, access, label));
                quote!(#(#labels)*)
            }
            Some(LocalHost::Off) => TokenStream::new(),
        }
    };
    let path = {
        let parent_path =
            inherit(quote!(#p::OwnerProjection::push_path(#parent, __concord_builder)?;));
        match &entry.path {
            None => parent_path,
            Some(LocalPath::Append(template)) => {
                let segments = template
                    .segments
                    .iter()
                    .map(|segment| path_segment(context, access, segment));
                quote!(#parent_path #(#segments)*)
            }
            Some(LocalPath::Replace(template)) => {
                let segments = template
                    .segments
                    .iter()
                    .map(|segment| path_segment(context, access, segment));
                quote!(#(#segments)*)
            }
            Some(LocalPath::Off) => TokenStream::new(),
        }
    };
    let auth = {
        let parent_auth =
            inherit(quote!(#p::OwnerProjection::push_auth(#parent, __concord_builder)?;));
        match &entry.auth {
            None => parent_auth,
            Some(LocalAuth::Append(placements)) => {
                let placements = placements
                    .iter()
                    .map(|placement| auth_placement(context, access, placement));
                quote!(#parent_auth #(#placements)*)
            }
            Some(LocalAuth::Replace(placements)) => {
                let placements = placements
                    .iter()
                    .map(|placement| auth_placement(context, access, placement));
                quote!(#(#placements)*)
            }
            Some(LocalAuth::Off) => TokenStream::new(),
        }
    };
    let rates = {
        let parent_rates =
            inherit(quote!(#p::OwnerProjection::push_rate_limits(#parent, __concord_builder)?;));
        let local = entry
            .rates
            .iter()
            .map(|rate| rate_declaration(context, access, rate));
        quote!(#parent_rates #(#local)*)
    };
    let has_auth = !entry.effective_auth.is_empty();
    let has_rates = !entry.effective_rates.is_empty();
    let program = if is_root {
        quote! {
            __concord_visit(&[#p::ProgramSegment { ops: Self::__OPS, emissions: self }])
        }
    } else {
        quote! {
            #p::OwnerProjection::visit_program(#parent, &mut |__concord_inherited| {
                #p::visit_extended_program(
                    __concord_inherited,
                    #p::ProgramSegment { ops: Self::__OPS, emissions: self },
                    |__concord_segments| __concord_visit(__concord_segments),
                )
            })
        }
    };
    let ids = supers(entry.depth);
    quote! {
        impl #p::OwnerProjection for __State {
            fn core(&self) -> &#p::ClientCore {
                #core
            }

            fn owner_path(&self) -> &'static str {
                #display
            }

            fn owner_timeout(&self) -> ::std::option::Option<::std::time::Duration> {
                #timeout
            }

            fn push_host(&self, __concord_builder: &mut #p::HeadBuilder) -> ::std::result::Result<(), #f::ApiError> {
                #host
                ::std::result::Result::Ok(())
            }

            fn push_path(&self, __concord_builder: &mut #p::HeadBuilder) -> ::std::result::Result<(), #f::ApiError> {
                #path
                ::std::result::Result::Ok(())
            }

            fn push_auth(&self, __concord_builder: &mut #p::HeadBuilder) -> ::std::result::Result<(), #f::ApiError> {
                #auth
                ::std::result::Result::Ok(())
            }

            fn push_rate_limits(&self, __concord_builder: &mut #p::HeadBuilder) -> ::std::result::Result<(), #f::ApiError> {
                #rates
                ::std::result::Result::Ok(())
            }

            fn owner_has_auth(&self) -> bool {
                #has_auth
            }

            fn owner_has_rate_limits(&self) -> bool {
                #has_rates
            }

            fn visit_program(
                &self,
                __concord_visit: &mut #p::ProgramVisitor<'_>,
            ) -> ::std::result::Result<(), #f::ApiError> {
                #program
            }

            fn generated_endpoint_ids(&self) -> &'static [&'static str] {
                #ids __CONCORD_ENDPOINT_IDS
            }
        }
    }
}

/// Runtime binding methods shared by client and scope handles.
pub(crate) fn binding_methods(context: &Context<'_>, owner: usize) -> TokenStream {
    let f = context.f;
    let p = context.p();
    let display = &context.model.owners[owner].display;
    let bind_doc = format!(
        "Binds a runtime endpoint under `{display}`: the endpoint inherits this owner's fixed origin, generated host labels, path, query/header program, authentication, timeout and every rate constraint (the same projection a generated endpoint uses). Binding validates structure without body, credential, limiter or transport work."
    );
    let plan_doc = format!(
        "Plans a runtime endpoint under `{display}` without binding it. Planning never dispatches, prepares a body, acquires credentials or reserves quota."
    );
    quote! {
        #[doc = #bind_doc]
        pub fn bind<R, O>(
            &self,
            __concord_endpoint: #f::endpoint::RuntimeEndpoint<R, O>,
        ) -> ::std::result::Result<#f::endpoint::BoundRuntimeRequest<R, O>, #f::ApiError>
        where
            R: #f::response::ResponseRepresentation<O>,
            O: ::std::marker::Send + 'static,
        {
            #p::bind_runtime_endpoint(::std::sync::Arc::clone(&self.__concord) as _, __concord_endpoint)
        }

        #[doc = #plan_doc]
        pub fn plan_runtime<R, O>(
            &self,
            __concord_endpoint: &#f::endpoint::RuntimeEndpoint<R, O>,
        ) -> ::std::result::Result<#f::endpoint::RequestPlan, #f::ApiError>
        where
            R: #f::response::ResponseRepresentation<O>,
            O: ::std::marker::Send + 'static,
        {
            #p::plan_runtime_endpoint(::std::sync::Arc::clone(&self.__concord) as _, __concord_endpoint)
        }
    }
}

fn scope_handle(context: &Context<'_>, owner: usize) -> TokenStream {
    let model = context.model;
    let f = context.f;
    let entry = &model.owners[owner];
    let handle = &entry.surface.handle;
    let method = entry.surface.method.as_ref().expect("scope navigation");
    let doc = docs::scope_handle(model, owner);
    let deprecated = docs::deprecated_attr(&entry.meta);
    let controls = value_controls(context, &entry.values, &entry.display);
    let binding = binding_methods(context, owner);
    let parent = entry.parent.expect("scope parent");
    let parent_handle = if parent == 0 {
        let client = &model.client.name;
        let path = supers(entry.depth + 1);
        quote!(#path #client)
    } else {
        let parent_handle = &model.owners[parent].surface.handle;
        quote!(super::#parent_handle)
    };
    let arguments = entry.positional.iter().map(|index| {
        let value = &entry.values[*index];
        let name = &value.name;
        let ty = context.rust_type(&value.ty);
        quote!(#name: impl #f::IntoValue<#ty>)
    });
    let conversions = entry.positional.iter().map(|index| {
        let value = &entry.values[*index];
        let name = &value.name;
        let ty = context.rust_type(&value.ty);
        quote!(#f::IntoValue::<#ty>::into_value(#name))
    });
    let arguments = quote!(#(#arguments),*);
    let conversions = quote!(#(#conversions),*);
    let nav_doc = docs::scope_navigation(model, owner);
    let display = lit(&entry.display);
    quote! {
        #[doc = #doc]
        #deprecated
        #[derive(Clone)]
        pub struct #handle {
            __concord: ::std::sync::Arc<__State>,
        }

        impl #handle {
            fn __with(self, __concord_update: impl ::std::ops::FnOnce(&mut __Local)) -> Self {
                let mut __concord_local = ::std::clone::Clone::clone(&self.__concord.__local);
                __concord_update(&mut __concord_local);
                Self {
                    __concord: ::std::sync::Arc::new(__State {
                        __parent: ::std::sync::Arc::clone(&self.__concord.__parent),
                        __local: __concord_local,
                    }),
                }
            }

            #controls
            #binding
        }

        impl ::std::fmt::Debug for #handle {
            fn fmt(&self, __concord_formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                __concord_formatter.debug_struct(#display).finish_non_exhaustive()
            }
        }

        impl #parent_handle {
            #[doc = #nav_doc]
            #deprecated
            #[must_use]
            pub fn #method(&self, #arguments) -> #handle {
                #handle {
                    __concord: ::std::sync::Arc::new(__State {
                        __parent: ::std::sync::Arc::clone(&self.__concord),
                        __local: __Local::__new(#conversions),
                    }),
                }
            }
        }
    }
}

fn unbound(context: &Context<'_>, owner: usize) -> TokenStream {
    let model = context.model;
    let f = context.f;
    let entry = &model.owners[owner];
    let client = &model.client.name;
    let client_path = supers(entry.depth + 1);
    let doc = docs::unbound_owner(model, owner);
    if entry.kind == OwnerKind::Client {
        return quote! {
            #[doc = #doc]
            #[derive(Clone, Copy, Debug, Default)]
            pub struct Unbound;

            impl Unbound {
                fn __bind(&self, __concord_client: &#client_path #client) -> ::std::sync::Arc<__State> {
                    ::std::sync::Arc::clone(&__concord_client.__concord)
                }

                fn __bind_owned(self, __concord_client: &#client_path #client) -> ::std::sync::Arc<__State> {
                    self.__bind(__concord_client)
                }
            }
        };
    }
    let method = entry.surface.method.as_ref().expect("scope navigation");
    let controls = value_controls(context, &entry.values, &entry.display);
    let arguments = entry.positional.iter().map(|index| {
        let value = &entry.values[*index];
        let name = &value.name;
        let ty = context.rust_type(&value.ty);
        quote!(#name: impl #f::IntoValue<#ty>)
    });
    let conversions = entry.positional.iter().map(|index| {
        let value = &entry.values[*index];
        let name = &value.name;
        let ty = context.rust_type(&value.ty);
        quote!(#f::IntoValue::<#ty>::into_value(#name))
    });
    let nav_doc = format!(
        "Selects the unbound `{}` scope; defaults are evaluated once now and are not re-evaluated when an endpoint intent is later bound to a client.",
        entry.display
    );
    // §3.5: selecting a deprecated scope warns on the unbound tree too, like
    // the unbound constructor of a deprecated endpoint.
    let deprecated = docs::deprecated_attr(&entry.meta);
    let display = lit(&format!("{} (unbound)", entry.display));
    quote! {
        #[doc = #doc]
        #[derive(Clone)]
        pub struct Unbound {
            __parent: super::Unbound,
            __local: __Local,
        }

        impl Unbound {
            fn __with(mut self, __concord_update: impl ::std::ops::FnOnce(&mut __Local)) -> Self {
                __concord_update(&mut self.__local);
                self
            }

            fn __bind(&self, __concord_client: &#client_path #client) -> ::std::sync::Arc<__State> {
                ::std::sync::Arc::new(self.__bind_state(__concord_client))
            }

            fn __bind_state(&self, __concord_client: &#client_path #client) -> __State {
                __State {
                    __parent: self.__parent.__bind(__concord_client),
                    __local: ::std::clone::Clone::clone(&self.__local),
                }
            }

            fn __bind_owned(self, __concord_client: &#client_path #client) -> ::std::sync::Arc<__State> {
                ::std::sync::Arc::new(self.__bind_owned_state(__concord_client))
            }

            fn __bind_owned_state(self, __concord_client: &#client_path #client) -> __State {
                __State {
                    __parent: self.__parent.__bind_owned(__concord_client),
                    __local: self.__local,
                }
            }

            #controls
        }

        impl ::std::fmt::Debug for Unbound {
            fn fmt(&self, __concord_formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                __concord_formatter.debug_struct(#display).finish_non_exhaustive()
            }
        }

        impl super::Unbound {
            #[doc = #nav_doc]
            #deprecated
            #[must_use]
            pub fn #method(&self, #(#arguments),*) -> Unbound {
                Unbound {
                    __parent: ::std::clone::Clone::clone(self),
                    __local: __Local::__new(#(#conversions),*),
                }
            }
        }
    }
}
