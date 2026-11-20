//! Generate declarations at tree paths and evaluate authored Rust at the invocation site.
use proc_macro2::{Ident, Span, TokenStream};
use quote::ToTokens;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};
use syn::spanned::Spanned;

use crate::{ast::*, flat, name};

const MAX_TUPLE_INPUTS: usize = 12;

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
mod call_docs;
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

fn stored(input: &Input, ty: TokenStream) -> TokenStream {
    if input.optional.is_some() {
        emit!(::core::option::Option<#ty>)
    } else {
        ty
    }
}

/// Plain paths, literals and calls can only use an owner through its bare name.
/// Keep the full lexical environment for other forms, including macro invocations.
fn without_owner(expr: &syn::Expr, owners: &[flat::Owner<'_>]) -> bool {
    match expr {
        syn::Expr::Path(path) => {
            path.attrs.is_empty()
                && path.qself.is_none()
                && path
                    .path
                    .segments
                    .iter()
                    .all(|part| part.arguments.is_none())
                && path.path.get_ident().is_none_or(|ident| {
                    let name = ident.to_string();
                    owners.iter().all(|owner| {
                        name.trim_start_matches("r#")
                            != flat::owner_name(owner.node).trim_start_matches("r#")
                    })
                })
        }
        syn::Expr::Call(call) => {
            call.attrs.is_empty()
                && without_owner(&call.func, owners)
                && call.args.iter().all(|arg| without_owner(arg, owners))
        }
        syn::Expr::Lit(literal) => literal.attrs.is_empty(),
        syn::Expr::Paren(paren) => paren.attrs.is_empty() && without_owner(&paren.expr, owners),
        syn::Expr::Group(group) => group.attrs.is_empty() && without_owner(&group.expr, owners),
        _ => false,
    }
}

pub(crate) fn check_client_type_names(client: &Client, diagnostics: &mut crate::diag::Diagnostics) {
    for child in &client.node.children {
        if child.parse_failed || child.name != "endpoint" {
            continue;
        }
        let mut message = crate::diag::reserved_child("endpoint", "endpoint_item")
            .label("reserved for the generated endpoint trait");
        if client
            .node
            .children
            .iter()
            .any(|other| name::type_name(&other.name.to_string()) == "EndpointItem")
        {
            message = message.help("choose a distinct child name and update every use");
        }
        diagnostics.raise(child.name.span(), message);
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
    deprecated: bool,
}

impl NodeInfo<'_> {
    fn endpoint_attrs(&self) -> TokenStream {
        let cfg = &self.cfg;
        let attrs = self.node.attrs.iter().filter(|attr| {
            attr.path().is_ident("deprecated")
                || (attr.path().is_ident("doc") && matches!(attr.meta, syn::Meta::List(_)))
        });
        emit!(#(#cfg)* #(#attrs)*)
    }

    fn endpoint_impl_attrs(&self) -> TokenStream {
        let attrs = self.endpoint_attrs();
        let allow = self
            .node
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("deprecated"))
            .then(|| emit!(#[allow(useless_deprecated)]));
        emit!(#allow #attrs)
    }
}

struct Writer<'a> {
    nodes: Vec<NodeInfo<'a>>,
    root: Ident,
    node_indices: HashMap<*const Node, usize>,
    visibility: &'a syn::Visibility,
    model: &'a flat::Client<'a>,
    input_ids: HashMap<(*const Node, *const Input), usize>,
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
        node_indices: HashMap::new(),
        visibility: &client.node.visibility,
        model,
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
        self.node_indices.insert(node, index);
        let deprecated = node.attrs.iter().any(|a| a.path().is_ident("deprecated"))
            || parent
                .and_then(|i| self.nodes.get(i))
                .is_some_and(|n| n.deprecated);
        if parent.is_some() {
            self.generated_names
                .insert(name::type_name(&node.name.to_string()));
        }
        if !matches!(node.kind, NodeKind::Endpoint(_)) {
            for input in &inputs {
                self.input_ids
                    .insert((input.owner, input.input), self.next_input);
                self.next_input += 1;
            }
        }
        self.nodes.push(NodeInfo {
            node,
            parent,
            path,
            modules: modules.clone(),
            cfg: cfg.clone(),
            inputs,
            children: Vec::new(),
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

    fn input_id(&self, input: &flat::InputLine<'_>) -> usize {
        let found = self.input_ids.get(&(input.owner, input.input)).copied();
        if found.is_none() {
            self.internal(input.input.span, "an input has no storage slot");
        }
        found.unwrap_or_default()
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

    fn input_position(&self, index: usize, input: &flat::InputLine<'_>) -> Option<usize> {
        self.nodes.get(index)?.inputs.iter().position(|line| {
            std::ptr::eq(line.owner, input.owner) && std::ptr::eq(line.input, input.input)
        })
    }

    fn tuple_data(&self, index: usize) -> bool {
        self.nodes.get(index).is_some_and(|node| {
            matches!(node.node.kind, NodeKind::Endpoint(_)) && node.inputs.len() <= MAX_TUPLE_INPUTS
        })
    }

    fn data_type(&self, index: usize) -> TokenStream {
        let key = self.client_key();
        // Wide endpoint state has its own position beyond inputs and markers.
        let id = self.node_id(self.nodes.len() + index);
        emit!(<#key as ::concord::__private::EndpointType<#id>>::Value)
    }

    // A specialization of the existing Call type is local to this client,
    // even when the description was written inside a function. It carries
    // types across the module boundary without exporting a helper name.
    fn client_key(&self) -> TokenStream {
        let root = &self.root;
        emit!(#root::Call<::concord::__private::ClientKey>)
    }

    fn input_type_at(&self, input: &flat::InputLine<'_>, key: TokenStream) -> TokenStream {
        let id = self.input_id(input);
        emit!(<#key as ::concord::__private::StoredType<#id>>::Value)
    }

    fn input_type(&self, input: &flat::InputLine<'_>) -> TokenStream {
        self.input_type_at(input, self.client_key())
    }

    fn node_id(&self, index: usize) -> usize {
        // Use a separate range from input indices on the same trait.
        self.next_input + index
    }

    fn node_type(&self, index: usize, key: TokenStream) -> TokenStream {
        let id = self.node_id(index);
        emit!(<#key as ::concord::__private::StoredType<#id>>::Value)
    }

    fn endpoint_marker(&self, index: usize, node: &NodeInfo<'_>) -> TokenStream {
        if node.deprecated {
            let key = self.client_key();
            let id = self.node_id(index);
            emit!(<#key as ::concord::__private::EndpointType<#id>>::Value)
        } else {
            node.path.clone()
        }
    }

    fn problem_alias(&self) -> TokenStream {
        let key = self.client_key();
        self.at_problem(emit!(<#key as ::concord::__private::ClientProblem>::Problem))
    }

    fn problem_span(&self) -> Span {
        self.problem_type().map_or(self.client_span, Spanned::span)
    }

    fn at_problem(&self, tokens: TokenStream) -> TokenStream {
        if let Some(ty) = self.problem_type() {
            located(tokens, ty.span())
        } else {
            tokens
        }
    }

    fn input_field(
        &self,
        current: usize,
        input: &flat::InputLine<'_>,
        base: TokenStream,
    ) -> TokenStream {
        let access = self.access(current, input.owner, base);
        if self.tuple_data(current) {
            if let Some(position) = self.input_position(current, input) {
                let position = syn::Index::from(position);
                return emit!(#access.__inputs.#position);
            }
        }
        let field = &input.input.name;
        emit!(#access.#field)
    }

    fn slot_value(&self, index: usize, input: &flat::InputLine<'_>) -> TokenStream {
        let key = self.client_key();
        let Some((ty, position)) = self.slot_arguments(index, input) else {
            return TokenStream::new();
        };
        emit!(::concord::__private::InputSlot::<#ty, #position, #key>::NEW)
    }

    fn slot_type(&self, index: usize, input: &flat::InputLine<'_>) -> TokenStream {
        let key = self.client_key();
        let Some((ty, position)) = self.slot_arguments(index, input) else {
            return TokenStream::new();
        };
        emit!(::concord::__private::InputSlot<#ty, #position, #key>)
    }

    fn slot_arguments(
        &self,
        index: usize,
        input: &flat::InputLine<'_>,
    ) -> Option<(TokenStream, usize)> {
        let node = self.nodes.get(index)?;
        let position = self.input_position(index, input)?;
        let root = &self.root;
        let marker = self.endpoint_marker(index, node);
        Some((emit!(#root::Call<#marker>), position))
    }

    fn core(&self, current: usize) -> TokenStream {
        let Some(root) = self.nodes.first() else {
            return TokenStream::new();
        };
        let access = self.access(current, root.node, emit!(__value));
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
        self.bindings_at(origin, current, base, slots, None)
    }

    fn bindings_at(
        &self,
        origin: &flat::Origin<'_>,
        current: usize,
        base: TokenStream,
        slots: bool,
        span: impl Into<Option<Span>>,
    ) -> TokenStream {
        self.write_bindings(origin, current, base, slots, span.into(), &origin.owners)
    }

    fn expression_bindings(
        &self,
        origin: &flat::Origin<'_>,
        current: usize,
        expression: &syn::Expr,
        span: Option<Span>,
    ) -> TokenStream {
        let owners = if without_owner(expression, &origin.owners) {
            &[][..]
        } else {
            &origin.owners
        };
        self.write_bindings(origin, current, emit!(__value), false, span, owners)
    }

    fn write_bindings(
        &self,
        origin: &flat::Origin<'_>,
        current: usize,
        base: TokenStream,
        slots: bool,
        span: Option<Span>,
        owners: &[flat::Owner<'_>],
    ) -> TokenStream {
        let at_expression = |tokens: TokenStream| match span {
            Some(span) => located(tokens, span),
            None => tokens,
        };
        let key = self.client_key();
        let owners = owners.iter().map(|owner| {
            let Some(index) = self.node_indices.get(&(owner.node as *const Node)) else {
                self.internal(owner.node.name.span(), "an owner has no view");
                return TokenStream::new();
            };
            let binding = if matches!(owner.node.kind, NodeKind::Client) {
                Ident::new("client", owner.node.name.span())
            } else {
                owner.node.name.clone()
            };
            let fields = owner.inputs.iter().map(|input| {
                let access = self.input_field(current, input, base.clone());
                self.borrow_field(current, access, false)
            });
            // The import lets the binding shadow a constant or unit struct. Keep its
            // span narrow so rustc's unused-import pass needn't read the whole description.
            let shadow = emit_at!(owner.node.name.span()=>
                #[allow(unused_imports)] use ::core::mem::drop as #binding;
            );
            let view = at_expression(emit!(
                let #binding = <#key as ::concord::__private::OwnerView<#index>>::View::new((#(#fields,)*));
            ));
            emit!(#shadow #view)
        });
        let locals = origin.locals.iter().map(|input| {
            let field = &input.input.name;
            let binding = if slots {
                let slot = self.slot_value(current, input);
                emit!(let #field = #slot;)
            } else {
                let access = self.input_field(current, input, base.clone());
                let reference = self.borrow_field(current, access, false);
                emit!(let #field = #reference;)
            };
            at_expression(binding)
        });
        emit!(#(#owners)* #(#locals)*)
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
        let key = self.client_key();
        let stored_inputs = self
            .nodes
            .iter()
            .filter(|node| !matches!(node.node.kind, NodeKind::Endpoint(_)))
            .flat_map(|node| {
                let key = &key;
                let cfg = &node.cfg;
                node.inputs.iter().map(move |input| {
                    let id = self.input_id(input);
                    let ty = &input.input.ty;
                    emit_at!(ty.span()=>
                        #(#cfg)* #[doc(hidden)] impl ::concord::__private::StoredType<#id> for #key {
                            type Value = #ty;
                        }
                    )
                })
            });
        let node_type_impls = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !matches!(node.node.kind, NodeKind::Endpoint(_)))
            .map(|(index, node)| {
                let id = self.node_id(index);
                let cfg = &node.cfg;
                let ty = &node.path;
                let allow = node.deprecated.then(|| emit!(#[allow(deprecated)]));
                emit!(#(#cfg)* #allow #[doc(hidden)] impl ::concord::__private::StoredType<#id> for #key {
                    type Value = #ty;
                })
            });
        let endpoint_types = self.nodes.iter().enumerate()
            .filter(|(_, node)| node.deprecated && matches!(node.node.kind, NodeKind::Endpoint(_)))
            .map(|(index, node)| {
                let id = self.node_id(index);
                let cfg = &node.cfg;
                let ty = &node.path;
                emit!(#(#cfg)* #[allow(deprecated)] #[doc(hidden)] impl ::concord::__private::EndpointType<#id> for #key {
                    type Value = #ty;
                })
            });
        let problem = self
            .problem_type()
            .map_or_else(|| emit!(()), ToTokens::to_token_stream);
        let problem_impl = emit_at!(self.problem_span()=>
            #[doc(hidden)] impl ::concord::__private::ClientProblem for #key { type Problem = #problem; }
        );
        let problem_alias = self.problem_alias();
        let fields = client.inputs.iter().map(|input| {
            let field = &input.input.name;
            let ty = stored(input.input, self.input_type(input));
            emit!(#field: #ty,)
        });
        let tree = self.declarations(0);
        let call_declarations = self.call_declarations();
        let call_implementations = self.call_implementations();
        let send_methods = self.send_methods();
        let implementations = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, _)| self.implementation(index));
        let views = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !matches!(node.node.kind, NodeKind::Endpoint(_)))
            .map(|(index, node)| {
                let cfg = &node.cfg;
                let fields = node
                    .inputs
                    .iter()
                    .map(|input| &input.input.name)
                    .collect::<Vec<_>>();
                let types = node
                    .inputs
                    .iter()
                    .map(|input| {
                        let ty = stored(input.input, self.input_type(input));
                        emit!(&'__a #ty)
                    })
                    .collect::<Vec<_>>();
                let lifetime = (!fields.is_empty()).then(|| emit!(<'__a>));
                let fields_by_position = fields.iter().enumerate().map(|(index, field)| {
                    let position = syn::Index::from(index);
                    emit!(#field: __fields.#position,)
                });
                let shadow = emit_at!(node.node.name.span()=>
                    #[allow(unused_imports)] use ::core::mem::drop as __fields;
                );
                emit!(#(#cfg)* const _: () = {
                    #[doc(hidden)]
                    pub struct __View #lifetime { #(pub #fields: #types,)* }
                    #shadow
                    impl #lifetime __View #lifetime {
                        #[doc(hidden)]
                        pub fn new(__fields: (#(#types,)*)) -> Self { Self { #(#fields_by_position)* } }
                    }
                    #[doc(hidden)] impl ::concord::__private::OwnerView<#index> for #key {
                        type View<'__a> = __View #lifetime;
                    }
                };)
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
            emit!(::concord::Error<__Problem>)
        } else {
            emit!(::concord::Error)
        };
        let retry_type = if self.has_problem() {
            emit!(::concord::Retry<__Problem>)
        } else {
            emit!(::concord::Retry)
        };
        let retry_doc = format!(
            "The retry policy for this client: [`concord::Retry`]{}.",
            problem_doc
                .as_ref()
                .map_or(String::new(), |p| format!("<{p}>"))
        );
        // rustc may infer a diverging type for a sibling while it recovers from
        // the description's primary error. Keep that inference lint out of the
        // recovered shell so the primary diagnostics stay readable.
        // PANIC-SAFETY: This only silences a type-inference lint on invalid input.
        let recovering = (!self.refused.is_empty())
            .then(|| emit!(#[allow(dependency_on_unit_never_type_fallback)]));
        emit! {
            #(#attrs)* #client_doc
            #vis struct #client_name {
                __core: ::concord::__private::Core<#problem_alias>,
                #(#fields)*
            }
            #(#cfg)*
            #[doc = #module_doc]
            #vis mod #root {
                type __Problem = <Call<::concord::__private::ClientKey> as ::concord::__private::ClientProblem>::Problem;
                #[doc = #error_doc]
                #module_vis type Error = #error_type;
                /// The result of a call, with this client's error.
                #module_vis type Result<T> = ::core::result::Result<T, Error>;
                #[doc = #retry_doc]
                #module_vis type Retry = #retry_type;
                #call_declarations
                #call_implementations
                #tree
            }
            #(#cfg)*
            #recovering
            #[allow(unused_variables, unused_mut, unused_braces, dead_code, clippy::redundant_closure_call)]
            const _: () = {
                #(#node_type_impls)*
                #(#stored_inputs)*
                #(#endpoint_types)*
                #(#views)*
                #problem_impl
                #send_methods
                #(#implementations)*
            };

        }
    }
}
