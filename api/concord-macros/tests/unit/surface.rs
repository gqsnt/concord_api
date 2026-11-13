//! Compare the public signatures with the independent hand-written clients.
use super::{description, package};
use proc_macro2::{Group, TokenStream, TokenTree};
use quote::{quote, ToTokens};
use std::collections::BTreeMap;
use syn::visit::{self, Visit};

fn canonical(tokens: impl ToTokens) -> String {
    let mut text = tokens.to_token_stream().to_string().replace(' ', "");
    for (old, new) in [
        ("::concord::", "concord::"),
        ("::std::", "std::"),
        ("::core::", "std::"),
        ("core::", "std::"),
        ("std::result::Result", "Result"),
        ("std::option::Option", "Option"),
        ("std::vec::Vec", "Vec"),
        ("std::string::String", "String"),
        ("std::marker::Send", "Send"),
        ("std::marker::Sync", "Sync"),
        ("std::future::Future", "Future"),
        ("std::ops::Fn", "Fn"),
    ] {
        text = text.replace(old, new);
    }
    text
}

pub(super) struct Arguments {
    pub(super) value: TokenStream,
    pub(super) problem: TokenStream,
    pub(super) result: TokenStream,
    pub(super) shown: bool,
}
fn substitute(tokens: TokenStream, args: &Arguments) -> TokenStream {
    let input: Vec<_> = tokens.into_iter().collect();
    let mut out = TokenStream::new();
    let mut i = 0;
    while i < input.len() {
        if matches!(&input[i],TokenTree::Punct(p) if p.as_char()=='$') {
            match &input[i + 1] {
                TokenTree::Ident(name) => {
                    out.extend(match name.to_string().as_str() {
                        "value" => args.value.clone(),
                        "problem" | "shown_problem" => args.problem.clone(),
                        "crate" => quote!(::concord),
                        _ => panic!("unknown signature variable {name}"),
                    });
                    i += 2;
                }
                TokenTree::Group(group) => {
                    if group.stream().to_string().contains("result") {
                        out.extend(args.result.clone());
                        i += 5; // $($result)::+
                    } else {
                        if args.shown {
                            out.extend(substitute(group.stream(), args));
                        }
                        i += 3; // $(...)?
                    }
                }
                _ => panic!("invalid helper signature"),
            }
        } else {
            match &input[i] {
                TokenTree::Group(g) => out.extend([TokenTree::Group(Group::new(
                    g.delimiter(),
                    substitute(g.stream(), args),
                ))]),
                token => out.extend([token.clone()]),
            }
            i += 1;
        }
    }
    out
}

pub(super) fn helper_functions(
    tokens: TokenStream,
    args: &Arguments,
    conditional: bool,
    out: &mut Vec<syn::Signature>,
) {
    let input: Vec<_> = tokens.into_iter().collect();
    let mut i = 0;
    while i < input.len() {
        if matches!(&input[i],TokenTree::Ident(n) if n=="pub")
            && matches!(input.get(i+1),Some(TokenTree::Ident(n)) if n=="fn")
        {
            let end = (i + 2..input.len()).find(|&j| matches!(&input[j],TokenTree::Group(g) if g.delimiter()==proc_macro2::Delimiter::Brace)).unwrap();
            let sig: syn::Signature = syn::parse2(substitute(
                input[i + 1..end].iter().cloned().collect(),
                args,
            ))
            .unwrap();
            if !sig.ident.to_string().starts_with("__")
                && (conditional || sig.ident != "send_if_none_match")
            {
                out.push(sig);
            }
            i = end + 1;
        } else {
            if let TokenTree::Group(g) = &input[i] {
                helper_functions(g.stream(), args, conditional, out);
            }
            i += 1;
        }
    }
}

fn helper_types(tokens: TokenStream, args: &Arguments, out: &mut BTreeMap<String, String>) {
    let input: Vec<_> = tokens.into_iter().collect();
    let mut i = 0;
    while i < input.len() {
        if matches!(&input[i],TokenTree::Ident(n) if n=="type")
            && matches!(input.get(i+1),Some(TokenTree::Ident(n)) if n=="Output" || n=="IntoFuture")
        {
            let end = (i + 3..input.len())
                .find(|&j| matches!(&input[j],TokenTree::Punct(p) if p.as_char()==';'))
                .unwrap();
            let name = input[i + 1].to_string();
            let ty = substitute(input[i + 3..end].iter().cloned().collect(), args);
            out.insert(name, canonical(ty));
            i = end + 1;
        } else {
            if let TokenTree::Group(g) = &input[i] {
                helper_types(g.stream(), args, out);
            }
            i += 1;
        }
    }
}

