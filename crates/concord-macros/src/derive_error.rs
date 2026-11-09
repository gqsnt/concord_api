//! `#[derive(concord::derive::Error)]` on an SDK's own error enum: `Display`, `std::error::Error`
//! (`source()` from the fields marked `#[source]`), one `From` per single-field tuple variant, and
//! the split of a declared error document's `concord::Error<D>` (`#[from(concord::Error<D>)]`).
//! The expansion names only `core` items (the split calls `into_provider()` by method syntax), so
//! it needs no path to Concord; its own bindings are hygienic (`Span::mixed_site`), so no field
//! name can shadow them.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Attribute, Fields, Ident, Item, ItemEnum, LitStr, Path, Token, Type, Variant};

syn::custom_keyword!(with);
syn::custom_keyword!(skip);

/// Expands the derive, or every authored mistake found (each at its attribute or variant).
pub fn expand(item: &Item) -> syn::Result<TokenStream> {
    let input = match item {
        Item::Enum(input) => input,
        Item::Struct(syn::ItemStruct { ident, .. }) | Item::Union(syn::ItemUnion { ident, .. }) => {
            return Err(syn::Error::new(
                ident.span(),
                "`concord::derive::Error` is derived for an enum (the SDK's error variants)",
            ));
        }
        other => {
            return Err(syn::Error::new(
                other.span(),
                "`concord::derive::Error` is derived for an enum",
            ));
        }
    };
    let mut errors = Errors::default();
    let hook = errors.keep(enum_hook(&input.attrs)).flatten();

    let mut arms = Vec::new();
    let mut froms = Vec::new();
    let mut causes = Vec::new();
    let mut sources: Vec<(String, &Ident)> = Vec::new();
    for variant in &input.variants {
        for attr in variant.fields.iter().flat_map(|field| &field.attrs) {
            let helper = ["from", "display"]
                .into_iter()
                .find(|h| attr.path().is_ident(h));
            if let Some(helper) = helper {
                errors.push(syn::Error::new_spanned(
                    attr,
                    format!("`#[{helper}]` belongs on the variant, not on one of its fields"),
                ));
            }
        }
        if let Some(arm) = errors.keep(source_arm(variant)).flatten() {
            causes.push(arm);
        }
        let Some(attrs) = errors.keep(VariantAttrs::parse(variant)) else {
            continue;
        };
        match (&hook, attrs.display) {
            (None, Some(lit)) => {
                if let Some(arm) = errors.keep(display_arm(variant, &lit)) {
                    arms.push(arm);
                }
            }
            (None, None) => errors.push(syn::Error::new(
                variant.ident.span(),
                format!(
                    "variant `{}` needs `#[display(\"...\")]` (or the enum needs `#[display(with = path)]`)",
                    variant.ident
                ),
            )),
            (Some(_), Some(lit)) => errors.push(syn::Error::new(
                lit.span(),
                "the enum's `#[display(with = ..)]` already formats every variant; keep one of the two",
            )),
            (Some(_), None) => {}
        }
        let field = match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => fields.unnamed.first(),
            _ => None,
        };
        match (field, attrs.from) {
            (Some(_), Some(FromAttr::Skip(_))) => {}
            (None, Some(FromAttr::Skip(skip))) => errors.push(syn::Error::new(
                skip,
                "`#[from(skip)]` belongs on a single-field tuple variant; this variant has no `From`",
            )),
            (None, Some(FromAttr::Provider(ty))) => errors.push(syn::Error::new(
                ty.span(),
                "`#[from(concord::Error<D>)]` belongs on a single-field tuple variant: its field is built from the provider's failure",
            )),
            (Some(field), from) => {
                let (inner, boxed) = conversion_source(&field.ty);
                let provider = match &from {
                    Some(FromAttr::Provider(ty)) => Some(&**ty),
                    _ => None,
                };
                let source = provider.unwrap_or(inner);
                let key = source.to_token_stream().to_string().replace(" :: ", "::");
                if let Some((_, first)) = sources.iter().find(|(k, _)| *k == key) {
                    errors.push(syn::Error::new(
                        field.ty.span(),
                        format!(
                            "variant `{first}` already converts from `{key}`; mark one of the two `#[from(skip)]`"
                        ),
                    ));
                    continue;
                }
                sources.push((key, &variant.ident));
                froms.push(match provider {
                    Some(provider) => provider_impl(input, variant, &field.ty, inner, provider, boxed),
                    None => from_impl(input, variant, &field.ty, source, boxed),
                });
            }
            (None, None) => {}
        }
    }
    errors.finish()?;

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let formatter = formatter();
    let body = match hook {
        Some(path) => {
            let args = quote!(self, #formatter);
            quote_spanned!(path.span()=> #path(#args))
        }
        None if input.variants.is_empty() => quote!(match *self {}),
        None => quote!(match self { #(#arms)* }),
    };
    let source = (!causes.is_empty()).then(|| {
        quote! {
            fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                match self {
                    #(#causes)*
                    _ => ::core::option::Option::None,
                }
            }
        }
    });
    Ok(quote! {
        impl #impl_generics ::core::fmt::Display for #name #ty_generics #where_clause {
            fn fmt(&self, #formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                #body
            }
        }
        impl #impl_generics ::core::error::Error for #name #ty_generics #where_clause {
            #source
        }
        #(#froms)*
    })
}

