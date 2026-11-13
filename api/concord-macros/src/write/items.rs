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
            let ty = Ident::new(&name::type_name(&node.node.name.to_string()), node.node.name.span());
            let supers = (1..depth).map(|_| ident("super")).collect::<Vec<_>>();
            let parent_id = ident(&format!("__Node{parent}"));
            let parent_ty = emit!(<() as #(#supers::)* __Types>::#parent_id);
            let fields = node.inputs.iter().map(|input| {
                let field = &input.input.name;
                let id = self.input_id(input);
                emit!(#field_vis #field: <() as #(#supers::)* __Types>::#id,)
            });
            let endpoint = self.endpoints.get(&(node.node as *const Node));
            let options = endpoint.map(|_| emit!(
                #field_vis __options: ::concord::__private::Options<<() as #(#supers::)* __Types>::__Problem>,
            ));
            let kind = if endpoint.is_some() { "call" } else { "handle" };
            let item_doc = format!("The {kind} `{}`.",node.node.name);
            let children = if matches!(node.node.kind, NodeKind::Scope) {
                let module = &node.node.name;
                let children = self.declarations(index);
                emit!(#(#attrs)* #[doc = #item_doc] #vis mod #module { #children })
            } else { TokenStream::new() };
            let must_use = endpoint.map(|e| if e.pager.is_some() {
                emit!(#[must_use = ".send() gives one page; .pages() gives them all"])
            } else { emit!(#[must_use = "a call does nothing until sent or awaited"]) });
            emit! {
                #(#attrs)* #[doc = #item_doc] #must_use
                #vis struct #ty { #field_vis __parent: #parent_ty, #(#fields)* #options }
                #children
            }
        });
        let slots = (parent == 0).then(|| self.write_slots());
        emit!(#(#children)* #slots)
    }

    fn write_slots(&self) -> TokenStream {
        let vis = emit!(pub(super));
        let vis = &vis;
        let slots = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                self.endpoints
                    .get(&(n.node as *const Node))
                    .is_some_and(|e| e.pager.is_some())
            })
            .flat_map(|(index, node)| {
                node.inputs.iter().map(move |input| {
                    let slot = self.slot(index, input);
                    let cfg = &node.cfg;
                    emit! {
                        #(#cfg)* #[doc(hidden)]
                        #[derive(::core::clone::Clone, ::core::marker::Copy)]
                        #vis struct #slot;
                    }
                })
            });
        emit!(#(#slots)*)
    }

    pub(super) fn implementation(&self, index: usize) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            self.internal(self.client_span, "an implementation has no node");
            return TokenStream::new();
        };
        let ty = &node.implementation;
        let cfg = &node.cfg;
        let setters=node.inputs.iter().filter(|i|i.input.optional.is_some()).map(|input| {
            let name=&input.input.name;
            let attrs:Vec<_>=input.input.attrs.iter().filter(|a|!a.path().is_ident("doc")).collect();
            let stored=stored(input.input);
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
                        quote::quote_spanned!(ty.span()=> #name: impl ::concord::arg::Arg<#ty>)
                    });
                let fields = child.inputs.iter().map(|i| {
                    let name = &i.input.name;
                    if i.input.optional.is_some() {
                        emit!(#name: ::core::option::Option::None)
                    } else {
                        emit!(#name: ::concord::arg::Arg::into_input(#name))
                    }
                });
                let options = matches!(child.node.kind, NodeKind::Endpoint(_))
                    .then(|| emit!(__options: ::concord::__private::Options::default(),));
                // The body only constructs our own item; it contains no authored expression.
                let allow = child.deprecated.then(|| emit!(#[allow(deprecated)]));
                emit! {
                    #(#attrs)* #docs #allow
                    pub fn #name(&self, #(#args),*) -> #path {
                        #path { __parent: ::core::clone::Clone::clone(self), #(#fields,)* #options }
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
        let methods = (!methods.is_empty()).then(|| emit!(#(#cfg)* impl #ty { #methods }));
        let endpoint = self.endpoints.get(&(node.node as *const Node));
        let endpoint_methods = endpoint.map(|e| self.endpoint(index, e));
        let debug_name = if index == 0 || endpoint.is_some() {
            name::type_name(&node.node.name.to_string())
        } else {
            node.modules
                .iter()
                .map(ToString::to_string)
                .chain(std::iter::once(name::type_name(
                    &node.node.name.to_string(),
                )))
                .collect::<Vec<_>>()
                .join("::")
        };
        let endpoint_field = endpoint.map(|e| {
            let path = &e.path;
            emit!(.field("endpoint",&#path))
        });
        let root = &self.root;
        let clone_fields = node.inputs.iter().map(|input| {
            let field = &input.input.name;
            let clone = self.clone_id(input);
            emit!(#field: <() as #root::__Types>::#clone(&self.#field),)
        });
        let clone_state = if index == 0 {
            emit!(__core: ::core::clone::Clone::clone(&self.__core),)
        } else {
            emit!(__parent: ::core::clone::Clone::clone(&self.__parent),)
        };
        let clone_options =
            endpoint.map(|_| emit!(__options: ::core::clone::Clone::clone(&self.__options),));
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        emit! {
            #methods
            #(#cfg)* #allow impl ::core::clone::Clone for #ty {
                fn clone(&self) -> Self { Self { #clone_state #(#clone_fields)* #clone_options } }
            }
            #(#cfg)* impl ::core::fmt::Debug for #ty {
                fn fmt(&self, __f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    __f.debug_struct(#debug_name) #endpoint_field .finish_non_exhaustive()
                }
            }
            #endpoint_methods
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
                quote::quote_spanned!(ty.span()=> #name: impl ::concord::arg::Arg<#ty>)
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
            let add = emit_at!(expr.span()=> __client.__core.add_middleware(__middleware));
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
            emit!({ #[allow(deprecated)] let __made = Self { __core: ::concord::__private::Core::default(), #(#fields),* }; __made })
        } else {
            emit!(Self { __core: ::concord::__private::Core::default(), #(#fields),* })
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
                self.__core=self.__core.with_retry(__retry); self
            }
            /// The longest a call may take. Accepts a [`std::time::Duration`].
            #allow pub fn #timeout(mut self, __timeout: ::std::time::Duration) -> Self {
                self.__core=self.__core.with_timeout(__timeout); self
            }
            /// The longest a transport or body read may be silent. Accepts a [`std::time::Duration`].
            #allow pub fn #idle(mut self, __timeout: ::std::time::Duration) -> Self {
                self.__core=self.__core.with_idle_timeout(__timeout); self
            }
            /// Replace the base URL. Accepts a [`concord::Base`] or text through [`concord::base::IntoBase`].
            #allow pub fn #base(mut self, __base: impl ::concord::base::IntoBase) -> Self {
                self.__core=self.__core.with_base(__base); self
            }
            /// Choose the [`concord::Transport`] beneath this client's middleware.
            #allow pub fn #transport(mut self, __transport: impl ::concord::Transport) -> Self {
                self.__core=self.__core.with_transport(__transport); self
            }
            /// Add a [`concord::Middleware`] before this client's middleware.
            #allow pub fn #middleware_name(mut self, __middleware: impl ::concord::Middleware) -> Self {
                self.__core=self.__core.with_middleware(__middleware); self
            }
        }
    }
}
