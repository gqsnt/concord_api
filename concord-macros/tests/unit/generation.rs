//! Generated code, lexical bindings, and expansion growth checks.
use super::{description, package};
use quote::quote;
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::{self, Visit};

fn compact(tokens: impl quote::ToTokens) -> String {
    tokens.to_token_stream().to_string().replace(' ', "")
}

pub(super) fn impls(file: &syn::File) -> Vec<(String, &syn::ItemImpl)> {
    #[derive(Default)]
    struct Found<'a> {
        module: Vec<String>,
        blocks: Vec<(String, &'a syn::ItemImpl)>,
    }
    impl<'a> Visit<'a> for Found<'a> {
        fn visit_item_mod(&mut self, module: &'a syn::ItemMod) {
            self.module.push(module.ident.to_string());
            visit::visit_item_mod(self, module);
            self.module.pop();
        }

        fn visit_item_impl(&mut self, block: &'a syn::ItemImpl) {
            self.blocks.push((self.module.join("::"), block));
            visit::visit_item_impl(self, block);
        }
    }
    let mut found = Found::default();
    found.visit_file(file);
    found.blocks
}

pub(super) fn slot_key(block: &syn::ItemImpl) -> String {
    let (_, slot, _) = block.trait_.as_ref().unwrap();
    let syn::Type::Path(ty) = &*block.self_ty else {
        panic!("slot needs a type path")
    };
    let specialization = ty.path.segments.last().unwrap();
    assert_eq!(specialization.ident, "InputSlot");
    let syn::PathArguments::AngleBracketed(inputs) = &specialization.arguments else {
        panic!("slot must specialize on call, input position, and key");
    };
    let syn::PathArguments::AngleBracketed(slot_call) = &slot.segments.last().unwrap().arguments
    else {
        panic!("Slot must name the call it serves");
    };
    assert_eq!(inputs.args.len(), 3);
    assert_eq!(compact(&inputs.args[0]), compact(&slot_call.args[0]));
    let Some(syn::GenericArgument::Type(syn::Type::Path(key))) = inputs.args.last() else {
        panic!("slot key must be a type path");
    };
    compact(key)
}

pub(super) fn struct_at<'a>(file: &'a syn::File, name: &str) -> &'a syn::ItemStruct {
    let mut items = file.items.as_slice();
    let mut segments = name.split("::").peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            return items
                .iter()
                .find_map(|item| match item {
                    syn::Item::Struct(item) if item.ident == segment => Some(item),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("struct {name} is missing"));
        }
        items = items
            .iter()
            .find_map(|item| match item {
                syn::Item::Mod(item) if item.ident == segment => {
                    Some(item.content.as_ref().unwrap().1.as_slice())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("module {segment} in {name} is missing"));
    }
    unreachable!("expected a nonempty path")
}

#[test]
fn c1_18_sound_siblings_compile_after_one_endpoint_is_refused() {
    let source = r#"concord::api! { pub client Three {
        base "https://three.test"; read concord::Json;
        GET first() -> u32;
        GET refused() { path "{missing}"; }
        GET last() -> u32;
    } }"#;
    let output = crate::expand(description(source));
    assert_eq!(output.to_string().matches("compile_error").count(), 1);
    let mut file: syn::File = syn::parse2(output).unwrap();
    file.items.retain(|item| !matches!(item, syn::Item::Macro(m) if m.mac.path.segments.last().is_some_and(|n|n.ident=="compile_error")));
    let text = quote!(#file).to_string();
    assert!(
        text.contains("fn first"),
        "the first sound endpoint disappeared"
    );
    assert!(
        text.contains("fn last"),
        "the last sound endpoint disappeared"
    );
    assert!(!text.contains("fn refused"));
    let api = package().parent().unwrap().to_path_buf();
    let target = api.join("target/diagnostic-recovery");
    std::fs::create_dir_all(&target).unwrap();
    let source = target.join("siblings.rs");
    std::fs::write(&source,format!("#![allow(dead_code)]\n{text}\npub async fn both() -> three::Result<()> {{ let c=Three::new(); let _=c.first().await?; let _=c.last().await?; Ok(()) }}\n")).unwrap();
    let executable = std::env::current_exe().unwrap();
    let deps = executable.parent().unwrap();
    let dependency = std::fs::read_dir(deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libconcord-")
                && p.extension().is_some_and(|x| x == "rlib")
        })
        .max_by_key(|p| p.metadata().unwrap().modified().unwrap())
        .unwrap();
    let result = std::process::Command::new("rustc")
        .args([
            "--edition=2021",
            "--crate-type=lib",
            "--emit=metadata",
            "--crate-name=diagnostic_siblings",
        ])
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("concord={}", dependency.display()))
        .arg("--out-dir")
        .arg(&target)
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn c1_18_a_refused_problem_type_does_not_reach_the_client_shell() {
    let source =
        "client Broken { base \"https://broken.test\"; problem Vec<Self> read Json; GET get(); }";
    let output = crate::expand(source.parse().unwrap()).to_string();
    assert_eq!(output.matches("compile_error").count(), 1);
    assert!(!output.contains("struct Broken"), "{output}");
}

