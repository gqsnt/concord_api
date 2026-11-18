//! Shared call storage and operations, with endpoint-specific typed construction.
use super::*;
use syn::visit::{self, Visit};

fn wrapped_value(mut ty: &mut syn::Type) -> Option<&mut syn::Type> {
    while let syn::Type::Paren(paren) = ty {
        ty = &mut paren.elem;
    }
    let syn::Type::Path(path) = ty else {
        return None;
    };
    let syn::PathArguments::AngleBracketed(args) = &mut path.path.segments.last_mut()?.arguments
    else {
        return None;
    };
    match args.args.first_mut()? {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

fn may_reference_signature_name(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => {
            matches!(
                ident.to_string().trim_start_matches("r#"),
                "E" | "retry" | "etag"
            )
        }
        proc_macro2::TokenTree::Group(group) => may_reference_signature_name(group.stream()),
        proc_macro2::TokenTree::Punct(punct) => punct.as_char() == '!',
        proc_macro2::TokenTree::Literal(_) => false,
    })
}

fn problem_needs_alias(ty: &syn::Type) -> bool {
    #[derive(Default)]
    struct ReferencesSignatureName(bool);

    impl<'ast> Visit<'ast> for ReferencesSignatureName {
        fn visit_ident(&mut self, ident: &'ast Ident) {
            self.0 |= ident.to_string().trim_start_matches("r#") == "E";
        }

        fn visit_expr(&mut self, expr: &'ast syn::Expr) {
            self.0 |= may_reference_signature_name(expr.to_token_stream());
        }

        fn visit_macro(&mut self, _: &'ast syn::Macro) {
            self.0 = true;
        }

        fn visit_token_stream(&mut self, tokens: &'ast TokenStream) {
            self.0 |= may_reference_signature_name(tokens.clone());
        }

        fn visit_generic_argument(&mut self, argument: &'ast syn::GenericArgument) {
            // A bare name in a generic argument can denote a constant.
            let ty = match argument {
                syn::GenericArgument::Type(ty) => Some(ty),
                syn::GenericArgument::AssocType(binding) => Some(&binding.ty),
                _ => None,
            };
            if let Some(syn::Type::Path(ty)) = ty {
                self.0 |= ty.path.get_ident().is_some_and(|ident| {
                    matches!(ident.to_string().trim_start_matches("r#"), "retry" | "etag")
                });
            }
            visit::visit_generic_argument(self, argument);
        }
    }

    let mut references = ReferencesSignatureName::default();
    references.visit_type(ty);
    references.0
}