/// The `Display::fmt` formatter parameter; hygienic, so no field binding can shadow it.
fn formatter() -> Ident {
    Ident::new("__formatter", Span::mixed_site())
}

/// The hygienic binding of a referenced field (`_0`, `status`); the format string reaches it
/// through an explicit named argument, never by capture.
fn alias(key: &str) -> Ident {
    Ident::new(&format!("__field_{key}"), Span::mixed_site())
}

/// Collects every authored mistake, so one build reports all of them.
#[derive(Default)]
struct Errors(Option<syn::Error>);

impl Errors {
    fn push(&mut self, error: syn::Error) {
        match &mut self.0 {
            Some(first) => first.combine(error),
            None => self.0 = Some(error),
        }
    }

    fn keep<T>(&mut self, result: syn::Result<T>) -> Option<T> {
        result.map_err(|error| self.push(error)).ok()
    }

    fn finish(self) -> syn::Result<()> {
        self.0.map_or(Ok(()), Err)
    }
}

/// The enum's `#[display(with = path)]`, a `fn(&Self, &mut Formatter<'_>) -> fmt::Result`.
fn enum_hook(attrs: &[Attribute]) -> syn::Result<Option<Path>> {
    let mut hook = None;
    for attr in attrs {
        if attr.path().is_ident("from") {
            return Err(syn::Error::new_spanned(
                attr,
                "`#[from(..)]` belongs on a variant",
            ));
        }
        if attr.path().is_ident("source") {
            return Err(syn::Error::new_spanned(
                attr,
                "`#[source]` belongs on the field that caused a variant's error",
            ));
        }
        if !attr.path().is_ident("display") {
            continue;
        }
        let path = attr.parse_args_with(|input: ParseStream<'_>| {
            if input.peek(LitStr) {
                return Err(input.error(
                    "a format string belongs on each variant; the enum takes `#[display(with = path)]`",
                ));
            }
            input.parse::<with>()?;
            input.parse::<Token![=]>()?;
            input.parse::<Path>()
        })?;
        if hook.replace(path).is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "a second `#[display]` on the enum",
            ));
        }
    }
    Ok(hook)
}

/// A variant's `#[from(..)]`: no conversion, or the provider's failure of a `concord::Error<D>`.
enum FromAttr {
    Skip(Span),
    Provider(Box<Type>),
}

/// A variant's `#[display("...")]` and `#[from(..)]`.
struct VariantAttrs {
    display: Option<LitStr>,
    from: Option<FromAttr>,
}

