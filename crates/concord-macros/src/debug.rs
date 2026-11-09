//! `#[derive(concord::Debug)]`: Rust's `Debug` derive, except for values marked `#[sensitive]`.
//!
//! - An unmarked field is formatted exactly as by `#[derive(Debug)]`, in declaration order.
//! - A `#[sensitive]` field prints `<redacted>`; written as `Option<T>`, it prints `None` or
//!   `Some(<redacted>)`.
//! - A `#[sensitive]` type or variant prints its name and `{ .. }`; inside it, `#[public]` named
//!   fields are formatted before the `..`.
//! - A withheld value is never formatted, so its type needs no `Debug`.

use proc_macro2::{TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;
use syn::{Attribute, Fields, Generics, Ident, Item, Type};

pub fn derive(input: TokenStream) -> TokenStream {
    expand(input).unwrap_or_else(|error| error.to_compile_error())
}

fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let (ident, generics, arms, shown) = match syn::parse2::<Item>(input)? {
        Item::Struct(item) => {
            let sensitive = marker(&item.attrs)?.is_some();
            let mut shown = Vec::new();
            let arm = arm(
                &item.ident,
                quote!(Self),
                &item.fields,
                sensitive,
                &mut shown,
            )?;
            (item.ident, item.generics, vec![arm], shown)
        }
        Item::Enum(item) => {
            let all = marker(&item.attrs)?.is_some();
            let mut shown = Vec::new();
            let mut arms = Vec::new();
            for variant in &item.variants {
                let own = marker(&variant.attrs)?;
                if let (true, Some(attr)) = (all, own) {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "this variant is already withheld: its enum is `#[sensitive]`",
                    ));
                }
                let name = &variant.ident;
                let path = quote!(Self::#name);
                let sensitive = all || own.is_some();
                arms.push(arm(name, path, &variant.fields, sensitive, &mut shown)?);
            }
            (item.ident, item.generics, arms, shown)
        }
        Item::Union(item) => {
            return Err(syn::Error::new(
                item.union_token.span,
                "`concord::Debug` supports structs and enums",
            ));
        }
        other => {
            return Err(syn::Error::new(
                other.span(),
                "`concord::Debug` supports structs and enums",
            ));
        }
    };
    let generics = bounded(generics, &shown);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let body = if arms.is_empty() {
        quote!(match *self {})
    } else {
        quote!(match self { #(#arms)* })
    };
    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics ::core::fmt::Debug for #ident #ty_generics #where_clause {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                struct __ConcordRedacted;
                impl ::core::fmt::Debug for __ConcordRedacted {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        f.write_str("<redacted>")
                    }
                }
                #body
            }
        }
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Mark {
    Shown,
    Sensitive,
    Public,
}

/// Reads `#[sensitive]` on a type or variant.
fn marker(attrs: &[Attribute]) -> syn::Result<Option<&Attribute>> {
    let mut found = None;
    for attr in attrs {
        let word = if attr.path().is_ident("sensitive") {
            "sensitive"
        } else if attr.path().is_ident("public") {
            return Err(syn::Error::new_spanned(
                attr,
                "`#[public]` marks a field of a `#[sensitive]` type or variant",
            ));
        } else {
            continue;
        };
        attr.meta.require_path_only().map_err(|_| {
            syn::Error::new_spanned(attr, format!("`#[{word}]` takes no arguments"))
        })?;
        if found.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                format!("duplicate `#[{word}]`"),
            ));
        }
        found = Some(attr);
    }
    Ok(found)
}

/// Reads the marks of one field.
fn field_mark(attrs: &[Attribute], container_sensitive: bool, named: bool) -> syn::Result<Mark> {
    let mut mark = Mark::Shown;
    for attr in attrs {
        let (word, this) = if attr.path().is_ident("sensitive") {
            ("sensitive", Mark::Sensitive)
        } else if attr.path().is_ident("public") {
            ("public", Mark::Public)
        } else {
            continue;
        };
        attr.meta.require_path_only().map_err(|_| {
            syn::Error::new_spanned(attr, format!("`#[{word}]` takes no arguments"))
        })?;
        let message = match (mark, this, container_sensitive) {
            (Mark::Shown, Mark::Sensitive, false) | (Mark::Shown, Mark::Public, true) => None,
            (Mark::Sensitive, Mark::Sensitive, _) | (Mark::Public, Mark::Public, _) => {
                Some(format!("duplicate `#[{word}]`"))
            }
            (Mark::Sensitive, Mark::Public, _) | (Mark::Public, Mark::Sensitive, _) => {
                Some("a field is either `#[public]` or `#[sensitive]`".to_owned())
            }
            (_, Mark::Sensitive, true) => Some(
                "this field is already withheld: its type or variant is `#[sensitive]`".to_owned(),
            ),
            (_, Mark::Public, false) => Some(
                "`#[public]` marks a field of a `#[sensitive]` type or variant; \
                 unmarked fields are already shown"
                    .to_owned(),
            ),
            (_, Mark::Shown, _) => None,
        };
        if let Some(message) = message {
            return Err(syn::Error::new_spanned(attr, message));
        }
        if this == Mark::Public && !named {
            return Err(syn::Error::new_spanned(
                attr,
                "`#[public]` marks a named field",
            ));
        }
        mark = this;
    }
    Ok(mark)
}