#[test]
fn c1_12_slots_are_visible_only_to_the_parent() {
    let file: syn::File = syn::parse2(crate::expand(
        r#"pub client Paging {
        base "https://pages.test"; read Json;
        scope nested { GET get(start?: u32) -> Vec<u32> {
            query {start;} paginate by_offset(start);
        } }
    }"#
        .parse()
        .unwrap(),
    ))
    .unwrap();
    let slots: Vec<_> = impls(&file)
        .into_iter()
        .filter(|(_, block)| {
            block
                .trait_
                .as_ref()
                .is_some_and(|(_, path, _)| compact(path).starts_with("::concord::page::Slot<"))
        })
        .collect();
    assert_eq!(slots.len(), 1, "one specialization for the one paged input");
    let (module, slot) = &slots[0];
    let key = slot_key(slot);
    let key = if module.is_empty() {
        key
    } else {
        format!("{module}::{key}")
    };
    assert!(key.starts_with("paging::"), "{key}");
    assert_eq!(compact(&struct_at(&file, &key).vis), "pub(super)");
}

#[test]
fn c1_16_missing_problem_reader_is_an_internal_error_without_a_format_fallback() {
    let mut parsed=crate::parse::parse("client Broken { base \"https://broken.test\"; read CustomReader; problem u32; GET get(); }".parse().unwrap());
    let client = parsed.client.as_ref().unwrap();
    let mut flat = crate::resolve::resolve(client, &mut parsed.diagnostics);
    flat.endpoints[0].problem_reader = None;
    let output = crate::write::client(client, &flat).to_string();
    assert!(output.contains("internal Concord macro error"), "{output}");
    assert!(!output.contains("concord :: Json"));
}

#[test]
fn c1_16_internal_lookup_failures_are_reported() {
    let mut parsed = crate::parse::parse(
        "client Broken { base \"https://broken.test\"; GET get(n:u32) { query {n;} } }"
            .parse()
            .unwrap(),
    );
    let client = parsed.client.as_ref().unwrap();
    let mut flat = crate::resolve::resolve(client, &mut parsed.diagnostics);
    flat.levels.clear();
    let output = crate::write::client(client, &flat).to_string();
    assert!(output.contains("internal Concord macro error"), "{output}");
    assert!(!output.contains("__InputMissing"));
}

