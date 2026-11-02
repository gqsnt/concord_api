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
use crate::syntax::{Family, Format, type_text};

pub fn emit(model: &Model, krate: TokenStream) -> TokenStream {
    let k = &krate;
    let notes = value_notes(model);
    let (header_items, headers) = header_statics(k, model);
    let vis = match &model.owners[0].kind {
        Kind::Client { vis, .. } => vis.clone(),
        _ => unreachable!("the first owner is the client"),
    };
    let mut out = header_items;
    out.extend(format_aliases(k, model));
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
            if let KeyName::Lit(name) = &c.key {
                if !headers.names.contains_key(name) {
                    let ident =
                        format_ident!("__CONCORD_{}_HEADER_{}", client, headers.names.len());
                    let key = lit(name);
                    items.extend(quote_spanned! {c.span=>
                        #[doc(hidden)]
                        #[allow(non_upper_case_globals, dead_code)]
                        static #ident: #k::reqwest::header::HeaderName =
                            #k::reqwest::header::HeaderName::from_static(#key);
                    });
                    headers.names.insert(name.clone(), ident);
                }
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
        let users: Vec<usize> = (0..model.owners.len())
            .filter(|&o| {
                model.owners[o]
                    .headers
                    .iter()
                    .any(|c| c.key == KeyName::Path(i))
            })
            .collect();
        let cfg = owners_cfg(model, &users);
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

/// A run-time withholding condition as a `bool` expression.
fn cond_expr(k: &TokenStream, model: &Model, headers: &Headers, cond: &Cond) -> TokenStream {
    match cond {
        Cond::Name(i) => {
            let name = key_ref(model, headers, &KeyName::Path(*i));
            quote!(#k::__private::sensitive_name(#name))
        }
        Cond::Same(a, b) => {
            let a = key_ref(model, headers, a);
            let b = key_ref(model, headers, b);
            quote!((#a == #b))
        }
    }
}

/// `static_flag || any run-time condition of a value read by `ids``.
fn flag_expr(
    k: &TokenStream,
    model: &Model,
    headers: &Headers,
    static_flag: bool,
    ids: &[ContribId],
) -> TokenStream {
    if static_flag {
        return quote!(true);
    }
    let mut conds: Vec<&Cond> = Vec::new();
    for &id in ids {
        for r in contrib_reads(model, id) {
            for c in &model.value(r).debug_when {
                if !conds.contains(&c) {
                    conds.push(c);
                }
            }
        }
    }
    if conds.is_empty() {
        return quote!(false);
    }
    let conds = conds.into_iter().map(|c| cond_expr(k, model, headers, c));
    quote!((false #(|| #conds)*))
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

/// The runtime type of a format written in place: a word is `$crate::{Json, Text, Binary, Form}`
/// located at the word (hygiene kept), so a missing feature or an unsupported payload is reported
/// there; a path is the user's tokens, resolved at the declaration like any type.
fn format_type(k: &TokenStream, format: &Format) -> TokenStream {
    match format {
        Format::Word(word, span) => {
            let marker = Ident::new(word.marker(), Span::call_site());
            located(quote!(#k::#marker), *span)
        }
        Format::Path(path) => quote!(#path),
    }
}

/// How an endpoint names its body's or output's format. An inherited default is named through
/// its owner's hidden alias, located at the endpoint's payload type (`at`): the alias reports a
/// missing format once, at the default, while an unsupported payload is reported at the endpoint
/// that declares it.
fn format_for(
    k: &TokenStream,
    model: &Model,
    format: &Format,
    origin: Origin,
    request: bool,
    at: Span,
) -> TokenStream {
    match origin {
        Origin::InPlace => format_type(k, format),
        Origin::Default(owner) => {
            let o = &model.owners[owner];
            let d = if request { &o.request } else { &o.response };
            let alias = &d.as_ref().expect("an origin names a default").alias;
            located(quote!(#alias), at)
        }
    }
}

/// One hidden alias per `request`/`response` default that has consumers:
/// `type __Concord{Owner}{Request|Response}Format = <format>;`, located at the default. It carries
/// `#[cfg(any(all(<cfgs of consumer 1>), ...))]`, the union of its consuming endpoints' effective
/// `cfg`s, so a default used only by cfg-disabled endpoints is never referenced, and an unused
/// default (no consumer) is not emitted at all.
fn format_aliases(k: &TokenStream, model: &Model) -> TokenStream {
    let mut out = TokenStream::new();
    for owner in &model.owners {
        for d in [&owner.request, &owner.response].into_iter().flatten() {
            if d.consumers.is_empty() {
                continue;
            }
            let alias = &d.alias;
            let ty = format_type(k, &d.format);
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
            return quote!();
        }
        let all = quote!(all(#(#predicates),*));
        if seen.insert(all.to_string()) {
            alternatives.push(all);
        }
    }
    quote!(#[cfg(any(#(#alternatives),*))])
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

/// An expression's text for documentation, spaced as Rust is usually written (`&ACCEPT`,
/// `HeaderValue::from_static("x")`, `vec![1, 2]`) rather than as a token stream
/// (`& ACCEPT`, `HeaderValue :: from_static ("x")`). Only the spacing differs from the tokens.
fn expr_text(tokens: TokenStream) -> String {
    use proc_macro2::{Delimiter, Spacing, TokenTree};

    #[derive(Clone, Copy, PartialEq)]
    enum Prev {
        Start,
        Word,
        Group,
        /// A punctuation character; `true` when it is unary or joined to the next one.
        Punct(char, bool),
    }

    fn walk(tokens: TokenStream, out: &mut String) {
        let mut prev = Prev::Start;
        // Open turbofish generics (`::<`): their `>` closes without a space.
        let mut generics = 0usize;
        for tt in tokens {
            let space = match (&tt, prev) {
                (_, Prev::Start) => false,
                (TokenTree::Punct(p), _) if p.as_char() == '>' && generics > 0 => false,
                (_, Prev::Punct(_, true)) => false,
                (_, Prev::Punct('.', _)) => false,
                (TokenTree::Punct(p), _) if matches!(p.as_char(), '.' | ',' | ';' | '?' | ':') => {
                    false
                }
                (TokenTree::Punct(p), _) if p.as_char() == '!' && prev == Prev::Word => false,
                (TokenTree::Group(g), Prev::Word | Prev::Group | Prev::Punct('!' | '>', _))
                    if g.delimiter() != Delimiter::Brace =>
                {
                    false
                }
                _ => true,
            };
            if space {
                out.push(' ');
            }
            prev = match tt {
                TokenTree::Group(g) => {
                    let (open, close) = match g.delimiter() {
                        Delimiter::Parenthesis => ("(", ")"),
                        Delimiter::Bracket => ("[", "]"),
                        Delimiter::Brace => ("{ ", " }"),
                        Delimiter::None => ("", ""),
                    };
                    out.push_str(open);
                    walk(g.stream(), out);
                    out.push_str(close);
                    Prev::Group
                }
                TokenTree::Punct(p) => {
                    let c = p.as_char();
                    out.push(c);
                    // `&x`, `-1`, `*p`, `!b` at the start or after an operator or `,` are unary.
                    let unary = matches!(c, '&' | '-' | '*' | '!')
                        && matches!(prev, Prev::Start | Prev::Punct(..));
                    // Both colons of `::` count as joined (no space after the path separator).
                    let path = c == ':' && prev == Prev::Punct(':', true);
                    // The end of a range operator (`..=`) binds like `.`.
                    let range = matches!(prev, Prev::Punct('.', _));
                    let turbofish = c == '<' && prev == Prev::Punct(':', true);
                    if turbofish {
                        generics += 1;
                    } else if c == '>' && generics > 0 {
                        generics -= 1;
                    }
                    Prev::Punct(
                        c,
                        p.spacing() == Spacing::Joint || unary || turbofish || path || range,
                    )
                }
                other => {
                    out.push_str(&other.to_string());
                    Prev::Word
                }
            };
        }
    }

    let mut out = String::new();
    walk(tokens, &mut out);
    out
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
    if let Kind::Endpoint { output, body, .. } = &model.owners[endpoint].kind {
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
            let media = match (&b.media, &b.format) {
                (Some((m, origin)), _) => format!(
                    " `Content-Type: {}` ({}) is sent with the body.",
                    m.value(),
                    match origin {
                        Origin::InPlace => "declared by the endpoint request statement".to_owned(),
                        Origin::Default(o) =>
                            format!("from the `request` default of {}", model.describe(*o)),
                    }
                ),
                (None, Format::Word(word, _)) if header => format!(
                    " `Content-Type`: the declared `content-type` header when it is sent (with or without the body), otherwise `{}` with the body.",
                    word.media_type()
                ),
                (None, Format::Word(word, _)) => format!(
                    " `Content-Type: {}` is sent with the body.",
                    word.media_type()
                ),
                (None, Format::Path(_)) if header => " `Content-Type`: the declared `content-type` header when it is sent (with or without the body), otherwise the format's `Encode::CONTENT_TYPE` with the body.".to_owned(),
                (None, Format::Path(_)) => {
                    " `Content-Type`: the format's `Encode::CONTENT_TYPE`, sent with the body."
                        .to_owned()
                }
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
            Some(o) => {
                let unit = if matches!(&o.ty, syn::Type::Tuple(t) if t.elems.is_empty()) {
                    match &o.format {
                        Format::Word(crate::syntax::Word::Json, _) => " `()` is decoded like any output type: the response body must be JSON `null`, so an empty response (such as a 204) is a decode error. An endpoint without `->` decodes no output and accepts any 2xx.".to_owned(),
                        f => format!(" `()` is decoded like any output type, with `{}`. An endpoint without `->` decodes no output and accepts any 2xx.", f.text()),
                    }
                } else {
                    String::new()
                };
                format!(
                    " Output: `{}`, decoded from a 2xx response with the format {}.{unit}",
                    type_text(&o.ty),
                    format_origin(model, &o.format, o.origin, false),
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
            let mut shown: Vec<String> = Vec::new();
            for &id in seq {
                let c = model.contrib(id);
                if c.op == crate::syntax::Op::Remove {
                    continue;
                }
                let text = format!("`{}`", model.key_text(&c.key));
                if !shown.contains(&text) {
                    shown.push(text);
                }
            }
            if !shown.is_empty() {
                lines.push(lit(&format!(
                    " Headers (when present; native names are composed by identity when the request is built): {}.",
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
        Origin::InPlace => "the endpoint statement".to_owned(),
        Origin::Default(o) => format!(
            "the `{}` default of {}",
            if request { "request" } else { "response" },
            model.describe(o)
        ),
    };
    format!("`{}` ({origin})", format.text())
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
            " Formats of the endpoints below: {}. The nearest representation statement wins (endpoint, inner scopes, client); an endpoint without `->` drains its response.",
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
    model: &Model,
    headers: &Headers,
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
        let shown = match (value.debug_sensitive, value.optional, value.usage) {
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
        let shown = if value.debug_sensitive || value.debug_when.is_empty() {
            shown
        } else {
            let conds = value
                .debug_when
                .iter()
                .map(|c| cond_expr(k, model, headers, c));
            quote_spanned!(span=> &#k::__private::Withheld(false #(|| #conds)*, #shown))
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
            let debug = debug_fields(k, model, headers, &owner.values, &quote!(__s));
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
                let config_doc = lit(&format!(
                    " The required values of [`{ty}`], passed to [`{ty}::new`]. Optional and \
                     initialized values are set afterwards with the client's `set_*` and \
                     `maybe_*` methods."
                ));
                let config_label = lit(&config.to_string());
                let config_debug =
                    debug_fields(k, model, headers, required.iter().copied(), &quote!(self));
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
            let debug = debug_fields(k, model, headers, &owner.values, &quote!(__s));
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
                notes,
            );
            let mut debug = debug_fields(k, model, headers, &owner.values, &quote!(self));
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
            let prepare = prepare_body(
                k,
                headers,
                model,
                index,
                method,
                output.as_ref(),
                body.as_ref(),
            );
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

                    /// Sends the request once (`prepare` then one `reqwest::Client::execute`) and
                    /// decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the
                    /// unread response (`Error::into_response`); Concord never retries.
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
            quote!(#defaults #summary)
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
            let mut ids = Vec::new();
            collect_items(items, &mut ids);
            (
                quote!(::core::clone::Clone::clone(#name)),
                flag_expr(k, model, headers, slot.sensitive, &ids),
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

fn collect_items(items: &[Item], out: &mut Vec<ContribId>) {
    for item in items {
        match item {
            Item::Emit(id) => out.push(*id),
            Item::Override { src, fallback } => {
                out.push(*src);
                collect_items(fallback, out);
            }
        }
    }
}

/// A chain with a native `HeaderName` constant: every key is bound first (so a key's type error
/// is never hidden by an absent value), `HeaderPlan` composes the contributions on names alone,
/// and only then are the surviving values converted, in chain order.
fn native_headers_code(
    k: &TokenStream,
    headers: &Headers,
    model: &Model,
    places: &Places,
    seq: &[ContribId],
    explicit_media: bool,
) -> TokenStream {
    let n = seq.len();
    let mut bindings = TokenStream::new();
    let mut names = Vec::new();
    let mut entries = Vec::new();
    let mut sends = TokenStream::new();
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
        let op = match (c.op, c.else_inherit) {
            (crate::syntax::Op::Introduce, _) => quote!(Set),
            (crate::syntax::Op::Append, _) => quote!(Append),
            (crate::syntax::Op::Override, false) => quote!(Override),
            (crate::syntax::Op::Override, true) => quote!(OverrideElseInherit),
            (crate::syntax::Op::Remove, _) => quote!(Remove),
        };
        // Presence matters only for `override ... else inherit`; it is read without converting.
        let present = if c.else_inherit {
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
        };
        let marked = flag_expr(k, model, headers, c.sensitive, &[id]);
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
        entries.push(quote!(#k::__private::HeaderEntry {
            op: #k::__private::HeaderOp::#op,
            present: #present,
            marked: #marked,
            label: #label,
        }));
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
            [#(#entries),*],
            #explicit_media,
            &__META,
        )?;
        __p.content_type_sensitive(__plan.content_type_sensitive);
        #sends
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
    let mut sensitive_query = Vec::new();
    for slot in &composed.query {
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
                Origin::InPlace => b.format.span(),
                Origin::Default(_) => ty.span(),
            };
            let format = format_for(k, model, &b.format, b.origin, true, at);
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

    let name = lit(&owner.dotted);
    let method_str = lit(&method.to_string());
    let label_count = composed.labels.len();
    let segment_count = composed.segments.len() + usize::from(composed.trailing_slash);
    let root = format_ident!("__o0");
    // No `->`: drained, whatever the defaults (a `response` default never applies).
    let decoder = match output {
        Some(o) => {
            let t = &o.ty;
            let at = match o.origin {
                Origin::InPlace => o.format.span(),
                Origin::Default(_) => t.span(),
            };
            let r = format_for(k, model, &o.format, o.origin, false, at);
            quote_spanned!(at=> #k::__private::decoder::<#r, #t>(&__META))
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
            sensitive_content_type: #sensitive_content_type,
        };
        let __body = #body_code;
        #bindings
        let mut __p = #k::__private::Parts::new(&__META, &#root.__base);
        #code
        __p.finish(#k::reqwest::Method::#method, &#root.__http, __body, #decoder)
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::emit;
    use crate::model::Model;
    use crate::syntax::Api;

    /// The generated rustdoc text of a declaration.
    fn docs(declaration: proc_macro2::TokenStream) -> String {
        let api: Api = syn::parse2(declaration).expect("declaration parses");
        let model = Model::build(api).unwrap_or_else(|e| panic!("model: {e}"));
        emit(&model, quote!(::concord)).to_string()
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
                request json;
                response json;
                POST create(body item: NewItem) -> Item;
                PATCH patch(body p: Patch) -> String { request json "application/merge-patch+json"; response text; }
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
            " Formats of the endpoints below: request bodies `json` (declared here); typed outputs `json` (declared here).",
            " Body (`item`): `NewItem`, encoded with the format `json` (the `request` default of the client `Shop`). `Content-Type: application/json` is sent with the body.",
            " Output: `Item`, decoded from a 2xx response with the format `json` (the `response` default of the client `Shop`).",
            " Body (`p`): `Patch`, encoded with the format `json` (the endpoint statement). `Content-Type: application/merge-patch+json` (declared by the endpoint request statement) is sent with the body.",
            " Output: `String`, decoded from a 2xx response with the format `text` (the endpoint statement).",
            " Absent: no body, and the format adds no `Content-Type`.",
            " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Shop` does not apply.",
            " `()` is decoded like any output type: the response body must be JSON `null`, so an empty response (such as a 204) is a decode error. An endpoint without `->` decodes no output and accepts any 2xx.",
            " Formats of the endpoints below: request bodies `crate::Csv` (declared here, media type `text/csv; header=present`); typed outputs `crate::Csv` (declared here).",
            " Body (`rows`): `Rows`, encoded with the format `crate::Csv` (the `request` default of scope `Shop.csv`). `Content-Type: text/csv; header=present` (from the `request` default of scope `Shop.csv`) is sent with the body.",
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
                POST up(body data: Vec<u8>) { request bytes; }
            }
        });
        let expected = " Body (`data`): `Vec<u8>`, encoded with the format `bytes` (the endpoint statement). `Content-Type`: the declared `content-type` header when it is sent (with or without the body), otherwise `application/octet-stream` with the body.";
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
