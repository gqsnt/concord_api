//! Rust code generation from a resolved [`Model`].

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Ident, LitStr};

use crate::model::{
    Body, Composed, Cond, ContribId, HeaderComposition, Item, KeyName, Kind, Model, Origin, Output,
    Owner, OwnerDefault, Piece, Slot, Source, Usage, Value, ValueRef,
};
use crate::syntax::{Family, Format, expr_text, type_text};

pub fn emit(model: &Model, krate: TokenStream) -> TokenStream {
    let k = &krate;
    let notes = value_notes(model);
    let (header_items, headers) = header_statics(k, model);
    let vis = match &model.owners[0].kind {
        Kind::Client { vis, .. } => vis.clone(),
        _ => unreachable!("the first owner is the client"),
    };
    let mut out = header_items;
    out.extend(withheld_fns(k, model, &headers));
    out.extend(format_aliases(model));
    out.extend(validate_header_names(k, model, &headers));
    for (index, (owner, notes)) in model.owners.iter().zip(&notes).enumerate() {
        let cfgs = &owner.cfgs;
        if cfgs.is_empty() {
            out.extend(emit_owner(k, model, index, &vis, notes, &headers));
        } else {
            // Every item of a `#[cfg]` owner carries its own and its ancestors' `cfg`s.
            out.extend(with_attrs(
                emit_owner(k, model, index, &vis, notes, &headers),
                cfgs,
            ));
        }
    }
    out.extend(group_traits(k, model, &vis));
    out
}

/// One trait per parameter group (`params Name(..);`): the group's setters, implemented by every
/// endpoint that includes it (`..Name`) by forwarding to the endpoint's own generated setters.
/// Code written once over `E: Name` configures any of those endpoints; the endpoints' inherent
/// setters stay what ordinary callers use.
fn group_traits(k: &TokenStream, model: &Model, vis: &syn::Visibility) -> TokenStream {
    let mut out = TokenStream::new();
    for group in &model.groups {
        let name = &group.name;
        let docs = &group.docs;
        let user_indices: Vec<usize> = model
            .owners
            .iter()
            .enumerate()
            .filter(|(_, o)| o.groups.iter().any(|g| plain(g) == plain(name)))
            .map(|(i, _)| i)
            .collect();
        let users: Vec<&Owner> = user_indices.iter().map(|&i| &model.owners[i]).collect();
        // Names, not links: an including endpoint may be compiled out by its `cfg`.
        let listed = users
            .iter()
            .map(|o| format!("`{}`", plain(&o.type_name)))
            .collect::<Vec<_>>()
            .join(", ");
        // The trait exists where one of its endpoints does: its signatures may name types that
        // only those endpoints' configurations define.
        let cfg = owners_cfg(model, &user_indices);
        let about = lit(&format!(
            " The setters of the parameter group `{}`, implemented by every endpoint that includes it (`..{0}`): {listed}. Each method is the endpoint's own setter of the same name.",
            plain(name)
        ));
        let mut methods = TokenStream::new();
        let mut forwards = TokenStream::new();
        for param in &group.params {
            let shown = plain(&param.name);
            let ty = &param.ty;
            let pdocs = &param.docs;
            let set = format_ident!("set_{}", shown);
            let set_doc = lit(&if param.optional {
                format!(" Sets `{shown}` (present).")
            } else {
                format!(" Replaces `{shown}`.")
            });
            methods.extend(quote! {
                #(#pdocs)*
                #[doc = ""]
                #[doc = #set_doc]
                #[must_use = "a setter returns the updated handle; discarding it discards the change"]
                fn #set(self, value: impl #k::IntoValue<#ty>) -> Self;
            });
            forwards.extend(quote! {
                #[inline] fn #set(self, value: impl #k::IntoValue<#ty>) -> Self {
                    Self::#set(self, value)
                }
            });
            if param.optional {
                let maybe = format_ident!("maybe_{}", shown);
                let maybe_doc = lit(&format!(
                    " Sets whether `{shown}` is present: `Some(v)` is `set_{shown}(v)`; `None` makes it absent."
                ));
                methods.extend(quote! {
                    #(#pdocs)*
                    #[doc = ""]
                    #[doc = #maybe_doc]
                    #[must_use = "a setter returns the updated handle; discarding it discards the change"]
                    fn #maybe(self, value: ::core::option::Option<#ty>) -> Self;
                });
                forwards.extend(quote! {
                    #[inline] fn #maybe(self, value: ::core::option::Option<#ty>) -> Self {
                        Self::#maybe(self, value)
                    }
                });
            }
        }
        out.extend(quote! {
            #cfg
            #(#docs)*
            #[doc = ""]
            #[doc = #about]
            #vis trait #name: ::core::marker::Sized {
                #methods
            }
        });
        for owner in users {
            let ty = &owner.type_name;
            let cfgs = &owner.cfgs;
            out.extend(quote! {
                #(#cfgs)*
                #[allow(deprecated)]
                impl #name for #ty {
                    #forwards
                }
            });
        }
    }
    out
}

/// The generated `static`s holding literal header names and literal header values.
struct Headers {
    /// Literal header name (normalised by `http`) -> its `HeaderName` static.
    names: std::collections::BTreeMap<String, Ident>,
    /// Literal header value (a template without holes, decoded) -> its `HeaderValue` static, for
    /// values `HeaderValue::from_static` accepts (visible ASCII). Other literals are built at run
    /// time through the checked constructor (`__private::header_literal`).
    values: std::collections::BTreeMap<String, Ident>,
}

