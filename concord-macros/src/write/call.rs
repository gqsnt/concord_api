//! Shared call storage and operations, with endpoint-specific typed construction.
use super::*;
use syn::visit::{self, Visit};

// Match the largest EndpointInputs trait in the core. Each authored type gets
// its own associated item and span so rustc can identify the type in an error.
const INPUTS_PER_GROUP: usize = 16;

fn endpoint_inputs(
    index: usize,
    node: &NodeInfo<'_>,
    key: &TokenStream,
) -> (Vec<TokenStream>, TokenStream) {
    let cfg = &node.cfg;
    let mut types = Vec::with_capacity(node.inputs.len());
    let mut implementations = TokenStream::new();
    for (group, inputs) in node.inputs.chunks(INPUTS_PER_GROUP).enumerate() {
        let storage = ident(&format!("EndpointInputs{}", inputs.len()));
        let group = (group != 0).then(|| emit!(, #group));
        let storage = emit!(::concord::__private::#storage<#index #group>);
        let items = inputs.iter().enumerate().map(|(position, input)| {
            let ty = &input.input.ty;
            let item = Ident::new(
                &format!("Input{position}"),
                Span::mixed_site().located_at(ty.span()),
            );
            types.push(emit!(<#key as #storage>::#item));
            emit_at!(ty.span()=> type #item = #ty;)
        });
        implementations.extend(emit!(
            #(#cfg)* #[doc(hidden)] impl #storage for #key { #(#items)* }
        ));
    }
    (types, implementations)
}

fn may_reference_signature_name(tokens: TokenStream) -> bool {
    type_has_token(tokens, &|token| match token {
        proc_macro2::TokenTree::Ident(ident) => {
            matches!(
                ident.to_string().trim_start_matches("r#"),
                "E" | "retry" | "etag"
            )
        }
        proc_macro2::TokenTree::Punct(punct) => punct.as_char() == '!',
        _ => false,
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

        fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
            self.0 = true;
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

fn type_has_token(tokens: TokenStream, matches: &impl Fn(&proc_macro2::TokenTree) -> bool) -> bool {
    tokens.into_iter().any(|token| {
        matches(&token)
            || match token {
                proc_macro2::TokenTree::Group(group) => type_has_token(group.stream(), matches),
                _ => false,
            }
    })
}

/// Whether `paginate` may take the expression as any `Pager`. The rule, word for word:
///
/// The pager path is taken only when the expression is certain to need no inference: after
/// removing parentheses, it is a path, a call or a method call; or a plain block, an `if`/`else` or
/// a `match` whose every value position is such a form again; and nowhere in the whole expression
/// is there a closure literal or a macro invocation in a value position, a `return`, a `break` with
/// a value, a loop, a label, or a `#[cfg]` or `#[cfg_attr]` attribute; and, at any depth (nested
/// closures, `async` blocks and blocks included), no attribute of any kind, no macro invocation of
/// any kind (also in a pattern or a type) and no item. Everything else takes the path of earlier
/// releases, unchanged. No name is interpreted.
///
/// So everything in a certain expression is plain Rust that the macro has read. The `return`s,
/// `?`s, loops and labels of a nested closure or `async` block cannot give the expression its
/// value, and are allowed there.
fn certain_pager(expr: &syn::Expr) -> bool {
    certain_form(expr) && {
        let mut scan = Uncertain {
            found: false,
            nested: false,
        };
        scan.visit_expr(expr);
        !scan.found
    }
}

/// A path, a call or a method call; or a plain block, `if`/`else` or `match` of such forms.
fn certain_form(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Paren(paren) => certain_form(&paren.expr),
        syn::Expr::Group(group) => certain_form(&group.expr),
        syn::Expr::Path(_) | syn::Expr::Call(_) | syn::Expr::MethodCall(_) => true,
        syn::Expr::Block(block) => block.label.is_none() && certain_tail(&block.block),
        syn::Expr::If(branch) => {
            certain_tail(&branch.then_branch)
                && branch
                    .else_branch
                    .as_ref()
                    .is_some_and(|(_, otherwise)| certain_form(otherwise))
        }
        syn::Expr::Match(choice) => choice.arms.iter().all(|arm| certain_form(&arm.body)),
        _ => false,
    }
}

fn certain_tail(block: &syn::Block) -> bool {
    matches!(block.stmts.last(), Some(syn::Stmt::Expr(tail, None)) if certain_form(tail))
}

/// Whether a closure is written where the expression's value is: at the top, or at a tail of a
/// block of any kind, a branch or an arm. Such a form keeps the entry of earlier releases exactly,
/// messages included; other forms that are not certain take the entry that words the refusal of a
/// pager value.
fn closure_in_value_position(expr: &syn::Expr) -> bool {
    let tail = |block: &syn::Block| matches!(block.stmts.last(), Some(syn::Stmt::Expr(tail, None)) if closure_in_value_position(tail));
    match expr {
        syn::Expr::Closure(_) => true,
        syn::Expr::Paren(paren) => closure_in_value_position(&paren.expr),
        syn::Expr::Group(group) => closure_in_value_position(&group.expr),
        syn::Expr::Block(block) => tail(&block.block),
        syn::Expr::Unsafe(block) => tail(&block.block),
        syn::Expr::Const(block) => tail(&block.block),
        syn::Expr::If(branch) => {
            tail(&branch.then_branch)
                || branch
                    .else_branch
                    .as_ref()
                    .is_some_and(|(_, otherwise)| closure_in_value_position(otherwise))
        }
        syn::Expr::Match(choice) => choice
            .arms
            .iter()
            .any(|arm| closure_in_value_position(&arm.body)),
        _ => false,
    }
}

/// Finds what could give the expression a value that the forms above do not show, and what the
/// macro cannot read.
struct Uncertain {
    found: bool,
    /// Inside a nested closure or `async` block: its exits are its own.
    nested: bool,
}
impl Uncertain {
    fn nested(&mut self, visit: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.nested, true);
        visit(self);
        self.nested = outer;
    }
}
impl<'ast> Visit<'ast> for Uncertain {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            syn::Expr::Closure(closure) => {
                self.nested(|this| visit::visit_expr_closure(this, closure))
            }
            syn::Expr::Async(block) => self.nested(|this| visit::visit_expr_async(this, block)),
            syn::Expr::Return(_)
            | syn::Expr::Try(_)
            | syn::Expr::Loop(_)
            | syn::Expr::While(_)
            | syn::Expr::ForLoop(_)
                if !self.nested =>
            {
                self.found = true
            }
            syn::Expr::Break(exit)
                if !self.nested && (exit.expr.is_some() || exit.label.is_some()) =>
            {
                self.found = true
            }
            syn::Expr::Block(block) if !self.nested && block.label.is_some() => self.found = true,
            _ => visit::visit_expr(self, expr),
        }
    }
    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.found = true;
    }
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
        self.found = true;
    }
    fn visit_item(&mut self, _: &'ast syn::Item) {
        self.found = true;
    }
}

impl Writer<'_> {
    pub(super) fn call_type(&self, node: &NodeInfo<'_>) -> TokenStream {
        let root = &self.root;
        let ty = &node.path;
        emit!(#root::Call<#ty>)
    }

    pub(super) fn endpoint_declaration(&self, index: usize) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        let depth = node.modules.len();
        let vis = self.visibility(depth);
        let ty = Ident::new(
            &name::type_name(&node.node.name.to_string()),
            node.node.name.span(),
        );
        let attrs = &node.node.attrs;
        let docs = self
            .endpoints
            .get(&(node.node as *const Node))
            .map(|endpoint| {
                let docs = self.endpoint_type_doc(index, endpoint);
                // Beside the author's description; the implementation shows the text itself.
                let name = format!(
                    " Its name in errors and logs: [`Endpoint::NAME`]({}{}::Endpoint::NAME).",
                    "super::".repeat(depth),
                    self.root
                );
                emit!(#[doc = ""] #[doc = #name] #docs)
            });
        emit! {
            #(#attrs)* #docs
            #[allow(missing_debug_implementations)]
            #vis enum #ty {}
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
                __data: E::Data,
                __options: ::concord::__private::Options<__Problem>,
            }
        );
        emit! {
            mod __sealed { pub trait Sealed {} }
            #(#seals)*
            impl __sealed::Sealed for ::concord::__private::ClientKey {}
            #[doc(hidden)]
            impl Endpoint for ::concord::__private::ClientKey {
                type Value = ::core::convert::Infallible;
                type Data = ::core::convert::Infallible;
                type Paging = ::concord::call::NoPaginate;
                type Conditional = ::concord::call::NotGetOrHead;
                const NAME: &'static ::core::primitive::str = "";
                const __DEBUG: &'static ::core::primitive::str = "";
                fn __core(value: &Self::Data) -> &::concord::__private::Core<__Problem> { match *value {} }
                fn __build_draft(value: &Self::Data) -> Result<::concord::__private::Draft> { match *value {} }
                async fn __read_answer(value: &Self::Data, _: ::concord::reqwest::Response, _: ::concord::transport::TryInfo)
                    -> Result<::concord::Answer<Self::Value>> { match *value {} }
                fn __pages(_: Call<Self>, proof: ::core::convert::Infallible) -> ::concord::Pages<Self::Value, __Problem> { match proof {} }
            }

            /// The endpoint types of this client. The client supplies all implementations.
            ///
            /// Use `E: Endpoint` with `Call<E>` to write code that accepts any call of this client.
            #vis trait Endpoint: __sealed::Sealed + ::core::marker::Sized + ::core::marker::Send + ::core::marker::Sync + 'static {
                /// The value of a successful send. For a paged endpoint, this is one page.
                type Value: ::core::marker::Send + 'static;
                #[doc(hidden)]
                type Data: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static;
                #[doc(hidden)]
                type Paging: ::concord::call::PagingMode;
                #[doc(hidden)]
                type Conditional: ::concord::call::ConditionalMode;
                /// The endpoint's stable name, the one `TryInfo::endpoint()` reports: the module of the
                /// client, its scopes and the endpoint, joined with `.` (`riot.matches.ids`).
                ///
                /// Use it as a key for limits and in logs.
                const NAME: &'static ::core::primitive::str;
                #[doc(hidden)]
                const __DEBUG: &'static ::core::primitive::str;
                #[doc(hidden)]
                fn __core(__value: &Self::Data) -> &::concord::__private::Core<__Problem>;
                #[doc(hidden)]
                fn __settings(__value: &Self::Data, __draft: &mut ::concord::__private::Options<__Problem>) -> ::concord::__private::Idempotent {
                    ::concord::__private::Idempotent::ByMethod
                }
                #[doc(hidden)]
                fn __build_draft(__value: &Self::Data) -> Result<::concord::__private::Draft>;
                #[doc(hidden)]
                fn __read_answer(__value: &Self::Data, __response: ::concord::reqwest::Response, __info: ::concord::transport::TryInfo)
                    -> impl ::core::future::Future<Output = Result<::concord::Answer<Self::Value>>> + ::core::marker::Send;
                #[doc(hidden)]
                fn __pages(__call: Call<Self>, __proof: <Self::Paging as ::concord::call::PagingMode>::Proof)
                    -> ::concord::Pages<Self::Value, __Problem>;
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
        let attrs = node.endpoint_attrs();
        let impl_attrs = node.endpoint_impl_attrs();
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        let endpoint = self.endpoints.get(&(node.node as *const Node));
        let marker = &node.path;
        let must_use = docs::call_must_use(endpoint.is_some_and(|ep| ep.pager.is_some()));
        let setters: Vec<_> = node.inputs.iter().filter(|input| input.input.optional.is_some()).map(|input| {
            let name = &input.input.name;
            let field = self.input_field(index, input, emit!((*#root::Call::__data_mut(&mut __call))));
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
                #impl_attrs #allow #[doc(hidden)] impl #ty { #(#chained)* }
            }
        });
        let endpoint = endpoint.map(|endpoint| self.endpoint_impl(index, endpoint));
        emit! {
            #setters
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
        let marker = self.endpoint_marker(index, node);
        let key = self.client_key();
        let parent = self.node_type(node.parent.unwrap_or_default(), key.clone());
        let data_span = match node.inputs.as_slice() {
            [input] => input.input.ty.span(),
            _ => node.node.name.span(),
        };
        let (inputs, input_storage) = endpoint_inputs(index, node, &key);
        let input_fields = node.inputs.iter().zip(&inputs);
        let (state, storage) = if self.tuple_data(index) {
            let types = input_fields
                .clone()
                .map(|(input, ty)| stored(input.input, ty.clone()));
            (
                emit!(::concord::__private::CallData<#parent, (#(#types,)*), Self>),
                TokenStream::new(),
            )
        } else {
            let id = self.node_id(self.nodes.len() + index);
            let fields = input_fields.clone().map(|(input, ty)| {
                let field = &input.input.name;
                let ty = stored(input.input, ty.clone());
                emit!(pub #field: #ty,)
            });
            let clones = node.inputs.iter().map(|input| {
                let field = &input.input.name;
                emit!(#field: ::core::clone::Clone::clone(&self.#field),)
            });
            let storage = emit!(
                #(#cfg)* const _: () = {
                    #[doc(hidden)]
                    pub struct __State {
                        pub __parent: #parent,
                        #(#fields)*
                    }
                    impl ::core::clone::Clone for __State {
                        fn clone(&self) -> Self {
                            Self { __parent: ::core::clone::Clone::clone(&self.__parent), #(#clones)* }
                        }
                    }
                    #[doc(hidden)] impl ::concord::__private::EndpointType<#id> for #key {
                        type Value = __State;
                    }
                };
            );
            (self.data_type(index), storage)
        };
        let data = emit_at!(data_span=> type Data = #state;);
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        let output = endpoint
            .node
            .output
            .as_ref()
            .map_or_else(|| emit!(()), |output| output.ty.to_token_stream());
        let read_type = if endpoint.wrapper.is_some() {
            emit!(<Self::Value as ::concord::__private::WrappedOutput>::Inner)
        } else {
            emit!(Self::Value)
        };
        // The check of the declared type against the client's problem type, for every endpoint:
        // a function that is never called, in a module of its own, where none of the author's
        // names is in scope; the module is inside an anonymous constant, so that no name of the
        // check can be written, or met, at the level of the description. The type is the endpoint's own `Value`, as the compiler resolved it:
        // the author's tokens are written once, in the endpoint, and never again here.
        let check = ident(&format!("__check_problem_{index}"));
        let declared = endpoint
            .node
            .output
            .as_ref()
            .map_or_else(|| node.node.name.span(), |written| written.ty.span());
        let problem_alias = self.problem_alias();
        // This one method call needs auto-reference dispatch. The module imports only
        // Concord's two candidate traits and inherits no traits from the author.
        let check_call = emit_at!(declared=>
            (&::concord::__private::Output::<<Witness as Declared>::Value>::new())
                .must_name_the_clients_problem_type::<<Witness as Declared>::Problem>();
        );
        let problem_check = emit!(
            #(#cfg)* #allow const _: () = {
            impl #check::Declared for #check::Witness {
                type Value = <#marker as #root::Endpoint>::Value;
                type Problem = #problem_alias;
            }
            #[allow(dead_code, clippy::needless_borrow)]
            mod #check {
                pub(super) struct Witness;
                pub(super) trait Declared {
                    type Value: ?::core::marker::Sized;
                    type Problem;
                }
                fn check() {
                    use ::concord::__private::{TypedStream as _, OrdinaryOutput as _};
                    #check_call
                }
            }
            };
        );
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
                ::concord::__private::Options::description_idle_timeout(__draft, __idle_timeout);
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
            fn __settings(__value: &Self::Data, __draft: &mut ::concord::__private::Options<#problem>) -> ::concord::__private::Idempotent {
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
        let paging_method = if let Some(pager) = &endpoint.pager {
            let expression = pager.value;
            let bindings = self.bindings_at(
                &pager.origin,
                index,
                emit!((*#root::Call::__data(&__owners))),
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
            // Certain: any `Pager`. A closure at the top: the helper of earlier releases. A closure
            // at another value position: the entry of earlier releases. Anything else: the `FnMut`
            // bound of earlier releases, with a message for a pager value.
            let build = if certain_pager(expression) {
                emit_at!(expression.span()=>
                    ::concord::__private::pages_of_pager(__call, || { #bindings #expression })
                )
            } else if matches!(unwrapped, syn::Expr::Closure(_)) {
                emit_at!(expression.span()=>
                    let __pager = ::concord::__private::make_pager(&__call, || { #bindings #expression });
                    ::concord::__private::pages_with_pager(__call, || __pager)
                )
            } else if closure_in_value_position(expression) {
                emit_at!(expression.span()=>
                    ::concord::__private::pages_with_pager(__call, || { #bindings #expression })
                )
            } else {
                emit_at!(expression.span()=>
                    ::concord::__private::pages_inferred(__call, || { #bindings #expression })
                )
            };
            emit_at!(expression.span()=>
                fn __pages(__call: #root::Call<Self>, _: ()) -> ::concord::Pages<Self::Value, #problem> {
                    let __owners = ::core::clone::Clone::clone(&__call);
                    #build
                }
            )
        } else {
            emit!(
                fn __pages(_: #root::Call<Self>, __proof: ::core::convert::Infallible)
                    -> ::concord::Pages<Self::Value, #problem> { match __proof {} }
            )
        };
        let slots = endpoint.pager.as_ref().map(|_| {
            let slots = input_fields.enumerate().map(|(position, (input, input_type))| {
                let slot = self.slot_type(index, input);
                let get_field = self.input_field(index, input, emit!((*#root::Call::__data(__call))));
                let set_field = self.input_field(index, input, emit!((*#root::Call::__data_mut(&mut __call))));
                let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
                let get = if input.input.optional.is_some() { emit!(::core::option::Option::as_ref(&#get_field)) } else { emit!(::core::option::Option::Some(&#get_field)) };
                let set = if input.input.optional.is_some() { emit!(::core::option::Option::Some(__value)) } else { emit!(__value) };
                emit!(#impl_attrs #allow #[doc(hidden)] impl ::concord::page::Slot<#ty> for #slot {
                    type Value = #input_type;
                    fn get<'__a>(&self, __call: &'__a #ty) -> ::core::option::Option<&'__a Self::Value> { #get }
                    fn set(&self, mut __call: #ty, __value: Self::Value) -> #ty { #set_field = #set; __call }
                    fn input(&self) -> ::concord::page::InputId { ::concord::page::InputId::new(#position) }
                })
            });
            emit!(#(#slots)*)
        });
        emit! {
            #problem_check
            #input_storage
            #storage
            #impl_attrs impl #root::Endpoint for #marker {
                #data
                type Value = #output;
                #modes
                const NAME: &'static ::core::primitive::str = #path;
                const __DEBUG: &'static ::core::primitive::str = #debug;
                fn __core(__value: &Self::Data) -> &::concord::__private::Core<#problem> { &#core }
                #settings #request #answer #paging_method
            }
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
                    ::concord::__private::Options::with_retry(&mut self.__options, retry);
                    self
                }
            }
        } else {
            emit_at!(self.client_span=>
                #retry_doc
                pub fn with_retry(mut self, retry: self::Retry) -> Self {
                    ::concord::__private::Options::with_retry(&mut self.__options, retry);
                    self
                }
            )
        };
        let timeout = emit_at!(self.client_span=>
            #timeout_doc
            pub fn with_timeout(mut self, timeout: ::std::time::Duration) -> Self {
                ::concord::__private::Options::with_timeout(&mut self.__options, timeout);
                self
            }
        );
        emit! {
            // A call is a value that is moved, never polled in place: no code takes a pin
            // to a call or to one of its fields. So it is `Unpin` whatever its inputs hold,
            // and adding an input that is not `Unpin` never takes `Unpin` from a call.
            impl<E: Endpoint> ::core::marker::Unpin for Call<E> {}
            impl<E: Endpoint> ::core::clone::Clone for Call<E> {
                fn clone(&self) -> Self {
                    Self { __data: ::core::clone::Clone::clone(&self.__data), __options: ::core::clone::Clone::clone(&self.__options) }
                }
            }
            impl<E: Endpoint> ::core::fmt::Debug for Call<E> {
                fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut debug = ::core::fmt::Formatter::debug_struct(formatter, <E as Endpoint>::__DEBUG);
                    ::core::fmt::DebugStruct::field(&mut debug, "endpoint", &<E as Endpoint>::NAME);
                    ::core::fmt::DebugStruct::finish_non_exhaustive(&mut debug)
                }
            }
            impl<E: Endpoint> Call<E> {
                #[doc(hidden)]
                pub(super) fn __new(data: E::Data) -> Self {
                    Self { __data: data, __options: ::concord::__private::Options::default() }
                }
                #[doc(hidden)]
                pub(super) fn __data(&self) -> &E::Data { &self.__data }
                #[doc(hidden)]
                pub(super) fn __data_mut(&mut self) -> &mut E::Data { &mut self.__data }

                fn __build_request(&self, take: bool) -> Result<::concord::reqwest::Request> {
                    ::concord::__private::Draft::finish_request(
                        <E as Endpoint>::__build_draft(&self.__data)?,
                        <E as Endpoint>::__core(&self.__data), &self.__options, <E as Endpoint>::NAME, take,
                    )
                }

                fn __prepare_inner(mut self) -> Result<::concord::__private::Prepared<E::Value, __Problem>> {
                    let run = ::concord::__private::Run::start(<E as Endpoint>::__core(&self.__data), &self.__options, <E as Endpoint>::NAME);
                    let idempotent = <E as Endpoint>::__settings(&self.__data, &mut self.__options);
                    let draft = <E as Endpoint>::__build_draft(&self.__data)?;
                    #prepare_call(
                        draft, run, ::core::clone::Clone::clone(<E as Endpoint>::__core(&self.__data)),
                        ::core::clone::Clone::clone(&self.__options), idempotent, self,
                        |call, response, info| async move { <E as Endpoint>::__read_answer(&call.__data, response, info).await },
                    )
                }

                #retry
                #timeout

                #idle_doc
                pub fn with_idle_timeout(mut self, timeout: ::std::time::Duration) -> Self {
                    ::concord::__private::Options::with_idle_timeout(&mut self.__options, timeout); self
                }

                #read_doc
                pub fn read_response(&self, response: ::concord::reqwest::Response)
                    -> impl ::core::future::Future<Output = Result<E::Value>> + ::core::marker::Send + '_ {
                    async move {
                        ::core::result::Result::map(
                            <E as Endpoint>::__read_answer(&self.__data, response, ::concord::transport::TryInfo::new(<E as Endpoint>::NAME, 0)).await,
                            ::concord::Answer::into_value,
                        )
                    }
                }

                #request_doc
                pub fn request(&self) -> Result<::concord::reqwest::Request> { Self::__build_request(self, false) }

                #into_request_doc
                pub fn into_request(self) -> Result<::concord::reqwest::Request> { Self::__build_request(&self, true) }

                #edit_doc
                pub fn edit_request(mut self, edit: impl ::core::ops::Fn(&mut ::concord::reqwest::Request) + ::core::marker::Send + ::core::marker::Sync + 'static) -> Self {
                    ::concord::__private::Options::edit_request(&mut self.__options, edit); self
                }
            }

            #[doc(hidden)]
            impl<E: Endpoint> ::concord::__private::ConditionalCall for Call<E>
            where E::Conditional: ::concord::call::IsGetOrHead {
                fn set_if_none_match(&mut self, _: ::concord::__private::ConditionalAccess<Self>, etag: &::core::primitive::str) {
                    ::concord::__private::Options::if_none_match(&mut self.__options, etag);
                }
            }

            #[doc(hidden)]
            impl<E: Endpoint> ::concord::page::Call for Call<E> {
                type Value = E::Value;
                type Problem = #page_problem;
                fn at_url(mut self, url: &::core::primitive::str) -> Self {
                    ::concord::__private::Options::at_url(&mut self.__options, ::concord::__private::without_fragment(url)); self
                }
            }

            #[doc(hidden)]
            impl<E: Endpoint> ::concord::page::NamedCall for Call<E> {
                const ENDPOINT: &'static ::core::primitive::str = E::NAME;
            }

            #[doc(hidden)]
            impl<E: Endpoint> ::concord::__private::Call for Call<E> {
                fn request(&self) -> Result<::concord::reqwest::Request> { Self::__build_request(self, false) }
                fn prepare(self) -> Result<::concord::__private::Prepared<E::Value, __Problem>> { Self::__prepare_inner(self) }
                fn answered_from(mut self, url: ::concord::reqwest::Url) -> Self { ::concord::__private::Options::answered_from(&mut self.__options, url); self }
                fn edit_request(self, edit: impl ::core::ops::Fn(&mut ::concord::reqwest::Request) + ::core::marker::Send + ::core::marker::Sync + 'static) -> Self { Self::edit_request(self, edit) }
            }
        }
    }

    pub(super) fn send_methods(&self) -> TokenStream {
        let root = &self.root;
        let problem_alias = self.problem_alias();
        let mut declaration = TokenStream::new();
        let shown_problem = self.problem_type().map(|ty| {
            if problem_needs_alias(ty) {
                // Put the alias in the send methods' own block. An opaque macro
                // may expand to the alias name, so resolve it outside this block.
                let mut name = "__DeclaredProblem".to_owned();
                let authored = ty.to_token_stream();
                let expands = type_has_token(authored.clone(), &|token| {
                    matches!(token, proc_macro2::TokenTree::Punct(p) if matches!(p.as_char(), '!' | '#'))
                });
                let target = if expands {
                    problem_alias.clone()
                } else {
                    while type_has_token(authored.clone(), &|token| {
                        matches!(token, proc_macro2::TokenTree::Ident(id)
                            if id.to_string().trim_start_matches("r#") == name)
                    }) {
                        name.push('_');
                    }
                    authored
                };
                let alias = ident(&name);
                declaration = emit!(type #alias = #target;);
                emit!(#alias)
            } else {
                ty.to_token_stream()
            }
        });
        let problem = shown_problem.as_ref().map(|ty| emit!(, #ty));
        let signature_problem = shown_problem.clone().unwrap_or_else(|| emit!(()));
        let output_doc = format!(
            "Awaiting an unpaged call gives [`{}::Result<E::Value>`]({}::Result).",
            root, root
        );
        let send_doc = call_docs::send();
        let send_full_doc = call_docs::send_full();
        let send_raw_doc = call_docs::send_raw();
        let conditional_doc = call_docs::send_if_none_match();
        let pages_doc = call_docs::pages();
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
                    Self::__with_retry(self, retry)
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
                    <E as #root_at_client::Endpoint>::__pages(self, ())
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
                    <Self as ::concord::__private::ConditionalCall>::send_conditional(self, etag)
                }
        );
        let conditional_impl = quote::quote_spanned!(span=>
            impl<E: #root_at_client::Endpoint> #root_at_client::Call<E>
            where E::Conditional: ::concord::call::IsGetOrHead {
                #conditional_methods
            }
        );
        // One impl, in the author's scope, so that its page shows the author's types. It
        // carries no `#[diagnostic::…]` attribute: a module `diagnostic` of the author's, or
        // a client named `Diagnostic`, would capture that path.
        let into_future = emit! {
            #[doc = #output_doc]
            impl<E: #root::Endpoint> ::core::future::IntoFuture for #root::Call<E>
            where E::Paging: ::concord::call::AwaitWithoutPaginate {
                type Output = ::core::result::Result<E::Value, ::concord::Error<#signature_problem>>;
                type IntoFuture = ::concord::Sending<E::Value #problem>;
                fn into_future(self) -> Self::IntoFuture { #root::Call::<E>::send(self) }
            }
        };
        let methods = emit! {
            const _: () = {
            #retry_binding
            #[allow(unused_imports)]
            use ::core::mem::drop as etag;
            #conditional_impl
            impl<E: #root::Endpoint> #root::Call<E> {
                #send_doc
                pub fn send(self) -> ::concord::Sending<E::Value #problem> {
                    ::concord::Sending::new(async move {
                        ::core::result::Result::map(<Self as ::concord::__private::Call>::send_full(self).await, ::concord::Answer::into_value)
                    })
                }

                #send_full_doc
                pub fn send_full(self) -> ::concord::Sending<::concord::Answer<E::Value> #problem> {
                    <Self as ::concord::__private::Call>::send_full(self)
                }

                #send_raw_doc
                pub fn send_raw(self) -> ::concord::Sending<::concord::reqwest::Response #problem> {
                    <Self as ::concord::__private::Call>::send_raw(self)
                }

                #retry
            }

            #pages_impl
            };
            #into_future
        };
        if declaration.is_empty() {
            methods
        } else {
            emit!(const _: () = { #declaration #methods };)
        }
    }
}
