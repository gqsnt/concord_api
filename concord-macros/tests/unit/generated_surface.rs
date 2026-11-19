//! Compare the public signatures with the independent hand-written clients.
use super::{description, package};
use quote::{quote, ToTokens};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::{self, Visit};

/// The tokens without their spacing. A string literal is kept as its decoded text, written
/// in hexadecimal between quotes, so that no rewriting of paths or names below can reach
/// inside it: `"tasks.cre ate"` and `"tasks.concord::reqwest::create"` differ from
/// `"tasks.create"`, and a raw string equals an ordinary one.
fn spelled(tokens: proc_macro2::TokenStream) -> String {
    use proc_macro2::{Delimiter, TokenTree};
    tokens
        .into_iter()
        .map(|token| match token {
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                format!("{open}{}{close}", spelled(group.stream()))
            }
            TokenTree::Literal(literal) => {
                match syn::parse2::<syn::LitStr>(TokenTree::Literal(literal.clone()).into()) {
                    Ok(text) => format!(
                        "\"{}\"",
                        text.value()
                            .bytes()
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<String>()
                    ),
                    Err(_) => literal.to_string(),
                }
            }
            other => other.to_string(),
        })
        .collect()
}

fn canonical(tokens: impl ToTokens) -> String {
    let mut text = spelled(tokens.to_token_stream());
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
        ("std::time::Duration", "Duration"),
        ("std::primitive::str", "str"),
        ("concord::reqwest::", ""),
        ("concord::Sending", "Sending"),
        ("concord::Answer", "Answer"),
        ("concord::Pages", "Pages"),
    ] {
        text = text.replace(old, new);
    }
    text
}

fn public(vis: &syn::Visibility, ident: &syn::Ident) -> bool {
    matches!(vis, syn::Visibility::Public(_)) && public_name(ident)
}

fn public_name(ident: &syn::Ident) -> bool {
    !ident.to_string().starts_with("__")
}

#[derive(Default)]
struct Surface {
    lines: BTreeMap<String, String>,
    prefix: Vec<String>,
    aliases: BTreeMap<String, String>,
    problems: BTreeMap<String, String>,
    local_types: BTreeMap<String, String>,
    future_aliases: BTreeMap<(String, String), String>,
    rules: BTreeMap<String, Rules>,
    mode_traits: BTreeSet<(String, String)>,
    /// The hand-written `call.rs`, which each tour client module includes.
    call_source: String,
}

#[derive(Debug, Default)]
struct Rules {
    paged: bool,
    awaitable: bool,
    conditional: bool,
}

fn hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("doc")
            && matches!(&attr.meta, syn::Meta::List(doc) if doc.tokens.to_string() == "hidden")
    })
}

/// The attributes of an item, compared as a user sees them: documentation by its decoded
/// text (a raw string and an ordinary one agree), every other attribute by its tokens.
fn attributes(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .map(|attr| match &attr.meta {
            syn::Meta::NameValue(doc) if doc.path.is_ident("doc") => match &doc.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(text),
                    ..
                }) => format!("doc={:?};", text.value()),
                other => format!("doc={};", canonical(other)),
            },
            _ => format!("{};", canonical(attr)),
        })
        .collect()
}

impl Surface {
    fn qualified(&self, name: &impl ToString) -> String {
        self.prefix
            .iter()
            .cloned()
            .chain(std::iter::once(name.to_string()))
            .collect::<Vec<_>>()
            .join("::")
    }

    fn add(&mut self, key: String, value: impl Into<String>) {
        assert!(
            self.lines.insert(key.clone(), value.into()).is_none(),
            "duplicate public item {key}"
        );
    }