/// One `match` arm formatting a struct or a variant. Field types that are formatted are pushed
/// to `shown`, for the impl's bounds.
fn arm(
    name: &Ident,
    path: TokenStream,
    fields: &Fields,
    sensitive: bool,
    shown: &mut Vec<Type>,
) -> syn::Result<TokenStream> {
    let label = name.unraw().to_string();
    let named = matches!(fields, Fields::Named(_));
    let mut binds = Vec::new();
    let mut calls = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let mark = field_mark(&field.attrs, sensitive, named)?;
        let binding = format_ident!("__self_{}", index, span = field.ty.span());
        let member = match &field.ident {
            Some(ident) => quote!(#ident),
            None => syn::Index::from(index).into_token_stream(),
        };
        let value = match mark {
            Mark::Shown if sensitive => {
                binds.push(quote!(#member: _));
                continue;
            }
            Mark::Shown | Mark::Public => {
                shown.push(field.ty.clone());
                quote!(#binding)
            }
            Mark::Sensitive if is_option(&field.ty) => quote! {
                &::core::option::Option::map(
                    ::core::option::Option::as_ref(#binding),
                    |_| __ConcordRedacted,
                )
            },
            Mark::Sensitive => {
                binds.push(quote!(#member: _));
                calls.push(match &field.ident {
                    Some(ident) => {
                        let key = ident.unraw().to_string();
                        quote!(.field(#key, &__ConcordRedacted))
                    }
                    None => quote!(.field(&__ConcordRedacted)),
                });
                continue;
            }
        };
        binds.push(quote!(#member: #binding));
        calls.push(match &field.ident {
            Some(ident) => {
                let key = ident.unraw().to_string();
                quote!(.field(#key, #value))
            }
            None => quote!(.field(#value)),
        });
    }
    let body = if sensitive {
        quote!(f.debug_struct(#label) #(#calls)* .finish_non_exhaustive())
    } else {
        match fields {
            Fields::Named(_) => quote!(f.debug_struct(#label) #(#calls)* .finish()),
            Fields::Unnamed(_) => quote!(f.debug_tuple(#label) #(#calls)* .finish()),
            Fields::Unit => quote!(f.write_str(#label)),
        }
    };
    Ok(quote!(#path { #(#binds,)* } => #body,))
}

/// `Option<T>`, `std::option::Option<T>` or `core::option::Option<T>`, as written.
fn is_option(ty: &Type) -> bool {
    let Type::Path(path) = ty else { return false };
    if path.qself.is_some() {
        return false;
    }
    let names: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let last_has_args = path
        .path
        .segments
        .last()
        .is_some_and(|s| matches!(s.arguments, syn::PathArguments::AngleBracketed(_)));
    let written = match names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["Option"] => path.path.leading_colon.is_none(),
        ["std" | "core", "option", "Option"] => true,
        _ => false,
    };
    written && last_has_args
}

/// The type's generics with `T: Debug` for each type parameter used by a formatted field, and
/// `T::Item: Debug` for each projection a formatted field uses, as std does.
fn bounded(mut generics: Generics, shown: &[Type]) -> Generics {
    let params: Vec<Ident> = generics.type_params().map(|p| p.ident.clone()).collect();
    let mut predicates = Vec::new();
    for param in &params {
        if shown.iter().any(|ty| mentions(ty.to_token_stream(), param)) {
            predicates.push(quote!(#param: ::core::fmt::Debug));
        }
    }
    let mut projections = Projections {
        params: &params,
        found: Vec::new(),
    };
    for ty in shown {
        projections.visit_type_mut(&mut ty.clone());
    }
    for ty in projections.found {
        predicates.push(quote!(#ty: ::core::fmt::Debug));
    }
    if !predicates.is_empty() {
        let clause = generics.make_where_clause();
        for predicate in predicates {
            clause.predicates.push(syn::parse_quote!(#predicate));
        }
    }
    generics
}

/// Collects `T::Item` and `<T as Trait>::Item` types whose base is a type parameter.
struct Projections<'a> {
    params: &'a [Ident],
    found: Vec<syn::TypePath>,
}

impl VisitMut for Projections<'_> {
    fn visit_type_path_mut(&mut self, ty: &mut syn::TypePath) {
        let base = match &ty.qself {
            Some(qself) => qself.ty.to_token_stream(),
            None if ty.path.segments.len() > 1 => ty.path.segments[0].ident.to_token_stream(),
            None => TokenStream::new(),
        };
        if self.params.iter().any(|p| mentions(base.clone(), p)) {
            self.found.push(ty.clone());
        }
        syn::visit_mut::visit_type_path_mut(self, ty);
    }
}

fn mentions(tokens: TokenStream, param: &Ident) -> bool {
    tokens.into_iter().any(|tt| match tt {
        TokenTree::Ident(ident) => ident == *param,
        TokenTree::Group(group) => mentions(group.stream(), param),
        _ => false,
    })
}