impl Writer<'_> {
    pub(super) fn data_path(&self, index: usize) -> TokenStream {
        if self.nodes.get(index).is_some_and(|n| n.deprecated) {
            let alias = ident(&format!("__DataImpl{index}"));
            return emit!(#alias);
        }
        self.raw_data_path(index)
    }

    pub(super) fn raw_data_path(&self, index: usize) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        let modules = &node.modules;
        let data = ident(&format!("__Data{index}"));
        emit!(#(#modules::)* #data)
    }

    pub(super) fn call_type(&self, node: &NodeInfo<'_>) -> TokenStream {
        let root = &self.root;
        let ty = &node.implementation;
        emit!(#root::Call<#ty>)
    }

    pub(super) fn endpoint_declaration(&self, index: usize) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        let depth = node.modules.len();
        let vis = self.visibility(depth);
        let field_vis = Self::field_visibility(depth);
        let supers = (1..depth).map(|_| ident("super")).collect::<Vec<_>>();
        let parent = ident(&format!("__Node{}", node.parent.unwrap_or(0)));
        let data = ident(&format!("__Data{index}"));
        let ty = Ident::new(
            &name::type_name(&node.node.name.to_string()),
            node.node.name.span(),
        );
        let attrs = &node.node.attrs;
        let hidden_attrs = node.endpoint_attrs();
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        let state = if self.tuple_data(index) {
            let inputs = node.inputs.iter().map(|input| {
                let id = self.input_id(input);
                stored(input.input, emit!(<__T as #(#supers::)* __Types>::#id))
            });
            emit!(#vis type #data<__T = ()> = ::concord::__private::CallData<<__T as #(#supers::)* __Types>::#parent, (#(#inputs,)*), #ty>;)
        } else {
            let fields = node.inputs.iter().map(|input| {
                let field = &input.input.name;
                let id = self.input_id(input);
                let ty = stored(input.input, emit!(<__T as #(#supers::)* __Types>::#id));
                emit!(#field_vis #field: #ty,)
            });
            emit!(#vis struct #data<__T: #(#supers::)* __Types = ()> {
                #field_vis __parent: <__T as #(#supers::)* __Types>::#parent,
                #(#fields)*
            })
        };
        let docs = self
            .endpoints
            .get(&(node.node as *const Node))
            .map(|endpoint| self.endpoint_type_doc(index, endpoint));
        emit! {
            #(#attrs)* #docs
            #[allow(missing_debug_implementations)]
            #vis enum #ty {}
            #hidden_attrs #allow #[doc(hidden)] #state
        }
    }

    pub(super) fn call_declarations(&self) -> TokenStream {
        let vis = self.visibility(1);
        let seals = self.nodes.iter().filter(|node| matches!(node.node.kind, NodeKind::Endpoint(_))).map(|node| {
            let marker = Ident::new(&name::type_name(&node.node.name.to_string()), node.node.name.span());
            let modules = node.modules.iter().skip(1);
            let attrs = node.endpoint_impl_attrs();
            emit!(#attrs #[allow(deprecated)] impl __sealed::Sealed for #(#modules::)* #marker {})
        });
        let must_use = docs::call_must_use(false);
        let call_vis = located(vis.clone(), self.client_span);
        let call = quote::quote_spanned!(self.client_span=>
            /// A call to the endpoint named by its type argument.
            ///
            /// The endpoint's page lists its request, inputs, options, and result.
            /// Use the endpoint name to find its options.
            #must_use
            #call_vis struct Call<E: Endpoint> {
                __data: E::Data<()>,
                __options: ::concord::__private::Options<__Problem>,
            }
        );
        emit! {
            mod __sealed { pub trait Sealed {} }
            #(#seals)*

            /// The endpoint types of this client. The client supplies all implementations.
            ///
            /// Use `E: Endpoint` with `Call<E>` to write code that accepts any call of this client.
            #vis trait Endpoint: __sealed::Sealed + ::core::marker::Sized + ::core::marker::Send + ::core::marker::Sync + 'static {
                /// The value of a successful send. For a paged endpoint, this is one page.
                type Value: ::core::marker::Send + 'static;
                #[doc(hidden)]
                type Data<__T: __Types>: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static;
                #[doc(hidden)]
                type Paging: ::concord::call::PagingMode + __PagesFor<Self>;
                #[doc(hidden)]
                type Conditional: ::concord::call::ConditionalMode;
                #[doc(hidden)]
                const __ENDPOINT: &'static str;
                #[doc(hidden)]
                const __DEBUG: &'static str;
                #[doc(hidden)]
                fn __core(__value: &Self::Data<()>) -> &::concord::__private::Core<__Problem>;
                #[doc(hidden)]
                fn __settings(__value: &Self::Data<()>, __draft: &mut ::concord::__private::Options<__Problem>) -> ::concord::__private::Idempotent {
                    ::concord::__private::Idempotent::ByMethod
                }
                #[doc(hidden)]
                fn __build_draft(__value: &Self::Data<()>) -> Result<::concord::__private::Draft>;
                #[doc(hidden)]
                fn __read_answer(__value: &Self::Data<()>, __response: ::concord::reqwest::Response, __info: ::concord::transport::TryInfo)
                    -> impl ::core::future::Future<Output = Result<::concord::Answer<Self::Value>>> + ::core::marker::Send;
            }
            #[doc(hidden)]
            #vis trait __Pager: Endpoint {
                fn __pages(__call: Call<Self>) -> ::concord::Pages<Self::Value, __Problem>;
            }
            #[doc(hidden)]
            #vis trait __PagesFor<__E: Endpoint>: ::concord::call::PagingMode {
                fn __pages(__call: Call<__E>, __proof: Self::Proof) -> ::concord::Pages<__E::Value, __Problem>;
            }
            impl<__E: Endpoint> __PagesFor<__E> for ::concord::call::NoPaginate {
                fn __pages(_: Call<__E>, __proof: ::core::convert::Infallible) -> ::concord::Pages<__E::Value, __Problem> {
                    match __proof {}
                }
            }
            impl<__E: __Pager> __PagesFor<__E> for ::concord::call::Paged {
                fn __pages(__call: Call<__E>, _: ()) -> ::concord::Pages<__E::Value, __Problem> {
                    __E::__pages(__call)
                }
            }

            #call
        }
    }

    pub(super) fn endpoint_methods(&self, index: usize) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        let ty = self.call_type(node);
        let root = &self.root;
        let data = self.data_path(index);
        let attrs = node.endpoint_attrs();
        let impl_attrs = node.endpoint_impl_attrs();
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        let endpoint = self.endpoints.get(&(node.node as *const Node));
        let marker = &node.implementation;
        let must_use = docs::call_must_use(endpoint.is_some_and(|ep| ep.pager.is_some()));
        let setters: Vec<_> = node.inputs.iter().filter(|input| input.input.optional.is_some()).map(|input| {
            let name = &input.input.name;
            let field = self.input_field(index, input, emit!((*__call.__data_mut())));
            let stored = stored(input.input, input.input.ty.to_token_stream());
            let named_doc = endpoint.map(|ep| self.endpoint_setter_doc(ep, input.input, node.modules.len()));
            let editor_doc = endpoint.map(|ep| self.endpoint_setter_doc(ep, input.input, 0));
            let alias = format!("{} {name}", node.node.name);
            let argument = quote::quote_spanned!(input.input.ty.span()=> #name: impl ::concord::arg::Arg<#stored>);
            let named = emit! {
                #attrs #named_doc
                #allow #must_use #[doc(alias = #alias)]
                pub fn #name(mut __call: #ty, #argument) -> #ty {
                    #field = ::concord::arg::Arg::into_input(#name);
                    __call
                }
            };
            let chained = emit! {
                #attrs #editor_doc #must_use #[allow(deprecated)]
                pub fn #name(self, #argument) -> Self { #marker::#name(self, #name) }
            };
            (named, chained)
        }).collect();
        let setters = (!setters.is_empty()).then(|| {
            let (named, chained): (Vec<_>, Vec<_>) = setters.into_iter().unzip();
            emit! {
                #impl_attrs #allow impl #marker { #(#named)* }
                #impl_attrs #[doc(hidden)] impl #ty { #(#chained)* }
            }
        });
        let clone = (!self.tuple_data(index)).then(|| {
        let clone_fields = node.inputs.iter().map(|input| {
            let field = &input.input.name;
            emit!(#field: ::core::clone::Clone::clone(&self.#field),)
        });
            emit! {
                #impl_attrs #allow impl<__T: #root::__Types> ::core::clone::Clone for #data<__T> {
                    fn clone(&self) -> Self { Self { __parent: ::core::clone::Clone::clone(&self.__parent), #(#clone_fields)* } }
                }
            }
        });
        let endpoint = endpoint.map(|endpoint| self.endpoint_impl(index, endpoint));
        emit! {
            #setters
            #clone
            #endpoint
        }
    }

    pub(super) fn endpoint_impl(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        let root = &self.root;
        let ty = self.call_type(node);
        let cfg = &node.cfg;
        let impl_attrs = node.endpoint_impl_attrs();
        let data = self.data_path(index);
        let marker = &node.implementation;
        let (output, read_type, read_alias) = match endpoint.node.output.as_ref() {
            Some(output) if endpoint.wrapper.is_some() => {
                let mut output = output.ty.clone();
                let Some(value) = wrapped_value(&mut output) else {
                    self.internal(
                        node.node.name.span(),
                        "a wrapped response has no value type",
                    );
                    return TokenStream::new();
                };
                // The reader and the wrapper use the same authored type from this scope.
                let mut alias = ident(&format!("__Read{index}"));
                alias.set_span(alias.span().located_at(value.span()));
                let authored = &*value;
                let declaration = emit!(#(#cfg)* type #alias = #authored;);
                *value = syn::parse_quote!(#alias);
                (output.into_token_stream(), emit!(#alias), declaration)
            }
            Some(output) => (
                output.ty.to_token_stream(),
                emit!(Self::Value),
                TokenStream::new(),
            ),
            None => (emit!(()), emit!(Self::Value), TokenStream::new()),
        };
        let path = &endpoint.path;
        let debug = name::type_name(&node.node.name.to_string());
        let problem = self.problem_alias();
        let core = self.core(index);
        let request = self.request(index, endpoint);
        let answer = self.answer(index, endpoint, &read_type);
        let idle_timeout = endpoint.idle_timeout.as_ref().map(|line| {
            let bindings = self.bindings(&line.origin, index, emit!(__value), false);
            let expression = line.value;
            emit! {
                let __idle_timeout = { #bindings (|| -> ::std::time::Duration { #expression })() };
                __draft.description_idle_timeout(__idle_timeout);
            }
        });
        let idempotent = match &endpoint.idempotent {
            Some(line) => match line.value {
                Some(expression) => {
                    let bindings = self.bindings(&line.origin, index, emit!(__value), false);
                    emit_at!(expression.span()=> { #bindings ::concord::__private::idempotent((|| { #expression })()) })
                }
                None => emit!(::concord::__private::Idempotent::Yes),
            },
            None => emit!(::concord::__private::Idempotent::ByMethod),
        };
        let settings = (endpoint.idle_timeout.is_some() || endpoint.idempotent.is_some()).then(|| emit! {
            fn __settings(__value: &Self::Data<()>, __draft: &mut ::concord::__private::Options<#problem>) -> ::concord::__private::Idempotent {
                #idle_timeout #idempotent
            }
        });
        let conditional = match &endpoint.node.kind {
            NodeKind::Endpoint(m) => match method(m).as_str() {
                "GET" | "HEAD" => ident("GetOrHead"),
                "POST" => ident("Post"),
                _ => ident("NotGetOrHead"),
            },
            _ => ident("NotGetOrHead"),
        };
        let paging_mode = ident(if endpoint.pager.is_some() {
            "Paged"
        } else {
            "NoPaginate"
        });
        let modes = emit! {
            type Paging = ::concord::call::#paging_mode;
            type Conditional = ::concord::call::#conditional;
        };
        let paging_method = endpoint.pager.as_ref().map(|pager| {
            let expression = pager.value;
            let bindings = self.bindings_at(
                &pager.origin,
                index,
                emit!((*__owners.__data())),
                true,
                expression.span(),
            );
            let mut unwrapped = expression;
            loop {
                match unwrapped {
                    syn::Expr::Paren(paren) => unwrapped = &paren.expr,
                    syn::Expr::Group(group) => unwrapped = &group.expr,
                    _ => break,
                }
            }
            let build = if matches!(unwrapped, syn::Expr::Closure(_)) {
                emit_at!(expression.span()=>
                    fn __pager<__C, __N, __F>(_: &__C, __make: impl ::core::ops::FnOnce() -> __F) -> __F
                    where __C: ::concord::page::Call,
                        __N: ::concord::page::IntoNext<__C>,
                        __F: ::core::ops::FnMut(__C, &::concord::Answer<__C::Value>) -> __N
                            + ::core::marker::Send + 'static,
                    { __make() }
                    let __pager = __pager(&__call, || { #bindings #expression });
                    ::concord::__private::pages_with_pager(__call, || __pager)
                )
            } else {
                emit_at!(expression.span()=>
                    ::concord::__private::pages_with_pager(__call, || { #bindings #expression })
                )
            };
            emit_at!(expression.span()=>
                #impl_attrs impl #root::__Pager for #marker {
                    fn __pages(__call: #root::Call<Self>) -> ::concord::Pages<Self::Value, #problem> {
                        let __owners = ::core::clone::Clone::clone(&__call);
                        #build
                    }
                }
            )
        });
        let slots = endpoint.pager.as_ref().map(|_| {
            let slots = node.inputs.iter().map(|input| {
                let slot = self.slot_type(index, input);
                let get_field = self.input_field(index, input, emit!((*__call.__data())));
                let set_field = self.input_field(index, input, emit!((*__call.__data_mut())));
                let input_type = self.input_id(input);
                let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
                let get = if input.input.optional.is_some() { emit!(#get_field.as_ref()) } else { emit!(::core::option::Option::Some(&#get_field)) };
                let set = if input.input.optional.is_some() { emit!(::core::option::Option::Some(__value)) } else { emit!(__value) };
                emit!(#impl_attrs #allow #[doc(hidden)] impl ::concord::page::Slot<#ty> for #slot {
                    type Value = #input_type;
                    fn get<'__a>(&self, __call: &'__a #ty) -> ::core::option::Option<&'__a Self::Value> { #get }
                    fn set(&self, mut __call: #ty, __value: Self::Value) -> #ty { #set_field = #set; __call }
                })
            });
            emit!(#(#slots)*)
        });
        emit! {
            #read_alias
            #impl_attrs impl #root::Endpoint for #marker {
                type Data<__T: #root::__Types> = #data<__T>;
                type Value = #output;
                #modes
                const __ENDPOINT: &'static str = #path;
                const __DEBUG: &'static str = #debug;
                fn __core(__value: &Self::Data<()>) -> &::concord::__private::Core<#problem> { &#core }
                #settings #request #answer
            }
            #paging_method
            #slots
        }
    }

    pub(super) fn call_implementations(&self) -> TokenStream {
        let page_problem = emit_at!(self.problem_span()=> __Problem);
        let prepare_call = self.at_problem(emit!(::concord::__private::prepare_call));
        let timeout_doc = call_docs::with_timeout();
        let idle_doc = call_docs::with_idle_timeout();
        let read_doc = call_docs::read_response();
        let request_doc = call_docs::request();
        let into_request_doc = call_docs::into_request();
        let edit_doc = call_docs::edit_request();
        let retry_doc = call_docs::with_retry();
        let retry = if self.has_problem() {
            emit! {
                #[doc(hidden)]
                pub(super) fn __with_retry(mut self, retry: Retry) -> Self {
                    self.__options.with_retry(retry);
                    self
                }
            }
        } else {
            emit_at!(self.client_span=>
                #retry_doc
                pub fn with_retry(mut self, retry: self::Retry) -> Self {
                    self.__options.with_retry(retry);
                    self
                }
            )
        };
        let timeout = emit_at!(self.client_span=>
            #timeout_doc
            pub fn with_timeout(mut self, timeout: ::std::time::Duration) -> Self {
                self.__options.with_timeout(timeout);
                self
            }
        );
        emit! {
            impl<E: Endpoint> ::core::clone::Clone for Call<E> {
                fn clone(&self) -> Self {
                    Self { __data: ::core::clone::Clone::clone(&self.__data), __options: ::core::clone::Clone::clone(&self.__options) }
                }
            }
            impl<E: Endpoint> ::core::fmt::Debug for Call<E> {
                fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    formatter.debug_struct(E::__DEBUG).field("endpoint", &E::__ENDPOINT).finish_non_exhaustive()
                }
            }
            impl<E: Endpoint> Call<E> {
                #[doc(hidden)]
                pub(super) fn __new(data: E::Data<()>) -> Self {
                    Self { __data: data, __options: ::concord::__private::Options::default() }
                }
                #[doc(hidden)]
                pub(super) fn __data(&self) -> &E::Data<()> { &self.__data }
                #[doc(hidden)]
                pub(super) fn __data_mut(&mut self) -> &mut E::Data<()> { &mut self.__data }

                fn __build_request(&self, take: bool) -> Result<::concord::reqwest::Request> {
                    E::__build_draft(&self.__data)?.finish_request(E::__core(&self.__data), &self.__options, E::__ENDPOINT, take)
                }

                fn __prepare_inner(mut self) -> Result<::concord::__private::Prepared<E::Value, __Problem>> {
                    let run = ::concord::__private::Run::start(E::__core(&self.__data), &self.__options, E::__ENDPOINT);
                    let idempotent = E::__settings(&self.__data, &mut self.__options);
                    let draft = E::__build_draft(&self.__data)?;
                    #prepare_call(
                        draft, run, ::core::clone::Clone::clone(E::__core(&self.__data)),
                        ::core::clone::Clone::clone(&self.__options), idempotent, self,
                        |call, response, info| async move { E::__read_answer(&call.__data, response, info).await },
                    )
                }

                #retry
                #timeout

                #idle_doc
                pub fn with_idle_timeout(mut self, timeout: ::std::time::Duration) -> Self {
                    self.__options.with_idle_timeout(timeout); self
                }

                #read_doc
                pub fn read_response(&self, response: ::concord::reqwest::Response)
                    -> impl ::core::future::Future<Output = Result<E::Value>> + ::core::marker::Send + '_ {
                    async move {
                        E::__read_answer(&self.__data, response, ::concord::transport::TryInfo::new(E::__ENDPOINT, 0))
                            .await.map(::concord::Answer::into_value)
                    }
                }

                #request_doc
                pub fn request(&self) -> Result<::concord::reqwest::Request> { self.__build_request(false) }

                #into_request_doc
                pub fn into_request(self) -> Result<::concord::reqwest::Request> { self.__build_request(true) }

                #edit_doc
                pub fn edit_request(mut self, edit: impl ::core::ops::Fn(&mut ::concord::reqwest::Request) + ::core::marker::Send + ::core::marker::Sync + 'static) -> Self {
                    self.__options.edit_request(edit); self
                }
            }

            #[doc(hidden)]
            impl<E: Endpoint> ::concord::__private::ConditionalCall for Call<E>
            where E::Conditional: ::concord::call::IsGetOrHead {
                fn set_if_none_match(&mut self, _: ::concord::__private::ConditionalAccess<Self>, etag: &::core::primitive::str) {
                    self.__options.if_none_match(etag);
                }
            }

            #[doc(hidden)]
            impl<E: Endpoint> ::concord::page::Call for Call<E> {
                type Value = E::Value;
                type Problem = #page_problem;
                fn at_url(mut self, url: &::core::primitive::str) -> Self {
                    self.__options.at_url(::concord::__private::without_fragment(url)); self
                }
            }

            #[doc(hidden)]
            impl<E: Endpoint> ::concord::__private::Call for Call<E> {
                fn request(&self) -> Result<::concord::reqwest::Request> { self.__build_request(false) }
                fn prepare(self) -> Result<::concord::__private::Prepared<E::Value, __Problem>> { self.__prepare_inner() }
                fn answered_from(mut self, url: ::concord::reqwest::Url) -> Self { self.__options.answered_from(url); self }
                fn edit_request(self, edit: impl ::core::ops::Fn(&mut ::concord::reqwest::Request) + ::core::marker::Send + ::core::marker::Sync + 'static) -> Self { Self::edit_request(self, edit) }
            }
        }
    }

    pub(super) fn send_methods(&self) -> TokenStream {
        let root = &self.root;
        let problem_alias = self.problem_alias();
        let shown_problem = self.problem_type().map(|ty| {
            if problem_needs_alias(ty) {
                problem_alias.to_token_stream()
            } else {
                ty.to_token_stream()
            }
        });
        let problem = shown_problem.as_ref().map(|ty| emit!(, #ty));
        let future_problem = shown_problem.as_ref().map(|_| emit!(, #problem_alias));
        let output_doc = format!(
            "Awaiting an unpaged call gives [`{}::Result<E::Value>`]({}::Result).",
            root, root
        );
        let send_doc = call_docs::send();
        let send_full_doc = call_docs::send_full();
        let send_raw_doc = call_docs::send_raw();
        let conditional_doc = call_docs::send_if_none_match();
        let pages_doc = call_docs::pages();
        let send_prepared = self.at_problem(emit!(send));
        let send_raw_prepared = self.at_problem(emit!(send_raw));
        // A function item in scope lets the argument bind even if the author
        // has declared a constant with the same name.
        let retry_binding = shown_problem.as_ref().map(|_| {
            emit!(
                #[allow(unused_imports)]
                use ::core::mem::drop as retry;
            )
        });
        let retry = shown_problem.map(|ty| {
            let retry_doc = call_docs::with_retry();
            emit_at!(self.client_span=>
                #retry_doc
                pub fn with_retry(self, retry: ::concord::Retry<#ty>) -> Self {
                    self.__with_retry(retry)
                }
            )
        });
        let span = self.client_span;
        let root_at_client = Ident::new(&root.to_string(), span);
        let pages_impl = quote::quote_spanned!(span=>
            impl<E: #root_at_client::Endpoint> #root_at_client::Call<E>
            where E::Paging: ::concord::call::DeclaresPaginate {
                #pages_doc
                pub fn pages(self) -> ::concord::Pages<E::Value #problem> {
                    <E::Paging as #root_at_client::__PagesFor<E>>::__pages(self, ())
                }
            }
        );
        let conditional_methods = emit!(
                #[doc(hidden)]
                pub fn __prepare_if_none_match(self, etag: &::core::primitive::str)
                    -> #root_at_client::Result<::concord::__private::Prepared<::core::option::Option<E::Value>, #problem_alias>> {
                    ::concord::__private::prepare_conditional(self, etag)
                }

                #conditional_doc
                pub fn send_if_none_match(self, etag: &::core::primitive::str)
                    -> ::concord::Sending<::concord::Answer<::core::option::Option<E::Value>> #problem> {
                    let etag = etag.to_owned();
                    ::concord::Sending::new(async move { self.__prepare_if_none_match(&etag)?.#send_prepared().await })
                }
        );
        let conditional_impl = quote::quote_spanned!(span=>
            impl<E: #root_at_client::Endpoint> #root_at_client::Call<E>
            where E::Conditional: ::concord::call::IsGetOrHead {
                #conditional_methods
            }
        );
        emit! {
            const _: () = {
            #retry_binding
            #[allow(unused_imports)]
            use ::core::mem::drop as etag;
            #conditional_impl
            impl<E: #root::Endpoint> #root::Call<E> {
                #send_doc
                pub fn send(self) -> ::concord::Sending<E::Value #problem> {
                    ::concord::Sending::new(async move {
                        <Self as ::concord::__private::Call>::send_full(self).await.map(::concord::Answer::into_value)
                    })
                }

                #send_full_doc
                pub fn send_full(self) -> ::concord::Sending<::concord::Answer<E::Value> #problem> {
                    <Self as ::concord::__private::Call>::send_full(self)
                }

                #send_raw_doc
                pub fn send_raw(self) -> ::concord::Sending<::concord::reqwest::Response #problem> {
                    ::concord::Sending::new(async move { <Self as ::concord::__private::Call>::prepare(self)?.#send_raw_prepared().await })
                }

                #retry
            }

            #pages_impl
            };
            #[doc = #output_doc]
            #[diagnostic::do_not_recommend]
            impl<E: #root::Endpoint> ::core::future::IntoFuture for #root::Call<E>
            where E::Paging: ::concord::call::AwaitWithoutPaginate {
                type Output = ::core::result::Result<E::Value, ::concord::Error<#problem_alias>>;
                type IntoFuture = ::concord::Sending<E::Value #future_problem>;
                fn into_future(self) -> Self::IntoFuture { self.send() }
            }
        }
    }
}