impl VariantAttrs {
    fn parse(variant: &Variant) -> syn::Result<Self> {
        let mut out = Self {
            display: None,
            from: None,
        };
        for attr in &variant.attrs {
            if attr.path().is_ident("display") {
                let lit = attr.parse_args_with(|input: ParseStream<'_>| {
                    if input.peek(with) {
                        return Err(input.error(
                            "`with = path` formats the whole enum: put `#[display(with = path)]` on the enum",
                        ));
                    }
                    input.parse::<LitStr>()
                })?;
                if out.display.replace(lit).is_some() {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "a second `#[display]` on this variant",
                    ));
                }
            } else if attr.path().is_ident("from") {
                let from = attr.parse_args_with(|input: ParseStream<'_>| {
                    if input.peek(skip) {
                        return Ok(FromAttr::Skip(input.parse::<skip>()?.span));
                    }
                    Ok(FromAttr::Provider(Box::new(input.parse::<Type>()?)))
                })?;
                if out.from.replace(from).is_some() {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "a second `#[from]` on this variant",
                    ));
                }
            } else if attr.path().is_ident("source") {
                return Err(syn::Error::new_spanned(
                    attr,
                    "`#[source]` belongs on the field that caused this variant's error",
                ));
            }
        }
        Ok(out)
    }
}