struct Surface<'a> {
    lines: BTreeMap<String, String>,
    prefix: Vec<String>,
    helper: &'a TokenStream,
    aliases: BTreeMap<String, String>,
    problems: BTreeMap<String, String>,
}
impl Surface<'_> {
    fn method(&mut self, owner: &str, sig: &syn::Signature) {
        let mut sig = sig.clone();
        sig.inputs.pop_punct();
        self.lines
            .insert(format!("{owner}::{}", sig.ident), canonical(sig));
    }
    fn owner(&self, ty: &syn::Type) -> String {
        let name = canonical(ty);
        self.aliases.get(&name).cloned().unwrap_or(name)
    }
    // Compare the type of IntoFuture::Output after expanding the client's
    // Result alias. Keep alias declarations themselves literal, so T/__T is
    // still checked; the documentation tests separately forbid projections.
    fn future_output(&self, owner: &str, value: String) -> String {
        let root = owner.split("::").next().unwrap();
        let prefix = format!("{root}::Result<");
        if let Some(inner) = value
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix('>'))
        {
            let p = self.problems.get(root).unwrap();
            format!("Result<{inner},concord::Error<{p}>>")
        } else {
            value
        }
    }
}
impl<'ast> Visit<'ast> for Surface<'_> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if matches!(item.vis, syn::Visibility::Public(_)) {
            self.lines.insert(
                self.prefix
                    .iter()
                    .chain(std::iter::once(&item.ident.to_string()))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::"),
                "module".into(),
            );
            self.prefix.push(item.ident.to_string());
            visit::visit_item_mod(self, item);
            self.prefix.pop();
        }
    }
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if matches!(item.vis, syn::Visibility::Public(_))
            && !item.ident.to_string().starts_with("__")
        {
            let owner = self
                .prefix
                .iter()
                .chain(std::iter::once(&item.ident.to_string()))
                .cloned()
                .collect::<Vec<_>>()
                .join("::");
            self.lines.insert(owner.clone(), "struct".into());
            if item
                .attrs
                .iter()
                .any(|a| a.path().is_ident("derive") && quote!(#a).to_string().contains("Clone"))
            {
                self.lines
                    .insert(format!("{owner}::__trait::Clone"), "implemented".into());
            }
        }
    }
    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if item.ident.to_string().starts_with("__Impl") {
            self.aliases
                .insert(item.ident.to_string(), canonical(&item.ty));
        }
        if matches!(item.vis, syn::Visibility::Public(_))
            && !item.ident.to_string().starts_with("__")
        {
            let (name, generics, ty) = (&item.ident, &item.generics, &item.ty);
            self.lines.insert(
                format!("{}::{}", self.prefix.join("::"), item.ident),
                canonical(quote!(pub type #name #generics = #ty;)),
            );
        }
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let owner = self.owner(&item.self_ty);
        if item.trait_.is_none() {
            for method in &item.items {
                if let syn::ImplItem::Fn(method) = method {
                    if matches!(method.vis, syn::Visibility::Public(_))
                        && !method.sig.ident.to_string().starts_with("__")
                    {
                        self.method(&owner, &method.sig);
                    }
                }
            }
        } else if let Some((_, path, _)) = &item.trait_ {
            let name = &path.segments.last().unwrap().ident;
            if name == "__Types" {
                let root = path.segments.first().unwrap().ident.to_string();
                for associated in &item.items {
                    if let syn::ImplItem::Type(t) = associated {
                        if t.ident == "__Problem" {
                            self.problems.insert(root.clone(), canonical(&t.ty));
                        }
                    }
                }
            }
            if name == "Clone" || name == "Debug" {
                self.lines
                    .insert(format!("{owner}::__trait::{name}"), "implemented".into());
            }
            if name == "IntoFuture" {
                for associated in &item.items {
                    if let syn::ImplItem::Type(associated) = associated {
                        let value = canonical(&associated.ty);
                        let value = if associated.ident == "Output" {
                            self.future_output(&owner, value)
                        } else {
                            value
                        };
                        self.lines.insert(
                            format!("{owner}::__trait::IntoFuture::{}", associated.ident),
                            value,
                        );
                    }
                }
            }
        }
    }
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if item.path.segments.last().unwrap().ident != "call_methods" {
            return;
        }
        use syn::parse::Parser;
        let (call, args, conditional, future, rest) = (|p: syn::parse::ParseStream<'_>| {
            let call: syn::Type = p.parse()?;
            p.parse::<syn::Token![,]>()?;
            let value: syn::Type = p.parse()?;
            p.parse::<syn::Token![,]>()?;
            let problem: syn::Type = p.parse()?;
            p.parse::<syn::Token![,]>()?;
            let result: syn::Path = p.parse()?;
            p.parse::<syn::Token![,]>()?;
            let _: syn::LitStr = p.parse()?;
            p.parse::<syn::Token![,]>()?;
            let future: syn::Ident = p.parse()?;
            p.parse::<syn::Token![,]>()?;
            let conditional: syn::Ident = p.parse()?;
            Ok((
                call,
                Arguments {
                    value: value.to_token_stream(),
                    shown: !matches!(&problem,syn::Type::Tuple(t) if t.elems.is_empty()),
                    problem: problem.to_token_stream(),
                    result: result.to_token_stream(),
                },
                conditional == "conditional",
                future == "awaitable",
                p.parse::<TokenStream>()?,
            ))
        })
        .parse2(item.tokens.clone())
        .unwrap();
        let mut methods = Vec::new();
        helper_functions(self.helper.clone(), &args, conditional, &mut methods);
        helper_functions(rest, &args, conditional, &mut methods);
        for sig in methods {
            self.method(&canonical(&call), &sig);
        }
        if future {
            let mut types = BTreeMap::new();
            helper_types(self.helper.clone(), &args, &mut types);
            for (name, ty) in types {
                let owner = canonical(&call);
                let ty = if name == "Output" {
                    self.future_output(&owner, ty)
                } else {
                    ty
                };
                self.lines
                    .insert(format!("{owner}::__trait::IntoFuture::{name}"), ty);
            }
        }
    }
}