    fn signature(&self, owner: &str, tokens: impl ToTokens) -> String {
        let root = owner.split("::").next().unwrap();
        let root = match root {
            "Tasks" => "tasks",
            "Riot" => "riot",
            "Files" => "files",
            "Feed" => "feed",
            "Shapes" => "shapes",
            _ => root,
        };
        let problem = self.problems.get(root).map_or("()", String::as_str);
        let storage_problem = format!("{root}::__Problem");
        let text = canonical(tokens)
            .replace(&storage_problem, problem)
            .replace("<Selfasconcord::page::Call>::Problem", problem)
            .replace("<()as__Types>::__Problem", problem)
            .replace("__Problem", problem)
            .replace("self::Retry", "Retry");
        let mut text = text
            .split_inclusive(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map(|part| {
                let name =
                    part.trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
                match self.local_types.get(name) {
                    Some(ty) if ty == &storage_problem => {
                        format!("{problem}{}", &part[name.len()..])
                    }
                    Some(ty) => format!("{ty}{}", &part[name.len()..]),
                    None => part.to_owned(),
                }
            })
            .collect::<String>()
            .replace(&format!("{root}::Result<"), "Result<");

        for alias in ["__Sending", "__Pages"] {
            if let Some((head, arg)) = text.split_once(&format!("{alias}<")) {
                let arg = arg.strip_suffix('>').unwrap();
                let ty = self
                    .future_aliases
                    .get(&(root.to_owned(), alias.to_owned()))
                    .unwrap();
                text = format!("{head}{}", ty.replacen("<T", &format!("<{arg}"), 1));
            }
        }

        // Sending and Pages default their final generic argument to ().
        if ["Sending<", "Pages<"]
            .iter()
            .any(|name| text.contains(name))
        {
            if let Some(without_default) = text.strip_suffix(",()>") {
                text = format!("{without_default}>");
            }
        }
        if owner.ends_with("Call<E>") && text == "Result<E::Value>" {
            text = format!("Result<E::Value,concord::Error<{problem}>>");
        }
        text
    }

    fn owner(&self, ty: &syn::Type) -> String {
        let name = canonical(ty);
        if let Some(alias) = self.aliases.get(&name) {
            return alias.clone();
        }
        if self.prefix.is_empty() || name.starts_with("concord::") {
            name
        } else {
            self.qualified(&name)
        }
    }

    fn struct_traits(&mut self, owner: &str, attrs: &[syn::Attribute]) {
        use syn::punctuated::Punctuated;
        for attr in attrs.iter().filter(|attr| attr.path().is_ident("derive")) {
            let paths = attr
                .parse_args_with(Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
                .unwrap();
            for path in paths {
                let name = &path.segments.last().unwrap().ident;
                if name == "Clone" || name == "Debug" {
                    self.add(format!("{owner}::__trait::{name}"), "implemented");
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Surface {
    fn visit_file(&mut self, file: &'ast syn::File) {
        // The client module and its storage types are siblings in the expansion.
        // Resolve the problem before comparing signatures from either scope.
        for item in &file.items {
            let syn::Item::Const(item) = item else {
                continue;
            };
            let syn::Expr::Block(block) = item.expr.as_ref() else {
                continue;
            };
            for statement in &block.block.stmts {
                let syn::Stmt::Item(syn::Item::Impl(item)) = statement else {
                    continue;
                };
                let Some((_, path, _)) = &item.trait_ else {
                    continue;
                };
                let mut segments = path.segments.iter();
                let (Some(client), Some(types), None) =
                    (segments.next(), segments.next(), segments.next())
                else {
                    continue;
                };
                if types.ident == "__Types" {
                    for item in &item.items {
                        if let syn::ImplItem::Type(ty) = item {
                            if ty.ident == "__Problem" {
                                self.problems
                                    .insert(client.ident.to_string(), canonical(&ty.ty));
                            }
                        }
                    }
                }
            }
        }
        visit::visit_file(self, file);
    }

    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        let enclosing = self.local_types.clone();
        if let syn::Expr::Block(block) = &*item.expr {
            for statement in &block.block.stmts {
                if let syn::Stmt::Item(syn::Item::Type(alias)) = statement {
                    let name = alias.ident.to_string();
                    if name.starts_with("__") {
                        self.local_types.insert(name, canonical(&alias.ty));
                    }
                }
            }
        }
        visit::visit_item_const(self, item);
        self.local_types = enclosing;
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !public(&item.vis, &item.ident) {
            return;
        }
        self.add(self.qualified(&item.ident), "module");
        self.prefix.push(item.ident.to_string());
        if let Some((_, items)) = &item.content {
            for item in items {
                if let syn::Item::Type(alias) = item {
                    if alias.ident == "__Problem" {
                        self.problems
                            .entry(self.prefix.join("::"))
                            .or_insert_with(|| canonical(&alias.ty).replace("super::", ""));
                    }
                    if alias.ident == "__Sending" || alias.ident == "__Pages" {
                        self.future_aliases.insert(
                            (self.prefix.join("::"), alias.ident.to_string()),
                            canonical(&alias.ty).replace("super::", ""),
                        );
                    }
                }
            }
        }
        visit::visit_item_mod(self, item);
        self.prefix.pop();
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if public(&item.vis, &item.ident) {
            let owner = self.qualified(&item.ident);
            self.add(
                owner.clone(),
                format!("struct{}", canonical(&item.generics)),
            );
            self.struct_traits(&owner, &item.attrs);
        }
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        if public(&item.vis, &item.ident) {
            let owner = self.qualified(&item.ident);
            let variants = &item.variants;
            self.add(
                owner.clone(),
                format!("enum{{{}}}", canonical(quote!(#variants))),
            );
            self.struct_traits(&owner, &item.attrs);
        }
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if public(&item.vis, &item.ident) {
            let owner = self.qualified(&item.ident);
            let public_bounds: BTreeSet<_> = item
                .supertraits
                .iter()
                .filter_map(|bound| match bound {
                    syn::TypeParamBound::Trait(bound) => {
                        let name = bound.path.segments.last().unwrap().ident.to_string();
                        ["Sized", "Send", "Sync"]
                            .contains(&name.as_str())
                            .then_some(name)
                    }
                    syn::TypeParamBound::Lifetime(lifetime) => Some(canonical(lifetime)),
                    _ => None,
                })
                .collect();
            self.add(owner.clone(), format!("trait {public_bounds:?}"));
            for associated in &item.items {
                match associated {
                    syn::TraitItem::Type(ty) if !hidden(&ty.attrs) => {
                        let mut signature = ty.clone();
                        signature.attrs.clear();
                        self.add(
                            format!("{owner}::{}", ty.ident),
                            format!("{}{}", attributes(&ty.attrs), canonical(&signature)),
                        );
                    }
                    syn::TraitItem::Fn(method) if !hidden(&method.attrs) => {
                        self.add(
                            format!("{owner}::{}", method.sig.ident),
                            canonical(&method.sig),
                        );
                    }
                    syn::TraitItem::Const(item) if !hidden(&item.attrs) => {
                        let mut signature = item.clone();
                        signature.attrs.clear();
                        self.add(
                            format!("{owner}::{}", item.ident),
                            format!("{}{}", attributes(&item.attrs), canonical(&signature)),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if item.ident.to_string().starts_with("__Impl") {
            let ty = canonical(&item.ty);
            let target = match ty.strip_prefix("self::") {
                Some(path) => self.qualified(&path),
                None => ty,
            };
            self.aliases.insert(self.qualified(&item.ident), target);
        }
        if public(&item.vis, &item.ident) {
            let (name, generics, ty) = (&item.ident, &item.generics, &item.ty);
            let owner = self.qualified(name);
            let alias = self.signature(&owner, quote!(pub type #name #generics = #ty;));
            self.add(owner, alias.replace("super::", ""));
        }
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let owner = self.owner(&item.self_ty);
        if owner.split("::").any(|part| part.starts_with("__")) {
            return;
        }
        if let Some((_, path, _)) = &item.trait_ {
            let name = &path.segments.last().unwrap().ident;
            if name == "Clone" || name == "Debug" {
                self.add(format!("{owner}::__trait::{name}"), "implemented");
            }
            if name == "Unpin" {
                // The whole header: a bound or a `where` clause would narrow the promise.
                // Everything but the spelling of the trait's path and of the type, which
                // the owner already names: polarity, `unsafe`, generics, `where`, items.
                let trait_path = canonical(path).replace("std::marker::Unpin", "Unpin");
                let generics = &item.generics;
                let where_clause = &item.generics.where_clause;
                let negative = item
                    .trait_
                    .as_ref()
                    .is_some_and(|(bang, _, _)| bang.is_some());
                let items = &item.items;
                self.add(
                    format!("{owner}::__trait::Unpin"),
                    format!(
                        "{}{}{}impl{}{}{trait_path}for{owner}{}{{{}}}",
                        attributes(&item.attrs),
                        if item.defaultness.is_some() {
                            "default"
                        } else {
                            ""
                        },
                        if item.unsafety.is_some() {
                            "unsafe"
                        } else {
                            ""
                        },
                        canonical(generics),
                        if negative { "!" } else { "" },
                        canonical(where_clause),
                        canonical(quote!(#(#items)*)),
                    ),
                );
            }
            if name == "Endpoint" {
                for associated in &item.items {
                    if let syn::ImplItem::Const(constant) = associated {
                        if public_name(&constant.ident) {
                            self.add(
                                format!("{owner}::__trait::Endpoint::{}", constant.ident),
                                format!(
                                    "{}{}={}",
                                    attributes(&constant.attrs),
                                    canonical(&constant.ty),
                                    // A string by its value, never rewritten.
                                    match &constant.expr {
                                        syn::Expr::Lit(syn::ExprLit {
                                            lit: syn::Lit::Str(text),
                                            ..
                                        }) => format!("{:?}", text.value()),
                                        other => canonical(other),
                                    }
                                ),
                            );
                        }
                    }
                }
            }
            match name.to_string().as_str() {
                "Awaitable" => self.rules.entry(owner.clone()).or_default().awaitable = true,
                "Paged" => self.rules.entry(owner.clone()).or_default().paged = true,
                "Conditional" => self.rules.entry(owner.clone()).or_default().conditional = true,
                _ => {}
            }
            if ["IntoFuture", "Endpoint", "Call"]
                .iter()
                .any(|expected| name == expected)
            {
                for associated in &item.items {
                    if let syn::ImplItem::Type(associated) = associated {
                        match (
                            name.to_string().as_str(),
                            associated.ident.to_string().as_str(),
                        ) {
                            ("Endpoint", "Paging") => {
                                let mode = canonical(&associated.ty);
                                let rules = self.rules.entry(owner.clone()).or_default();
                                rules.paged = self
                                    .mode_traits
                                    .contains(&(mode.clone(), "DeclaresPaginate".into()));
                                rules.awaitable = self
                                    .mode_traits
                                    .contains(&(mode, "AwaitWithoutPaginate".into()));
                            }
                            ("Endpoint", "Conditional") => {
                                self.rules.entry(owner.clone()).or_default().conditional = self
                                    .mode_traits
                                    .contains(&(canonical(&associated.ty), "IsGetOrHead".into()));
                            }
                            ("Endpoint", "Value")
                            | ("IntoFuture", _)
                            | ("Call", "Value" | "Problem") => {
                                if name == "Endpoint" {
                                    self.rules.entry(owner.clone()).or_default();
                                }
                                self.add(
                                    format!("{owner}::__trait::{name}::{}", associated.ident),
                                    self.signature(&owner, &associated.ty),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else {
            for method in &item.items {
                if let syn::ImplItem::Fn(method) = method {
                    if public(&method.vis, &method.sig.ident) {
                        let mut sig = method.sig.clone();
                        sig.inputs.pop_punct();
                        // Binding mutability is local to the method body.
                        for argument in &mut sig.inputs {
                            match argument {
                                syn::FnArg::Receiver(receiver) if receiver.reference.is_none() => {
                                    receiver.mutability = None;
                                }
                                syn::FnArg::Typed(argument) => {
                                    if let syn::Pat::Ident(binding) = &mut *argument.pat {
                                        binding.mutability = None;
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.add(
                            format!("{owner}::{}", sig.ident),
                            self.signature(&owner, &sig),
                        );
                    }
                }
            }
        }
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac.path.is_ident("include")
            && syn::parse2::<syn::LitStr>(mac.tokens.clone())
                .is_ok_and(|name| name.value() == "call.rs")
        {
            let call = syn::parse_file(&self.call_source).unwrap();
            self.visit_file(&call);
        }
    }
}

fn core_mode_traits() -> BTreeSet<(String, String)> {
    let file = syn::parse_file(include_str!("../../../concord-core/src/call.rs")).unwrap();
    super::generation::impls(&file)
        .into_iter()
        .filter_map(|(_, block)| {
            let (_, path, _) = block.trait_.as_ref()?;
            let name = path.get_ident()?.to_string();
            assert!(block.generics.params.is_empty() && block.generics.where_clause.is_none());
            Some((
                format!("concord::call::{}", canonical(&block.self_ty)),
                name,
            ))
        })
        .collect()
}

fn check_impl_gates(file: &syn::File, generated: bool, violations: &mut Vec<String>) {
    let mut gates = BTreeSet::new();
    for (module, block) in super::generation::impls(file) {
        let owner = canonical(&block.self_ty);
        if !owner.ends_with("Call<E>") {
            continue;
        }
        let trait_name = block
            .trait_
            .as_ref()
            .map(|(_, name, _)| name.segments.last().unwrap().ident.to_string());
        for item in &block.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            let name = method.sig.ident.to_string();
            let (field, gate, hand_written_trait) = match (trait_name.as_deref(), name.as_str()) {
                (Some("IntoFuture"), "into_future") => {
                    ("Paging", "AwaitWithoutPaginate", "Awaitable")
                }
                (None, "pages") => ("Paging", "DeclaresPaginate", "Paged"),
                (None, "send_if_none_match" | "__prepare_if_none_match")
                | (Some("ConditionalCall"), "set_if_none_match") => {
                    ("Conditional", "IsGetOrHead", "Conditional")
                }
                _ => continue,
            };
            assert!(gates.insert(name.clone()), "duplicate {name}");
            let expected = if generated {
                format!("whereE::{field}:concord::call::{gate}")
            } else {
                format!("__spec::{hand_written_trait}")
            };
            let actual = if generated {
                block
                    .generics
                    .where_clause
                    .as_ref()
                    .map_or(String::new(), |where_clause| {
                        canonical(where_clause).trim_end_matches(',').to_string()
                    })
            } else {
                block
                    .generics
                    .type_params()
                    .next()
                    .map_or(String::new(), |p| canonical(&p.bounds))
            };
            let expected_parameter = if generated {
                owner.split_once("::").map_or_else(
                    || "Endpoint".into(),
                    |(root, _)| format!("{root}::Endpoint"),
                )
            } else {
                expected.clone()
            };
            let correct_parameter = block.generics.params.len() == 1
                && block
                    .generics
                    .type_params()
                    .next()
                    .is_some_and(|parameter| {
                        parameter.ident == "E"
                            && parameter.default.is_none()
                            && canonical(&parameter.bounds) == expected_parameter
                    });
            if actual != expected
                || !correct_parameter
                || (!generated && block.generics.where_clause.is_some())
                || method.sig.generics.where_clause.is_some()
                || !method.sig.generics.params.is_empty()
            {
                violations.push(format!("{module} {owner}::{name} must be gated on its impl by {expected}; got {actual} and {}", canonical(&method.sig.generics)));
            }
        }
    }
    let expected = [
        "into_future",
        "pages",
        "send_if_none_match",
        "__prepare_if_none_match",
        "set_if_none_match",
    ]
    .map(str::to_owned)
    .into();
    if gates != expected {
        violations.push(format!(
            "eligibility impls: expected {expected:?}, got {gates:?}"
        ));
    }
}

fn check_authored_arguments(
    description: proc_macro2::TokenStream,
    file: &syn::File,
    violations: &mut Vec<String>,
) {
    use crate::ast::Node;
    type Method = (String, String);
    fn expected<'a>(
        node: &'a Node,
        root: &str,
        module: &str,
        parent: &str,
        levels: &[crate::flat::Level<'a>],
        methods: &mut BTreeMap<Method, BTreeMap<String, (bool, String)>>,
    ) {
        let local = node.name.to_string();
        let kind = &node.kind;
        let is_client = matches!(kind, crate::ast::NodeKind::Client);
        let is_scope = matches!(kind, crate::ast::NodeKind::Scope);
        let typed = if is_client {
            local.clone()
        } else {
            format!("{module}::{}", crate::name::type_name(&local))
        };
        let factory = if is_client {
            (typed.clone(), "new".into())
        } else {
            (parent.into(), local.clone())
        };
        let level = levels
            .iter()
            .find(|level| std::ptr::eq(level.node, node))
            .unwrap();
        for entry in level
            .inputs
            .iter()
            .filter(|entry| std::ptr::eq(entry.owner, node))
        {
            let input = entry.input;
            let optional = input.optional.is_some();
            let name = input.name.to_string();
            let targets = if optional {
                vec![
                    (typed.clone(), name.clone()),
                    (format!("{root}::Call<{typed}>"), name.clone()),
                ]
            } else {
                vec![factory.clone()]
            };
            for target in targets {
                methods.entry(target).or_default().insert(
                    name.clone(),
                    (optional, input.ty.to_token_stream().to_string()),
                );
            }
        }
        let child_module = if is_scope {
            format!("{module}::{local}")
        } else {
            module.into()
        };
        for child in &node.children {
            expected(child, root, &child_module, &typed, levels, methods);
        }
    }

    fn generic_type<'a>(ty: &'a syn::Type, name: &str) -> Option<&'a syn::Type> {
        let syn::Type::Path(path) = ty else {
            return None;
        };
        let segment = path.path.segments.last()?;
        if segment.ident != name {
            return None;
        }
        let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
            return None;
        };
        match args.args.first()? {
            syn::GenericArgument::Type(value) if args.args.len() == 1 => Some(value),
            _ => None,
        }
    }

    let mut parsed = crate::parse::parse(description);
    let client = parsed.client.as_ref().unwrap();
    let flat = crate::resolve::resolve(client, &mut parsed.diagnostics);
    assert!(parsed.diagnostics.is_empty());
    let root = client.node.name.to_string().to_ascii_lowercase();
    let mut methods = BTreeMap::new();
    expected(&client.node, &root, &root, "", &flat.levels, &mut methods);
    for (module, block) in super::generation::impls(file) {
        if block.trait_.is_some() {
            continue;
        }
        let owner = canonical(&block.self_ty);
        let owner = if module.is_empty() {
            owner
        } else {
            format!("{module}::{owner}")
        };
        for item in &block.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            let Some(inputs) = methods.remove(&(owner.clone(), method.sig.ident.to_string()))
            else {
                continue;
            };
            assert!(
                matches!(method.vis, syn::Visibility::Public(_)),
                "{owner}::{}",
                method.sig.ident
            );
            for (name, (optional, expected)) in inputs {
                let actual = method.sig.inputs.iter().find_map(|arg| match arg {
                    syn::FnArg::Typed(arg) if matches!(&*arg.pat, syn::Pat::Ident(binding) if binding.ident == name.as_str()) => Some(&*arg.ty),
                    _ => None,
                }).and_then(|ty| {
                    let syn::Type::ImplTrait(ty) = ty else { return None };
                    let syn::TypeParamBound::Trait(bound) = ty.bounds.first()? else { return None };
                    let plain = syn::Type::Path(syn::TypePath { qself: None, path: bound.path.clone() });
                    let arg = generic_type(&plain, "Arg")?;
                    let authored = if optional { generic_type(arg, "Option")? } else { arg };
                    Some(authored.to_token_stream().to_string())
                });
                if actual.as_deref() != Some(&expected) {
                    violations.push(format!(
                        "{owner}::{} argument {name}: expected authored {expected}, got {actual:?}",
                        method.sig.ident
                    ));
                }
            }
        }
    }
    for ((owner, method), _) in methods {
        violations.push(format!("missing authored signature {owner}::{method}"));
    }
}

pub(super) fn rendered_signature(item: &str) -> String {
    let (_, declaration) = item.split_once("<h4 class=\"code-header\">").unwrap();
    let (declaration, _) = declaration.split_once("</h4>").unwrap();
    signature_text(declaration)
}

pub(super) fn signature_text(declaration: &str) -> String {
    let mut label = String::new();
    let mut tag = false;
    for ch in declaration.chars() {
        match ch {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => label.push(ch),
            _ => {}
        }
    }
    label
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn rustdoc_output(probe: &str, source_text: &str) -> std::path::PathBuf {
    use std::{fs, process::Command};
    let work = package().join("../target/call-surface-doc").join(probe);
    fs::create_dir_all(&work).unwrap();
    let source = work.join("surface.rs");
    fs::write(&source, source_text).unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let concord = fs::read_dir(&deps)
        .unwrap()
        .map(|file| file.unwrap())
        .filter(|file| {
            file.file_name()
                .to_string_lossy()
                .starts_with("libconcord-")
                && file.path().extension().is_some_and(|ext| ext == "rlib")
        })
        .max_by_key(|file| file.metadata().unwrap().modified().unwrap())
        .unwrap()
        .path();
    let output = Command::new("rustdoc")
        .args(["--edition=2021", "--crate-name", probe, "-L"])
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("concord={}", concord.display()))
        .arg("-o")
        .arg(&work)
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    work.join(probe)
}

fn documented_call_signatures() -> (String, bool, String) {
    let output = rustdoc_output(
        "surface_probe",
        r#"concord::api! {
            pub client Riot {
                base "https://types.test"; read concord::Json; problem String;
                GET get();
            }
        }"#,
    );
    let page = std::fs::read_to_string(output.join("riot/struct.Call.html")).unwrap();
    let (before, item) = page
        .split_once("id=\"associatedtype.Output\"")
        .expect("missing IntoFuture::Output");
    let (_, future) = before
        .rsplit_once("id=\"impl-IntoFuture-for-Call")
        .expect("missing IntoFuture impl");
    let alias_prose = future.contains("Awaiting an unpaged call gives")
        && future.contains("<code>riot::Result&lt;E::Value&gt;</code>")
        && future.contains("href=\"type.Result.html\"");
    let (_, retry) = page.split_once("id=\"method.with_retry\"").unwrap();
    (
        rendered_signature(item),
        alias_prose,
        rendered_signature(retry),
    )
}

fn collect(files: Vec<syn::File>, call_source: &str) -> BTreeMap<String, String> {
    let mut surface = Surface {
        mode_traits: core_mode_traits(),
        call_source: call_source.to_owned(),
        ..Surface::default()
    };
    for file in files {
        surface.visit_file(&file);
    }
    for (endpoint, rules) in std::mem::take(&mut surface.rules) {
        surface.add(format!("{endpoint}::capabilities"), format!("{rules:?}"));
    }
    // Compare the retry argument's type through the client's public alias.
    // The rendered signature is checked separately for its spelling.
    for root in ["tasks", "riot", "files", "feed", "shapes"] {
        let retry = format!("{root}::Call<E>::with_retry");
        if surface.lines.get(&retry).map(String::as_str)
            == Some("fnwith_retry(self,retry:Retry)->Self")
        {
            let target = surface.lines[&format!("{root}::Retry")]
                .strip_prefix("pubtypeRetry=")
                .unwrap()
                .strip_suffix(';')
                .unwrap();
            surface
                .lines
                .insert(retry, format!("fnwith_retry(self,retry:{target})->Self"));
        }
    }
    surface.lines
}

fn generated_surface(violations: &mut Vec<String>) -> BTreeMap<String, String> {
    collect(
        ["tasks", "riot", "files", "feed", "shapes"]
            .iter()
            .map(|name| {
                let source = std::fs::read_to_string(
                    package().join(format!("../tests/tour-gen/src/{name}.rs")),
                )
                .unwrap();
                let authored = description(&source);
                let file: syn::File = syn::parse2(crate::expand(authored.clone())).unwrap();
                check_authored_arguments(authored, &file, violations);
                check_impl_gates(&file, true, violations);
                file
            })
            .collect(),
        "",
    )
}

/// The hand-written client's files, by name, and its shared `call.rs`.
fn hand_written_sources() -> (Vec<(&'static str, String)>, String) {
    let files = [
        "declarations",
        "tasks",
        "riot",
        "files",
        "grant",
        "feed",
        "shapes",
    ]
    .map(|name| {
        let path = package().join(format!("../tests/tour/src/{name}.rs"));
        (name, std::fs::read_to_string(path).unwrap())
    });
    let call = std::fs::read_to_string(package().join("../tests/tour/src/call.rs")).unwrap();
    (files.into(), call)
}

fn hand_written_surface(files: &[(&str, String)], call: &str) -> BTreeMap<String, String> {
    collect(
        files
            .iter()
            .map(|(_, source)| syn::parse_file(source).unwrap())
            .collect(),
        call,
    )
}

fn differences(
    hand_written: &BTreeMap<String, String>,
    generated: &BTreeMap<String, String>,
) -> Vec<String> {
    hand_written
        .keys()
        .chain(generated.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| hand_written.get(*key) != generated.get(*key))
        .map(|key| {
            format!(
                "{key}\nhand-written client: {:?}\nmacro: {:?}",
                hand_written.get(key),
                generated.get(key)
            )
        })
        .collect()
}

/// What is compared, item by item: the public items of both clients and their
/// signatures; the documentation and the other attributes of the public items of each
/// `Endpoint` trait; each endpoint's `Value` and `NAME`; whole `Unpin` headers; `Clone` and
/// `Debug`; the capabilities of each call. What is not compared: method bodies, struct
/// fields, the documentation of items outside the `Endpoint` trait, and the impls of other
/// traits beyond those named here.
#[test]
fn c1_16_public_surface_matches_the_hand_written_client() {
    let mut violations = Vec::new();
    let generated = generated_surface(&mut violations);
    let (files, call) = hand_written_sources();
    let hand_written_calls = syn::parse_file(&call).unwrap();
    check_impl_gates(&hand_written_calls, false, &mut violations);
    let (output, alias_prose, retry) = documented_call_signatures();
    let expected = "type Output = Result<<E as Endpoint>::Value, Error<String>>";
    if output != expected {
        violations.push(format!(
            "IntoFuture output should be `{expected}`, rustdoc shows `{output}`"
        ));
    }
    if !alias_prose {
        violations.push("IntoFuture must document and link riot::Result<E::Value>".into());
    }
    if retry != "pub fn with_retry(self, retry: Retry<String>) -> Self" {
        violations.push(format!(
            "with_retry must show the authored problem type: {retry}"
        ));
    }
    violations.extend(differences(
        &hand_written_surface(&files, &call),
        &generated,
    ));
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

/// Each drift of the hand-written client from what the macro writes is a difference:
/// an `Unpin` taken away, an endpoint's name changed, an attribute added to `NAME`.
#[test]
fn c1_16_surface_comparison_reports_each_drift_of_the_hand_written_client() {
    let generated = generated_surface(&mut Vec::new());
    let (files, call) = hand_written_sources();
    let drifted = |file: &str, old: &str, new: &str| {
        let mut files = files.clone();
        let mut call = call.clone();
        let source = if file == "call" {
            &mut call
        } else {
            &mut files.iter_mut().find(|(name, _)| *name == file).unwrap().1
        };
        assert_eq!(source.matches(old).count(), 1, "{old}");
        *source = source.replacen(old, new, 1);
        differences(&hand_written_surface(&files, &call), &generated)
    };
    assert!(differences(&hand_written_surface(&files, &call), &generated).is_empty());
    for (file, old, new, key) in [
        (
            "declarations",
            "impl Unpin for Tasks {}",
            "",
            "Tasks::__trait::Unpin",
        ),
        (
            "declarations",
            "impl Unpin for Tasks {}",
            "impl Unpin for Tasks where Tasks: Send {}",
            "Tasks::__trait::Unpin",
        ),
        (
            "tasks",
            "const NAME: &'static str = \"tasks.create\";",
            "const NAME: &'static str = \"tasks.drift\";",
            "tasks::Create::__trait::Endpoint::NAME",
        ),
        (
            "tasks",
            "const NAME: &'static str = \"tasks.create\";",
            "const NAME: &'static str = \"tasks.cre ate\";",
            "tasks::Create::__trait::Endpoint::NAME",
        ),
        (
            "tasks",
            "const NAME: &'static str = \"tasks.create\";",
            "const NAME: &'static str = \"tasks.concord::reqwest::create\";",
            "tasks::Create::__trait::Endpoint::NAME",
        ),
        (
            "declarations",
            "impl Unpin for Tasks {}",
            "impl !Unpin for Tasks {}",
            "Tasks::__trait::Unpin",
        ),
        (
            "call",
            "    /// The value of a successful send. For a paged endpoint, this is one page.",
            "    /// The value of a send.",
            "tasks::Endpoint::Value",
        ),
        (
            "call",
            "    const NAME: &'static str;",
            "    /// More.\n    #[deprecated]\n    const NAME: &'static str;",
            "tasks::Endpoint::NAME",
        ),
    ] {
        let found = drifted(file, old, new);
        assert!(
            found
                .iter()
                .any(|difference| difference.starts_with(&format!("{key}\n"))),
            "{file}: `{new}` drifts unseen: {found:?}"
        );
    }
}