/// The variant's `source()` arm: its field marked `#[source]` (at most one; a syntactic `Box<T>`
/// is the `T`, an `Option<T>` is a source when present).
fn source_arm(variant: &Variant) -> syn::Result<Option<TokenStream>> {
    let mut marked = None;
    for (index, field) in variant.fields.iter().enumerate() {
        for attr in field.attrs.iter().filter(|a| a.path().is_ident("source")) {
            if !matches!(attr.meta, syn::Meta::Path(_)) {
                return Err(syn::Error::new_spanned(
                    attr,
                    "`#[source]` takes no arguments",
                ));
            }
            if marked.replace((index, field)).is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "a second `#[source]` in this variant: an error has one source",
                ));
            }
        }
    }
    let Some((index, field)) = marked else {
        return Ok(None);
    };
    let ident = &variant.ident;
    // Hygienic, but located at the field: a field that is not an error is reported there.
    let bound = Ident::new("__source", Span::mixed_site().located_at(field.ty.span()));
    let member = match &field.ident {
        Some(name) => quote!(#name),
        None => syn::Index::from(index).into_token_stream(),
    };
    let dyn_error = |ty: &Type| quote_spanned!(ty.span()=> &(dyn ::core::error::Error + 'static));
    let (inner, boxed) = conversion_source(&field.ty);
    let cause = if boxed {
        let target = dyn_error(inner);
        quote_spanned!(inner.span()=> ::core::option::Option::Some(&**#bound as #target))
    } else if let Some(inner) = option_inner(&field.ty) {
        let target = dyn_error(inner);
        quote_spanned!(inner.span()=> #bound.as_ref().map(|cause| cause as #target))
    } else {
        let target = dyn_error(&field.ty);
        quote_spanned!(field.ty.span()=> ::core::option::Option::Some(#bound as #target))
    };
    Ok(Some(
        quote!(Self::#ident { #member: #bound, .. } => #cause,),
    ))
}

/// `T` of a syntactic `Option<T>`.
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else { return None };
    let last = path.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    match (last.ident == "Option", args.args.first()) {
        (true, Some(syn::GenericArgument::Type(inner))) if args.args.len() == 1 => Some(inner),
        _ => None,
    }
}

/// A reference to a field in a format string.
enum FieldRef {
    Index(usize),
    Name(String),
}

/// One `match` arm. A string that is exactly one field (`"{0}"`, `"{error}"`) delegates to that
/// field's `Display` with the caller's formatter (flags, width and precision pass through); any
/// other string is written with `write!`, each referenced field passed as a named argument.
fn display_arm(variant: &Variant, lit: &LitStr) -> syn::Result<TokenStream> {
    let text = lit.value();
    let (refs, rewritten) = scan(&text).map_err(|message| syn::Error::new(lit.span(), message))?;
    let ident = &variant.ident;
    let error = |message: String| Err(syn::Error::new(lit.span(), message));
    // Each referenced field once, in first-use order, by its name in the rewritten string.
    let mut keys: Vec<String> = Vec::new();
    for r in &refs {
        let key = match r {
            FieldRef::Index(i) => format!("_{i}"),
            FieldRef::Name(name) => name.clone(),
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let used = |key: &str| keys.iter().any(|k| k == key);
    let pattern = match &variant.fields {
        Fields::Unit => {
            if !refs.is_empty() {
                return error("this variant has no fields to format".into());
            }
            quote!(Self::#ident)
        }
        Fields::Unnamed(fields) => {
            for r in &refs {
                match r {
                    FieldRef::Index(i) if *i < fields.unnamed.len() => {}
                    FieldRef::Index(i) => {
                        return error(format!("variant `{ident}` has no field `{i}`"));
                    }
                    FieldRef::Name(name) => {
                        return error(format!(
                            "`{{{name}}}`: a tuple variant's fields are `{{0}}`, `{{1}}`, ..."
                        ));
                    }
                }
            }
            let parts = (0..fields.unnamed.len()).map(|i| {
                let key = format!("_{i}");
                if used(&key) {
                    alias(&key).into_token_stream()
                } else {
                    quote!(_)
                }
            });
            quote!(Self::#ident(#(#parts),*))
        }
        Fields::Named(fields) => {
            let names: Vec<&Ident> = fields
                .named
                .iter()
                .filter_map(|f| f.ident.as_ref())
                .collect();
            for r in &refs {
                match r {
                    FieldRef::Name(name) if names.iter().any(|n| *n == name) => {}
                    FieldRef::Name(name) => {
                        return error(format!("variant `{ident}` has no field `{name}`"));
                    }
                    FieldRef::Index(i) => {
                        return error(format!("`{{{i}}}`: name the field of variant `{ident}`"));
                    }
                }
            }
            let parts = names.iter().filter(|n| used(&n.to_string())).map(|n| {
                let alias = alias(&n.to_string());
                quote!(#n: #alias)
            });
            quote!(Self::#ident { #(#parts,)* .. })
        }
    };
    let formatter = formatter();
    let whole = text.strip_prefix('{').and_then(|t| t.strip_suffix('}'));
    let body = match whole {
        Some(inner) if refs.len() == 1 && !inner.contains([':', '{', '}']) => {
            let field = alias(&keys[0]);
            quote_spanned!(lit.span()=> ::core::fmt::Display::fmt(#field, #formatter))
        }
        _ => {
            let rewritten = LitStr::new(&rewritten, lit.span());
            let args = keys.iter().map(|key| {
                let name = Ident::new(key, lit.span());
                let alias = alias(key);
                quote!(, #name = #alias)
            });
            quote_spanned!(lit.span()=> ::core::write!(#formatter, #rewritten #(#args)*))
        }
    };
    Ok(quote!(#pattern => #body,))
}

/// The field references of a format string, and the string with `{N` rewritten to `{_N`, the
/// name of the named argument that carries field `N`.
fn scan(text: &str) -> Result<(Vec<FieldRef>, String), String> {
    let mut refs = Vec::new();
    let mut out = String::with_capacity(text.len() + 4);
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..2]);
            rest = &tail[2..];
            continue;
        }
        if let Some(after) = tail.strip_prefix('}') {
            out.push('}');
            rest = after;
            continue;
        }
        let end = tail
            .find('}')
            .ok_or("an unclosed `{` in the format string")?;
        let inner = &tail[1..end];
        let (arg, spec) = inner.split_at(inner.find(':').unwrap_or(inner.len()));
        let arg = arg.trim();
        // A fill character may be `$`; after it, `$` and `.*` only name width/precision arguments.
        let flags = spec.get(1..).unwrap_or("");
        let flags = match flags.chars().nth(1) {
            Some('<' | '^' | '>') => &flags[flags.chars().next().map_or(0, char::len_utf8)..],
            _ => flags,
        };
        if flags.contains('$') || flags.contains(".*") {
            return Err(format!(
                "`{{{inner}}}`: a width or precision argument (`name$`, `0$`, `.*`) is not supported; \
                 write the number, or format with the enum's `#[display(with = path)]`"
            ));
        }
        if arg.is_empty() {
            return Err(format!(
                "`{{{inner}}}` names no field: write `{{0{spec}}}` or `{{field{spec}}}`"
            ));
        }
        // A field index, or a Rust identifier (Unicode `XID` rules, keywords excluded).
        let index = arg.bytes().all(|b| b.is_ascii_digit());
        if !index && syn::parse_str::<Ident>(arg).is_err() {
            return Err(format!("`{{{inner}}}`: `{arg}` is not a field name"));
        }
        if let Ok(index) = arg.parse::<usize>() {
            refs.push(FieldRef::Index(index));
            out.push_str(&format!("{{_{index}{spec}}}"));
        } else {
            refs.push(FieldRef::Name(arg.to_owned()));
            out.push_str(&format!("{{{arg}{spec}}}"));
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok((refs, out))
}

/// The `From` source of a field: `T` for a syntactic `Box<T>` (built with `Box::new`), the field
/// type otherwise.
fn conversion_source(ty: &Type) -> (&Type, bool) {
    if let Type::Path(path) = ty
        && path.qself.is_none()
    {
        let segments: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let boxed = matches!(
            segments
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            ["Box"] | ["std" | "alloc", "boxed", "Box"]
        );
        if boxed
            && let Some(last) = path.path.segments.last()
            && let syn::PathArguments::AngleBracketed(args) = &last.arguments
            && args.args.len() == 1
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return (inner, true);
        }
    }
    (ty, false)
}

/// `From<concord::Error<D>>` for the variant marked `#[from(concord::Error<D>)]`: the provider's
/// failure builds its field (`From<Failure<D>>`); every other failure goes through the enum's own
/// `From<concord::Error>`.
fn provider_impl(
    input: &ItemEnum,
    variant: &Variant,
    field: &Type,
    inner: &Type,
    provider: &Type,
    boxed: bool,
) -> TokenStream {
    let name = &input.ident;
    let ident = &variant.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    // Hygienic, but located where a missing conversion is the author's to add.
    let error = Ident::new("error", Span::mixed_site().located_at(provider.span()));
    let failure = Ident::new("failure", Span::mixed_site().located_at(inner.span()));
    // The argument is a block at the field, so a missing `From<Failure<D>>` is reported there.
    let value = quote_spanned!(inner.span()=> ::core::convert::From::from({ #failure }));
    let built = if boxed {
        quote_spanned!(field.span()=> <#field>::new(#value))
    } else {
        value
    };
    let rest = quote_spanned!(provider.span()=> <Self as ::core::convert::From<_>>::from(#error));
    quote_spanned! {provider.span()=>
        impl #impl_generics ::core::convert::From<#provider> for #name #ty_generics #where_clause {
            fn from(#error: #provider) -> Self {
                match #error.into_provider() {
                    ::core::result::Result::Ok(#failure) => Self::#ident(#built),
                    ::core::result::Result::Err(#error) => #rest,
                }
            }
        }
    }
}

fn from_impl(
    input: &ItemEnum,
    variant: &Variant,
    field: &Type,
    source: &Type,
    boxed: bool,
) -> TokenStream {
    let name = &input.ident;
    let ident = &variant.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let value = Ident::new("value", Span::mixed_site());
    let built = if boxed {
        quote_spanned!(field.span()=> <#field>::new(#value))
    } else {
        quote!(#value)
    };
    quote_spanned! {field.span()=>
        impl #impl_generics ::core::convert::From<#source> for #name #ty_generics #where_clause {
            fn from(#value: #source) -> Self {
                Self::#ident(#built)
            }
        }
    }
}