#[test]
fn c1_16_opus_public_surface_matches_the_hand_written_model() {
    let core = syn::parse_file(
        &std::fs::read_to_string(package().join("../concord-core/src/__private/call.rs")).unwrap(),
    )
    .unwrap();
    let helper = core
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Macro(m) if m.ident.as_ref().is_some_and(|n| n == "call_methods") => {
                Some(&m.mac.tokens)
            }
            _ => None,
        })
        .unwrap();
    let collect = |files: Vec<syn::File>| {
        let mut s = Surface {
            lines: BTreeMap::new(),
            prefix: Vec::new(),
            helper,
            aliases: BTreeMap::new(),
            problems: BTreeMap::new(),
        };
        for file in files {
            s.visit_file(&file);
        }
        s.lines
    };
    let generated = collect(
        ["tasks", "riot", "files", "feed", "shapes"]
            .iter()
            .map(|name| {
                let source =
                    std::fs::read_to_string(package().join(format!("../tour-gen/src/{name}.rs")))
                        .unwrap();
                syn::parse2(crate::expand(description(&source))).unwrap()
            })
            .collect(),
    );
    let model = collect(
        [
            "declarations",
            "tasks",
            "riot",
            "files",
            "grant",
            "feed",
            "shapes",
        ]
        .iter()
        .map(|name| {
            syn::parse_file(
                &std::fs::read_to_string(package().join(format!("../tour/src/{name}.rs"))).unwrap(),
            )
            .unwrap()
        })
        .collect(),
    );
    let differences: Vec<_> = model
        .keys()
        .chain(generated.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|key| model.get(*key) != generated.get(*key))
        .map(|key| {
            format!(
                "{key}\nmodel: {:?}\nmacro: {:?}",
                model.get(key),
                generated.get(key)
            )
        })
        .collect();
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
