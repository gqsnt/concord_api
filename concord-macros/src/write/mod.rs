//! Generate declarations at tree paths and evaluate authored Rust at the invocation site.
use proc_macro2::{Ident, Span, TokenStream};
use quote::ToTokens;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use syn::spanned::Spanned;

use crate::{ast::*, flat, name};

// Only syntax written here receives mixed-site hygiene. Interpolated author
// expressions and promised bindings retain their original spans.
macro_rules! emit {
    ($($tokens:tt)*) => { ::quote::quote_spanned!(::proc_macro2::Span::mixed_site()=> $($tokens)*) };
}
macro_rules! emit_at {
    ($span:expr => $($tokens:tt)*) => {
        ::quote::quote_spanned!(::proc_macro2::Span::mixed_site().located_at($span)=> $($tokens)*)
    };
}

mod answer;
mod call;
mod docs;
mod items;
mod recovery;
mod request;
pub(crate) use recovery::sound_client;

fn ident(text: &str) -> Ident {
    Ident::new(text, Span::mixed_site())
}

/// Relocate generated bindings without changing their hygiene contexts. No
/// authored expression is passed here.
fn located(tokens: TokenStream, span: Span) -> TokenStream {
    tokens
        .into_iter()
        .map(|mut token| {
            if let proc_macro2::TokenTree::Group(group) = &token {
                let mut next =
                    proc_macro2::Group::new(group.delimiter(), located(group.stream(), span));
                next.set_span(group.span().located_at(span));
                token = proc_macro2::TokenTree::Group(next);
            } else {
                token.set_span(token.span().located_at(span));
            }
            token
        })
        .collect()
}

fn method(method: &Method) -> String {
    method
        .custom
        .as_ref()
        .map_or_else(|| method.keyword.to_string(), syn::LitStr::value)
}

fn stored(input: &Input) -> TokenStream {
    let ty = &input.ty;
    if input.optional.is_some() {
        emit!(::core::option::Option<#ty>)
    } else {
        emit!(#ty)
    }
}

struct NodeInfo<'a> {
    node: &'a Node,
    parent: Option<usize>,
    path: TokenStream,
    modules: Vec<Ident>,
    cfg: Vec<syn::Attribute>,
    inputs: Vec<flat::InputLine<'a>>,
    children: Vec<usize>,
    implementation: TokenStream,
    deprecated: bool,
}

struct Writer<'a> {
    nodes: Vec<NodeInfo<'a>>,
    root: Ident,
    visibility: &'a syn::Visibility,
    model: &'a flat::Client<'a>,
    node_indices: HashMap<*const Node, usize>,
    input_ids: HashMap<(*const Node, *const Input), (Ident, Ident)>,
    levels: HashMap<*const Node, &'a flat::Level<'a>>,
    endpoints: HashMap<*const Node, &'a flat::Endpoint<'a>>,
    generated_names: HashSet<String>,
    errors: RefCell<Option<syn::Error>>,
    next_input: usize,
    refused: HashSet<*const Node>,
    client_span: Span,
}

/// Write an already checked client without inspecting authored Rust semantics.
pub(crate) fn client<'a>(client: &'a Client, model: &'a flat::Client<'a>) -> TokenStream {
    recovered(client, model, HashSet::new())
}

pub(crate) fn recovered<'a>(
    client: &'a Client,
    model: &'a flat::Client<'a>,
    refused: HashSet<*const Node>,
) -> TokenStream {
    let root = ident(&name::module_name(&client.node.name.to_string()));
    let mut writer = Writer {
        nodes: Vec::new(),
        root,
        visibility: &client.node.visibility,
        model,
        node_indices: HashMap::new(),
        input_ids: HashMap::new(),
        levels: model
            .levels
            .iter()
            .map(|l| (l.node as *const Node, l))
            .collect(),
        endpoints: model
            .endpoints
            .iter()
            .map(|e| (e.node as *const Node, e))
            .collect(),
        generated_names: HashSet::new(),
        errors: RefCell::new(None),
        next_input: 0,
        refused,
        client_span: client.node.name.span(),
    };
    writer.collect(&client.node, None, Vec::new(), Vec::new());
    let output = writer.write();
    writer
        .errors
        .into_inner()
        .map_or(output, |error| error.into_compile_error())
}

