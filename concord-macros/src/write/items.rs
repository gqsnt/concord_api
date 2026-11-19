//! Public declarations and implementations at the author's scope.
use super::*;

impl Writer<'_> {
    pub(super) fn declarations(&self, parent: usize) -> TokenStream {
        let Some(parent_node) = self.nodes.get(parent) else {
            self.internal(self.client_span, "a declaration has no parent");
            return TokenStream::new();
        };
        let children = parent_node.children.iter().map(|&index| {
            let Some(node) = self.nodes.get(index) else {
                self.internal(self.client_span, "a declaration has an unknown child");
                return TokenStream::new();
            };
            let attrs = &node.node.attrs;
            let depth = node.modules.len();
            let vis = self.visibility(depth);
            let field_vis = Self::field_visibility(depth);
            let ty = Ident::new(
                &name::type_name(&node.node.name.to_string()),
                node.node.name.span(),
            );
            let supers = (1..depth).map(|_| ident("super")).collect::<Vec<_>>();
            let key = emit!(#(#supers::)* Call<::concord::__private::ClientKey>);
            let parent_ty = self.node_type(parent, key.clone());
            let fields = node.inputs.iter().map(|input| {
                let field = &input.input.name;
                let ty = stored(input.input, self.input_type_at(input, key.clone()));
                emit!(#field_vis #field: #ty,)
            });
            if matches!(node.node.kind, NodeKind::Endpoint(_)) {
                return self.endpoint_declaration(index);
            }
            let item_doc = format!("The handle `{}`.", node.node.name);
            let children = if matches!(node.node.kind, NodeKind::Scope) {
                let module = &node.node.name;
                let children = self.declarations(index);
                emit!(#(#attrs)* #[doc = #item_doc] #vis mod #module { #children })
            } else {
                TokenStream::new()
            };
            emit! {
                #(#attrs)* #[doc = #item_doc]
                #vis struct #ty { #field_vis __parent: #parent_ty, #(#fields)* }
                #children
            }
        });
        emit!(#(#children)*)
    }

    pub(super) fn implementation(&self, index: usize) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            self.internal(self.client_span, "an implementation has no node");
            return TokenStream::new();
        };
        let ty = &node.path;
        let cfg = &node.cfg;
        if matches!(node.node.kind, NodeKind::Endpoint(_)) {
            return self.endpoint_methods(index);
        }
        let setters=node.inputs.iter().filter(|i|i.input.optional.is_some()).map(|input| {
            let name=&input.input.name;
            let attrs:Vec<_>=input.input.attrs.iter().filter(|a|!a.path().is_ident("doc")).collect();
            let stored=stored(input.input, input.input.ty.to_token_stream());
            let docs=self.setter_doc(input.input);
            let argument=quote::quote_spanned!(input.input.ty.span()=> #name: impl ::concord::arg::Arg<#stored>);
            let allow=node.deprecated.then(||emit!(#[allow(deprecated)]));
            emit! {
                #(#attrs)* #docs #allow
                pub fn #name(mut self, #argument) -> Self {
                    self.#name=::concord::arg::Arg::into_input(#name); self
                }
            }
        }).collect::<TokenStream>();
        let children = node
            .children
            .iter()
            .map(|&child_index| {
                let Some(child) = self.nodes.get(child_index) else {
                    self.internal(self.client_span, "an implementation has an unknown child");
                    return TokenStream::new();
                };
                let name = &child.node.name;
                let attrs = &child.node.attrs;
                let path = &child.path;
                let docs = if let Some(endpoint) = self.endpoints.get(&(child.node as *const Node))
                {
                    self.endpoint_doc(child_index, endpoint)
                } else {
                    docs::lines(
                        std::iter::once("The handle owns its inputs. Nothing is sent yet.".into())
                            .chain(child.inputs.iter().flat_map(|i| {
                                std::iter::once(format!(
                                    "`{}`: {}",
                                    i.input.name,
                                    self.accepts(i.input, i.input.optional.is_some())
                                ))
                                .chain(docs::authored(&i.input.attrs))
                            })),
                    )
                };
                let args = child
                    .inputs
                    .iter()
                    .filter(|i| i.input.optional.is_none())
                    .map(|i| {
                        let name = &i.input.name;
                        let ty = &i.input.ty;
                        quote::quote_spanned!(i.input.ty.span()=> #name: impl ::concord::arg::Arg<#ty>)
                    });
                let value = |i: &flat::InputLine<'_>| {
                    let name = &i.input.name;
                    if i.input.optional.is_some() {
                        emit!(::core::option::Option::None)
                    } else {
                        emit!(::concord::arg::Arg::into_input(#name))
                    }
                };
                let fields = child.inputs.iter().map(|i| {
                    let name = &i.input.name;
                    let value = value(i);
                    emit!(#name: #value)
                });
                // The body only constructs our own item; it contains no authored expression.
                let allow = child.deprecated.then(|| emit!(#[allow(deprecated)]));
                let endpoint = matches!(child.node.kind, NodeKind::Endpoint(_));
                let return_type = if endpoint {
                    let root = &self.root;
                    emit!(#root::Call<#path>)
                } else {
                    path.clone()
                };
                let construct = if endpoint {
                    let root = &self.root;
                    let state = if self.tuple_data(child_index) {
                        let inputs = child.inputs.iter().map(value);
                        emit!(::concord::__private::CallData::new(::core::clone::Clone::clone(self), (#(#inputs,)*)))
                    } else {
                        let ty = self.data_type(child_index);
                        emit!({
                            type __State = #ty;
                            __State { __parent: ::core::clone::Clone::clone(self), #(#fields,)* }
                        })
                    };
                    emit!(#root::Call::__new(#state))
                } else {
                    emit!(#path { __parent: ::core::clone::Clone::clone(self), #(#fields,)* })
                };
                let must_use = self.endpoints.get(&(child.node as *const Node))
                    .map(|endpoint| docs::call_must_use(endpoint.pager.is_some()));
                emit! {
                    #(#attrs)* #docs #allow #must_use
                    pub fn #name(&self, #(#args),*) -> #return_type {
                        #construct
                    }
                }
            })
            .collect::<TokenStream>();
        let client = if index == 0 {
            self.constructor()
        } else {
            TokenStream::new()
        };
        let methods = emit!(#client #setters #children);
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        let methods = (!methods.is_empty()).then(|| emit!(#(#cfg)* #allow impl #ty { #methods }));
        let debug_name = if index == 0 {
            name::type_name(&node.node.name.to_string())
        } else {
            node.modules
                .iter()
                .map(ToString::to_string)
                .chain([name::type_name(&node.node.name.to_string())])
                .collect::<Vec<_>>()
                .join("::")
        };
        let clone_fields = node.inputs.iter().map(|input| {
            let field = &input.input.name;
            emit!(#field: ::core::clone::Clone::clone(&self.#field),)
        });
        let clone_state = if index == 0 {
            emit!(__core: ::core::clone::Clone::clone(&self.__core),)
        } else {
            emit!(__parent: ::core::clone::Clone::clone(&self.__parent),)
        };
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        emit! {
            #methods
            // Like a call, a client or a handle is never pinned in place: it is `Unpin`
            // whatever its inputs hold, so adding an input never takes `Unpin` from it.
            #(#cfg)* #allow impl ::core::marker::Unpin for #ty {}
            #(#cfg)* #allow impl ::core::clone::Clone for #ty {
                fn clone(&self) -> Self { Self { #clone_state #(#clone_fields)* } }
            }
            #(#cfg)* #allow impl ::core::fmt::Debug for #ty {
                fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    ::core::fmt::DebugStruct::finish_non_exhaustive(&mut ::core::fmt::Formatter::debug_struct(__f, #debug_name))
                }
            }
        }
    }

    fn constructor(&self) -> TokenStream {
        let Some(node) = self.nodes.first() else {
            self.internal(self.client_span, "a constructor has no client");
            return TokenStream::new();
        };
        let args = node
            .inputs
            .iter()
            .filter(|i| i.input.optional.is_none())
            .map(|i| {
                let name = &i.input.name;
                let ty = &i.input.ty;
                quote::quote_spanned!(i.input.ty.span()=> #name: impl ::concord::arg::Arg<#ty>)
            });
        let locals = node.inputs.iter().map(|i| {
            let name = &i.input.name;
            if i.input.optional.is_some() {
                emit!(let #name=::core::option::Option::None;)
            } else {
                emit!(let #name=::concord::arg::Arg::into_input(#name);)
            }
        });
        let fields = node.inputs.iter().map(|i| &i.input.name);
        let middleware = self.model.middleware.iter().map(|m| {
            let expr = m.value;
            let bindings = self.bindings(&m.origin, 0, emit!(__client), false);
            let value = emit_at!(expr.span()=> (|| {#expr})());
            let add = emit_at!(expr.span()=> ::concord::__private::Core::add_middleware(__client.__core, __middleware));
            let assignment = if node.deprecated {
                emit!(#[allow(deprecated)] { __client.__core = #add; })
            } else {
                emit!(__client.__core = #add;)
            };
            emit!({ #bindings let __middleware=#value; #assignment })
        });
        let input_docs = docs::lines(
            node.inputs
                .iter()
                .filter(|i| i.input.optional.is_none())
                .flat_map(|i| {
                    std::iter::once(format!(
                        "`{}`: {}",
                        i.input.name,
                        self.accepts(i.input, false)
                    ))
                    .chain(docs::authored(&i.input.attrs))
                }),
        );
        let setting = |name| Ident::new(name, self.client_span);
        let (new, retry, timeout, idle, base, transport, middleware_name) = (
            setting("new"),
            setting("with_retry"),
            setting("with_timeout"),
            setting("with_idle_timeout"),
            setting("with_base"),
            setting("with_transport"),
            setting("with_middleware"),
        );
        let problem = self.problem_type().map(|ty| emit!(<#ty>));
        let fields = fields.collect::<Vec<_>>();
        let construct = if node.deprecated {
            emit!({ #[allow(deprecated)] let __made = Self { __core: <::concord::__private::Core<_> as ::core::default::Default>::default(), #(#fields),* }; __made })
        } else {
            emit!(Self { __core: <::concord::__private::Core<_> as ::core::default::Default>::default(), #(#fields),* })
        };
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        emit_at! { self.client_span =>
            /// Create a client with the default transport and settings.
            #input_docs
            #[allow(clippy::new_without_default)]
            pub fn #new(#(#args),*) -> Self {
                #(#locals)*
                let mut __client=#construct;
                #(#middleware)* __client
            }
            /// The retry policy of this client. Accepts a [`concord::Retry`].
            #allow pub fn #retry(mut self, __retry: ::concord::Retry #problem) -> Self {
                self.__core=::concord::__private::Core::with_retry(self.__core, __retry); self
            }
            /// The longest a call may take. Accepts a [`std::time::Duration`].
            #allow pub fn #timeout(mut self, __timeout: ::std::time::Duration) -> Self {
                self.__core=::concord::__private::Core::with_timeout(self.__core, __timeout); self
            }
            /// The longest a transport or body read may be silent. Accepts a [`std::time::Duration`].
            #allow pub fn #idle(mut self, __timeout: ::std::time::Duration) -> Self {
                self.__core=::concord::__private::Core::with_idle_timeout(self.__core, __timeout); self
            }
            /// Replace the base URL. Accepts a [`concord::Base`] or text through [`concord::base::IntoBase`].
            #allow pub fn #base(mut self, __base: impl ::concord::base::IntoBase) -> Self {
                self.__core=::concord::__private::Core::with_base(self.__core, __base); self
            }
            /// Choose the [`concord::Transport`] beneath this client's middleware.
            #allow pub fn #transport(mut self, __transport: impl ::concord::Transport) -> Self {
                self.__core=::concord::__private::Core::with_transport(self.__core, __transport); self
            }
            /// Add a [`concord::Middleware`] before this client's middleware.
            #allow pub fn #middleware_name(mut self, __middleware: impl ::concord::Middleware) -> Self {
                self.__core=::concord::__private::Core::with_middleware(self.__core, __middleware); self
            }
        }
    }
}
