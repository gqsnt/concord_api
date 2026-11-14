//! Generated code, lexical bindings, and expansion growth checks.
use super::{description, package};
use quote::quote;
use syn::visit::{self, Visit};

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
    let deps = api.join("target/debug/deps");
    let dependency = std::fs::read_dir(&deps)
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
    struct Slots(usize);
    impl<'ast> Visit<'ast> for Slots {
        fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
            if item.ident.to_string().starts_with("__Slot") {
                let vis = &item.vis;
                assert_eq!(quote!(#vis).to_string(), "pub (super)");
                self.0 += 1;
            }
        }
    }
    let file=syn::parse2(crate::expand("pub client Paging { base \"https://pages.test\"; read Json; GET get(start?: u32) -> Vec<u32> { query {start;} paginate by_offset(start); } }".parse().unwrap())).unwrap();
    let mut slots = Slots(0);
    slots.visit_file(&file);
    assert_eq!(slots.0, 1);
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
    struct Impls;
    impl<'ast> Visit<'ast> for Impls {
        fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
            assert!(!item.items.is_empty(), "{}", quote!(#item));
            visit::visit_item_impl(self, item);
        }
    }
    let file = syn::parse2(crate::expand(
        "client Minimal { base \"https://minimal.test\"; scope empty {} GET get(); }"
            .parse()
            .unwrap(),
    ))
    .unwrap();
    Impls.visit_file(&file);
}

#[test]
fn c1_16_growth_binds_inputs_once_and_emits_linear_sized_clients() {
    let wide = |count| {
        let inputs = (0..count)
            .map(|i| format!("input_{i}:u32"))
            .collect::<Vec<_>>()
            .join(",");
        let entries = (0..count)
            .map(|i| format!("input_{i};"))
            .collect::<String>();
        crate::expand(format!("client Wide {{ base \"https://wide.test\"; GET get({inputs}) {{ query {{{entries}}} }} }}").parse().unwrap()).to_string()
    };
    assert_eq!(wide(200).matches("let input_0 =").count(), 1);
    assert!(wide(40).len() * 10 < wide(20).len() * 28);
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
    struct Delegated {
        helpers: usize,
        pages: usize,
    }
    impl<'ast> Visit<'ast> for Delegated {
        fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
            if !matches!(&*item.self_ty,syn::Type::Path(p) if p.path.is_ident("Calls")) {
                visit::visit_item_impl(self, item);
            }
        }
        fn visit_macro(&mut self, mac: &'ast syn::Macro) {
            if mac
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "call_methods")
            {
                self.helpers += 1;
                self.pages += usize::from(mac.tokens.to_string().contains("pages {"));
            }
        }
        fn visit_impl_item_fn(&mut self, method: &'ast syn::ImplItemFn) {
            assert!(
                ![
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
                    "__prepare_if_none_match"
                ]
                .contains(&method.sig.ident.to_string().as_str()),
                "a common call method was copied: {}",
                method.sig.ident
            );
        }
    }
    let source = r#"client Calls { base "https://calls.test"; read Json;
        GET get() -> u32; HEAD head() -> bool { status {2xx=>true} }
        POST post(); GET paged(start?:u32) -> Vec<u32> {query {start;} paginate by_offset(start);}
        METHOD "GET" custom_get(); METHOD "POST" custom_post();
    }"#;
    let file = syn::parse2(crate::expand(source.parse().unwrap())).unwrap();
    let mut delegated = Delegated {
        helpers: 0,
        pages: 0,
    };
    delegated.visit_file(&file);
    assert_eq!(delegated.helpers, 6);
    assert_eq!(delegated.pages, 1);
}