impl<'a> Writer<'a> {
    fn internal(&self, span: Span, text: &str) {
        let error = syn::Error::new(span, format!("internal Concord macro error: {text}"));
        let mut errors = self.errors.borrow_mut();
        if let Some(errors) = errors.as_mut() {
            errors.combine(error);
        } else {
            *errors = Some(error);
        }
    }
    fn collect(
        &mut self,
        node: &'a Node,
        parent: Option<usize>,
        modules: Vec<Ident>,
        mut cfg: Vec<syn::Attribute>,
    ) {
        if parent.is_some() && (node.parse_failed || self.refused.contains(&(node as *const Node)))
        {
            return;
        }
        cfg.extend(
            node.attrs
                .iter()
                .filter(|a| a.path().is_ident("cfg"))
                .cloned(),
        );
        let ty = if parent.is_none() {
            node.name.clone()
        } else {
            ident(&name::type_name(&node.name.to_string()))
        };
        let path = emit!(#(#modules::)* #ty);
        let Some(level) = self.levels.get(&(node as *const Node)) else {
            self.internal(node.name.span(), "a node has no resolved level");
            return;
        };
        let inputs = level.inputs.clone();
        let index = self.nodes.len();
        let deprecated = node.attrs.iter().any(|a| a.path().is_ident("deprecated"))
            || parent
                .and_then(|i| self.nodes.get(i))
                .is_some_and(|n| n.deprecated);
        let implementation = if deprecated {
            let alias = ident(&format!("__Impl{index}"));
            emit!(#alias)
        } else {
            path.clone()
        };
        self.node_indices.insert(node, index);
        if parent.is_some() {
            self.generated_names
                .insert(name::type_name(&node.name.to_string()));
        }
        for input in &inputs {
            let id = ident(&format!("__Input{}", self.next_input));
            let clone = ident(&format!("__clone_input{}", self.next_input));
            self.next_input += 1;
            self.input_ids
                .insert((input.owner, input.input), (id, clone));
        }
        self.nodes.push(NodeInfo {
            node,
            parent,
            path,
            modules: modules.clone(),
            cfg: cfg.clone(),
            inputs,
            children: Vec::new(),
            implementation,
            deprecated,
        });
        if let Some(parent) = parent {
            if let Some(parent) = self.nodes.get_mut(parent) {
                parent.children.push(index);
            }
        }
        let mut children_modules = modules;
        children_modules.push(if parent.is_none() {
            self.root.clone()
        } else {
            node.name.clone()
        });
        for child in &node.children {
            self.collect(child, Some(index), children_modules.clone(), cfg.clone());
        }
    }

    fn index(&self, node: &Node) -> Option<usize> {
        let found = self.node_indices.get(&(node as *const Node)).copied();
        if found.is_none() {
            self.internal(
                node.name.span(),
                "an expression owner has no generated node",
            );
        }
        found
    }

    fn input_id(&self, input: &flat::InputLine<'_>) -> Option<Ident> {
        let found = self
            .input_ids
            .get(&(input.owner, input.input))
            .map(|p| p.0.clone());
        if found.is_none() {
            self.internal(input.input.span, "an input has no storage slot");
        }
        found
    }

    fn clone_id(&self, input: &flat::InputLine<'_>) -> Option<Ident> {
        let found = self
            .input_ids
            .get(&(input.owner, input.input))
            .map(|p| p.1.clone());
        if found.is_none() {
            self.internal(input.input.span, "an input has no clone helper");
        }
        found
    }

    fn access(&self, current: usize, owner: &Node, base: TokenStream) -> TokenStream {
        let mut index = current;
        let mut access = base;
        loop {
            let Some(node) = self.nodes.get(index) else {
                self.internal(owner.name.span(), "an input access has no node");
                return TokenStream::new();
            };
            if std::ptr::eq(node.node, owner) {
                break;
            }
            let Some(parent) = node.parent else {
                self.internal(owner.name.span(), "an input owner is not an ancestor");
                return TokenStream::new();
            };
            access = emit!(#access.__parent);
            index = parent;
        }
        access
    }

    fn core(&self, current: usize) -> TokenStream {
        let Some(root) = self.nodes.first() else {
            return TokenStream::new();
        };
        let access = self.access(current, root.node, emit!(self));
        let field = emit!(#access.__core);
        if self.nodes.get(current).is_some_and(|n| n.deprecated) {
            let value = self.borrow_field(current, field, false);
            emit!(*#value)
        } else {
            field
        }
    }

    /// Only this generated field read is allowed to use our deprecated item.
    /// An authored expression is never put under this attribute.
    fn borrow_field(&self, current: usize, field: TokenStream, mutable: bool) -> TokenStream {
        let reference = if mutable {
            emit!(&mut #field)
        } else {
            emit!(&#field)
        };
        if self.nodes.get(current).is_some_and(|n| n.deprecated) {
            emit!({ #[allow(deprecated)] let __borrowed_field=#reference; __borrowed_field })
        } else {
            reference
        }
    }

    fn bindings(
        &self,
        origin: &flat::Origin<'_>,
        current: usize,
        base: TokenStream,
        slots: bool,
    ) -> TokenStream {
        let owners = origin.owners.iter().map(|owner| {
            let Some(owner_index) = self.index(owner.node) else {
                return TokenStream::new();
            };
            let view = ident(&format!("__View{owner_index}"));
            let binding = if matches!(owner.node.kind, NodeKind::Client) {
                Ident::new("client", owner.node.name.span())
            } else {
                owner.node.name.clone()
            };
            let fields = owner.inputs.iter().map(|input| {
                let field = &input.input.name;
                let access = self.access(current, input.owner, base.clone());
                let reference = self.borrow_field(current, emit!(#access.#field), false);
                emit!(#field: #reference)
            });
            if owner.inputs.is_empty() {
                emit!(let #binding = #view;)
            } else {
                emit!(let #binding = #view { #(#fields),* };)
            }
        });
        let locals = origin.locals.iter().map(|input| {
            let field = &input.input.name;
            if slots {
                let slot = self.slot(current, input);
                let root = &self.root;
                emit!(let #field = #root::#slot;)
            } else {
                let access = self.access(current, input.owner, base.clone());
                let reference = self.borrow_field(current, emit!(#access.#field), false);
                emit!(let #field = #reference;)
            }
        });
        emit!(#(#owners)* #(#locals)*)
    }

    fn bindings_at(
        &self,
        origin: &flat::Origin<'_>,
        current: usize,
        base: TokenStream,
        slots: bool,
        span: Span,
    ) -> TokenStream {
        located(self.bindings(origin, current, base, slots), span)
    }

    fn slot(&self, current: usize, input: &flat::InputLine<'_>) -> Option<Ident> {
        Some(ident(&format!("__Slot{current}{}", self.input_id(input)?)))
    }

    fn visibility(&self, depth: usize) -> TokenStream {
        match self.visibility {
            syn::Visibility::Inherited if depth > 0 => Self::field_visibility(depth),
            syn::Visibility::Restricted(restricted) => {
                let Some(first) = restricted.path.segments.first() else {
                    return restricted.to_token_stream();
                };
                if first.ident == "crate" {
                    return restricted.to_token_stream();
                }
                if depth == 0 {
                    return restricted.to_token_stream();
                }
                let supers = (0..depth).map(|_| ident("super"));
                let tail = restricted
                    .path
                    .segments
                    .iter()
                    .filter(|segment| segment.ident != "self");
                let tail = tail.collect::<Vec<_>>();
                if tail.is_empty() {
                    emit!(pub(in #(#supers)::*))
                } else {
                    emit!(pub(in #(#supers)::* :: #(#tail)::*))
                }
            }
            visibility => visibility.to_token_stream(),
        }
    }

    fn field_visibility(depth: usize) -> TokenStream {
        if depth == 0 {
            return TokenStream::new();
        }
        let supers = (0..depth).map(|_| ident("super"));
        emit!(pub(in #(#supers)::*))
    }

    fn write(&self) -> TokenStream {
        let root = &self.root;
        let Some(client) = self.nodes.first() else {
            return TokenStream::new();
        };
        let client_name = &client.node.name;
        let cfg = &client.cfg;
        let attrs = &client.node.attrs;
        let vis = self.visibility;
        let module_vis = self.visibility(1);
        let all_inputs = self
            .nodes
            .iter()
            .flat_map(|n| n.inputs.iter().map(move |i| (n, i)))
            .collect::<Vec<_>>();
        let associated = all_inputs.iter().map(|(n, input)| {
            let id = self.input_id(input);
            let cfg = &n.cfg;
            let declaration = quote::quote_spanned!(input.input.ty.span()=>
                type #id: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static;
            );
            emit!(#(#cfg)* #declaration)
        });
        let cloners = all_inputs.iter().map(|(n, input)| {
            let id = self.input_id(input);
            let clone = self.clone_id(input);
            let cfg = &n.cfg;
            emit! {
                #(#cfg)* fn #clone(__value: &Self::#id) -> Self::#id {
                    ::core::clone::Clone::clone(__value)
                }
            }
        });
        let node_types = self.nodes.iter().enumerate().filter(|(_, n)| !matches!(n.node.kind, NodeKind::Endpoint(_))).map(|(i, n)| {
            let id = ident(&format!("__Node{i}")); let cfg = &n.cfg;
            emit!(#(#cfg)* type #id: ::core::clone::Clone + ::core::marker::Send + ::core::marker::Sync + 'static;)
        });
        let node_type_impls = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !matches!(n.node.kind, NodeKind::Endpoint(_)))
            .map(|(i, n)| {
                let id = ident(&format!("__Node{i}"));
                let cfg = &n.cfg;
                let ty = &n.path;
                let allow = n.deprecated.then(|| emit!(#[allow(deprecated)]));
                emit!(#(#cfg)* #allow type #id = #ty;)
            });
        let associated_impl = all_inputs.iter().map(|(n, input)| {
            let id = self.input_id(input);
            let ty = stored(input.input);
            let cfg = &n.cfg;
            emit!(#(#cfg)* type #id = #ty;)
        });
        let problem = self
            .problem_type()
            .map(|ty| emit!(#ty))
            .unwrap_or_else(|| emit!(()));
        let problem_bound = quote::quote_spanned!(self.problem_type().map_or(self.client_span,Spanned::span)=>
            type __Problem: ::core::marker::Send + ::core::marker::Sync + 'static;
        );
        let fields = client.inputs.iter().map(|input| {
            let field = &input.input.name;
            let id = self.input_id(input);
            emit!(#field: <() as #root::__Types>::#id,)
        });
        let tree = self.declarations(0);
        let views = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !matches!(n.node.kind, NodeKind::Endpoint(_)))
            .map(|(index, node)| {
                let view = ident(&format!("__View{index}"));
                let cfg = &node.cfg;
                let fields = node.inputs.iter().map(|input| {
                    let name = &input.input.name;
                    let ty = stored(input.input);
                    emit!(#name: &'__a #ty)
                });
                if node.inputs.is_empty() {
                    emit!(#(#cfg)* struct #view;)
                } else {
                    emit!(#(#cfg)* struct #view<'__a> { #(#fields),* })
                }
            });
        let implementations = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, _)| self.implementation(index));
        let aliases = self.nodes.iter().filter(|n| n.deprecated).map(|node| {
            let alias = &node.implementation;
            let path = &node.path;
            let cfg = &node.cfg;
            emit!(#(#cfg)* #[allow(deprecated)] type #alias = #path;)
        });
        let client_doc = self.client_doc();
        let module_doc = format!("Calls and results for [`{client_name}`].");
        let problem_doc = self.problem_type().map(|ty| self.alias_type_doc(ty));
        let error_doc = format!(
            "The error of each call: [`concord::Error`]{}.",
            problem_doc
                .as_ref()
                .map_or(String::new(), |p| format!("<{p}>"))
        );
        let error_type = if self.has_problem() {
            emit!(::concord::Error<<() as __Types>::__Problem>)
        } else {
            emit!(::concord::Error)
        };
        let retry_type = if self.has_problem() {
            emit!(::concord::Retry<<() as __Types>::__Problem>)
        } else {
            emit!(::concord::Retry)
        };
        let retry_doc = format!(
            "The retry policy for this client: [`concord::Retry`]{}.",
            problem_doc
                .as_ref()
                .map_or(String::new(), |p| format!("<{p}>"))
        );
        emit! {
            #(#attrs)* #client_doc
            #vis struct #client_name {
                __core: ::concord::__private::Core<<() as #root::__Types>::__Problem>,
                #(#fields)*
            }
            #(#cfg)*
            #[doc = #module_doc]
            #vis mod #root {
                #[doc(hidden)]
                #module_vis trait __Types {
                    #(#associated)* #(#node_types)*
                    #problem_bound
                    #(#cloners)*
                }
                #[doc = #error_doc]
                #module_vis type Error = #error_type;
                /// The result of a call, with this client's error.
                #module_vis type Result<T> = ::core::result::Result<T, Error>;
                #[doc = #retry_doc]
                #module_vis type Retry = #retry_type;
                #tree
            }
            #(#cfg)*
            #[allow(unused_variables, unused_mut, unused_braces, dead_code, clippy::redundant_closure_call)]
            const _: () = {
                #[allow(unused_imports)] use ::concord::__private::AdaptValue as _;
                impl #root::__Types for () { #(#node_type_impls)* #(#associated_impl)* type __Problem = #problem; }
                #(#aliases)*
                #(#views)*
                #(#implementations)*
            };
        }
    }

    fn endpoint(&self, index: usize, endpoint: &flat::Endpoint<'_>) -> TokenStream {
        let Some(node) = self.nodes.get(index) else {
            return TokenStream::new();
        };
        let ty = &node.implementation;
        let cfg = &node.cfg;
        let root = &self.root;
        let output = endpoint
            .node
            .output
            .as_ref()
            .map(|o| o.ty.to_token_stream())
            .unwrap_or_else(|| emit!(()));
        let core = self.core(index);
        let path = &endpoint.path;
        let request = self.request(index, endpoint);
        let answer = self.answer(index, endpoint);
        let shown_problem = self.problem_type().map(|ty| emit!(, #ty));
        let paging = endpoint.pager.as_ref().map(|pager| {
            let expression = pager.value;
            let bindings = self.bindings_at(&pager.origin, index, emit!(__owners), true, expression.span());
            let make=emit_at!(expression.span()=> __pager(&self, || { #bindings #expression }));
            let call=emit_at!(expression.span()=> ::concord::__private::pages(self, __pager));
            emit_at!(expression.span()=>
                /// `.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.
                pub fn pages(self) -> ::concord::Pages<#output #shown_problem> {
                    fn __pager<__C, __N, __F>(_: &__C, __make: impl ::core::ops::FnOnce() -> __F) -> __F
                    where __C: ::concord::page::Call,
                        __N: ::concord::page::IntoNext<__C>,
                        __F: ::core::ops::FnMut(__C, &::concord::Answer<__C::Value>) -> __N
                            + ::core::marker::Send + 'static,
                    { __make() }
                    let __owners = ::core::clone::Clone::clone(&self);
                    let __pager = #make;
                    #call
                }
            )
        });
        let slots = endpoint.pager.as_ref().map(|_| {
            let slots = node.inputs.iter().map(|input| {
                let slot = self.slot(index, input);
                let field = &input.input.name;
                let input_type = &input.input.ty;
                let get = if input.input.optional.is_some() { emit!(__call.#field.as_ref()) } else { emit!(::core::option::Option::Some(&__call.#field)) };
                let set = if input.input.optional.is_some() { emit!(::core::option::Option::Some(__value)) } else { emit!(__value) };
                let allow=node.deprecated.then(||emit!(#[allow(deprecated)]));
                emit!(#(#cfg)* #allow #[doc(hidden)] impl ::concord::page::Slot<#ty> for #root::#slot {
                    type Value = #input_type;
                    fn get<'__a>(&self, __call: &'__a #ty) -> ::core::option::Option<&'__a Self::Value> { #get }
                    fn set(&self, mut __call: #ty, __value: Self::Value) -> #ty { __call.#field = #set; __call }
                })
            });
            emit!(#(#slots)*)
        });
        let idle_timeout = endpoint.idle_timeout.as_ref().map(|line| {
            let bindings = self.bindings(&line.origin, index, emit!(self), false);
            let expression = line.value;
            let options = self.borrow_field(index, emit!(self.__options), true);
            emit! {
                let __idle_timeout = { #bindings (|| -> ::std::time::Duration { #expression })() };
                (#options).description_idle_timeout(__idle_timeout);
            }
        });
        let mutable = endpoint.idle_timeout.is_some().then(|| emit!(mut));
        let idempotent = match &endpoint.idempotent {
            Some(line) => match line.value {
                Some(expression) => {
                    let bindings = self.bindings(&line.origin, index, emit!(self), false);
                    emit_at!(expression.span()=> { #bindings ::concord::__private::idempotent((|| { #expression })()) })
                }
                None => emit!(::concord::__private::Idempotent::Yes),
            },
            None => emit!(::concord::__private::Idempotent::ByMethod),
        };
        let common = self.call_methods(ty, &output, endpoint, paging.as_ref());
        let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
        let problem = self
            .problem_type()
            .map(|ty| emit!(#ty))
            .unwrap_or_else(|| emit!(()));
        let options = self.borrow_field(index, emit!(self.__options), false);
        emit! {
            #(#cfg)* impl #ty {
                const __ENDPOINT: &'static str = #path;
                #request
                #answer
                fn __prepare_inner(#mutable self) -> #root::Result<::concord::__private::Prepared<#output, <() as #root::__Types>::__Problem>> {
                    let __run = ::concord::__private::Run::start(&#core, #options, Self::__ENDPOINT);
                     #idle_timeout
                    let __idempotent = #idempotent;
                    let __draft = self.__build_draft()?;
                    __draft.prepare::<#output, #problem, _, _>(
                        __run, ::core::clone::Clone::clone(&#core),
                        ::core::clone::Clone::clone(#options), __idempotent,
                        move |__response, __info| {
                            let __call = ::core::clone::Clone::clone(&self);
                            async move { __call.__read_answer(__response, __info).await }
                        },
                    )
                }
            }
            #(#cfg)* #allow const _: () = { #common };
            #slots
        }
    }
}
