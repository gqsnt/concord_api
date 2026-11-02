//! Rust code generation from a resolved [`Model`].

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Ident, LitStr};

use crate::model::{
    Body, Composed, ContribId, Item, Kind, Model, Owner, Piece, Slot, Source, Usage, Value,
    ValueRef,
};
use crate::syntax::Family;

pub fn emit(model: &Model, krate: TokenStream) -> TokenStream {
    let k = &krate;
    let vis = match &model.owners[0].kind {
        Kind::Client { vis, .. } => vis.clone(),
        _ => unreachable!("the first owner is the client"),
    };
    let mut out = TokenStream::new();
    for index in 0..model.owners.len() {
        let cfgs = &model.owners[index].cfgs;
        if cfgs.is_empty() {
            out.extend(emit_owner(k, model, index, &vis));
        } else {
            // Every item of a `#[cfg]` owner carries its own and its ancestors' `cfg`s.
            out.extend(with_attrs(emit_owner(k, model, index, &vis), cfgs));
        }
    }
    out
}

/// Puts `attrs` in front of every top-level item in `items`.
fn with_attrs(items: TokenStream, attrs: &[syn::Attribute]) -> TokenStream {
    let file: syn::File = syn::parse2(items).expect("generated items parse");
    let mut out = TokenStream::new();
    for item in file.items {
        out.extend(quote!(#(#attrs)* #item));
    }
    out
}

/// For each value of `owner`: notes for its controls when it feeds a conditional `override`
/// (its absence keeps what the chain inherits).
fn override_notes(model: &Model, owner: usize) -> Vec<Vec<String>> {
    use crate::syntax::Op;
    let mut notes = vec![Vec::new(); model.owners[owner].values.len()];
    for o in &model.owners {
        for (family, list) in [(Family::Query, &o.query), (Family::Header, &o.headers)] {
            for c in list.iter().filter(|c| c.op == Op::Override) {
                let reads: Vec<ValueRef> = match &c.source {
                    Some(Source::Ref { value, .. }) => vec![*value],
                    Some(Source::Template { pieces }) => pieces
                        .iter()
                        .filter_map(|p| match p {
                            Piece::Hole(r) => Some(*r),
                            Piece::Lit(_) => None,
                        })
                        .collect(),
                    None => Vec::new(),
                };
                let what = match family {
                    Family::Query => "query parameter",
                    Family::Header => "header",
                };
                for r in reads.into_iter().filter(|r| r.owner == owner) {
                    if model.value(r).optional {
                        notes[r.index].push(format!(
                            " When `{}` is absent, the inherited `{}` {what} is sent unchanged (`override ... else inherit` in `{}`).",
                            plain(&model.value(r).name),
                            c.key,
                            o.dotted
                        ));
                    }
                }
            }
        }
    }
    notes
}

fn lit(s: &str) -> LitStr {
    LitStr::new(s, Span::call_site())
}

fn plain(ident: &Ident) -> String {
    let s = ident.to_string();
    s.strip_prefix("r#").map(str::to_owned).unwrap_or(s)
}

fn field_type(value: &Value) -> TokenStream {
    let ty = &value.ty;
    if value.optional {
        quote!(::core::option::Option<#ty>)
    } else {
        quote!(#ty)
    }
}

/// The payload type a body is supplied as (`body b: T via R` → `T`).
fn body_value_type(body: &Body) -> TokenStream {
    let ty = &body.ty;
    quote!(#ty)
}

/// The struct literal fields initializing an owner's values, in declaration order, with default
/// functions that cannot capture the entry method's arguments or `self`.
fn value_inits(k: &TokenStream, values: &[Value]) -> (TokenStream, TokenStream) {
    let mut fns = TokenStream::new();
    let mut fields = TokenStream::new();
    for value in values {
        let name = &value.name;
        let ty = &value.ty;
        let init = match &value.default {
            Some(expr) => {
                let f = format_ident!("__concord_default_{}", plain(name));
                fns.extend(quote! {
                    #[allow(non_snake_case)]
                    #[inline]
                    fn #f() -> #ty { <_ as #k::IntoValue<#ty>>::into_value(#expr) }
                });
                if value.optional {
                    quote!(::core::option::Option::Some(#f()))
                } else {
                    quote!(#f())
                }
            }
            None if value.optional => quote!(::core::option::Option::None),
            None => quote!(#k::IntoValue::into_value(#name)),
        };
        fields.extend(quote!(#name: #init,));
    }
    (fns, fields)
}

fn positional_params(k: &TokenStream, values: &[Value], body: Option<&Body>) -> TokenStream {
    // Required values and a required body, in declaration order.
    let body = body.filter(|b| !b.optional);
    let body_param = |b: &Body| {
        let name = &b.name;
        let ty = body_value_type(b);
        quote!(#name: impl #k::IntoValue<#ty>,)
    };
    let mut params = TokenStream::new();
    for (i, value) in values.iter().enumerate() {
        if let Some(b) = body.filter(|b| b.position == i) {
            params.extend(body_param(b));
        }
        if value.is_positional() {
            let name = &value.name;
            let ty = &value.ty;
            params.extend(quote!(#name: impl #k::IntoValue<#ty>,));
        }
    }
    if let Some(b) = body.filter(|b| b.position >= values.len()) {
        params.extend(body_param(b));
    }
    params
}

/// Where an owner's required values are supplied, for generated documentation.
fn supplied_by(owner: &str) -> String {
    format!("supplied when {owner} is created")
}

/// One line describing a value's starting state, for generated documentation.
fn initially(value: &Value, owner: &str) -> String {
    match (&value.default, value.optional) {
        (Some(expr), _) => format!(
            " Initially `{}`, evaluated once when {owner} is created.",
            quote!(#expr)
        ),
        (None, true) => " Initially absent.".to_owned(),
        (None, false) => format!(" Required: {}.", supplied_by(owner)),
    }
}

/// `set_x` for every value and body; `maybe_x` for optional ones. `owner` names the receiver in
/// docs ("the client", "this scope", "this request").
fn controls(
    k: &TokenStream,
    values: &[Value],
    body: Option<&Body>,
    target: &TokenStream,
    owner: &str,
    notes: &[Vec<String>],
) -> TokenStream {
    let mut out = TokenStream::new();
    for (value, notes) in values.iter().zip(notes) {
        let notes: Vec<LitStr> = notes.iter().map(|n| lit(n)).collect();
        let name = &value.name;
        let ty = &value.ty;
        let docs = &value.docs;
        let shown = plain(name);
        let set = format_ident!("set_{}", shown);
        let set_doc = lit(&if value.optional {
            format!(" Sets `{shown}` (present).")
        } else {
            format!(" Replaces `{shown}`.")
        });
        let initially = lit(&initially(value, owner));
        let assign = if value.optional {
            quote!(::core::option::Option::Some(
                #k::IntoValue::into_value(value)
            ))
        } else {
            quote!(#k::IntoValue::into_value(value))
        };
        out.extend(quote! {
            #(#docs)*
            #[doc = ""]
            #[doc = #set_doc]
            #[doc = #initially]
            #(#[doc = #notes])*
            #[inline] pub fn #set(mut self, value: impl #k::IntoValue<#ty>) -> Self {
                #target.#name = #assign;
                self
            }
        });
        if value.optional {
            let maybe = format_ident!("maybe_{}", shown);
            let maybe_doc = lit(&format!(
                " Sets whether `{shown}` is present: `Some(v)` is `set_{shown}(v)`; `None` makes it \
                 absent{}.",
                if value.default.is_some() {
                    " (the initial value is not used)"
                } else {
                    ""
                }
            ));
            out.extend(quote! {
                #(#docs)*
                #[doc = ""]
                #[doc = #maybe_doc]
                #[doc = #initially]
                #(#[doc = #notes])*
                #[inline] pub fn #maybe(mut self, value: ::core::option::Option<#ty>) -> Self {
                    #target.#name = value;
                    self
                }
            });
        }
    }
    if let Some(body) = body {
        let name = &body.name;
        let ty = body_value_type(body);
        let docs = &body.docs;
        let set = format_ident!("set_{}", plain(name));
        if body.optional {
            let maybe = format_ident!("maybe_{}", plain(name));
            out.extend(quote! {
                #(#docs)*
                #[doc = ""]
                #[doc = " Sets the request body. Initially absent: no body is sent."]
                #[inline] pub fn #set(mut self, value: impl #k::IntoValue<#ty>) -> Self {
                    #target.#name = ::core::option::Option::Some(#k::IntoValue::into_value(value));
                    self
                }
                #(#docs)*
                #[doc = ""]
                #[doc = " Sets the request body to exactly `value`: `None` sends no body."]
                #[inline] pub fn #maybe(mut self, value: ::core::option::Option<#ty>) -> Self {
                    #target.#name = value;
                    self
                }
            });
        } else {
            out.extend(quote! {
                #(#docs)*
                #[doc = ""]
                #[doc = " Replaces the request body supplied when this request was created."]
                #[inline] pub fn #set(mut self, value: impl #k::IntoValue<#ty>) -> Self {
                    #target.#name = #k::IntoValue::into_value(value);
                    self
                }
            });
        }
    }
    out
}

/// `tokens` with every span moved to `at`'s location, keeping each token's own hygiene, so a
/// trait-bound error in generated code points at the declaration that caused it.
fn located(tokens: TokenStream, at: Span) -> TokenStream {
    tokens
        .into_iter()
        .map(|tt| match tt {
            proc_macro2::TokenTree::Group(g) => {
                let mut group = proc_macro2::Group::new(g.delimiter(), located(g.stream(), at));
                group.set_span(g.span().located_at(at));
                proc_macro2::TokenTree::Group(group)
            }
            mut tt => {
                tt.set_span(tt.span().located_at(at));
                tt
            }
        })
        .collect()
}

/// How build errors name a query parameter or header.
fn placement_label(family: Family, key: &str) -> LitStr {
    lit(&match family {
        Family::Query => format!("query parameter `{key}`"),
        Family::Header => format!("header `{key}`"),
    })
}

/// A rustdoc list of an entry call's positional arguments with their documentation.
fn argument_docs(values: &[Value], body: Option<&Body>) -> TokenStream {
    let mut lines = Vec::new();
    let mut item = |name: &Ident, docs: &[syn::Attribute], what: &str| {
        let text = doc_text(docs);
        let text = if text.is_empty() {
            what.to_owned()
        } else {
            text
        };
        lines.push(lit(&format!(" - `{}`: {text}", plain(name))));
    };
    let required_body = body.filter(|b| !b.optional);
    for (i, value) in values.iter().enumerate() {
        if let Some(b) = required_body.filter(|b| b.position == i) {
            item(&b.name, &b.docs, "the request body.");
        }
        if value.is_positional() {
            item(&value.name, &value.docs, "required value.");
        }
    }
    if let Some(b) = required_body.filter(|b| b.position >= values.len()) {
        item(&b.name, &b.docs, "the request body.");
    }
    if lines.is_empty() {
        return quote!();
    }
    quote!(#[doc = ""] #[doc = " Arguments:"] #(#[doc = #lines])*)
}

/// The text of `///` doc attributes, joined into one line.
fn doc_text(docs: &[syn::Attribute]) -> String {
    let mut out = Vec::new();
    for attr in docs {
        if let syn::Meta::NameValue(nv) = &attr.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                let line = s.value();
                let line = line.trim();
                if !line.is_empty() {
                    out.push(line.to_owned());
                }
            }
        }
    }
    out.join(" ")
}

fn debug_fields<'a>(
    k: &TokenStream,
    values: impl IntoIterator<Item = &'a Value>,
    base: &TokenStream,
) -> TokenStream {
    let mut out = TokenStream::new();
    for value in values {
        let name = &value.name;
        let label = lit(&plain(name));
        let place = quote!(#base.#name);
        let span = value.ty.span();
        let shown = match (value.debug_sensitive, value.optional, value.usage) {
            (true, false, _) => quote_spanned!(span=> &#k::__private::Redacted),
            (true, true, _) => {
                quote_spanned!(span=> &#place.as_ref().map(|_| #k::__private::Redacted))
            }
            (false, false, Some(Usage::Each)) => {
                quote_spanned!(span=> &#k::__private::DebugEach(&#place))
            }
            (false, true, Some(Usage::Each)) => {
                quote_spanned!(span=> &#place.as_ref().map(#k::__private::DebugEach))
            }
            (false, false, _) => {
                quote_spanned!(span=> &#k::__private::DebugDisplay(&#place))
            }
            (false, true, _) => {
                quote_spanned!(span=> &#place.as_ref().map(#k::__private::DebugDisplay))
            }
        };
        // One statement per field (not a method chain): thousands of values must not nest.
        out.extend(located(quote!(__d.field(#label, #shown);), span));
    }
    out
}

fn emit_owner(k: &TokenStream, model: &Model, index: usize, vis: &syn::Visibility) -> TokenStream {
    let owner = &model.owners[index];
    let ty = &owner.type_name;
    let state = &owner.state_name;
    let docs = owner_docs(owner);
    let fields: Vec<TokenStream> = owner
        .values
        .iter()
        .map(|v| {
            let name = &v.name;
            let fty = field_type(v);
            quote!(#name: #fty,)
        })
        .collect();
    let nav = navigation(k, model, index);
    let type_label = lit(&ty.to_string());

    match &owner.kind {
        Kind::Client { base, .. } => {
            let (default_fns, inits) = value_inits(k, &owner.values);
            let controls = controls(
                k,
                &owner.values,
                None,
                &quote!(::std::sync::Arc::make_mut(&mut self.__s)),
                "the client",
                &override_notes(model, index),
            );
            let debug = debug_fields(k, &owner.values, &quote!(__s));
            let required: Vec<&Value> = owner.values.iter().filter(|v| v.is_positional()).collect();
            let (config_item, config_param, config_bind) = if required.is_empty() {
                (quote!(), quote!(), quote!())
            } else {
                let config = owner.config_name();
                let names: Vec<&Ident> = required.iter().map(|v| &v.name).collect();
                let config_fields = required.iter().map(|v| {
                    let name = &v.name;
                    let fty = &v.ty;
                    let docs = if v.docs.is_empty() {
                        let line = lit(&format!(" The client's required value `{}`.", plain(name)));
                        quote!(#[doc = #line])
                    } else {
                        let docs = &v.docs;
                        quote!(#(#docs)*)
                    };
                    quote!(#docs pub #name: #fty,)
                });
                let config_doc = lit(&format!(
                    " The required values of [`{ty}`], passed to [`{ty}::new`]. Optional and \
                     initialized values are set afterwards with the client's `set_*` and \
                     `maybe_*` methods."
                ));
                let config_label = lit(&config.to_string());
                let config_debug = debug_fields(k, required.iter().copied(), &quote!(self));
                (
                    quote! {
                        #[doc = #config_doc]
                        #[derive(Clone)]
                        #vis struct #config {
                            #(#config_fields)*
                        }

                        impl ::core::fmt::Debug for #config {
                            #[inline] fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                                let mut __d = f.debug_struct(#config_label);
                                #config_debug
                                __d.finish()
                            }
                        }
                    },
                    quote!(, config: #config),
                    quote!(let #config { #(#names),* } = config;),
                )
            };
            quote! {
                #config_item

                #docs
                #[derive(Clone)]
                #vis struct #ty {
                    __s: ::std::sync::Arc<#state>,
                }

                #[doc(hidden)]
                #[derive(Clone)]
                #[allow(non_camel_case_types)]
                struct #state {
                    __http: #k::reqwest::Client,
                    __base: #k::__private::Base,
                    #(#fields)*
                }

                impl #ty {
                    /// Creates the client. `http` is used for every request; configure timeouts,
                    /// redirects, proxies, TLS and default headers on it. Builds no connection.
                    /// Initializers of the client's values are evaluated here, once.
                    #[inline] pub fn new(http: #k::reqwest::Client #config_param) -> Self {
                        #config_bind
                        #default_fns
                        Self {
                            __s: ::std::sync::Arc::new(#state {
                                __http: http,
                                __base: #k::__private::Base::from_static(#base),
                                #inits
                            }),
                        }
                    }

                    /// Replaces the declared base URL (for example with a staging server or a
                    /// local test server). It must be an `http`/`https` URL with a host and no
                    /// user information, query or fragment.
                    #[inline] pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>) -> ::core::result::Result<Self, #k::Error> {
                        let base = #k::__private::Base::parse(url.as_ref())?;
                        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
                        ::core::result::Result::Ok(self)
                    }

                    #controls
                    #nav
                }

                impl ::core::fmt::Debug for #ty {
                    #[inline] fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        let __s = &*self.__s;
                        let mut __d = f.debug_struct(#type_label);
                        __d.field("base", &__s.__base);
                        #debug
                        __d.finish()
                    }
                }
            }
        }
        Kind::Scope => {
            let parent_ty = &model.owners[owner.parent.unwrap()].type_name;
            let controls = controls(
                k,
                &owner.values,
                None,
                &quote!(::std::sync::Arc::make_mut(&mut self.__s)),
                "this scope",
                &override_notes(model, index),
            );
            let debug = debug_fields(k, &owner.values, &quote!(__s));
            quote! {
                #docs
                #[derive(Clone)]
                #vis struct #ty {
                    __s: ::std::sync::Arc<#state>,
                }

                #[doc(hidden)]
                #[derive(Clone)]
                #[allow(non_camel_case_types)]
                struct #state {
                    __parent: #parent_ty,
                    #(#fields)*
                }

                impl #ty {
                    #controls
                    #nav
                }

                impl ::core::fmt::Debug for #ty {
                    #[inline] fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        let __s = &*self.__s;
                        let mut __d = f.debug_struct(#type_label);
                        #debug
                        __d.finish()
                    }
                }
            }
        }
        Kind::Endpoint {
            method,
            output,
            body,
        } => {
            let parent_ty = &model.owners[owner.parent.unwrap()].type_name;
            let controls = controls(
                k,
                &owner.values,
                body.as_ref(),
                &quote!(self),
                "this request",
                &override_notes(model, index),
            );
            let mut debug = debug_fields(k, &owner.values, &quote!(self));
            let body_field = body.as_ref().map(|b| {
                let name = &b.name;
                let vty = body_value_type(b);
                let label = lit(&plain(name));
                if b.optional {
                    debug.extend(quote!(__d.field(#label, &self.#name.as_ref().map(|_| ".."));));
                    quote!(#name: ::core::option::Option<#vty>,)
                } else {
                    debug.extend(quote!(__d.field(#label, &"..");));
                    quote!(#name: #vty,)
                }
            });
            let clone_impl = endpoint_clone(ty, &owner.values, body.as_ref());
            let out = match output {
                Some(p) => {
                    let t = &p.ty;
                    quote!(#t)
                }
                None => quote!(()),
            };
            let prepare = prepare_body(k, model, index, method, output.as_ref(), body.as_ref());
            quote! {
                #docs
                #[must_use = "an endpoint request does nothing until `send` or `prepare` is called"]
                #vis struct #ty {
                    __parent: #parent_ty,
                    #(#fields)*
                    #body_field
                }

                #clone_impl

                impl #ty {
                    #controls

                    /// Builds the native request without sending it.
                    #[inline] pub fn prepare(self) -> ::core::result::Result<#k::Prepared<#out>, #k::Error> {
                        #prepare
                    }

                    /// Sends the request once and decodes a 2xx response.
                    #[inline] pub async fn send(self) -> ::core::result::Result<#out, #k::Error> {
                        self.prepare()?.send().await
                    }
                }

                impl ::core::fmt::Debug for #ty {
                    #[inline] fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        let mut __d = f.debug_struct(#type_label);
                        #debug
                        __d.finish()
                    }
                }
            }
        }
    }
}

/// `Clone` for an endpoint request exactly when every value type (and the body value) is `Clone`.
///
/// The type is not generic, so a plain `where Value: Clone` bound would be rejected on stable
/// (trivially false bound, rust-lang/rust#48214). Each bound carries an unused higher-ranked
/// lifetime instead: such bounds are not checked where the impl is declared, so the impl simply
/// does not apply when a value is not `Clone`, and a `.clone()` call reports the unmet bound.
fn endpoint_clone(ty: &Ident, values: &[Value], body: Option<&Body>) -> TokenStream {
    let mut seen = std::collections::HashSet::new();
    let mut bounds = Vec::new();
    let mut add = |t: TokenStream, span: Span| {
        if seen.insert(t.to_string()) {
            bounds.push(quote_spanned!(span=> for<'__concord> #t: ::core::clone::Clone));
        }
    };
    for value in values {
        let t = &value.ty;
        add(quote!(#t), value.ty.span());
    }
    if let Some(b) = body {
        add(body_value_type(b), b.ty.span());
    }
    let mut fields: Vec<&Ident> = values.iter().map(|v| &v.name).collect();
    fields.extend(body.map(|b| &b.name));
    quote! {
        impl ::core::clone::Clone for #ty where #(#bounds,)* {
            #[inline]
            fn clone(&self) -> Self {
                Self {
                    __parent: ::core::clone::Clone::clone(&self.__parent),
                    #(#fields: ::core::clone::Clone::clone(&self.#fields),)*
                }
            }
        }
    }
}

/// The owner's documentation, or a generated line when it has none.
fn owner_docs(owner: &Owner) -> TokenStream {
    if !owner.docs.is_empty() {
        let docs = &owner.docs;
        return quote!(#(#docs)*);
    }
    let line = match &owner.kind {
        Kind::Client { .. } => format!(
            " Client for `{}`, generated by `concord::api!`.",
            owner.dotted
        ),
        Kind::Scope => format!(" Scope `{}`.", owner.dotted),
        Kind::Endpoint { method, .. } => format!(" `{method}` endpoint `{}`.", owner.dotted),
    };
    let line = lit(&line);
    quote!(#[doc = #line])
}

/// Navigation methods on `index` for each of its children.
fn navigation(k: &TokenStream, model: &Model, index: usize) -> TokenStream {
    let mut out = TokenStream::new();
    for &child in &model.owners[index].children {
        let c = &model.owners[child];
        let name = &c.name;
        let cty = &c.type_name;
        let cstate = &c.state_name;
        let docs = &c.docs;
        let body = c.body();
        let params = positional_params(k, &c.values, body);
        let (default_fns, inits) = value_inits(k, &c.values);
        let scope_line = lit(&format!(" Enters scope `{}`.", c.dotted));
        let args = argument_docs(&c.values, body);
        // `#[cfg]` and `#[deprecated]` of a child apply to its navigation method.
        let nav_attrs: Vec<&syn::Attribute> =
            c.own_cfgs.iter().chain(c.deprecated.iter()).collect();
        match &c.kind {
            Kind::Scope => out.extend(quote! {
                #(#docs)*
                #[doc = #scope_line]
                #args
                #(#nav_attrs)*
                #[inline] #[allow(clippy::too_many_arguments)] pub fn #name(&self, #params) -> #cty {
                    #default_fns
                    #cty {
                        __s: ::std::sync::Arc::new(#cstate {
                            __parent: ::core::clone::Clone::clone(self),
                            #inits
                        }),
                    }
                }
            }),
            Kind::Endpoint { method, .. } => {
                let body_init = body.map(|b| {
                    let bname = &b.name;
                    if b.optional {
                        quote!(#bname: ::core::option::Option::None,)
                    } else {
                        quote!(#bname: #k::IntoValue::into_value(#bname),)
                    }
                });
                let line = lit(&format!(" `{}` endpoint `{}`.", method, c.dotted));
                let sep = if docs.is_empty() {
                    quote!()
                } else {
                    quote!(#[doc = ""])
                };
                out.extend(quote! {
                    #(#docs)*
                    #sep
                    #[doc = #line]
                    #args
                    #(#nav_attrs)*
                    #[inline] #[allow(clippy::too_many_arguments)] pub fn #name(&self, #params) -> #cty {
                        #default_fns
                        #cty {
                            __parent: ::core::clone::Clone::clone(self),
                            #inits
                            #body_init
                        }
                    }
                })
            }
            Kind::Client { .. } => unreachable!(),
        }
    }
    out
}

/// Place expressions for every owner on an endpoint's chain.
struct Places {
    chain: Vec<usize>,
}

impl Places {
    fn owner_var(&self, owner: usize) -> TokenStream {
        let depth = self
            .chain
            .iter()
            .position(|&o| o == owner)
            .expect("owner on chain");
        if depth == self.chain.len() - 1 {
            quote!(self)
        } else {
            let var = format_ident!("__o{}", depth);
            quote!(#var)
        }
    }

    fn value(&self, model: &Model, r: ValueRef) -> TokenStream {
        let base = self.owner_var(r.owner);
        let name = &model.value(r).name;
        quote!(#base.#name)
    }
}

/// Pattern + scrutinee binding every optional value a source reads, and the emission arguments.
struct Bound {
    patterns: Vec<TokenStream>,
    scrutinees: Vec<TokenStream>,
}

impl Bound {
    fn wrap(&self, then: TokenStream, otherwise: Option<TokenStream>) -> TokenStream {
        if self.patterns.is_empty() {
            return then;
        }
        let (pat, scr) = if self.patterns.len() == 1 {
            (self.patterns[0].clone(), self.scrutinees[0].clone())
        } else {
            let p = &self.patterns;
            let s = &self.scrutinees;
            (quote!((#(#p),*)), quote!((#(#s),*)))
        };
        match otherwise {
            Some(otherwise) => quote!(if let #pat = #scr { #then } else { #otherwise }),
            None => quote!(if let #pat = #scr { #then }),
        }
    }
}

fn source_code(
    k: &TokenStream,
    model: &Model,
    places: &Places,
    id: ContribId,
    key: &str,
    sensitive: bool,
) -> (Bound, TokenStream) {
    let contrib = model.contrib(id);
    let mut bound = Bound {
        patterns: Vec::new(),
        scrutinees: Vec::new(),
    };
    let key = lit(key);
    let source = contrib.source.as_ref().expect("emitting sources only");
    let (arg, each, at) = match source {
        Source::Ref { value, each } => {
            let place = places.value(model, *value);
            let span = model.value(*value).ty.span();
            let bound_v = quote!(__v);
            if model.value(*value).optional {
                bound
                    .patterns
                    .push(quote!(::core::option::Option::Some(__v)));
                bound.scrutinees.push(quote!(&#place));
                (quote_spanned!(span=> &*#bound_v), *each, Some(span))
            } else {
                (quote_spanned!(span=> &#place), *each, Some(span))
            }
        }
        Source::Template { pieces } => {
            let mut fmt = String::new();
            let mut literal = String::new();
            let mut args = Vec::new();
            for piece in pieces {
                match piece {
                    Piece::Lit(s) => {
                        fmt.push_str(&s.replace('{', "{{").replace('}', "}}"));
                        literal.push_str(s);
                    }
                    Piece::Hole(r) => {
                        fmt.push_str("{}");
                        let place = places.value(model, *r);
                        let at = model.value(*r).ty.span();
                        if model.value(*r).optional {
                            let b = format_ident!("__h{}", bound.patterns.len());
                            bound
                                .patterns
                                .push(quote!(::core::option::Option::Some(#b)));
                            bound.scrutinees.push(quote!(&#place));
                            args.push(located(quote!(#b), at));
                        } else {
                            args.push(located(quote!(&#place), at));
                        }
                    }
                }
            }
            if args.is_empty() {
                // No holes: the decoded literal itself (`{{` already became `{`).
                let literal = lit(&literal);
                (quote!(#literal), false, None)
            } else {
                let fmt = lit(&fmt);
                let what = placement_label(id.family, key.value().as_str());
                (
                    quote!(&#k::__private::format_template(::core::format_args!(#fmt, #(#args),*), &__META, #what)?),
                    false,
                    None,
                )
            }
        }
    };
    // Header names go through the native validator at compile time: the unnamed `const` item is
    // always evaluated (even when `prepare` is never instantiated), so an invalid literal — e.g.
    // longer than `http` allows — is a compile error; the `const` block builds the name without
    // a runtime check or panic path.
    let name = quote!(const { #k::reqwest::header::HeaderName::from_static(#key) });
    let guard = quote_spanned!(contrib.span=> const _: () = ::core::mem::forget(#k::reqwest::header::HeaderName::from_static(#key)););
    let what = placement_label(id.family, key.value().as_str());
    let call = match (id.family, each) {
        (Family::Query, false) => quote!(__p.query(#key, #arg, #what)?;),
        (Family::Query, true) => quote!(__p.query_each(#key, #arg, #what)?;),
        (Family::Header, false) => quote!(#guard __p.header(#name, #what, #arg, #sensitive)?;),
        (Family::Header, true) => quote!(#guard __p.header_each(#name, #what, #arg, #sensitive)?;),
    };
    // A value placed directly: trait errors (no `Display`, not iterable) point at its type.
    let call = match at {
        Some(at) => located(call, at),
        None => call,
    };
    (bound, call)
}

fn items_code(
    k: &TokenStream,
    model: &Model,
    places: &Places,
    items: &[Item],
    slot: &Slot,
) -> TokenStream {
    let mut out = TokenStream::new();
    for item in items {
        match item {
            Item::Emit(id) => {
                let (bound, call) = source_code(k, model, places, *id, &slot.key, slot.sensitive);
                out.extend(bound.wrap(call, None));
            }
            Item::Override { src, fallback } => {
                let (bound, call) = source_code(k, model, places, *src, &slot.key, slot.sensitive);
                let otherwise = items_code(k, model, places, fallback, slot);
                out.extend(bound.wrap(call, Some(otherwise)));
            }
        }
    }
    out
}

fn component_code(
    k: &TokenStream,
    model: &Model,
    places: &Places,
    owner: usize,
    pieces: &[Piece],
    label: bool,
    what: &str,
) -> TokenStream {
    let what = lit(what);
    if let [Piece::Lit(s)] = pieces {
        let s = lit(s);
        return if label {
            quote!(__p.label(::std::string::String::from(#s), #what)?;)
        } else {
            quote!(__p.segment_static(#s);)
        };
    }
    let _ = owner;
    let mut build = TokenStream::new();
    for piece in pieces {
        match piece {
            Piece::Lit(s) => {
                let s = lit(s);
                build.extend(quote!(__c.push_str(#s);));
            }
            Piece::Hole(r) => {
                let place = places.value(model, *r);
                let span = model.value(*r).ty.span();
                // Located at the value's type, so a missing `Display` is reported there.
                let call = if label {
                    quote!(#k::__private::display_into(&mut __c, &#place, &__META, #what)?;)
                } else {
                    quote!(#k::__private::encode_segment(&mut __c, &#place, &__META, #what)?;)
                };
                build.extend(located(call, span));
            }
        }
    }
    let finish = if label {
        quote!(__p.label(__c, #what)?;)
    } else {
        quote!(__p.segment(__c, #what)?;)
    };
    quote!({
        let mut __c = ::std::string::String::new();
        #build
        #finish
    })
}

fn prepare_body(
    k: &TokenStream,
    model: &Model,
    index: usize,
    method: &Ident,
    output: Option<&crate::syntax::Payload>,
    body: Option<&Body>,
) -> TokenStream {
    let owner: &Owner = &model.owners[index];
    let composed: &Composed = owner.composed.as_ref().expect("endpoints are composed");
    let places = Places {
        chain: model.chain(index),
    };

    // Bindings for ancestors (the endpoint itself is `self`).
    let mut bindings = TokenStream::new();
    let depth = places.chain.len() - 1;
    for d in (0..depth).rev() {
        let var = format_ident!("__o{}", d);
        let from = if d == depth - 1 {
            quote!(self.__parent.__s)
        } else {
            let up = format_ident!("__o{}", d + 1);
            quote!(#up.__parent.__s)
        };
        bindings.extend(quote!(let #var = &*#from;));
    }

    let mut code = TokenStream::new();
    let mut sensitive_labels = Vec::new();
    for (i, &(o, li)) in composed.labels.iter().enumerate() {
        let c = &model.owners[o].labels[li];
        if c.sensitive {
            sensitive_labels.push(i);
        }
        code.extend(component_code(
            k,
            model,
            &places,
            o,
            &c.pieces,
            true,
            &format!("subdomain label `{}`", c.text),
        ));
    }
    if !composed.labels.is_empty() {
        code.extend(quote!(__p.host()?;));
    }
    let mut sensitive_segments = Vec::new();
    for (i, &(o, si)) in composed.segments.iter().enumerate() {
        let c = &model.owners[o].segments[si];
        if c.sensitive {
            sensitive_segments.push(i);
        }
        code.extend(component_code(
            k,
            model,
            &places,
            o,
            &c.pieces,
            false,
            &format!("path segment `{}`", c.text),
        ));
    }
    if composed.trailing_slash {
        code.extend(quote!(__p.trailing_slash();));
    }
    let mut sensitive_query = Vec::new();
    for slot in &composed.query {
        if slot.sensitive {
            sensitive_query.push(lit(&slot.key));
        }
        code.extend(items_code(k, model, &places, &slot.items, slot));
    }
    for slot in &composed.headers {
        code.extend(items_code(k, model, &places, &slot.items, slot));
    }

    let body_code = match body {
        None => quote!(::core::option::Option::None),
        Some(b) => {
            let name = &b.name;
            let ty = &b.ty;
            let repr = &b.repr;
            // Spanned at the declared representation, so an unsupported payload is reported there.
            let encode = quote_spanned!(repr.span()=> #k::__private::encode_body::<#repr, #ty>);
            if b.optional {
                quote!(match self.#name {
                    ::core::option::Option::Some(__b) => ::core::option::Option::Some(#encode(__b, &__META)?),
                    ::core::option::Option::None => ::core::option::Option::None,
                })
            } else {
                quote!(::core::option::Option::Some(#encode(self.#name, &__META)?))
            }
        }
    };

    let name = lit(&owner.dotted);
    let method_str = lit(&method.to_string());
    let label_count = composed.labels.len();
    let segment_count = composed.segments.len() + usize::from(composed.trailing_slash);
    let root = format_ident!("__o0");
    let decoder = match output {
        Some(p) => {
            let (t, r) = (&p.ty, &p.repr);
            quote_spanned!(r.span()=> #k::__private::decoder::<#r, #t>(&__META))
        }
        None => quote!(#k::__private::drain(&__META)),
    };
    quote! {
        static __META: #k::__private::EndpointMeta = #k::__private::EndpointMeta {
            name: #name,
            method: #method_str,
            labels: #label_count,
            sensitive_labels: &[#(#sensitive_labels),*],
            segments: #segment_count,
            sensitive_segments: &[#(#sensitive_segments),*],
            sensitive_query: &[#(#sensitive_query),*],
        };
        let __body = #body_code;
        #bindings
        let mut __p = #k::__private::Parts::new(&__META, &#root.__base);
        #code
        __p.finish(#k::reqwest::Method::#method, &#root.__http, __body, #decoder)
    }
}
