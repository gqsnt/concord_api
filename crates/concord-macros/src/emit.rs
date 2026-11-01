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

pub fn emit(model: &Model) -> TokenStream {
    let vis = match &model.owners[0].kind {
        Kind::Client { vis, .. } => vis.clone(),
        _ => unreachable!("the first owner is the client"),
    };
    let mut out = TokenStream::new();
    for index in 0..model.owners.len() {
        out.extend(emit_owner(model, index, &vis));
    }
    out
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

fn body_value_type(body: &Body) -> TokenStream {
    let ty = &body.ty;
    quote!(<#ty as ::concord::Encode>::Value)
}

/// The struct literal fields initializing an owner's values, in declaration order, with default
/// functions that cannot capture the entry method's arguments or `self`.
fn value_inits(values: &[Value]) -> (TokenStream, TokenStream) {
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
                    fn #f() -> #ty { <_ as ::concord::IntoValue<#ty>>::into_value(#expr) }
                });
                if value.optional {
                    quote!(::core::option::Option::Some(#f()))
                } else {
                    quote!(#f())
                }
            }
            None if value.optional => quote!(::core::option::Option::None),
            None => quote!(::concord::IntoValue::into_value(#name)),
        };
        fields.extend(quote!(#name: #init,));
    }
    (fns, fields)
}

fn positional_params(values: &[Value], body: Option<&Body>) -> TokenStream {
    // Required values and a required body, in declaration order.
    let body = body.filter(|b| !b.optional);
    let body_param = |b: &Body| {
        let name = &b.name;
        let ty = body_value_type(b);
        quote!(#name: impl ::concord::IntoValue<#ty>,)
    };
    let mut params = TokenStream::new();
    for (i, value) in values.iter().enumerate() {
        if let Some(b) = body.filter(|b| b.position == i) {
            params.extend(body_param(b));
        }
        if value.is_positional() {
            let name = &value.name;
            let ty = &value.ty;
            params.extend(quote!(#name: impl ::concord::IntoValue<#ty>,));
        }
    }
    if let Some(b) = body.filter(|b| b.position >= values.len()) {
        params.extend(body_param(b));
    }
    params
}

fn controls(values: &[Value], body: Option<&Body>, target: &TokenStream) -> TokenStream {
    let mut out = TokenStream::new();
    for value in values.iter().filter(|v| !v.is_positional()) {
        let name = &value.name;
        let ty = &value.ty;
        let docs = &value.docs;
        let set = format_ident!("set_{}", plain(name));
        let set_doc = lit(&format!(
            " Sets `{}`{}.",
            plain(name),
            if value.optional { " (present)" } else { "" }
        ));
        let assign = if value.optional {
            quote!(::core::option::Option::Some(
                ::concord::IntoValue::into_value(value)
            ))
        } else {
            quote!(::concord::IntoValue::into_value(value))
        };
        out.extend(quote! {
            #(#docs)*
            #[doc = ""]
            #[doc = #set_doc]
            pub fn #set(mut self, value: impl ::concord::IntoValue<#ty>) -> Self {
                #target.#name = #assign;
                self
            }
        });
        if value.optional {
            let maybe = format_ident!("maybe_{}", plain(name));
            let maybe_doc = lit(&format!(
                " Sets `{}` to exactly `value`: `None` leaves it absent.",
                plain(name)
            ));
            out.extend(quote! {
                #(#docs)*
                #[doc = ""]
                #[doc = #maybe_doc]
                pub fn #maybe(mut self, value: ::core::option::Option<#ty>) -> Self {
                    #target.#name = value;
                    self
                }
            });
        }
    }
    if let Some(body) = body.filter(|b| b.optional) {
        let name = &body.name;
        let ty = body_value_type(body);
        let docs = &body.docs;
        let set = format_ident!("set_{}", plain(name));
        let maybe = format_ident!("maybe_{}", plain(name));
        out.extend(quote! {
            #(#docs)*
            #[doc = ""]
            #[doc = " Sets the request body."]
            pub fn #set(mut self, value: impl ::concord::IntoValue<#ty>) -> Self {
                #target.#name = ::core::option::Option::Some(::concord::IntoValue::into_value(value));
                self
            }
            #(#docs)*
            #[doc = ""]
            #[doc = " Sets the request body to exactly `value`: `None` sends no body."]
            pub fn #maybe(mut self, value: ::core::option::Option<#ty>) -> Self {
                #target.#name = value;
                self
            }
        });
    }
    out
}

fn debug_fields(values: &[Value], base: &TokenStream) -> TokenStream {
    let mut out = TokenStream::new();
    for value in values {
        let name = &value.name;
        let label = lit(&plain(name));
        let place = quote!(#base.#name);
        let span = value.ty.span();
        let shown = match (value.debug_sensitive, value.optional, value.usage) {
            (true, false, _) => quote_spanned!(span=> &::concord::__private::Redacted),
            (true, true, _) => {
                quote_spanned!(span=> &#place.as_ref().map(|_| ::concord::__private::Redacted))
            }
            (false, false, Some(Usage::Each)) => {
                quote_spanned!(span=> &::concord::__private::DebugEach(&#place))
            }
            (false, true, Some(Usage::Each)) => {
                quote_spanned!(span=> &#place.as_ref().map(::concord::__private::DebugEach))
            }
            (false, false, _) => {
                quote_spanned!(span=> &::concord::__private::DebugDisplay(&#place))
            }
            (false, true, _) => {
                quote_spanned!(span=> &#place.as_ref().map(::concord::__private::DebugDisplay))
            }
        };
        out.extend(quote_spanned!(span=> .field(#label, #shown)));
    }
    out
}

fn emit_owner(model: &Model, index: usize, vis: &syn::Visibility) -> TokenStream {
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
    let nav = navigation(model, index);
    let type_label = lit(&ty.to_string());

    match &owner.kind {
        Kind::Client { base, .. } => {
            let params = positional_params(&owner.values, None);
            let (default_fns, inits) = value_inits(&owner.values);
            let controls = controls(
                &owner.values,
                None,
                &quote!(::std::sync::Arc::make_mut(&mut self.__s)),
            );
            let debug = debug_fields(&owner.values, &quote!(__s));
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
                    __http: ::concord::reqwest::Client,
                    __base: ::concord::__private::Base,
                    #(#fields)*
                }

                impl #ty {
                    /// Creates the client. `http` is used for every request; configure timeouts,
                    /// redirects, proxies, TLS and default headers on it. Builds no connection.
                    pub fn new(http: ::concord::reqwest::Client, #params) -> Self {
                        #default_fns
                        Self {
                            __s: ::std::sync::Arc::new(#state {
                                __http: http,
                                __base: ::concord::__private::Base::from_static(#base),
                                #inits
                            }),
                        }
                    }

                    /// Replaces the declared base URL (for example with a staging server or a
                    /// local test server). It must be an `http`/`https` URL with a host and no
                    /// user information, query or fragment.
                    pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>) -> ::core::result::Result<Self, ::concord::Error> {
                        let base = ::concord::__private::Base::parse(url.as_ref())?;
                        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
                        ::core::result::Result::Ok(self)
                    }

                    #controls
                    #nav
                }

                impl ::core::fmt::Debug for #ty {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        let __s = &*self.__s;
                        f.debug_struct(#type_label).field("base", &__s.__base) #debug .finish()
                    }
                }
            }
        }
        Kind::Scope => {
            let parent_ty = &model.owners[owner.parent.unwrap()].type_name;
            let controls = controls(
                &owner.values,
                None,
                &quote!(::std::sync::Arc::make_mut(&mut self.__s)),
            );
            let debug = debug_fields(&owner.values, &quote!(__s));
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
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        let __s = &*self.__s;
                        f.debug_struct(#type_label) #debug .finish()
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
            let controls = controls(&owner.values, body.as_ref(), &quote!(self));
            let mut debug = debug_fields(&owner.values, &quote!(self));
            let body_field = body.as_ref().map(|b| {
                let name = &b.name;
                let vty = body_value_type(b);
                let label = lit(&plain(name));
                if b.optional {
                    debug.extend(quote!(.field(#label, &self.#name.as_ref().map(|_| ".."))));
                    quote!(#name: ::core::option::Option<#vty>,)
                } else {
                    debug.extend(quote!(.field(#label, &"..")));
                    quote!(#name: #vty,)
                }
            });
            let derive_clone = if body.is_none() {
                quote!(#[derive(Clone)])
            } else {
                quote!()
            };
            let repr = match output {
                Some(t) => quote!(#t),
                None => quote!(()),
            };
            let prepare = prepare_body(model, index, method, &repr, body.as_ref());
            quote! {
                #docs
                #[must_use = "an endpoint request does nothing until `send` or `prepare` is called"]
                #derive_clone
                #vis struct #ty {
                    __parent: #parent_ty,
                    #(#fields)*
                    #body_field
                }

                impl #ty {
                    #controls

                    /// Builds the native request without sending it.
                    pub fn prepare(self) -> ::core::result::Result<::concord::Prepared<#repr>, ::concord::Error> {
                        #prepare
                    }

                    /// Sends the request once and decodes a 2xx response.
                    pub async fn send(self) -> ::core::result::Result<<#repr as ::concord::Decode>::Output, ::concord::Error> {
                        self.prepare()?.send().await
                    }
                }

                impl ::core::fmt::Debug for #ty {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        f.debug_struct(#type_label) #debug .finish()
                    }
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
fn navigation(model: &Model, index: usize) -> TokenStream {
    let mut out = TokenStream::new();
    for &child in &model.owners[index].children {
        let c = &model.owners[child];
        let name = &c.name;
        let cty = &c.type_name;
        let cstate = &c.state_name;
        let docs = &c.docs;
        let body = c.body();
        let params = positional_params(&c.values, body);
        let (default_fns, inits) = value_inits(&c.values);
        let scope_line = lit(&format!(" Enters scope `{}`.", c.dotted));
        match &c.kind {
            Kind::Scope => out.extend(quote! {
                #(#docs)*
                #[doc = #scope_line]
                pub fn #name(&self, #params) -> #cty {
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
                        quote!(#bname: ::concord::IntoValue::into_value(#bname),)
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
                    pub fn #name(&self, #params) -> #cty {
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
    let mut span = Span::call_site();
    let (arg, each) = match source {
        Source::Ref { value, each } => {
            let place = places.value(model, *value);
            span = model.value(*value).ty.span();
            if model.value(*value).optional {
                bound
                    .patterns
                    .push(quote!(::core::option::Option::Some(__v)));
                bound.scrutinees.push(quote!(&#place));
                (quote_spanned!(span=> __v), *each)
            } else {
                (quote_spanned!(span=> &#place), *each)
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
                        if model.value(*r).optional {
                            let b = format_ident!("__h{}", bound.patterns.len());
                            bound
                                .patterns
                                .push(quote!(::core::option::Option::Some(#b)));
                            bound.scrutinees.push(quote!(&#place));
                            args.push(quote!(#b));
                        } else {
                            args.push(quote!(&#place));
                        }
                    }
                }
            }
            if args.is_empty() {
                // No holes: the decoded literal itself (`{{` already became `{`).
                let literal = lit(&literal);
                (quote!(#literal), false)
            } else {
                let fmt = lit(&fmt);
                (quote!(&::std::format!(#fmt, #(#args),*)), false)
            }
        }
    };
    let call = match (id.family, each) {
        (Family::Query, false) => quote_spanned!(span=> __p.query(#key, #arg);),
        (Family::Query, true) => quote_spanned!(span=> __p.query_each(#key, #arg);),
        (Family::Header, false) => quote_spanned!(span=> __p.header(#key, #arg, #sensitive)?;),
        (Family::Header, true) => quote_spanned!(span=> __p.header_each(#key, #arg, #sensitive)?;),
    };
    (bound, call)
}

fn items_code(model: &Model, places: &Places, items: &[Item], slot: &Slot) -> TokenStream {
    let mut out = TokenStream::new();
    for item in items {
        match item {
            Item::Emit(id) => {
                let (bound, call) = source_code(model, places, *id, &slot.key, slot.sensitive);
                out.extend(bound.wrap(call, None));
            }
            Item::Override { src, fallback } => {
                let (bound, call) = source_code(model, places, *src, &slot.key, slot.sensitive);
                let otherwise = items_code(model, places, fallback, slot);
                out.extend(bound.wrap(call, Some(otherwise)));
            }
        }
    }
    out
}

fn component_code(
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
                if label {
                    build.extend(quote_spanned!(span=> ::concord::__private::display_into(&mut __c, &#place);));
                } else {
                    build.extend(quote_spanned!(span=> ::concord::__private::encode_segment(&mut __c, &#place);));
                }
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
    model: &Model,
    index: usize,
    method: &Ident,
    repr: &TokenStream,
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
            model,
            &places,
            o,
            &c.pieces,
            true,
            &format!("subdomain label `{}`", c.text),
        ));
    }
    let mut sensitive_segments = Vec::new();
    for (i, &(o, si)) in composed.segments.iter().enumerate() {
        let c = &model.owners[o].segments[si];
        if c.sensitive {
            sensitive_segments.push(i);
        }
        code.extend(component_code(
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
        code.extend(items_code(model, &places, &slot.items, slot));
    }
    for slot in &composed.headers {
        code.extend(items_code(model, &places, &slot.items, slot));
    }

    let body_code = match body {
        None => quote!(::core::option::Option::None),
        Some(b) => {
            let name = &b.name;
            let ty = &b.ty;
            if b.optional {
                quote!(match self.#name {
                    ::core::option::Option::Some(__b) => ::core::option::Option::Some(::concord::__private::encode_body::<#ty>(__b, &__META)?),
                    ::core::option::Option::None => ::core::option::Option::None,
                })
            } else {
                quote!(::core::option::Option::Some(::concord::__private::encode_body::<#ty>(self.#name, &__META)?))
            }
        }
    };

    let name = lit(&owner.dotted);
    let method_str = lit(&method.to_string());
    let label_count = composed.labels.len();
    let segment_count = composed.segments.len() + usize::from(composed.trailing_slash);
    let root = format_ident!("__o0");
    quote! {
        static __META: ::concord::__private::EndpointMeta = ::concord::__private::EndpointMeta {
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
        let mut __p = ::concord::__private::Parts::new(&__META);
        #code
        __p.finish::<#repr>(::concord::reqwest::Method::#method, &#root.__http, &#root.__base, __body)
    }
}