/// One `static` per distinct literal header name of the declaration, and one per distinct
/// visible-ASCII literal header value. Statics are evaluated at compile time, so the native
/// constructors check them again in the downstream `http` (the macro checked them with its own
/// copy of `http` first); requests clone the validated statics. Native `HeaderName` constants
/// written as keys are used as they are. One static per distinct text keeps compile-time
/// evaluation linear in the distinct literals, not in endpoints x inherited headers.
fn header_statics(k: &TokenStream, model: &Model) -> (TokenStream, Headers) {
    let client = plain(&model.owners[0].type_name);
    let mut headers = Headers {
        names: Default::default(),
        values: Default::default(),
    };
    let mut items = TokenStream::new();
    for owner in &model.owners {
        for c in &owner.headers {
            if let KeyName::Lit(name) = &c.key
                && !headers.names.contains_key(name)
            {
                let ident = format_ident!("__CONCORD_{}_HEADER_{}", client, headers.names.len());
                let key = lit(name);
                items.extend(quote_spanned! {c.span=>
                    #[doc(hidden)]
                    #[allow(non_upper_case_globals, dead_code)]
                    static #ident: #k::reqwest::header::HeaderName =
                        #k::reqwest::header::HeaderName::from_static(#key);
                });
                headers.names.insert(name.clone(), ident);
            }
            let Some(literal) = literal_value(c) else {
                continue;
            };
            let ascii =
                http::HeaderValue::from_bytes(literal.as_bytes()).is_ok_and(|v| v.to_str().is_ok());
            if !ascii || headers.values.contains_key(&literal) {
                continue;
            }
            let ident = format_ident!("__CONCORD_{}_HEADER_VALUE_{}", client, headers.values.len());
            let value = lit(&literal);
            items.extend(quote_spanned! {c.span=>
                #[doc(hidden)]
                #[allow(non_upper_case_globals, dead_code)]
                static #ident: #k::reqwest::header::HeaderValue =
                    #k::reqwest::header::HeaderValue::from_static(#value);
            });
            headers.values.insert(literal, ident);
        }
    }
    // One accessor per native key path, under the union of its declaring owners' `cfg`s: the
    // path is resolved (and type-checked, with Concord's message) once, where it is written.
    for (i, key) in model.key_paths.iter().enumerate() {
        let cfg = owners_cfg(model, key_owner(model, i));
        let ident = key_fn(model, i);
        let path = &key.path;
        let check = quote_spanned!(path.span()=> #k::__private::NativeKey::native_key(&#path));
        items.extend(quote! {
            #cfg
            #[doc(hidden)]
            #[allow(non_snake_case, dead_code)]
            #[inline]
            fn #ident() -> #k::reqwest::header::HeaderName {
                ::core::clone::Clone::clone(#check)
            }
        });
    }
    (items, headers)
}

/// The accessor generated for native key path `i`.
fn key_fn(model: &Model, i: usize) -> Ident {
    format_ident!("__concord_{}_key_{}", plain(&model.owners[0].type_name), i)
}

/// The decoded text of a header template without holes.
fn literal_value(c: &crate::model::Contrib) -> Option<String> {
    let Some(Source::Template { pieces }) = &c.source else {
        return None;
    };
    if pieces.iter().any(|p| matches!(p, Piece::Hole(_))) {
        return None;
    }
    Some(
        pieces
            .iter()
            .map(|p| match p {
                Piece::Lit(s) => s.as_str(),
                Piece::Hole(_) => "",
            })
            .collect(),
    )
}

/// A key's native name as an expression of type `&HeaderName`: the literal's static, or the
/// author's path (resolved by rustc where it is written; a path that is not a `HeaderName` is a
/// type error there).
fn key_ref(model: &Model, headers: &Headers, key: &KeyName) -> TokenStream {
    match key {
        KeyName::Lit(name) => {
            let ident = &headers.names[name];
            quote!(&#ident)
        }
        KeyName::Path(i) => {
            let f = key_fn(model, *i);
            quote!(&#f())
        }
    }
}

/// Where `value` is declared (it is one of the model's values).
fn value_ref(model: &Model, value: &Value) -> ValueRef {
    for (owner, o) in model.owners.iter().enumerate() {
        if let Some(index) = o.values.iter().position(|v| std::ptr::eq(v, value)) {
            return ValueRef { owner, index };
        }
    }
    unreachable!("a value of the model")
}

/// The function deciding at run time whether value `r` is withheld (its `debug_when` conditions).
fn when_fn(model: &Model, r: ValueRef) -> Ident {
    format_ident!(
        "__concord_{}_withheld_{}_{}",
        plain(&model.owners[0].type_name),
        r.owner,
        r.index
    )
}

/// One function per value whose classification depends on native header names. Its conditions
/// read only constants (the names the key paths hold), so it is computed once and cached; every
/// `Debug` and every request built reads the cached answer instead of repeating the comparisons
/// inline, which kept generated code proportional to endpoints × conditions.
fn withheld_fns(k: &TokenStream, model: &Model, headers: &Headers) -> TokenStream {
    let mut out = TokenStream::new();
    for (o, owner) in model.owners.iter().enumerate() {
        for (index, value) in owner.values.iter().enumerate() {
            if value.debug_sensitive || value.debug_when.is_empty() {
                continue;
            }
            let ident = when_fn(model, ValueRef { owner: o, index });
            let conds = value
                .debug_when
                .iter()
                .map(|c| cond_expr(k, model, headers, c));
            out.extend(with_attrs(
                quote! {
                    #[doc(hidden)]
                    #[allow(non_snake_case, dead_code)]
                    fn #ident() -> bool {
                        static __C: ::std::sync::OnceLock<bool> = ::std::sync::OnceLock::new();
                        *__C.get_or_init(|| false #(|| #conds)*)
                    }
                },
                &owner.cfgs,
            ));
        }
    }
    out
}

/// The owner whose headers name native key path `i` (the occurrence's provenance, recorded when
/// the model is built); empty for an occurrence whose declaration failed, whose accessor is then
/// never compiled.
fn key_owner(model: &Model, i: usize) -> &[usize] {
    model.key_paths[i].owner.as_slice()
}

/// A run-time withholding condition as a `bool` expression. A key path written only under
/// `#[cfg]` owners has its accessor under the same `cfg`; where those owners are compiled out the
/// contributions do not exist, so the condition is `false` there.
fn cond_expr(k: &TokenStream, model: &Model, headers: &Headers, cond: &Cond) -> TokenStream {
    let (expr, keys) = match cond {
        Cond::Name(i) => {
            let name = key_ref(model, headers, &KeyName::Path(*i));
            (quote!(#k::__private::sensitive_name(#name)), vec![*i])
        }
        Cond::Same(a, b) => {
            let keys = [a, b]
                .into_iter()
                .filter_map(|key| match key {
                    KeyName::Path(i) => Some(*i),
                    KeyName::Lit(_) => None,
                })
                .collect();
            let a = key_ref(model, headers, a);
            let b = key_ref(model, headers, b);
            (quote!((#a == #b)), keys)
        }
    };
    let present: Vec<TokenStream> = keys
        .into_iter()
        .filter_map(|i| owners_cfg_predicate(model, key_owner(model, i)))
        .collect();
    if present.is_empty() {
        return expr;
    }
    quote!({
        #[cfg(all(#(#present),*))]
        let __r = #expr;
        #[cfg(not(all(#(#present),*)))]
        let __r = false;
        __r
    })
}

/// `static_flag || any run-time condition of a value read by `ids``.
fn flag_expr(model: &Model, static_flag: bool, ids: &[ContribId]) -> TokenStream {
    if static_flag {
        return quote!(true);
    }
    let mut reads: Vec<ValueRef> = Vec::new();
    for &id in ids {
        for r in contrib_reads(model, id) {
            let value = model.value(r);
            if !value.debug_sensitive && !value.debug_when.is_empty() && !reads.contains(&r) {
                reads.push(r);
            }
        }
    }
    if reads.is_empty() {
        return quote!(false);
    }
    let calls = reads.into_iter().map(|r| when_fn(model, r));
    quote!((false #(|| #calls())*))
}

fn contrib_reads(model: &Model, id: ContribId) -> Vec<ValueRef> {
    match &model.contrib(id).source {
        Some(Source::Ref { value, .. }) => vec![*value],
        Some(Source::Template { pieces }) => pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Hole(r) => Some(*r),
                Piece::Lit(_) => None,
            })
            .collect(),
        None => Vec::new(),
    }
}

/// The type of a format named by a statement: the user's path, resolved at the declaration like
/// any type (a missing import or an unsupported payload is reported there).
fn format_type(format: &Format) -> TokenStream {
    let path = &format.0;
    quote!(#path)
}

/// How an endpoint names its body's or output's format. An inherited default is named through
/// its owner's hidden alias, located at the endpoint's payload type (`at`): the alias reports a
/// missing format once, at the default, while an unsupported payload is reported at the endpoint
/// that declares it.
fn format_for(
    model: &Model,
    format: &Format,
    origin: Origin,
    request: bool,
    at: Span,
) -> TokenStream {
    match origin {
        Origin::Endpoint => format_type(format),
        Origin::Default(owner) => {
            let o = &model.owners[owner];
            let d = if request { &o.request } else { &o.response };
            let alias = &d.as_ref().expect("an origin names a default").alias;
            located(quote!(#alias), at)
        }
    }
}

/// The format path of an output, as its decoder names it (for the stream reader's type).
fn output_format(model: &Model, o: &Output) -> TokenStream {
    let at = match o.origin {
        Origin::Endpoint => o.format.span(),
        Origin::Default(_) => o.ty.span(),
    };
    format_for(model, &o.format, o.origin, false, at)
}

/// One hidden alias per `request`/`response` default that has consumers:
/// `type __Concord{Owner}{Request|Response}Format = <format>;`, located at the default. It carries
/// `#[cfg(any(all(<cfgs of consumer 1>), ...))]`, the union of its consuming endpoints' effective
/// `cfg`s, so a default used only by cfg-disabled endpoints is never referenced, and an unused
/// default (no consumer) is not emitted at all.
fn format_aliases(model: &Model) -> TokenStream {
    let mut out = TokenStream::new();
    for owner in &model.owners {
        for d in [&owner.request, &owner.response].into_iter().flatten() {
            if d.consumers.is_empty() {
                continue;
            }
            let alias = &d.alias;
            let ty = format_type(&d.format);
            let cfg = consumers_cfg(model, d);
            out.extend(quote_spanned! {d.format.span()=>
                #cfg
                #[doc(hidden)]
                #[allow(non_camel_case_types, dead_code)]
                type #alias = #ty;
            });
        }
    }
    out
}

/// `#[cfg(any(all(..), ..))]` over the consumers' own and inherited `cfg` predicates; nothing when
/// one consumer is unconditional.
fn consumers_cfg(model: &Model, d: &OwnerDefault) -> TokenStream {
    owners_cfg(model, &d.consumers)
}

/// `#[cfg(any(all(..), ..))]` over `owners`' own and inherited `cfg` predicates; nothing when one
/// of them is unconditional.
fn owners_cfg(model: &Model, owners: &[usize]) -> TokenStream {
    match owners_cfg_predicate(model, owners) {
        Some(predicate) => quote!(#[cfg(#predicate)]),
        None => quote!(),
    }
}

/// The `cfg` predicate under which some of `owners` is compiled, or `None` when one of them is
/// always compiled.
fn owners_cfg_predicate(model: &Model, owners: &[usize]) -> Option<TokenStream> {
    let mut alternatives: Vec<TokenStream> = Vec::new();
    let mut seen = HashSet::new();
    for &e in owners {
        let predicates: Vec<TokenStream> = model.owners[e]
            .cfgs
            .iter()
            .filter_map(|attr| match &attr.meta {
                syn::Meta::List(list) => Some(list.tokens.clone()),
                _ => None,
            })
            .collect();
        if predicates.is_empty() {
            return None;
        }
        let all = quote!(all(#(#predicates),*));
        if seen.insert(all.to_string()) {
            alternatives.push(all);
        }
    }
    Some(quote!(any(#(#alternatives),*)))
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

/// Distinct notes in first-seen order. Membership is a hash lookup, so a value feeding N distinct
/// placements costs O(N) rather than a quadratic scan of the list.
#[derive(Clone, Default)]
struct Distinct {
    order: Vec<String>,
    seen: HashSet<String>,
}

impl Distinct {
    fn add(&mut self, what: String) {
        if !self.seen.contains(&what) {
            self.seen.insert(what.clone());
            self.order.push(what);
        }
    }
}

/// Generated notes for every owner's values (`notes[owner][value]`), computed in one pass over
/// the model: where the value is sent (every placement, template and block on any chain), whether
/// it is withheld from Concord's output, and what its absence means under a conditional
/// `override`. One pass, so the cost is linear in the declaration, not owners × contributions.
fn value_notes(model: &Model) -> Vec<Vec<Vec<String>>> {
    use crate::syntax::Op;
    let mut places: Vec<Vec<Distinct>> = model
        .owners
        .iter()
        .map(|o| vec![Distinct::default(); o.values.len()])
        .collect();
    let mut fallbacks = places.clone();
    let holes = |pieces: &[Piece]| -> Vec<ValueRef> {
        pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Hole(r) => Some(*r),
                Piece::Lit(_) => None,
            })
            .collect()
    };
    let add = |places: &mut Vec<Vec<Distinct>>, r: ValueRef, what: String| {
        places[r.owner][r.index].add(what);
    };
    for o in &model.owners {
        for c in &o.labels {
            for r in holes(&c.pieces) {
                add(&mut places, r, format!("subdomain label `{}`", c.text));
            }
        }
        for c in &o.segments {
            for r in holes(&c.pieces) {
                add(&mut places, r, format!("path segment `{}`", c.text));
            }
        }
        for (family, list) in [(Family::Query, &o.query), (Family::Header, &o.headers)] {
            let what = match family {
                Family::Query => "query parameter",
                Family::Header => "header",
            };
            for c in list.iter().filter(|c| c.op != Op::Remove) {
                let (reads, each) = match &c.source {
                    Some(Source::Ref { value, each }) => (vec![*value], *each),
                    Some(Source::Template { pieces }) => (holes(pieces), false),
                    None => (Vec::new(), false),
                };
                for r in reads {
                    let mut label = placement_label(family, &model.key_text(&c.key)).value();
                    if each {
                        label.push_str(" (one occurrence per item)");
                    }
                    add(&mut places, r, label);
                    if c.op == Op::Override && each && model.value(r).optional {
                        let x = plain(&model.value(r).name);
                        add(
                            &mut fallbacks,
                            r,
                            format!(
                                " Per call: `maybe_{x}(None)`{} keeps the inherited `{}`; `set_{x}(v)` replaces it with one occurrence per item of `v`, and with nothing at all when `v` has no items (for example `None` or an empty collection).",
                                // An initialized value starts present: absence is then not the default.
                                if model.value(r).default.is_none() {
                                    " (the default)"
                                } else {
                                    ""
                                },
                                model.key_text(&c.key)
                            ),
                        );
                    }
                    if c.op == Op::Override && model.value(r).optional {
                        add(
                            &mut fallbacks,
                            r,
                            format!(
                                " When `{}` is absent, the inherited `{}` {what} is sent unchanged (`override ... else inherit` in `{}`).",
                                plain(&model.value(r).name),
                                model.key_text(&c.key),
                                o.dotted
                            ),
                        );
                    }
                }
            }
        }
    }
    model
        .owners
        .iter()
        .zip(places.into_iter().zip(fallbacks))
        .map(|(owner, (places, fallbacks))| {
            owner
                .values
                .iter()
                .zip(places.into_iter().zip(fallbacks))
                .map(|(value, (places, fallbacks))| {
                    let mut notes = Vec::new();
                    if !places.order.is_empty() {
                        notes.push(format!(" Sent as: {}.", places.order.join("; ")));
                    }
                    if value.header {
                        notes.push(
                            " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text is checked by `http::HeaderValue::from_str`; integers and `bool` are written in decimal / `true`/`false`."
                                .to_owned(),
                        );
                    }
                    if value.debug_sensitive {
                        notes.push(
                            " Sensitive: sent normally, withheld from Concord's `Debug` output and errors."
                                .to_owned(),
                        );
                    } else if !value.debug_when.is_empty() {
                        notes.push(
                            " Withheld from Concord's `Debug` output when a native header name it is sent under is on Concord's list of sensitive names, or names the same header as a sensitive value."
                                .to_owned(),
                        );
                    }
                    notes.extend(fallbacks.order);
                    notes
                })
                .collect()
        })
        .collect()
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

/// The payload type a body is supplied as (`json b: T` / `body b: T` → `T`).
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
            expr_text(quote!(#expr))
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
            #[must_use = "a setter returns the updated handle; discarding it discards the change"]
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
                #[must_use = "a setter returns the updated handle; discarding it discards the change"]
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
                #[doc = " Sets the request body: `value` encoded by the request format, sent even if it encodes as `null` or as nothing. Initially absent: no body is sent."]
                #[inline] pub fn #set(mut self, value: impl #k::IntoValue<#ty>) -> Self {
                    #target.#name = ::core::option::Option::Some(#k::IntoValue::into_value(value));
                    self
                }
                #(#docs)*
                #[doc = ""]
                #[doc = " Sets the request body to exactly `value`: `None` sends no body, `Some(v)` sends `v` encoded by the request format."]
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
///
/// A group holding a group without delimiters (a fragment forwarded by `macro_rules!`,
/// `$e:expr`), or that group itself, is kept as it came: rebuilt from its tokens, rustc would
/// parse the fragment as if ungrouped, and `pick!($e * 2)` with `$e = 1 + 2` would name
/// `Payload<5>` here and `Payload<6>` elsewhere.
fn located(tokens: TokenStream, at: Span) -> TokenStream {
    fn invisible(stream: TokenStream) -> bool {
        stream.into_iter().any(|tt| match tt {
            proc_macro2::TokenTree::Group(g) => {
                g.delimiter() == proc_macro2::Delimiter::None || invisible(g.stream())
            }
            _ => false,
        })
    }
    tokens
        .into_iter()
        .map(|tt| match tt {
            proc_macro2::TokenTree::Group(g)
                if g.delimiter() == proc_macro2::Delimiter::None || invisible(g.stream()) =>
            {
                proc_macro2::TokenTree::Group(g)
            }
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

/// The request an endpoint builds, as rustdoc derived from the resolved model: method and URL
/// template (`{x}` holes are filled from values), then the query and header keys it may send.
fn request_summary(model: &Model, endpoint: usize, method: &Ident) -> TokenStream {
    let Some(composed) = model.owners[endpoint].composed.as_ref() else {
        return quote!();
    };
    let base = match &model.owners[0].kind {
        Kind::Client { base, .. } => base.value(),
        _ => return quote!(),
    };
    let (scheme, rest) = base.split_once("://").unwrap_or(("", base.as_str()));
    let (host, base_path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let mut url = format!("{scheme}://");
    for &(o, i) in &composed.labels {
        url.push_str(&model.owners[o].labels[i].text);
        url.push('.');
    }
    url.push_str(host);
    if composed.segments.is_empty() && !composed.trailing_slash {
        url.push_str(base_path);
    } else {
        url.push_str(base_path.strip_suffix('/').unwrap_or(base_path));
        for &(o, i) in &composed.segments {
            url.push('/');
            url.push_str(&model.owners[o].segments[i].text);
        }
        if composed.trailing_slash {
            url.push('/');
        }
    }
    let keys = |slots: &[crate::model::Slot]| -> String {
        let shown: Vec<String> = slots
            .iter()
            .filter(|s| !s.items.is_empty())
            .map(|s| {
                if s.sensitive {
                    format!("`{}` (sensitive)", model.key_text(&s.key))
                } else {
                    format!("`{}`", model.key_text(&s.key))
                }
            })
            .collect();
        shown.join(", ")
    };
    let content_type = KeyName::Lit("content-type".to_owned());
    let mut lines = vec![lit(""), lit(&format!(" Request: `{method} {url}`."))];
    if let Kind::Endpoint {
        output,
        incoming,
        body,
        ..
    } = &model.owners[endpoint].kind
    {
        if let Some(b) = body {
            let presence = if b.optional { "optional " } else { "" };
            // 8b: a `content-type` header contribution surviving composition replaces the
            // format's media type, and is sent with or without the body.
            let header = match &composed.headers {
                HeaderComposition::Static(slots) => slots
                    .iter()
                    .any(|s| s.key == content_type && !s.items.is_empty()),
                HeaderComposition::Dynamic(seq) => seq.iter().any(|&id| {
                    let c = model.contrib(id);
                    c.key == content_type && c.op != crate::syntax::Op::Remove
                }),
            };
            // A native key may name `content-type` (the macro cannot tell): say so.
            let native_maybe = !header
                && matches!(&composed.headers, HeaderComposition::Dynamic(seq) if seq.iter().any(|&id| {
                    let c = model.contrib(id);
                    matches!(c.key, KeyName::Path(_)) && c.op != crate::syntax::Op::Remove
                }));
            let media = match (&b.media, &b.format) {
                (Some((m, origin)), _) => format!(
                    " `Content-Type: {}` ({}) is sent with the body; a format that binds another media type to the body at run time (`Encode::encode_body`) is a `Build` error.",
                    m.value(),
                    match origin {
                        Origin::Endpoint => "declared by the endpoint request statement".to_owned(),
                        Origin::Default(o) =>
                            format!("from the `request` default of {}", model.describe(*o)),
                    }
                ),
                (None, _) if header => " `Content-Type`: the declared `content-type` header when it is sent (with or without the body), otherwise the format's media type with the body; a media type the format binds to the body at run time (`Encode::encode_body`) must equal a sent header, else a `Build` error.".to_owned(),
                (None, _) => {
                    " `Content-Type`: the format's media type, sent with the body: the one its `Encode::encode_body` binds to the body at run time, else its `Encode::CONTENT_TYPE`."
                        .to_owned()
                }
            };
            let media = if native_maybe && b.media.is_none() {
                format!(
                    "{media} A header of this chain whose native name is `content-type` replaces it (and is sent with or without the body)."
                )
            } else {
                media
            };
            let absent = if b.optional {
                " Absent: no body, and the format adds no `Content-Type`."
            } else {
                ""
            };
            lines.push(lit(&format!(
                " Body ({presence}`{}`): `{}`, encoded with the format {}.{media}{absent}",
                plain(&b.name),
                type_text(&b.ty),
                format_origin(model, &b.format, b.origin, true),
            )));
        }
        lines.push(lit(&match output {
            Some(o) if o.stream => format!(
                " Output: a stream of `{}`, opened from a 2xx response with the format {} (`Incremental<{}>`): `send()` returns the format's reader without reading the body; the format checks the response head first (a refusal is `ErrorKind::Metadata`, the response kept unread), its later read errors name this endpoint, and its `Incremental::ACCEPT`, if any, is sent when this endpoint's headers leave no `accept` and do not end by removing it.",
                type_text(&o.ty),
                format_origin(model, &o.format, o.origin, false),
                type_text(&o.ty),
            ),
            Some(o) => {
                let unit = if matches!(&o.ty, syn::Type::Tuple(t) if t.elems.is_empty()) {
                    format!(" `()` is decoded like any output type, with `{}`. An endpoint without `->` decodes no output and accepts any 2xx.", o.format.text())
                } else {
                    String::new()
                };
                format!(
                    " Output: `{}`, decoded from a 2xx response with the format {}: the format checks the response head before the body is read (a refusal is `ErrorKind::Metadata`, the response kept unread), and its `Decode::ACCEPT`, if any, is sent when this endpoint's headers leave no `accept` and do not end by removing it.{unit}",
                    type_text(&o.ty),
                    format_origin(model, &o.format, o.origin, false),
                )
            }
            None if *incoming => {
                let ignored = match model.nearest_default(endpoint, false) {
                    Some(o) => format!(
                        "; the `response` default `{}` of {} does not apply",
                        model.owners[o].response.as_ref().expect("nearest").format.text(),
                        model.describe(o)
                    ),
                    None => String::new(),
                };
                format!(
                    " Output: the unread body (`Incoming`) of a 2xx response, for an incremental reader chosen by the caller (it checks the head itself and names this endpoint in its errors); nothing is read here{ignored}."
                )
            }
            None => {
                let ignored = match model.nearest_default(endpoint, false) {
                    Some(o) => format!(
                        "; the `response` default `{}` of {} does not apply",
                        model.owners[o].response.as_ref().expect("nearest").format.text(),
                        model.describe(o)
                    ),
                    None => String::new(),
                };
                format!(
                    " Output: none (`()`): any 2xx succeeds and the response body is drained{ignored}."
                )
            }
        }));
        // With a send policy, `send()` returns the policy's output, not the declared handling's.
        if let Kind::Client {
            send: Some(policy), ..
        } = &model.owners[0].kind
        {
            let shown = match model.send_value() {
                Some(vi) => format!(
                    "the client value `{}`",
                    plain(&model.owners[0].values[vi].name)
                ),
                None => format!("`{}`", type_text(policy)),
            };
            lines.push(lit(&format!(
                " With the client's send policy {shown}, `send()` returns its `SendWith<T>::Output` for this output `T`; `prepare()` (then `send()` or `receive()` on the `Prepared`) applies the declared handling above."
            )));
        }
    }
    let query = keys(&composed.query);
    if !query.is_empty() {
        lines.push(lit(&format!(" Query (when present): {query}.")));
    }
    match &composed.headers {
        HeaderComposition::Static(slots) => {
            let headers = keys(slots);
            if !headers.is_empty() {
                lines.push(lit(&format!(" Headers (when present): {headers}.")));
            }
        }
        HeaderComposition::Dynamic(seq) => {
            // The macro cannot tell which contributions name the same header, so the operations
            // are listed in chain order (`remove X_RIOT_TOKEN` under a client that sets it).
            let mut shown: Vec<String> = Vec::new();
            for &id in seq {
                let c = model.contrib(id);
                let op = match (c.op, c.else_inherit) {
                    (crate::syntax::Op::Introduce, _) => "",
                    (crate::syntax::Op::Append, _) => "append ",
                    (crate::syntax::Op::Override, false) => "override ",
                    (crate::syntax::Op::Override, true) => "override else inherit ",
                    (crate::syntax::Op::Remove, _) => "remove ",
                };
                let text = format!(
                    "`{op}{}`{}",
                    model.key_text(&c.key),
                    if c.sensitive { " (sensitive)" } else { "" }
                );
                // Every operation, repeats included: `KEY`, `remove KEY`, `KEY` sends the second.
                shown.push(text);
            }
            if !shown.is_empty() {
                lines.push(lit(&format!(
                    " Headers, in chain order (sent when present; native names are compared when the request is built): {}.",
                    shown.join(", ")
                )));
            }
        }
    }
    quote!(#(#[doc = #lines])*)
}

/// "`json` (the endpoint statement)" / "`json` (the `request` default of the client `Shop`)".
fn format_origin(model: &Model, format: &Format, origin: Origin, request: bool) -> String {
    let origin = match origin {
        Origin::Endpoint => "the endpoint statement".to_owned(),
        Origin::Default(o) => format!(
            "the `{}` default of {}",
            if request { "request" } else { "response" },
            model.describe(o)
        ),
    };
    // An intra-doc link: the format's own documentation says what it sends and accepts.
    format!("[`{0}`]({0}) ({origin})", format.text())
}

/// The format defaults that reach the endpoints below a client or scope, for its documentation.
fn defaults_line(model: &Model, index: usize) -> Option<LitStr> {
    if model.owners[index].is_endpoint() {
        return None;
    }
    let describe = |o: usize, request: bool| {
        let owner = &model.owners[o];
        let d = if request {
            &owner.request
        } else {
            &owner.response
        };
        let d = d.as_ref().expect("nearest has a default");
        let origin = if o == index {
            "declared here".to_owned()
        } else {
            format!("the default of {}", model.describe(o))
        };
        let media = match &d.media {
            Some(m) => format!(", media type `{}`", m.value()),
            None => String::new(),
        };
        format!("`{}` ({origin}{media})", d.format.text())
    };
    let parts: Vec<String> = [
        model
            .nearest_default(index, true)
            .map(|o| format!("request bodies {}", describe(o, true))),
        model
            .nearest_default(index, false)
            .map(|o| format!("typed outputs {}", describe(o, false))),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| {
        lit(&format!(
            " Formats of the endpoints below: {}. The nearest format statement wins (endpoint, inner scopes, client); an endpoint without `->` drains its response.",
            parts.join("; ")
        ))
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
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
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
    out.join(" ")
}

fn debug_fields<'a>(
    k: &TokenStream,
    model: &Model,
    values: impl IntoIterator<Item = &'a Value>,
    base: &TokenStream,
) -> TokenStream {
    let mut out = TokenStream::new();
    for value in values {
        let name = &value.name;
        let label = lit(&plain(name));
        let place = quote!(#base.#name);
        let span = value.ty.span();
        // A value read as text is shown through `Display`; one read only as whole header values
        // through its header value (no `Display` needed). Either way a value whose header value
        // carries the native sensitivity flag is withheld.
        let (scalar, each) = if value.text {
            (quote!(DebugDisplay), quote!(DebugEach))
        } else {
            (quote!(DebugHeader), quote!(DebugHeaderEach))
        };
        // A `query(..)` value has no `Display`: withheld like a sensitive one.
        let shown = match (
            value.debug_sensitive || value.pairs.is_some(),
            value.optional,
            value.usage,
        ) {
            // Never used: the declaration is rejected (`Model::build`); its value is not shown
            // and adds no trait obligation to the error.
            (_, _, None) => quote_spanned!(span=> &#k::__private::Redacted),
            (true, false, _) => quote_spanned!(span=> &#k::__private::Redacted),
            (true, true, _) => {
                quote_spanned!(span=> &#place.as_ref().map(|_| #k::__private::Redacted))
            }
            (false, false, Some(Usage::Each)) => {
                quote_spanned!(span=> &#k::__private::#each(&#place))
            }
            (false, true, Some(Usage::Each)) => {
                quote_spanned!(span=> &#place.as_ref().map(#k::__private::#each))
            }
            (false, false, _) => {
                quote_spanned!(span=> &#k::__private::#scalar(&#place))
            }
            (false, true, _) => {
                quote_spanned!(span=> &#place.as_ref().map(#k::__private::#scalar))
            }
        };
        // A classification that depends on native header names is decided when rendering.
        let shown = if value.debug_sensitive || value.debug_when.is_empty() || value.usage.is_none()
        {
            shown
        } else {
            let when = when_fn(model, value_ref(model, value));
            quote_spanned!(span=> &#k::__private::Withheld(#when(), #shown))
        };
        // One statement per field (not a method chain): thousands of values must not nest.
        out.extend(located(quote!(__d.field(#label, #shown);), span));
    }
    out
}

fn emit_owner(
    k: &TokenStream,
    model: &Model,
    index: usize,
    vis: &syn::Visibility,
    notes: &[Vec<String>],
    headers: &Headers,
) -> TokenStream {
    let owner = &model.owners[index];
    let ty = &owner.type_name;
    let state = &owner.state_name;
    let docs = owner_docs(model, index);
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
    let type_label = lit(&plain(ty));

    match &owner.kind {
        Kind::Client { base, .. } => {
            let (default_fns, inits) = value_inits(k, &owner.values);
            let controls = controls(
                k,
                &owner.values,
                None,
                &quote!(::std::sync::Arc::make_mut(&mut self.__s)),
                "the client",
                notes,
            );
            let debug = debug_fields(k, model, &owner.values, &quote!(__s));
            let required: Vec<&Value> = owner.values.iter().filter(|v| v.is_positional()).collect();
            let (config_item, config_param, config_bind) = if required.is_empty() {
                (quote!(), quote!(), quote!())
            } else {
                let config = owner.config_name();
                let names: Vec<&Ident> = required.iter().map(|v| &v.name).collect();
                let config_fields = owner
                    .values
                    .iter()
                    .zip(notes)
                    .filter(|(v, _)| v.is_positional())
                    .map(|(v, notes)| {
                        let name = &v.name;
                        let fty = &v.ty;
                        let docs = if v.docs.is_empty() {
                            let line =
                                lit(&format!(" The client's required value `{}`.", plain(name)));
                            quote!(#[doc = #line])
                        } else {
                            let docs = &v.docs;
                            quote!(#(#docs)*)
                        };
                        let notes: Vec<LitStr> = notes.iter().map(|n| lit(n)).collect();
                        quote!(#docs #[doc = ""] #(#[doc = #notes])* pub #name: #fty,)
                    });
                let later = match (
                    owner.values.iter().any(|v| v.optional),
                    owner
                        .values
                        .iter()
                        .any(|v| !v.optional && v.default.is_some()),
                ) {
                    (true, true) => {
                        " Optional and initialized values are set afterwards with the client's `set_*` and `maybe_*` methods."
                    }
                    (true, false) => {
                        " Optional values are set afterwards with the client's `set_*` and `maybe_*` methods."
                    }
                    (false, true) => {
                        " Initialized values are set afterwards with the client's `set_*` methods."
                    }
                    (false, false) => "",
                };
                let config_doc = lit(&format!(
                    " The required values of [`{ty}`], passed to [`{ty}::new`].{later}"
                ));
                let config_label = lit(&config.to_string());
                let config_debug = debug_fields(k, model, required.iter().copied(), &quote!(self));
                (
                    quote! {
                        #[doc = #config_doc]
                        #[derive(::core::clone::Clone)]
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
                // The generated docs link each format by its path (`[`F`](F)`); rustdoc calls such a
                // link redundant when the path also resolves as an intra-doc link.
                #[allow(rustdoc::redundant_explicit_links)]
                #[derive(::core::clone::Clone)]
                #vis struct #ty {
                    __s: ::std::sync::Arc<#state>,
                }

                #[doc(hidden)]
                #[derive(::core::clone::Clone)]
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
                    /// local test server). It must be an `http`/`https` URL with a host, no port 0
                    /// and no user information, query or fragment; anything else is an error.
                    /// Declared paths are appended to its path. With `subdomain` labels the host
                    /// must be a domain name: to reach a local server, keep a domain and map it
                    /// with `reqwest::ClientBuilder::resolve`. Returns a new handle: scopes and
                    /// requests created from the old one keep the old base.
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
                notes,
            );
            let debug = debug_fields(k, model, &owner.values, &quote!(__s));
            quote! {
                #docs
                // The generated docs link each format by its path (`[`F`](F)`); rustdoc calls such a
                // link redundant when the path also resolves as an intra-doc link.
                #[allow(rustdoc::redundant_explicit_links)]
                #[derive(::core::clone::Clone)]
                #vis struct #ty {
                    __s: ::std::sync::Arc<#state>,
                }

                #[doc(hidden)]
                #[derive(::core::clone::Clone)]
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
            incoming,
            body,
        } => {
            let incoming = *incoming;
            let op_name = lit(&owner.dotted);
            let op_method = lit(&method.to_string());
            let parent_ty = &model.owners[owner.parent.unwrap()].type_name;
            let controls = controls(
                k,
                &owner.values,
                body.as_ref(),
                &quote!(self),
                "this request",
                notes,
            );
            let mut debug = debug_fields(k, model, &owner.values, &quote!(self));
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
                Some(p) if p.stream => {
                    let t = &p.ty;
                    let r = output_format(model, p);
                    quote!(#k::Records<#r, #t>)
                }
                Some(p) => {
                    let t = &p.ty;
                    quote!(#t)
                }
                None if incoming => quote!(#k::Incoming),
                None => quote!(()),
            };
            let send_docs = if incoming || output.as_ref().is_some_and(|o| o.stream) {
                quote! {
                    /// Sends the request once (`prepare` then one `reqwest::Client::execute`) and,
                    /// for a 2xx response, returns the body **unread** for incremental reading
                    /// (the stream format's reader, or the `Incoming` body for a reader you
                    /// choose): nothing is drained or collected. A non-2xx status is
                    /// `ErrorKind::Status` holding the unread response (`Error::into_response`);
                    /// Concord never retries. Dropping the result closes the response.
                    ///
                    /// # Errors
                    ///
                    /// `Build` as for `prepare`, then `Transport`, `Status` or `Metadata` (the
                    /// stream format refused the response head; the response is kept unread).
                    /// Later read errors come from the reader and name this endpoint.
                }
            } else {
                quote! {
                    /// Sends the request once (`prepare` then one `reqwest::Client::execute`) and
                    /// decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the
                    /// unread response (`Error::into_response`); Concord never retries.
                    ///
                    /// # Errors
                    ///
                    /// `Build` as for `prepare`, then `Transport`, `Status`, `Metadata` (the
                    /// format refused the response head; the response is kept unread) or
                    /// `Decode` (see `ErrorKind`).
                }
            };
            let (policy, declared_output) = match &model.owners[0].kind {
                Kind::Client {
                    send, send_output, ..
                } => (send.as_ref(), send_output.as_ref()),
                _ => (None, None),
            };
            // `Sends<O>` for an endpoint whose client names a send policy (its `send()` future is
            // already `Send`); none otherwise.
            let mut sends_impl = TokenStream::new();
            let send_fn = match policy {
                None => quote! {
                    #send_docs
                    #[inline] pub async fn send(self) -> ::core::result::Result<#out, #k::Error> {
                        self.prepare()?.send().await
                    }
                },
                Some(policy) => {
                    // `send value;`: clone the client value; `send Type;`: `Type::default()`.
                    let (policy_ty, policy_init, shown) = match model.send_value() {
                        Some(vi) => {
                            let value = &model.owners[0].values[vi];
                            let name = &value.name;
                            let vty = &value.ty;
                            let depth = model.chain(index).len() - 1;
                            let mut root = quote!(self.__parent.__s);
                            for _ in 1..depth {
                                root = quote!(#root.__parent.__s);
                            }
                            (
                                quote!(#vty),
                                quote!(::core::clone::Clone::clone(&#root.#name)),
                                format!("the client value `{}`", plain(name)),
                            )
                        }
                        None => (
                            quote!(#policy),
                            quote!(<#policy as ::core::default::Default>::default()),
                            format!("`{}`", type_text(policy)),
                        ),
                    };
                    // Name the policy as a link when it is a plain path, so a reader reaches the
                    // `SendWith` implementation that defines this method's result.
                    let policy_ty_text = type_text(&policy_ty_for_docs(model, policy));
                    let linked = if policy_ty_text
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
                    {
                        format!("[`{policy_ty_text}`]")
                    } else {
                        format!("`{policy_ty_text}`")
                    };
                    // `send P -> Template;`: the declared result, `_` replaced by this output.
                    let declared = declared_output.map(|t| (t.span(), with_output(t, &out)));
                    let policy_text = lit(&match &declared {
                        Some(_) => format!(
                            " Sends the request with the client's send policy {shown}, whose result the client declares; its errors, metadata and signing are defined by the `SendWith` implementation of {linked}. `prepare()` builds the declared request without the policy, and `prepare()?.send()` uses Concord's own handling and error instead."
                        ),
                        None => format!(
                            " Sends the request with the client's send policy {shown}: the result is `<{policy_ty_text} as SendWith<T>>::Output`, defined by the `SendWith` implementation of {linked} (its errors, metadata, signing). `prepare()` builds the declared request without the policy, and `prepare()?.send()` uses Concord's own handling and error instead."
                        ),
                    });
                    // Spanned at the `send` statement: a type that is no policy is reported there
                    // (with the generated `send()`'s own error), not inside Concord's trait.
                    let at = policy.span();
                    let send_output = match &declared {
                        Some((_, declared)) => declared.clone(),
                        None => quote_spanned!(at=> <#policy_ty as #k::SendWith<#out>>::Output),
                    };
                    // A declared result that is not the policy's output is reported at the
                    // template (`expected Result<..>, found <P as SendWith<T>>::Output`).
                    let call = match &declared {
                        Some((tat, declared)) => located(
                            quote! {
                                let __out: #declared = <#policy_ty as #k::SendWith<#out>>::send(&__policy, self).await;
                                __out
                            },
                            *tat,
                        ),
                        None => {
                            quote!(<#policy_ty as #k::SendWith<#out>>::send(&__policy, self).await)
                        }
                    };
                    sends_impl = quote_spanned! {at=>
                        impl #k::Sends<#send_output> for #ty where #policy_ty: #k::SendWith<#out> {
                            #[inline] fn send(self) -> impl ::core::future::Future<Output = #send_output> + ::core::marker::Send {
                                #ty::send(self)
                            }
                        }
                    };
                    quote! {
                        #[doc = #policy_text]
                        #[inline] pub fn send(self) -> impl ::core::future::Future<Output = #send_output> + ::core::marker::Send {
                            let __policy: #policy_ty = #policy_init;
                            async move { #call }
                        }
                    }
                }
            };
            let prepare = prepare_body(
                k,
                headers,
                model,
                index,
                method,
                output.as_ref(),
                body.as_ref(),
            );
            let prepare_note = if policy.is_some() {
                quote! {
                    #[doc = ""]
                    #[doc = " With the client's send policy this is the declared request **before** the policy (not signed or decorated by it); `send()` goes through the policy, `prepare()?.send()` does not."]
                }
            } else {
                TokenStream::new()
            };
            let shapes = shapes_static(k, model, index);
            let limit = limit_const(model, index, output.as_ref());
            quote! {
                #shapes

                #docs
                #[allow(rustdoc::redundant_explicit_links)]
                #[must_use = "an endpoint request does nothing until `send` or `prepare` is called"]
                #vis struct #ty {
                    __parent: #parent_ty,
                    #(#fields)*
                    #body_field
                }

                #clone_impl

                impl #ty {
                    #controls
                    #limit

                    /// Builds the native request without sending it.
                    ///
                    /// # Errors
                    ///
                    /// `ErrorKind::Build` when a value cannot be written (a path segment, host
                    /// label, header value or body) or two native header names collide; nothing
                    /// is sent.
                    #prepare_note
                    #[inline] pub fn prepare(self) -> ::core::result::Result<#k::Prepared<#out>, #k::Error> {
                        #prepare
                    }

                    #send_fn
                }

                #sends_impl

                impl #k::Endpoint<#out> for #ty {
                    #[inline] fn prepare(self) -> ::core::result::Result<#k::Prepared<#out>, #k::Error> {
                        #ty::prepare(self)
                    }
                    #[inline] fn operation() -> #k::Operation {
                        #k::__private::declared(#op_name, #op_method)
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

/// The declared finite-body bound of an endpoint with a finite typed output, as a private
/// associated constant: its initializer resolves names in the caller's module only (no local
/// statics in scope) and its type is the primitive `usize` whatever the caller calls `usize`.
fn limit_const(model: &Model, index: usize, output: Option<&Output>) -> TokenStream {
    match (output, model.nearest_limit(index)) {
        (Some(o), Some(limit)) if !o.stream => {
            quote!(const __CONCORD_LIMIT: ::core::primitive::usize = #limit;)
        }
        _ => TokenStream::new(),
    }
}

/// A `send P -> Template;` result for one endpoint: every `_` of the template becomes `out`.
fn with_output(template: &syn::Type, out: &TokenStream) -> TokenStream {
    fn replace(tokens: TokenStream, out: &TokenStream) -> TokenStream {
        tokens
            .into_iter()
            .map(|tt| match tt {
                proc_macro2::TokenTree::Ident(i) if i == "_" => proc_macro2::TokenTree::Group(
                    proc_macro2::Group::new(proc_macro2::Delimiter::None, out.clone()),
                ),
                proc_macro2::TokenTree::Group(g) => {
                    let mut group =
                        proc_macro2::Group::new(g.delimiter(), replace(g.stream(), out));
                    group.set_span(g.span());
                    proc_macro2::TokenTree::Group(group)
                }
                tt => tt,
            })
            .collect()
    }
    replace(quote!(#template), out)
}

/// The unused binder of a `Clone` bound: `'__concord`, unless the type itself already names that
/// lifetime (a payload such as `for<'__concord> fn(&'__concord str)` is valid Rust), in which case
/// the first `'__concordN` the type does not name. A binder that shadowed one of the type's own
/// lifetimes would be E0496.
fn unused_lifetime(ty: &TokenStream, span: Span) -> syn::Lifetime {
    fn collect(tokens: TokenStream, used: &mut HashSet<String>) {
        let mut after_apostrophe = false;
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Punct(p) => {
                    after_apostrophe = p.as_char() == '\'';
                    continue;
                }
                proc_macro2::TokenTree::Ident(i) if after_apostrophe => {
                    used.insert(i.to_string());
                }
                proc_macro2::TokenTree::Group(g) => collect(g.stream(), used),
                _ => {}
            }
            after_apostrophe = false;
        }
    }
    let mut used = HashSet::new();
    collect(ty.clone(), &mut used);
    let mut name = String::from("__concord");
    let mut n = 0u32;
    while used.contains(&name) {
        n += 1;
        name = format!("__concord{n}");
    }
    syn::Lifetime::new(&format!("'{name}"), span)
}

/// `Clone` for an endpoint request exactly when every value type (and the body value) is `Clone`.
///
/// The type is not generic, so a plain `where Value: Clone` bound would be rejected on stable
/// (trivially false bound, rust-lang/rust#48214). Each bound carries an unused higher-ranked
/// lifetime instead: such bounds are not checked where the impl is declared, so the impl simply
/// does not apply when a value is not `Clone`, and a `.clone()` call reports the unmet bound.
///
/// Every occurrence keeps its own bound, even when two print the same: equal tokens can name
/// different types (`$crate::Payload` forwarded by two crates' macros), which a proc macro cannot
/// tell apart, so a bound dropped as a duplicate could be the one of the non-`Clone` type.
fn endpoint_clone(ty: &Ident, values: &[Value], body: Option<&Body>) -> TokenStream {
    let mut bounds = Vec::new();
    let mut add = |t: TokenStream, span: Span| {
        let binder = unused_lifetime(&t, span);
        bounds.push(quote_spanned!(span=> for<#binder> #t: ::core::clone::Clone));
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
    // Shown by rustdoc beside the bounds, which otherwise read as an artifact (SDK23 consumer).
    let note = if bounds.is_empty() {
        quote!()
    } else {
        quote!(#[doc = " `Clone` exactly when every value of the request (and its body) is `Clone`. Each `for<'__concord> T: Clone` bound states that for one value type; the unused lifetime only lets the bound be written on a non-generic type."])
    };
    quote! {
        #note
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

/// The owner's documentation, or a generated line when it has none, and the format defaults that
/// reach the endpoints below a client or scope.
fn owner_docs(model: &Model, index: usize) -> TokenStream {
    let owner = &model.owners[index];
    let defaults = defaults_line(model, index).map(|l| quote!(#[doc = ""] #[doc = #l]));
    // An endpoint's own type (where `prepare` and `send` live) states the request it builds, as
    // its navigation method does: method, URL, body format and media type, output format, origins.
    let defaults = match &owner.kind {
        Kind::Endpoint { method, .. } => {
            let summary = request_summary(model, index, method);
            // The parameter groups it includes, and the bound of its finite body read.
            let mut notes = Vec::new();
            if !owner.groups.is_empty() {
                let names: Vec<String> = owner
                    .groups
                    .iter()
                    .map(|g| format!("`{}`", plain(g)))
                    .collect();
                notes.push(format!(
                    " Includes the parameter group{} {} (each also a trait of its setters).",
                    if names.len() > 1 { "s" } else { "" },
                    names.join(", ")
                ));
            }
            // `query(..)` values choose their keys at run time: name the type that documents them.
            for value in owner.values.iter().filter(|v| v.pairs.is_some()) {
                let ty = &value.ty;
                notes.push(format!(
                    " `{}` writes its own query keys: see `{}` for which.",
                    plain(&value.name),
                    quote!(#ty).to_string().replace(' ', "")
                ));
            }
            let has_finite =
                matches!(&owner.kind, Kind::Endpoint { output: Some(o), .. } if !o.stream);
            if has_finite && let Some(limit) = model.nearest_limit(index) {
                let text = quote!(#limit).to_string();
                notes.push(format!(
                    " Its finite body read is bounded by `limit {text};` (`ErrorKind::Limit` beyond)."
                ));
            }
            let notes = notes.iter().map(|n| lit(n));
            quote!(#defaults #summary #(#[doc = ""] #[doc = #notes])*)
        }
        _ => quote!(#defaults),
    };
    if !owner.docs.is_empty() {
        let docs = &owner.docs;
        return quote!(#(#docs)* #defaults);
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
    quote!(#[doc = #line] #defaults)
}

/// The type named by the client's `send` statement: the policy type, or the type of the client
/// value named by `send value;`.
fn policy_ty_for_docs(model: &Model, policy: &syn::Type) -> syn::Type {
    match model.send_value() {
        Some(vi) => model.owners[0].values[vi].ty.clone(),
        None => policy.clone(),
    }
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
        let scope_defaults = defaults_line(model, child).map(|l| quote!(#[doc = ""] #[doc = #l]));
        let args = argument_docs(&c.values, body);
        // `#[cfg]` and `#[deprecated]` of a child apply to its navigation method.
        let nav_attrs: Vec<&syn::Attribute> =
            c.own_cfgs.iter().chain(c.deprecated.iter()).collect();
        let scope_sep = if docs.is_empty() {
            quote!()
        } else {
            quote!(#[doc = ""])
        };
        match &c.kind {
            Kind::Scope => out.extend(quote! {
                #(#docs)*
                #scope_sep
                #[doc = #scope_line]
                #scope_defaults
                #args
                #(#nav_attrs)*
                #[must_use = "entering a scope only returns its handle"]
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
                let summary = request_summary(model, child, method);
                out.extend(quote! {
                    #(#docs)*
                    #sep
                    #[doc = #line]
                    #summary
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

/// The call sending one contribution. For a header, `name` is an expression of type
/// `HeaderName` and `sensitive` one of type `bool` (Concord's classification, OR-ed with a native
/// value's own flag).
fn source_code(
    k: &TokenStream,
    headers: &Headers,
    model: &Model,
    places: &Places,
    id: ContribId,
    name: &TokenStream,
    sensitive: &TokenStream,
) -> (Bound, TokenStream) {
    let contrib = model.contrib(id);
    let mut bound = Bound {
        patterns: Vec::new(),
        scrutinees: Vec::new(),
    };
    let key_text = model.key_text(&contrib.key);
    let what = placement_label(id.family, &key_text);
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
                        // Through `__private::text`, so a value without `Display` gets
                        // Concord's diagnostic, at its type.
                        if model.value(*r).optional {
                            let b = format_ident!("__h{}", bound.patterns.len());
                            bound
                                .patterns
                                .push(quote!(::core::option::Option::Some(#b)));
                            bound.scrutinees.push(quote!(&#place));
                            args.push(located(quote!(#k::__private::text(#b)), at));
                        } else {
                            args.push(located(quote!(#k::__private::text(&#place)), at));
                        }
                    }
                }
            }
            if args.is_empty() && id.family == Family::Header {
                // No holes: the validated static `HeaderValue` of a visible-ASCII literal, or the
                // checked native constructor for any other literal `http` accepts.
                match headers.values.get(&literal) {
                    Some(value) => (quote!(&#value), false, None),
                    None => {
                        let bytes = syn::LitByteStr::new(literal.as_bytes(), contrib.span);
                        (
                            quote!(&#k::__private::header_literal(#bytes, &__META, #what)?),
                            false,
                            None,
                        )
                    }
                }
            } else if args.is_empty() {
                // No holes: the decoded literal itself (`{{` already became `{`).
                let literal = lit(&literal);
                (quote!(#literal), false, None)
            } else {
                let fmt = lit(&fmt);
                (
                    quote!(&#k::__private::format_template(::core::format_args!(#fmt, #(#args),*), &__META, #what)?),
                    false,
                    None,
                )
            }
        }
    };
    let call = match (id.family, each) {
        (Family::Query, false) => {
            let key = lit(&key_text);
            quote!(__p.query(#key, #arg, #what)?;)
        }
        (Family::Query, true) => {
            let key = lit(&key_text);
            quote!(__p.query_each(#key, #arg, #what)?;)
        }
        (Family::Header, false) => quote!(__p.header(#name, #what, #arg, #sensitive)?;),
        (Family::Header, true) => quote!(__p.header_each(#name, #what, #arg, #sensitive)?;),
    };
    // A value placed directly: trait errors (no `Display`, not iterable) point at its type.
    let call = match at {
        Some(at) => located(call, at),
        None => call,
    };
    (bound, call)
}

/// A statically composed slot's items. Header names are the literal's validated static.
fn items_code(
    k: &TokenStream,
    headers: &Headers,
    model: &Model,
    places: &Places,
    items: &[Item],
    slot: &Slot,
    family: Family,
) -> TokenStream {
    let (name, sensitive) = match (family, &slot.key) {
        (Family::Header, key) => {
            let name = key_ref(model, headers, key);
            // Sticky, like `slot.sensitive`: every contribution ever applied to the slot.
            (
                quote!(::core::clone::Clone::clone(#name)),
                flag_expr(model, slot.sensitive, &slot.history),
            )
        }
        (Family::Query, _) => (quote!(), quote!()),
    };
    let mut out = TokenStream::new();
    for item in items {
        match item {
            Item::Emit(id) => {
                let (bound, call) = source_code(k, headers, model, places, *id, &name, &sensitive);
                out.extend(bound.wrap(call, None));
            }
            Item::Override { src, fallback } => {
                let (bound, call) = source_code(k, headers, model, places, *src, &name, &sensitive);
                let otherwise = items_code(k, headers, model, places, fallback, slot, family);
                out.extend(bound.wrap(call, Some(otherwise)));
            }
        }
    }
    out
}

/// The resolved names of a native chain, as `let` bindings (literal names are their validated
/// statics; a native key path goes through its accessor) and the expressions naming them. Shared
/// by `prepare()` and the client's `validate_header_names()`, which evaluate nothing else.
fn native_names(
    k: &TokenStream,
    headers: &Headers,
    model: &Model,
    seq: &[ContribId],
) -> (TokenStream, Vec<TokenStream>) {
    let mut bindings = TokenStream::new();
    let mut names = Vec::new();
    for (i, &id) in seq.iter().enumerate() {
        let c = model.contrib(id);
        let name = key_ref(model, headers, &c.key);
        let var = format_ident!("__n{}", i);
        match &c.key {
            // The literal's validated static.
            KeyName::Lit(_) => {
                bindings.extend(quote!(let #var: &#k::reqwest::header::HeaderName = #name;))
            }
            // The key's accessor (checked once, where the path is written).
            KeyName::Path(p) => {
                let f = key_fn(model, *p);
                let owned = format_ident!("__o_n{}", i);
                bindings.extend(quote! {
                    let #owned: #k::reqwest::header::HeaderName = #f();
                    let #var: &#k::reqwest::header::HeaderName = &#owned;
                });
            }
        }
        names.push(quote!(#var));
    }
    (bindings, names)
}

/// The `static` holding a native chain's contribution shapes (operation and label), shared by
/// the endpoint's `prepare()` and the client's `validate_header_names()`.
fn shapes_ident(model: &Model, endpoint: usize) -> Ident {
    format_ident!(
        "__CONCORD_{}_SHAPES_{}",
        plain(&model.owners[0].type_name),
        endpoint
    )
}

/// The shapes `static` of an endpoint whose chain writes a native key (none otherwise).
fn shapes_static(k: &TokenStream, model: &Model, endpoint: usize) -> TokenStream {
    let Some(HeaderComposition::Dynamic(seq)) =
        model.owners[endpoint].composed.as_ref().map(|c| &c.headers)
    else {
        return TokenStream::new();
    };
    let ident = shapes_ident(model, endpoint);
    let n = seq.len();
    let shapes = seq.iter().map(|&id| {
        let c = model.contrib(id);
        let op = match (c.op, c.else_inherit) {
            (crate::syntax::Op::Introduce, _) => quote!(Set),
            (crate::syntax::Op::Append, _) => quote!(Append),
            (crate::syntax::Op::Override, false) => quote!(Override),
            (crate::syntax::Op::Override, true) => quote!(OverrideElseInherit),
            (crate::syntax::Op::Remove, _) => quote!(Remove),
        };
        let label = lit(&format!(
            "`{}{}` on `{}`",
            match c.op {
                crate::syntax::Op::Introduce => "",
                crate::syntax::Op::Append => "append ",
                crate::syntax::Op::Override => "override ",
                crate::syntax::Op::Remove => "remove ",
            },
            model.key_text(&c.key),
            model.owners[id.owner].dotted
        ));
        quote!(#k::__private::HeaderShape {
            op: #k::__private::HeaderOp::#op,
            label: #label,
        })
    });
    quote! {
        #[doc(hidden)]
        #[allow(non_upper_case_globals, dead_code)]
        static #ident: [#k::__private::HeaderShape; #n] = [#(#shapes),*];
    }
}

fn native_headers_code(
    k: &TokenStream,
    headers: &Headers,
    model: &Model,
    places: &Places,
    endpoint: usize,
    seq: &[ContribId],
    explicit_media: bool,
) -> TokenStream {
    let n = seq.len();
    let shapes = shapes_ident(model, endpoint);
    let (bindings, names) = native_names(k, headers, model, seq);
    let mut present = Vec::new();
    let mut marked = Vec::new();
    let mut sends = TokenStream::new();
    for (i, &id) in seq.iter().enumerate() {
        let c = model.contrib(id);
        // Presence matters only for `override ... else inherit`; it is read without converting.
        present.push(if c.else_inherit {
            let checks: Vec<TokenStream> = contrib_reads(model, id)
                .into_iter()
                .filter(|r| model.value(*r).optional)
                .map(|r| {
                    let place = places.value(model, r);
                    quote!(#place.is_some())
                })
                .collect();
            quote!((true #(&& #checks)*))
        } else {
            quote!(true)
        });
        marked.push(flag_expr(model, c.sensitive, &[id]));
        if c.source.is_some() {
            let name = quote!(::core::clone::Clone::clone(__names[#i]));
            let sensitive = quote!(__plan.sensitive[#i]);
            let (bound, call) = source_code(k, headers, model, places, id, &name, &sensitive);
            let call = bound.wrap(call, None);
            sends.extend(quote!(if __plan.live[#i] { #call }));
        }
    }
    quote! {
        #bindings
        let __names: [&#k::reqwest::header::HeaderName; #n] = [#(#names),*];
        let __plan = #k::__private::HeaderPlan::<#n>::compose(
            __names,
            &#shapes,
            [#(#present),*],
            [#(#marked),*],
            #explicit_media,
            &__META,
        )?;
        __p.content_type_sensitive(__plan.content_type_sensitive);
        __p.accept(__plan.accept_sensitive, __plan.accept_removed);
        #sends
    }
}

/// The client's `validate_header_names()`: the structural rules of every endpoint compiled
/// under the current `cfg` whose chain writes a native key, evaluated on names only.
fn validate_header_names(k: &TokenStream, model: &Model, headers: &Headers) -> TokenStream {
    let client = &model.owners[0].type_name;
    let mut blocks = TokenStream::new();
    let mut checked = Vec::new();
    for (index, owner) in model.owners.iter().enumerate() {
        let Kind::Endpoint { body, .. } = &owner.kind else {
            continue;
        };
        let Some(HeaderComposition::Dynamic(seq)) = owner.composed.as_ref().map(|c| &c.headers)
        else {
            continue;
        };
        let cfg = owners_cfg(model, &[index]);
        let (bindings, names) = native_names(k, headers, model, seq);
        let shapes = shapes_ident(model, index);
        let name = lit(&owner.dotted);
        let explicit_media = body.as_ref().is_some_and(|b| b.media.is_some());
        blocks.extend(quote! {
            #cfg
            {
                #bindings
                __check.endpoint(#name, [#(#names),*], &#shapes, #explicit_media);
            }
        });
        checked.push(format!("`{}`", owner.dotted));
    }
    let scope = if checked.is_empty() {
        " No endpoint of this declaration writes a native `HeaderName` constant: every header name was checked when the declaration compiled, and this returns `Ok(())`.".to_owned()
    } else {
        format!(
            " Endpoints whose header chain writes a native `HeaderName` constant (checked when compiled under their `cfg`): {}. The others use literal names only, checked when the declaration compiled.",
            checked.join(", ")
        )
    };
    let docs = [
        " Checks the header names of every endpoint compiled under the current `cfg`, without a",
        " client, a configuration, an endpoint value, a network or an async runtime: only the native",
        " `HeaderName` constants are read. It applies the structural rules `prepare()` applies when",
        " it composes the headers (a header introduced twice, an `override`/`else inherit`/`remove`",
        " with nothing to act on, a header field reqwest manages, a `content-type` header next to a",
        " declared body media type); their verdict does not depend on which optional values are",
        " present, so no request of a reported endpoint can be built: its `prepare()` fails with",
        " this problem unless an earlier step (encoding the body, formatting the host, path or",
        " query) fails first. `Ok(())` means no `prepare()` fails on header names. Values, their",
        " conversion and servers are not checked.",
        "",
        " # Errors",
        "",
        " The endpoints whose header names break a rule, one problem per endpoint.",
        "",
    ]
    .into_iter()
    .chain(std::iter::once(scope.as_str()))
    .map(lit);
    quote! {
        impl #client {
            #(#[doc = #docs])*
            pub fn validate_header_names() -> ::core::result::Result<(), #k::HeaderNameErrors> {
                #[allow(unused_mut)]
                let mut __check = #k::__private::NameCheck::new();
                #blocks
                __check.finish()
            }
        }
    }
}

fn component_code(
    k: &TokenStream,
    model: &Model,
    places: &Places,
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
    headers: &Headers,
    model: &Model,
    index: usize,
    method: &Ident,
    output: Option<&Output>,
    body: Option<&Body>,
) -> TokenStream {
    let owner: &Owner = &model.owners[index];
    let incoming = matches!(owner.kind, Kind::Endpoint { incoming: true, .. });
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
            &c.pieces,
            false,
            &format!("path segment `{}`", c.text),
        ));
    }
    if composed.trailing_slash {
        code.extend(quote!(__p.trailing_slash();));
    }
    // `query(..)` values of the endpoint, in parameter order: each is written where it is
    // declared among the endpoint's own query keys (inherited keys first, block keys last).
    let mut pairs = owner
        .values
        .iter()
        .enumerate()
        .filter_map(|(param, value)| value.pairs.map(|at| (param, value, at)))
        .peekable();
    let mut pairs_code = |code: &mut TokenStream, before: usize| {
        while let Some((_, value, at)) = pairs.next_if(|&(param, _, _)| param < before) {
            let name = &value.name;
            let what = lit(&format!("query value `{}`", plain(name)));
            let call = if value.optional {
                quote!(if let ::core::option::Option::Some(__v) = &self.#name { __p.query_pairs(__v, #what)?; })
            } else {
                quote!(__p.query_pairs(&self.#name, #what)?;)
            };
            // Located at the placement (an unimplemented `QueryPairs` points there), macro
            // hygiene kept.
            code.extend(located(call, at));
        }
    };
    let mut sensitive_query = Vec::new();
    for slot in &composed.query {
        // Where the slot's key first appears: an ancestor's key comes first; one introduced by
        // this endpoint's direct placement sits at that parameter; a block entry comes last.
        let rank = match slot.history.first() {
            Some(first) if first.owner == index => model.owners[index].query[first.index]
                .param
                .unwrap_or(usize::MAX),
            _ => 0,
        };
        pairs_code(&mut code, rank);
        if slot.sensitive {
            sensitive_query.push(lit(&model.key_text(&slot.key)));
        }
        code.extend(items_code(
            k,
            headers,
            model,
            &places,
            &slot.items,
            slot,
            Family::Query,
        ));
    }
    pairs_code(&mut code, usize::MAX);
    match &composed.headers {
        HeaderComposition::Static(slots) => {
            for slot in slots {
                code.extend(items_code(
                    k,
                    headers,
                    model,
                    &places,
                    &slot.items,
                    slot,
                    Family::Header,
                ));
            }
        }
        HeaderComposition::Dynamic(seq) => {
            let explicit_media = body.is_some_and(|b| b.media.is_some());
            code.extend(native_headers_code(
                k,
                headers,
                model,
                &places,
                index,
                seq,
                explicit_media,
            ));
        }
    }

    // The body's format and media type live in a `static`, evaluated when the crate is compiled:
    // `Encode::CONTENT_TYPE` (or a declared media type) goes through `HeaderValue::from_static`,
    // so an invalid one is a compile error, and a declared media type replaces the constant
    // without reading it. Requests clone the validated value.
    let body_code = match body {
        None => quote!(::core::option::Option::None),
        Some(b) => {
            let name = &b.name;
            let ty = &b.ty;
            let at = match b.origin {
                Origin::Endpoint => b.format.span(),
                Origin::Default(_) => ty.span(),
            };
            let format = format_for(model, &b.format, b.origin, true, at);
            let init = match &b.media {
                // The payload type is named first, so an unsupported pair is reported as
                // `Encode<T>` with its message, not as a mismatch of inferred `BodyFormat`s.
                None => quote!(#k::__private::BodyFormat::<#ty>::new::<#format>()),
                Some((media, _)) => {
                    quote!(#k::__private::BodyFormat::<#ty>::with_media_type::<#format>(
                        #k::reqwest::header::HeaderValue::from_static(#media)
                    ))
                }
            };
            let init = located(init, at);
            let encode = quote! {
                static __FORMAT: #k::__private::BodyFormat<#ty> = #init;
            };
            if b.optional {
                quote!({
                    #encode
                    match self.#name {
                        ::core::option::Option::Some(__b) => ::core::option::Option::Some(#k::__private::encode_body(&__FORMAT, __b, &__META)?),
                        ::core::option::Option::None => ::core::option::Option::None,
                    }
                })
            } else {
                quote!({
                    #encode
                    ::core::option::Option::Some(#k::__private::encode_body(&__FORMAT, self.#name, &__META)?)
                })
            }
        }
    };
    // 8f: the inserted `Content-Type` carries the key's static classification on this endpoint
    // (marker or sticky composition, also after a `remove`).
    // With a native constant on the chain, `HeaderPlan` decides it at run time instead.
    let sensitive_content_type = match &composed.headers {
        HeaderComposition::Static(slots) => slots
            .iter()
            .any(|slot| slot.key.lit() == Some("content-type") && slot.sensitive),
        HeaderComposition::Dynamic(_) => false,
    };
    // The same for a format's `Accept` suggestion, plus whether the chain removed `accept` last
    // (the suggestion is then not sent). Native chains decide both in `HeaderPlan`.
    let (sensitive_accept, accept_removed) = match &composed.headers {
        HeaderComposition::Static(slots) => slots
            .iter()
            .find(|slot| slot.key.lit() == Some("accept"))
            .map_or((false, false), |slot| (slot.sensitive, slot.removed)),
        HeaderComposition::Dynamic(_) => (false, false),
    };

    let name = lit(&owner.dotted);
    let method_str = lit(&method.to_string());
    let label_count = composed.labels.len();
    let segment_count = composed.segments.len() + usize::from(composed.trailing_slash);
    let root = format_ident!("__o0");
    // No `->`: drained, whatever the defaults (a `response` default never applies). `-> stream`
    // without an item: the unread body, likewise without a format.
    let decoder = match output {
        Some(o) if o.stream => {
            let t = &o.ty;
            let at = match o.origin {
                Origin::Endpoint => o.format.span(),
                Origin::Default(_) => t.span(),
            };
            let r = format_for(model, &o.format, o.origin, false, at);
            // The item type is named first, so an unsupported pair is reported as
            // `Incremental<T>`.
            let init = located(
                quote!(#k::__private::OpenFormat::<#k::Records<#r, #t>>::new::<#t, #r>()),
                at,
            );
            quote!({
                static __OPEN: #k::__private::OpenFormat<#k::Records<#r, #t>> = #init;
                (__OPEN.decoder(&__META), __OPEN.accept())
            })
        }
        Some(o) => {
            let t = &o.ty;
            let at = match o.origin {
                Origin::Endpoint => o.format.span(),
                Origin::Default(_) => t.span(),
            };
            let r = format_for(model, &o.format, o.origin, false, at);
            // A static: the format's `Accept` suggestion is a compile-time-checked value. The
            // output type is named first, so an unsupported pair is reported as `Decode<T>`.
            let init = located(quote!(#k::__private::DecodeFormat::<#t>::new::<#r>()), at);
            // `limit N;` (endpoint or nearest owner): evaluated in the endpoint type's private
            // associated constant (`limit_const`), where only the caller's names are in scope —
            // not this block's generated statics.
            let init = match model.nearest_limit(index) {
                Some(_) => {
                    let ty = &model.owners[index].type_name;
                    quote!(#init.with_limit(#ty::__CONCORD_LIMIT))
                }
                None => init,
            };
            quote!({
                static __DECODE: #k::__private::DecodeFormat<#t> = #init;
                (__DECODE.decoder(&__META), __DECODE.accept())
            })
        }
        None if incoming => {
            quote!((#k::__private::incoming(&__META), ::core::option::Option::None))
        }
        None => quote!((#k::__private::drain(&__META), ::core::option::Option::None)),
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
            sensitive_content_type: #sensitive_content_type,
            sensitive_accept: #sensitive_accept,
            accept_removed: #accept_removed,
        };
        let __body = #body_code;
        #bindings
        let mut __p = #k::__private::Parts::new(&__META, &#root.__base);
        #code
        let (__decoder, __accept) = #decoder;
        __p.finish(#k::reqwest::Method::#method, &#root.__http, __body, __decoder, __accept)
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::{emit, key_owner};
    use crate::model::{KeyName, Model};
    use crate::syntax::Api;

    /// The generated rustdoc text of a declaration.
    fn docs(declaration: proc_macro2::TokenStream) -> String {
        let api: Api = syn::parse2(declaration).expect("declaration parses");
        let (model, unused) = Model::build(api).unwrap_or_else(|e| panic!("model: {e}"));
        assert!(unused.is_none(), "unused values in a unit fixture");
        emit(&model, quote!(::concord)).to_string()
    }

    /// The provenance recorded for each native key occurrence (R5) is the one owner that the old
    /// scan over every owner's contributions found: per occurrence (equal tokens stay distinct),
    /// across nested and `cfg` scopes, client parameters, removals and repeated operations.
    #[test]
    fn key_occurrence_owner_matches_a_scan_of_every_owner() {
        let api: Api = syn::parse2(quote! {
            pub client Api(#[sensitive] header(X_KEY) key: String) {
                base "http://x.test";
                headers { ACCEPT = "application/json"; X_TRACE = "t"; }
                #[cfg(feature = "a")]
                scope a {
                    headers { X_TRACE = "a"; }
                    scope deep {
                        GET one() { headers { remove X_KEY; X_KEY = "TWO"; append X_KEY = "3"; } }
                    }
                }
                scope b {
                    GET two() { headers { remove ACCEPT; ACCEPT = "text/plain"; } }
                }
                GET three();
            }
        })
        .expect("declaration parses");
        let (model, _) = Model::build(api).unwrap_or_else(|e| panic!("model: {e}"));
        assert_eq!(model.key_paths.len(), 9, "one entry per written path");
        for (i, key) in model.key_paths.iter().enumerate() {
            let scanned: Vec<usize> = (0..model.owners.len())
                .filter(|&o| {
                    model.owners[o]
                        .headers
                        .iter()
                        .any(|c| c.key == KeyName::Path(i))
                })
                .collect();
            assert_eq!(scanned.len(), 1, "{}: {scanned:?}", key.text);
            assert_eq!(key_owner(&model, i), &scanned[..], "{}", key.text);
        }
    }

    /// With a native key on the chain, the header operations are listed in chain order, removals
    /// included: the macro cannot tell which of them name the same header.
    #[test]
    fn request_summary_lists_native_header_operations_in_order() {
        let docs = docs(quote! {
            pub client Api(header(X_KEY) key: String) {
                base "http://x.test";
                headers { ACCEPT = "application/json"; }
                GET me(token: String) {
                    headers { remove X_KEY; AUTHORIZATION = "Bearer {token}"; }
                }
                GET other();
            }
        });
        let line = |names: &str| {
            format!(
                "Headers, in chain order (sent when present; native names are compared when the request is built): {names}."
            )
        };
        // Each summary is on the navigation method and on the request type.
        // The token is written as text under a native key: withheld statically (F09).
        let me = "`X_KEY`, `ACCEPT`, `remove X_KEY`, `AUTHORIZATION` (sensitive)";
        assert_eq!(docs.matches(&line(me)).count(), 2);
        assert_eq!(docs.matches(&line("`X_KEY`, `ACCEPT`")).count(), 2);
        // A repeated operation after a removal is listed again (final executable re-review).
        let repeated = self::docs(quote! {
            pub client Api(header(X_KEY) key: String) {
                base "http://x.test";
                GET ops() { headers { remove X_KEY; X_KEY = "TWO"; append X_KEY = "THREE"; } }
            }
        });
        let ops = "`X_KEY`, `remove X_KEY`, `X_KEY`, `append X_KEY`";
        assert_eq!(repeated.matches(&line(ops)).count(), 2);
    }

    /// Placement notes are deduplicated (a label seen twice is listed once) and keep first-seen
    /// order, whatever the number of distinct placements.
    #[test]
    fn placement_notes_are_distinct_in_first_seen_order() {
        let docs = docs(quote! {
            pub client Api(v: String = "x") {
                base "http://x.test";
                GET one() { query { "b" = client.v; "a" = client.v; } }
                GET two() { query { "a" = client.v; "c" = client.v; } }
            }
        });
        let expected = "Sent as: query parameter `b`; query parameter `a`; query parameter `c`.";
        // Once: on `set_v`, the only control of a required value.
        assert_eq!(docs.matches(expected).count(), 1, "{docs}");
    }

    /// Format row 12e: every endpoint states its body and output format, the media type and where
    /// an inherited format comes from; an endpoint without `->` says the response default does
    /// not apply; `-> ()` says what is decoded; owners state their defaults.
    #[test]
    fn format_docs_state_format_media_type_and_origin() {
        let docs = docs(quote! {
            pub client Shop {
                base "http://x.test";
                request Json;
                response Json;
                POST create(body item: NewItem) -> Item;
                PATCH patch(body p: Patch) -> String { request Json "application/merge-patch+json"; response Text; }
                PUT maybe(body p?: NewItem);
                DELETE remove();
                GET unit() -> ();
                scope csv {
                    request crate::Csv "text/csv; header=present";
                    response crate::Csv;
                    POST import(body rows: Rows) -> Report;
                }
            }
        });
        for expected in [
            " Formats of the endpoints below: request bodies `Json` (declared here); typed outputs `Json` (declared here).",
            " Body (`item`): `NewItem`, encoded with the format [`Json`](Json) (the `request` default of the client `Shop`). `Content-Type`: the format's media type, sent with the body: the one its `Encode::encode_body` binds to the body at run time, else its `Encode::CONTENT_TYPE`.",
            " Output: `Item`, decoded from a 2xx response with the format [`Json`](Json) (the `response` default of the client `Shop`): the format checks the response head before the body is read (a refusal is `ErrorKind::Metadata`, the response kept unread), and its `Decode::ACCEPT`, if any, is sent when this endpoint's headers leave no `accept` and do not end by removing it.",
            " Body (`p`): `Patch`, encoded with the format [`Json`](Json) (the endpoint statement). `Content-Type: application/merge-patch+json` (declared by the endpoint request statement) is sent with the body; a format that binds another media type to the body at run time (`Encode::encode_body`) is a `Build` error.",
            " Output: `String`, decoded from a 2xx response with the format [`Text`](Text) (the endpoint statement): the format checks the response head before the body is read (a refusal is `ErrorKind::Metadata`, the response kept unread), and its `Decode::ACCEPT`, if any, is sent when this endpoint's headers leave no `accept` and do not end by removing it.",
            " Absent: no body, and the format adds no `Content-Type`.",
            " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `Json` of the client `Shop` does not apply.",
            " `()` is decoded like any output type, with `Json`. An endpoint without `->` decodes no output and accepts any 2xx.",
            " Formats of the endpoints below: request bodies `crate::Csv` (declared here, media type `text/csv; header=present`); typed outputs `crate::Csv` (declared here).",
            " Body (`rows`): `Rows`, encoded with the format [`crate::Csv`](crate::Csv) (the `request` default of scope `Shop.csv`). `Content-Type: text/csv; header=present` (from the `request` default of scope `Shop.csv`) is sent with the body; a format that binds another media type to the body at run time (`Encode::encode_body`) is a `Build` error.",
        ] {
            // The expected text, escaped as in a doc literal of the token stream.
            let quoted = format!("{expected:?}");
            let escaped = &quoted[1..quoted.len() - 1];
            assert!(docs.contains(escaped), "missing {expected:?}");
        }
    }

    /// Format row 8b in the docs: a surviving `content-type` header replaces the format's media
    /// type and is sent with or without the body.
    #[test]
    fn format_docs_state_the_header_rule() {
        let docs = docs(quote! {
            pub client Api(header("content-type") ct?: String) {
                base "http://x.test";
                POST up(body data: Vec<u8>) { request Binary; }
            }
        });
        let expected = " Body (`data`): `Vec<u8>`, encoded with the format [`Binary`](Binary) (the endpoint statement). `Content-Type`: the declared `content-type` header when it is sent (with or without the body), otherwise the format's media type with the body; a media type the format binds to the body at run time (`Encode::encode_body`) must equal a sent header, else a `Build` error.";
        assert!(
            docs.contains(&format!("{expected:?}")),
            "missing {expected:?}"
        );
    }

    /// Initializers are documented as Rust is usually written, not as spaced tokens.
    #[test]
    fn initializer_text_is_spaced_like_rust() {
        let text = |t: proc_macro2::TokenStream| super::expr_text(t);
        assert_eq!(text(quote!(&ACCEPT)), "&ACCEPT");
        assert_eq!(
            text(quote!(HeaderValue::from_static("init"))),
            "HeaderValue::from_static(\"init\")"
        );
        assert_eq!(
            text(quote!(Some("fr".to_owned()))),
            "Some(\"fr\".to_owned())"
        );
        assert_eq!(text(quote!(vec![1, 2])), "vec![1, 2]");
        assert_eq!(text(quote!(-1)), "-1");
        assert_eq!(text(quote!(a - 1)), "a - 1");
        assert_eq!(text(quote!(Vec::<u8>::new())), "Vec::<u8>::new()");
        assert_eq!(text(quote!(x.y().z)), "x.y().z");
        assert_eq!(
            text(quote!(Config { a: 1, b: !c })),
            "Config { a: 1, b: !c }"
        );
        assert_eq!(text(quote!(f(&x, &mut y)?)), "f(&x, &mut y)?");
        assert_eq!(text(quote!(1..=3)), "1..=3");
    }

    /// F03 (4.2): generated documentation renders payload types from their tokens, so a literal
    /// inside a type keeps its contents (`"a :: b"`), as in the generated signatures.
    #[test]
    fn documented_types_keep_literal_contents() {
        let text = docs(quote! {
            pub client Api {
                base "http://x.test";
                format Fmt;
                POST put(body p: Payload<{ count("a :: b") }>) -> Payload<{ count(r#"c , d"#) }>;
            }
        });
        assert!(
            text.contains(r#"`Payload<{ count(\"a :: b\") }>`"#),
            "{text}"
        );
        assert!(
            text.contains(r##"`Payload<{ count(r#\"c , d\"#) }>`"##),
            "{text}"
        );
        assert!(!text.contains("a::b") && !text.contains("c, d"), "{text}");
    }

    /// Review of F03 (4.2): an initializer holding a fragment forwarded by `macro_rules!` is
    /// documented with its grouping (`(1 + 2) * 2`), which is also what the generated code computes
    /// (`crates/concord/tests/forwarded_types.rs` sends `n=6`).
    #[test]
    fn documented_initializers_keep_forwarded_grouping() {
        use proc_macro2::{Delimiter, Group, TokenTree};
        let sum = TokenTree::Group(Group::new(Delimiter::None, quote!(1 + 2)));
        let text = docs(quote! {
            pub client Api {
                base "http://x.test";
                GET x(query n: usize = #sum * 2);
            }
        });
        assert!(text.contains("Initially `(1 + 2) * 2`"), "{text}");
        // A negative literal is grouped before a method call only (a range bound stays bare).
        let minus = TokenTree::Group(Group::new(Delimiter::None, quote!(-1isize)));
        let text = docs(quote! {
            pub client Api {
                base "http://x.test";
                GET x(query n: usize = #minus.unsigned_abs(), query m: usize = match 0isize { #minus..=1 => 7, _ => 8 });
            }
        });
        assert!(
            text.contains("Initially `(-1isize).unsigned_abs()`"),
            "{text}"
        );
        assert!(text.contains("match 0isize { -1isize..=1"), "{text}");
        // Written delimiters are never taken for a fragment's (re-review C3).
        let text = docs(quote! {
            pub client Api {
                base "http://x.test";
                GET x(
                    query a: usize = [-1isize].len(),
                    query b: isize = if true { -1isize } else { 2isize },
                    query c: usize = count!(-1isize),
                );
            }
        });
        assert!(text.contains("Initially `[-1isize].len()`"), "{text}");
        assert!(
            text.contains("Initially `if true { -1isize } else { 2isize }`"),
            "{text}"
        );
        assert!(text.contains("Initially `count!(- 1isize)`"), "{text}");
    }

    /// Final review R1: the per-call note may call absence "the default" only when the value has
    /// no initializer; an initialized optional value starts present.
    #[test]
    fn per_call_note_names_the_actual_default() {
        let plain = docs(quote! {
            pub client Api(query locale: String = "en") {
                base "http://x.test";
                GET page(locale?: Option<String>) {
                    query { override "locale" = each locale else inherit; }
                }
            }
        });
        assert!(
            plain.contains("`maybe_locale(None)` (the default) keeps"),
            "{plain}"
        );

        let initialized = docs(quote! {
            pub client Api(query locale: String = "en") {
                base "http://x.test";
                GET page(locale?: Option<String> = Some("fr".to_owned())) {
                    query { override "locale" = each locale else inherit; }
                }
            }
        });
        assert!(
            initialized.contains("`maybe_locale(None)` keeps"),
            "{initialized}"
        );
        assert!(!initialized.contains("(the default)"), "{initialized}");
        assert!(
            initialized.contains("Initially `Some(\\\"fr\\\".to_owned())`"),
            "{initialized}"
        );
    }
}