#[test]
fn c1_16_no_empty_impl_is_written() {
    let file: syn::File = syn::parse2(crate::expand(
        r#"pub client Minimal { base "https://minimal.test";
            scope empty {} GET get(); scope nested { POST post(); }
            #[cfg(not(any()))] scope gated {} #[deprecated] scope old {}
        }"#
        .parse()
        .unwrap(),
    ))
    .unwrap();
    let Some(syn::Item::Mod(client)) = file.items.iter().find(|item| {
        matches!(item,
            syn::Item::Mod(module) if module.ident == "minimal"
        )
    }) else {
        panic!("missing client module")
    };
    let items = &client.content.as_ref().unwrap().1;
    let endpoint = items
        .iter()
        .find_map(|item| match item {
            syn::Item::Trait(item) if item.ident == "Endpoint" => Some(item),
            _ => None,
        })
        .unwrap();
    let Some(syn::Item::Mod(seal)) = items.iter().find(|item| {
        matches!(item,
            syn::Item::Mod(module) if module.ident == "__sealed"
        )
    }) else {
        panic!("missing seal module")
    };
    assert!(matches!(seal.vis, syn::Visibility::Inherited));
    assert!(seal
        .content
        .as_ref()
        .unwrap()
        .1
        .iter()
        .any(|item| matches!(item,
            syn::Item::Trait(item) if item.ident == "Sealed"
        )));
    assert!(endpoint
        .supertraits
        .iter()
        .any(|bound| compact(bound) == "__sealed::Sealed"));

    let mut endpoint_types = BTreeSet::from(["Get".to_owned(), "nested::Post".to_owned()]);
    // `Unpin` is a marker: its implementations are empty by nature, one for the call and
    // one for the client and each handle (C10.2). Each is matched whole: its module, its
    // attributes, its generics and bounds, and no `where` clause.
    let mut unpinned = BTreeSet::from(
        [
            (
                "minimal",
                "impl<E:Endpoint>::core::marker::UnpinforCall<E>{}",
            ),
            ("", "impl::core::marker::UnpinforMinimal{}"),
            ("", "impl::core::marker::Unpinforminimal::Empty{}"),
            ("", "impl::core::marker::Unpinforminimal::Nested{}"),
            (
                "",
                "#[cfg(not(any()))]impl::core::marker::Unpinforminimal::Gated{}",
            ),
            (
                "",
                "#[allow(deprecated)]impl::core::marker::Unpinfor__Impl6{}",
            ),
        ]
        .map(|(module, block)| (module.to_owned(), block.to_owned())),
    );
    for (module, block) in impls(&file)
        .into_iter()
        .filter(|(_, block)| block.items.is_empty())
    {
        if let Some((_, path, _)) = &block.trait_ {
            if compact(path).ends_with("Unpin") {
                let whole = (module.clone(), compact(block));
                assert!(unpinned.remove(&whole), "unexpected {whole:?}");
                continue;
            }
        }
        assert_eq!(module, "minimal", "{}", quote!(#block));
        let (_, path, _) = block.trait_.as_ref().expect("empty inherent impl");
        assert_eq!(compact(path), "__sealed::Sealed");
        assert!(
            endpoint_types.remove(&compact(&block.self_ty)),
            "{}",
            quote!(#block)
        );
    }
    assert!(
        endpoint_types.is_empty(),
        "missing seal for {endpoint_types:?}"
    );
    assert!(unpinned.is_empty(), "missing Unpin for {unpinned:?}");
}

#[test]
fn c1_16_growth_binds_inputs_once_and_emits_linear_sized_clients() {
    let wide = |count, authored| {
        let inputs = (0..count)
            .map(|i| format!("input_{i}:u32"))
            .collect::<Vec<_>>()
            .join(",");
        let entries: String = (0..count)
            .map(|i| {
                if authored && i == 0 {
                    r#""input_0" = input_0.saturating_add(1);"#.into()
                } else {
                    format!("input_{i};")
                }
            })
            .collect();
        crate::expand(
            format!(
                r#"client Wide {{ base "https://wide.test";
            GET get({inputs}) {{ query {{{entries}}} }} }}"#
            )
            .parse()
            .unwrap(),
        )
    };

    #[derive(Default)]
    struct Inputs {
        bindings: BTreeMap<String, usize>,
        fields: BTreeMap<String, Vec<u32>>,
        authored_expressions: usize,
    }

    impl<'ast> Visit<'ast> for Inputs {
        fn visit_local(&mut self, local: &'ast syn::Local) {
            if let syn::Pat::Ident(binding) = &local.pat {
                if binding.ident.to_string().starts_with("input_") {
                    *self.bindings.entry(binding.ident.to_string()).or_default() += 1;
                }
            }
            visit::visit_local(self, local);
        }

        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            if call.method == "saturating_add"
                && matches!(&*call.receiver, syn::Expr::Path(path) if path.path.is_ident("input_0"))
            {
                self.authored_expressions += 1;
            }
            // The method spelling is counted too: a second write of one pair, in either
            // spelling, shows as a repeated field.
            if call.method == "query_value" {
                self.query_value(&call.args[0], &call.args[2]);
            }
            visit::visit_expr_method_call(self, call);
        }

        // A query pair is written by `Draft::query_value(&mut draft, key, name, value)`, a
        // call by its full path so that no trait of the author's can take it.
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            let draft_query = matches!(&*call.func, syn::Expr::Path(path)
                if path.path.segments.last().is_some_and(|s| s.ident == "query_value"));
            if draft_query {
                self.query_value(&call.args[1], &call.args[3]);
            }
            visit::visit_expr_call(self, call);
        }
    }

    impl Inputs {
        /// One pair: its key, and the inputs that its value reads.
        fn query_value(&mut self, key: &syn::Expr, value: &syn::Expr) {
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(key),
                ..
            }) = key
            else {
                panic!("shorthand must pass its literal query key");
            };
            struct Fields(Vec<u32>);
            impl<'ast> Visit<'ast> for Fields {
                fn visit_expr_field(&mut self, field: &'ast syn::ExprField) {
                    match &field.member {
                        syn::Member::Named(name) => {
                            if let Some(index) = name.to_string().strip_prefix("input_") {
                                self.0.push(index.parse().unwrap());
                            }
                        }
                        syn::Member::Unnamed(index) => {
                            if matches!(field.base.as_ref(), syn::Expr::Field(base)
                                if matches!(&base.member, syn::Member::Named(name) if name == "__inputs"))
                            {
                                self.0.push(index.index);
                            }
                        }
                    }
                    visit::visit_expr_field(self, field);
                }
            }
            let mut fields = Fields(Vec::new());
            fields.visit_expr(value);
            self.fields.entry(key.value()).or_default().extend(fields.0);
        }
    }

    for authored in [false, true] {
        let output = wide(200, authored);
        assert!(!output.to_string().contains("compile_error"));
        let mut inputs = Inputs::default();
        inputs.visit_file(&syn::parse2(output).unwrap());
        if authored {
            // The authored expression has all declared inputs in scope. Only
            // its referenced input is required here, and each local is bound once.
            assert_eq!(inputs.bindings.get("input_0"), Some(&1));
            assert!(inputs.bindings.values().all(|count| *count == 1));
        } else {
            assert!(inputs.bindings.is_empty(), "shorthand introduced bindings");
        }
        let first = u32::from(authored);
        assert_eq!(inputs.authored_expressions, first as usize);
        assert_eq!(inputs.fields.len(), 200);
        for index in first..200 {
            let key = format!("input_{index}");
            assert_eq!(
                inputs.fields.get(&key),
                Some(&vec![index]),
                "shorthand {key}"
            );
        }
    }
    assert!(wide(40, false).to_string().len() * 10 < wide(20, false).to_string().len() * 28);
    let many = |count| {
        let endpoints = (0..count)
            .map(|i| format!("GET e_{i}(n?:u32) {{query{{n;}}}}"))
            .collect::<String>();
        crate::expand(
            format!("client Many {{ base \"https://many.test\"; {endpoints} }}")
                .parse()
                .unwrap(),
        )
        .to_string()
        .len()
    };
    assert!(many(40) * 10 < many(20) * 28);
}

#[test]
fn c1_16_every_call_delegates_once_without_copying_common_methods() {
    const COMMON: &[&str] = &[
        "send",
        "send_full",
        "send_raw",
        "send_if_none_match",
        "with_retry",
        "with_timeout",
        "with_idle_timeout",
        "request",
        "into_request",
        "read_response",
        "edit_request",
        "pages",
        "__prepare_if_none_match",
        "__prepare_inner",
        "__build_request",
    ];
    fn calls(body: &syn::Block, target: &str) -> usize {
        struct Calls<'a> {
            target: &'a str,
            count: usize,
        }
        impl<'ast> Visit<'ast> for Calls<'_> {
            fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
                if matches!(&*call.func, syn::Expr::Path(path)
                    if path.path.segments.last().unwrap().ident == self.target)
                {
                    self.count += 1;
                }
                visit::visit_expr_call(self, call);
            }
            fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
                if call.method == self.target {
                    self.count += 1;
                }
                visit::visit_expr_method_call(self, call);
            }
        }
        let mut calls = Calls { target, count: 0 };
        calls.visit_block(body);
        calls.count
    }
    let source = r#"client Calls { base "https://calls.test"; read Json;
        GET get() -> u32; HEAD head() -> bool { status {2xx=>true} }
        POST post(); GET paged(start?:u32) -> Vec<u32> {query {start;} paginate by_offset(start);}
        METHOD "GET" custom_get(); METHOD "POST" custom_post();
    }"#;
    let file: syn::File = syn::parse2(crate::expand(source.parse().unwrap())).unwrap();
    let expected = ["Get", "Head", "Post", "Paged", "CustomGet", "CustomPost"];
    let factories = ["get", "head", "post", "paged", "custom_get", "custom_post"];
    let mut generated = BTreeMap::new();
    let mut endpoints = BTreeSet::new();
    let mut pager = BTreeSet::new();
    let mut found_factories = BTreeSet::new();

    for (module, block) in impls(&file) {
        let owner = compact(&block.self_ty);
        let trait_name = block.trait_.as_ref().map(|(_, path, _)| compact(path));
        let methods: Vec<_> = block
            .items
            .iter()
            .filter_map(|item| match item {
                syn::ImplItem::Fn(f) => Some(f),
                _ => None,
            })
            .collect();
        match trait_name.as_deref() {
            Some("calls::Endpoint") => {
                assert!(endpoints.insert(owner.clone()), "duplicate {owner}");
                for method in &methods {
                    assert!(
                        !COMMON.contains(&method.sig.ident.to_string().as_str()),
                        "{owner} copied common method {}",
                        method.sig.ident
                    );
                    assert_eq!(
                        calls(&method.block, "prepare_call"),
                        0,
                        "{owner} copied preparation"
                    );
                }
                for delegate in ["__core", "__build_draft", "__read_answer"] {
                    assert_eq!(
                        methods.iter().filter(|f| f.sig.ident == delegate).count(),
                        1,
                        "{owner}: {delegate}"
                    );
                }
            }
            Some("calls::__Pager") => {
                assert!(pager.insert(owner.clone()), "duplicate pager {owner}");
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0].sig.ident, "__pages");
                assert_eq!(calls(&methods[0].block, "pages_with_pager"), 1);
                assert_eq!(calls(&methods[0].block, "by_offset"), 1);
            }
            Some("::concord::__private::Call") => {
                let prepare = methods.iter().find(|f| f.sig.ident == "prepare").unwrap();
                assert_eq!(calls(&prepare.block, "__prepare_inner"), 1);
            }
            None if owner == "Calls" => {
                for method in methods {
                    if let Some(i) = factories.iter().position(|name| method.sig.ident == *name) {
                        assert!(found_factories.insert(i));
                        assert!(compact(&method.sig.output)
                            .contains(&format!("calls::Call<calls::{}>", expected[i])));
                        assert_eq!(calls(&method.block, "__new"), 1, "{}", factories[i]);
                        assert_eq!(calls(&method.block, "prepare_call"), 0, "{}", factories[i]);
                    }
                }
            }
            _ => {
                for method in methods {
                    let name = method.sig.ident.to_string();
                    if COMMON.contains(&name.as_str()) {
                        assert!(trait_name.is_none(), "unexpected copy of {name} on {owner}");
                        let generic = block
                            .generics
                            .type_params()
                            .next()
                            .unwrap()
                            .ident
                            .to_string();
                        let owner = if module.is_empty() {
                            owner.clone()
                        } else {
                            format!("{module}::{owner}")
                        };
                        assert_eq!(owner, format!("calls::Call<{generic}>"), "{name}");
                        assert!(
                            generated.insert(name, &method.block).is_none(),
                            "copied {}",
                            method.sig.ident
                        );
                    }
                }
            }
        }
    }
    assert_eq!(found_factories.len(), expected.len());
    assert_eq!(
        endpoints,
        expected.map(|name| format!("calls::{name}")).into()
    );
    assert_eq!(pager, BTreeSet::from(["calls::Paged".into()]));
    assert_eq!(generated.len(), COMMON.len());

    for (method, delegate) in [
        ("send", "send_full"),
        ("send_full", "send_full"),
        ("send_raw", "prepare"),
        ("send_raw", "send_raw"),
        ("send_if_none_match", "__prepare_if_none_match"),
        ("send_if_none_match", "send"),
        ("__prepare_if_none_match", "prepare_conditional"),
        ("__prepare_inner", "__settings"),
        ("__prepare_inner", "__build_draft"),
        ("__prepare_inner", "prepare_call"),
        ("__prepare_inner", "__read_answer"),
        ("request", "__build_request"),
        ("into_request", "__build_request"),
        ("__build_request", "__build_draft"),
        ("read_response", "__read_answer"),
        ("pages", "__pages"),
        ("edit_request", "edit_request"),
        ("with_retry", "with_retry"),
        ("with_timeout", "with_timeout"),
        ("with_idle_timeout", "with_idle_timeout"),
    ] {
        assert_eq!(
            calls(generated[method], delegate),
            1,
            "{method} -> {delegate}"
        );
    }
}
