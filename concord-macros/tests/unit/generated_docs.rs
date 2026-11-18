//! Each rule in docs/internals/macro.md, "The generated documentation", is checked as a whole line.
use quote::ToTokens;
use std::collections::BTreeMap;
use syn::visit::{self, Visit};

const SOURCE: &str = r#"
/// The author's example.
/// ```ignore
/// use example::Docs;
/// ```
pub client Docs(token: String) {
    base "https://docs.test";
    read Json;
    headers { "x-key" = "HEADER_VALUE_MUST_NOT_LEAK"; }
    /// Signs every outgoing request.
    middleware Layer::new(token, "MIDDLEWARE_LITERAL_MUST_NOT_LEAK");
    /// PART_NAME_MUST_NOT_LEAK
    part pagination(#[doc = "Page size."] limit?: u32) {
        query { limit; }
        paginate by_page(limit, 1);
    }
    /// Fetch rows.
    GET items(id: u64, flag?: bool, ..pagination) -> Vec<Item> {
        path "items/{id}"; query { flag; }
        status { 404 => Vec::new() }
    }
    POST upload(body: Body) { path "upload"; body body; idempotent false; }
}
"#;
fn docs(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|a| {
            if a.path().is_ident("doc") {
                if let syn::Meta::NameValue(v) = &a.meta {
                    if let syn::Expr::Lit(v) = &v.value {
                        if let syn::Lit::Str(s) = &v.lit {
                            return Some(s.value().trim_start().into());
                        }
                    }
                }
            }
            None
        })
        .collect()
}
pub(super) fn hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("doc")
            && matches!(&a.meta, syn::Meta::List(l) if l.tokens.to_string() == "hidden")
    })
}

fn documented_method<'a>(file: &'a syn::File, owner: &str, name: &str) -> &'a syn::ImplItemFn {
    let matching: Vec<_> = super::generation::impls(file)
        .into_iter()
        .filter(|(module, block)| {
            let receiver = block.self_ty.to_token_stream().to_string().replace(' ', "");
            let path = if module.is_empty() {
                receiver
            } else {
                format!("{module}::{receiver}")
            };
            block.trait_.is_none() && path == owner && !hidden(&block.attrs)
        })
        .flat_map(|(_, block)| &block.items)
        .filter_map(|item| match item {
            syn::ImplItem::Fn(method)
                if method.sig.ident == name
                    && matches!(method.vis, syn::Visibility::Public(_))
                    && !hidden(&method.attrs) =>
            {
                Some(method)
            }
            _ => None,
        })
        .collect();
    assert_eq!(matching.len(), 1, "expected one visible {owner}::{name}");
    matching[0]
}

fn method_docs(file: &syn::File, owner: &str, name: &str) -> Vec<String> {
    docs(&documented_method(file, owner, name).attrs)
}

fn displayed_line(line: &str) -> String {
    // The endpoint type and factory use different paths for the same intra-doc link.
    // Keep their entire visible wording and discard only explicit destinations.
    let mut rest = line;
    let mut displayed = String::new();
    while let Some((label, target)) = rest.split_once("](") {
        displayed.push_str(label);
        displayed.push(']');
        rest = target.split_once(')').unwrap().1;
    }
    displayed.push_str(rest);
    displayed
}

fn tree(source: &str) -> syn::File {
    syn::parse2(crate::expand(source.parse().unwrap())).unwrap()
}

#[test]
fn c8_8_each_factory_and_setter_declares_its_unused_call_message() {
    let file = tree(
        r#"pub client Api {
            base "https://api.test"; read Json;
            POST submit(label?: String) { query { label; } }
            GET history(cursor?: u32) -> Vec<u32> {
                query { cursor; }
                paginate by_offset(cursor);
            }
        }"#,
    );
    let mut expected = BTreeMap::new();
    for (factory, endpoint, setter, message) in [
        (
            "submit",
            "Submit",
            "label",
            "a call does nothing until sent or awaited",
        ),
        (
            "history",
            "History",
            "cursor",
            "await `.send()` for one page, or use `.pages()` for all pages",
        ),
    ] {
        expected.insert(("Api".to_owned(), factory.to_owned()), message);
        expected.insert((format!("api::{endpoint}"), setter.to_owned()), message);
        expected.insert(
            (format!("api::Call<api::{endpoint}>"), setter.to_owned()),
            message,
        );
    }
    for (module, block) in super::generation::impls(&file) {
        if block.trait_.is_some() {
            continue;
        }
        let mut owner = block.self_ty.to_token_stream().to_string().replace(' ', "");
        if !module.is_empty() {
            owner = format!("{module}::{owner}");
        }
        for item in &block.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            let key = (owner.clone(), method.sig.ident.to_string());
            if let Some(message) = expected.remove(&key) {
                let expected_attribute: syn::Meta = syn::parse_quote!(must_use = #message);
                let attributes: Vec<_> = method
                    .attrs
                    .iter()
                    .filter(|attribute| attribute.path().is_ident("must_use"))
                    .map(|attribute| &attribute.meta)
                    .collect();
                assert_eq!(attributes, [&expected_attribute], "{}::{}", key.0, key.1);
            }
        }
    }
    assert!(
        expected.is_empty(),
        "missing factory or setter: {expected:?}"
    );
}

#[test]
fn c1_12_a_deprecated_endpoint_marks_each_setter() {
    let file = tree(
        r#"pub client Api {
            base "https://api.test";
            #[deprecated(note = "use current")]
            GET legacy(limit?: u32) { query { limit; } }
        }"#,
    );
    let deprecation: syn::Meta = syn::parse_quote!(deprecated(note = "use current"));
    for (form, takes_self) in [("associated", false), ("fluent", true)] {
        let setter = super::generation::impls(&file)
            .into_iter()
            .filter(|(_, block)| block.trait_.is_none())
            .flat_map(|(_, block)| &block.items)
            .find_map(|item| match item {
                syn::ImplItem::Fn(method)
                    if method.sig.ident == "limit"
                        && method.sig.receiver().is_some() == takes_self
                        && matches!(method.vis, syn::Visibility::Public(_)) =>
                {
                    Some(method)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {form} setter"));
        assert!(
            setter.attrs.iter().any(|attr| attr.meta == deprecation),
            "the {form} setter must carry the endpoint's deprecation"
        );
    }
}

#[derive(Default)]
struct Found {
    methods: BTreeMap<String, Vec<String>>,
    endpoints: BTreeMap<String, Vec<String>>,
    all: Vec<String>,
    signatures: Vec<String>,
}
impl<'ast> Visit<'ast> for Found {
    fn visit_item_enum(&mut self, endpoint: &'ast syn::ItemEnum) {
        if matches!(endpoint.vis, syn::Visibility::Public(_)) {
            self.endpoints
                .insert(endpoint.ident.to_string(), docs(&endpoint.attrs));
        }
        visit::visit_item_enum(self, endpoint);
    }
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        self.all.extend(docs(std::slice::from_ref(attr)));
    }
    fn visit_impl_item_fn(&mut self, method: &'ast syn::ImplItemFn) {
        if matches!(method.vis, syn::Visibility::Public(_)) {
            self.signatures
                .push(method.sig.to_token_stream().to_string());
            let lines = docs(&method.attrs);
            if !lines.is_empty() {
                self.methods.insert(method.sig.ident.to_string(), lines);
            }
        }
        visit::visit_impl_item_fn(self, method);
    }
}
fn found(source: &str) -> Found {
    let mut f = Found::default();
    f.visit_file(&tree(source));
    f
}

#[test]
fn c10_4_doc_rule_1_endpoint_lines_have_the_required_order_and_no_values() {
    let f = found(SOURCE);
    let lines = &f.methods["items"];
    let expected = [
        "Fetch rows.",
        "Call: `docs.items(id).send().await?`.",
        "Request: `GET https://docs.test/items/{id} ?limit&flag`.",
        "Headers: `x-key`.",
        "Inputs:",
        "Options:",
        "Status: `404`.",
        "`.send().await` returns [`docs::Result`]<[`Vec`]<[`Item`]>>.",
    ];
    let positions: Vec<_> = expected
        .iter()
        .map(|line| {
            lines
                .iter()
                .position(|l| l == line)
                .unwrap_or_else(|| panic!("missing whole line {line}\n{lines:?}"))
        })
        .collect();
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
    assert!(f.methods["upload"].contains(
        &"A retry does not repeat this endpoint unless `Retry::not_applied_on` lists the status."
            .into()
    ));
    assert!(!f.all.join("\n").contains("HEADER_VALUE_MUST_NOT_LEAK"));
    let f = found("client Api { base \"https://repeat.test\"; POST once(); POST safe() { idempotent; } GET get(); }");
    assert!(f.methods["once"].contains(
        &"A retry does not repeat this endpoint unless `Retry::not_applied_on` lists the status."
            .into()
    ));
    assert!(!f.methods["safe"].contains(
        &"A retry does not repeat this endpoint unless `Retry::not_applied_on` lists the status."
            .into()
    ));
    assert!(!f.methods["get"].contains(
        &"A retry does not repeat this endpoint unless `Retry::not_applied_on` lists the status."
            .into()
    ));
}
#[test]
fn c10_4_doc_rule_2_links_keep_caller_paths_and_disambiguate_data() {
    let f=found("pub client Api { base \"https://types.test\"; read Json; scope account { GET get() -> Account; } }");
    assert!(
        f.methods["get"].contains(&"`.await` returns [`api::Result`]<[`self::Account`]>.".into())
    );
    assert!(!f.all.join("\n").contains("crate::api::Result"));
    let f = found(SOURCE);
    assert!(f.methods["items"]
        .contains(&"`.send().await` returns [`docs::Result`]<[`Vec`]<[`Item`]>>.".into()));
}
#[test]
fn c10_4_doc_rule_3_no_problem_shows_no_unit_problem_parameter() {
    let f = found(SOURCE);
    assert!(f
        .all
        .contains(&"The error of each call: [`concord::Error`].".into()));
    assert!(f
        .all
        .contains(&"The retry policy for this client: [`concord::Retry`].".into()));
    assert!(!f
        .all
        .iter()
        .any(|line| line.contains("<()>") || line.contains(", ()>")));
    let file = f.signatures.join("\n");
    assert!(
        file.contains("fn send (self) -> :: concord :: Sending < E :: Value >"),
        "{file}"
    );
    let f = found("client Empty { base \"https://empty.test\"; read Json; problem Problem; }");
    assert!(f.all.contains(
        &"The error of each call: [`concord::Error`]<[`Problem`](super::Problem)>.".into()
    ));
}
#[test]
fn c10_4_doc_rule_4_setters_preserve_spelling_and_state_accepted_values() {
    let docs_api = tree(SOURCE);
    let api = tree(
        "pub client Api { base \"https://types.test\"; GET get(text?: String) { query { text; } } }",
    );
    let mut violations = Vec::new();
    for (file, owner, name, expected) in [
        (&docs_api, "docs::Items", "limit", vec![
            "Option `limit` of `items`.",
            "The first argument is the call.",
            "Page size.",
            "Accepts a [`u32`], a &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it.",
        ]),
        (&api, "api::Get", "text", vec![
            "Option `text` of `get`.",
            "The first argument is the call.",
            "Accepts a [`String`], a &[`String`], or an [`Option`]<[`String`]>. `None` clears it. Text and list conversions: [`concord::arg::Arg`].",
        ]),
    ] {
        let method = documented_method(file, owner, name);
        if !matches!(method.sig.inputs.first(), Some(syn::FnArg::Typed(first))
            if matches!(&*first.pat, syn::Pat::Ident(binding) if binding.ident == "__call"))
        {
            violations.push(format!("{owner}::{name} first parameter must be __call"));
        }
        let lines = docs(&method.attrs);
        let paragraphs: Vec<_> = lines.split(|line| line.is_empty())
            .filter(|paragraph| !paragraph.is_empty())
            .map(|paragraph| paragraph.iter().map(|line| displayed_line(line)).collect::<Vec<_>>().join("\n"))
            .collect();
        assert_eq!(paragraphs, expected, "{owner}::{name}");
        let (_, endpoint) = owner.rsplit_once("::").unwrap();
        let mut pages = Found::default();
        pages.visit_file(file);
        let page = &pages.endpoints[endpoint];
        let options_at = page.iter().position(|line| line == "Options:").unwrap();
        let options = &page[options_at + 1..];
        let first_option = options.iter().find_map(|line| {
            line.strip_prefix("[`")?.split_once('(').map(|(name, _)| name)
        }).unwrap();
        let intro = format!(
            "Each option is a method of the call (`call.{first_option}(v)`) and a function of this type (`{endpoint}::{first_option}(call, v)`)."
        );
        assert_eq!(options.iter().find(|line| !line.is_empty()), Some(&intro));
        assert!(!page.iter().any(|line| line == "The first argument is the call."));
        if name == "text" {
            assert!(lines.iter().any(|line| line.contains(
                "[`concord::arg::Arg`](mod@concord::arg)"
            )));
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn c10_4_doc_rule_5_only_the_authors_example_is_copied_to_the_client() {
    let file = tree(SOURCE);
    let client = file
        .items
        .iter()
        .find_map(|i| {
            if let syn::Item::Struct(s) = i {
                (s.ident == "Docs").then_some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        &docs(&client.attrs)[..4],
        [
            "The author's example.",
            "```ignore",
            "use example::Docs;",
            "```"
        ]
    );
    assert_eq!(
        found(SOURCE)
            .all
            .iter()
            .filter(|line| line.starts_with("```"))
            .count(),
        2
    );
}
#[test]
fn c10_4_doc_rule_6_internal_traits_slots_and_impls_cannot_leak() {
    use super::generation::{impls, slot_key, struct_at};
    use super::surface::{rendered_signature, rustdoc_output, signature_text};
    use std::collections::BTreeSet;
    use syn::visit_mut::{self, VisitMut};

    fn check_signature(tokens: impl ToTokens, violations: &mut Vec<String>) {
        fn private(tokens: proc_macro2::TokenStream) -> bool {
            tokens.into_iter().any(|token| match token {
                proc_macro2::TokenTree::Ident(name) => name.to_string().starts_with("__"),
                proc_macro2::TokenTree::Group(group) => private(group.stream()),
                _ => false,
            })
        }
        let tokens = tokens.to_token_stream();
        if private(tokens.clone()) {
            violations.push(tokens.to_string());
        }
    }

    #[derive(Default)]
    struct Visible {
        types: usize,
        violations: Vec<String>,
    }
    impl<'ast> Visit<'ast> for Visible {
        fn visit_item_trait(&mut self, tr: &'ast syn::ItemTrait) {
            if tr.ident == "__Types" {
                assert!(hidden(&tr.attrs));
                self.types += 1;
            }
            if tr.ident == "Endpoint" {
                for item in &tr.items {
                    if matches!(item, syn::TraitItem::Type(ty) if !hidden(&ty.attrs))
                        || matches!(item, syn::TraitItem::Fn(method) if !hidden(&method.attrs))
                        || matches!(item, syn::TraitItem::Const(c) if !hidden(&c.attrs))
                    {
                        check_signature(item, &mut self.violations);
                    }
                }
            }
        }
        fn visit_item_type(&mut self, ty: &'ast syn::ItemType) {
            if matches!(ty.vis, syn::Visibility::Public(_))
                && !hidden(&ty.attrs)
                && ty.ident != "Error"
                && ty.ident != "Retry"
            {
                check_signature(ty, &mut self.violations);
            }
        }
    }

    struct ProblemAlias;
    impl VisitMut for ProblemAlias {
        fn visit_type_mut(&mut self, ty: &mut syn::Type) {
            if matches!(ty, syn::Type::Path(path)
                if path.qself.is_none() && path.path.is_ident("__DeclaredProblem"))
            {
                *ty = syn::parse_quote!(declared_problem!());
            } else {
                visit_mut::visit_type_mut(self, ty);
            }
        }
    }

    let mut violations = Vec::new();
    for (problem, source) in [
        ("none", SOURCE.into()),
        (
            "named",
            SOURCE.replace("read Json;", "read Json; problem Problem;"),
        ),
        (
            "macro",
            SOURCE.replace("read Json;", "read Json; problem declared_problem!();"),
        ),
    ] {
        let file = tree(&source);
        let mut visible = Visible::default();
        visible.visit_file(&file);
        assert_eq!(visible.types, 1, "one doc-hidden type trait per client");

        let mut engine = BTreeSet::new();
        let mut slots = 0;
        for (module, block) in impls(&file) {
            let receiver = block.self_ty.to_token_stream().to_string().replace(' ', "");
            let trait_path = block
                .trait_
                .as_ref()
                .map(|(_, path, _)| path.to_token_stream().to_string().replace(' ', ""));
            match trait_path.as_deref() {
                Some(
                    "::concord::__private::Call"
                    | "::concord::__private::ConditionalCall"
                    | "::concord::page::Call",
                ) => {
                    assert_eq!(module, "docs");
                    assert_eq!(receiver, "Call<E>");
                    assert!(hidden(&block.attrs));
                    assert!(engine.insert(trait_path.unwrap()));
                }
                Some(path) if path.starts_with("::concord::page::Slot<") => {
                    assert!(hidden(&block.attrs));
                    assert!(hidden(&struct_at(&file, &slot_key(block)).attrs));
                    slots += 1;
                }
                None => {
                    let shared_call = block
                        .generics
                        .type_params()
                        .any(|parameter| receiver.ends_with(&format!("Call<{}>", parameter.ident)));
                    for item in &block.items {
                        let syn::ImplItem::Fn(method) = item else {
                            continue;
                        };
                        if !matches!(method.vis, syn::Visibility::Public(_))
                            || hidden(&method.attrs)
                        {
                            continue;
                        }
                        let mut sig = method.sig.clone();
                        // Shared Call methods document their parameter names.
                        // Other signatures are checked for private type names.
                        if shared_call {
                            check_signature(&block.generics, &mut visible.violations);
                            if problem == "macro" {
                                // Rustdoc resolves this local alias. Check the
                                // actual signatures on the rendered page below.
                                ProblemAlias.visit_signature_mut(&mut sig);
                            }
                        } else {
                            for arg in &mut sig.inputs {
                                if let syn::FnArg::Typed(arg) = arg {
                                    *arg.pat = syn::parse_quote!(_);
                                }
                            }
                        }
                        check_signature(sig, &mut visible.violations);
                    }
                }
                _ => {}
            }
        }
        assert_eq!(
            engine,
            BTreeSet::from([
                "::concord::__private::Call".into(),
                "::concord::__private::ConditionalCall".into(),
                "::concord::page::Call".into(),
            ])
        );
        assert_eq!(slots, 3, "id and both optional inputs are slots");
        violations.extend(
            visible
                .violations
                .into_iter()
                .map(|sig| format!("Docs ({problem} problem): {sig}")),
        );
    }
    let arg = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../concord-core/src/arg.rs"),
    )
    .unwrap();
    assert!(arg.lines().any(|line| line == "mod contract {"));
    assert!(arg.contains("pub use contract::Arg;"));
    assert!(arg.contains("#[doc(hidden)]\n        impl<$($g)*> Arg"));
    assert!(
        violations.is_empty(),
        "private names in visible signatures:\n{}",
        violations.join("\n")
    );

    let rendered_docs = rustdoc_output(
        "declared_problem_doc",
        r#"
        pub type E = String;
        macro_rules! declared_problem { () => { E }; }
        concord::api! {
            pub client Docs {
                base "https://docs.test";
                read concord::Json;
                problem declared_problem!();
                GET get();
                GET items(cursor?: u32) -> Vec<u32> {
                    query { cursor; }
                    paginate concord::by_offset(cursor);
                }
            }
        }"#,
    );
    let page = std::fs::read_to_string(rendered_docs.join("docs/struct.Call.html")).unwrap();
    let endpoint_page =
        std::fs::read_to_string(rendered_docs.join("docs/enum.Items.html")).unwrap();
    for (owner, rendered) in [("Call", &page), ("Items", &endpoint_page)] {
        let mut signatures = Vec::new();
        for (opening, closing) in [
            ("<pre class=\"rust item-decl\">", "</pre>"),
            ("<h3 class=\"code-header\">", "</h3>"),
            ("<h4 class=\"code-header\">", "</h4>"),
        ] {
            signatures.extend(
                rendered
                    .split(opening)
                    .skip(1)
                    .map(|rest| signature_text(rest.split_once(closing).unwrap().0)),
            );
        }
        assert!(
            signatures.iter().any(|signature| {
                signature.starts_with(if owner == "Call" {
                    "pub struct Call<"
                } else {
                    "pub enum Items"
                })
            }),
            "missing {owner} declaration"
        );
        if owner == "Items" {
            assert!(signatures
                .iter()
                .any(|signature| signature.starts_with("pub fn cursor(")));
        }
        for signature in signatures {
            let lexical = if owner == "Items"
                && signature
                    .strip_prefix("pub fn cursor(")
                    .is_some_and(|arguments| {
                        let first = arguments.trim_start();
                        first.starts_with("__call:") || first.starts_with("mut __call:")
                    }) {
                signature.replacen("__call:", "_:", 1)
            } else {
                signature.clone()
            };
            assert!(
                !lexical
                    .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                    .any(|name| name.starts_with("__"))
                    && !lexical
                        .split_whitespace()
                        .collect::<String>()
                        .contains("<Selfas"),
                "Docs::{owner}: {signature}"
            );
        }
    }
    for (method, expected) in [
        ("send", "pub fn send(self) -> Sending<E::Value, E>"),
        (
            "send_full",
            "pub fn send_full(self) -> Sending<Answer<E::Value>, E>",
        ),
        ("send_raw", "pub fn send_raw(self) -> Sending<Response, E>"),
        (
            "send_if_none_match",
            "pub fn send_if_none_match(self, etag: &str) -> Sending<Answer<Option<E::Value>>, E>",
        ),
        ("pages", "pub fn pages(self) -> Pages<E::Value, E>"),
        (
            "with_retry",
            "pub fn with_retry(self, retry: Retry<E>) -> Self",
        ),
    ] {
        let (_, item) = page.split_once(&format!("id=\"method.{method}\"")).unwrap();
        let rendered = rendered_signature(item);
        let compact = |signature: &str| {
            signature
                .trim_end_matches('ⓘ')
                .split_whitespace()
                .collect::<String>()
                .replace(",)", ")")
        };
        assert_eq!(
            compact(&rendered),
            compact(expected),
            "Docs::Call::{method}: {rendered}"
        );
    }
}
#[test]
fn c1_16_send_names_its_public_result() {
    let source = "pub client Api { base \"https://types.test\"; read Json; GET get() -> u32; }";
    let file = tree(source);
    let mut found = Found::default();
    found.visit_file(&file);
    let result_lines = |lines: &[String]| {
        lines
            .iter()
            .filter(|line| line.starts_with("`.await` returns"))
            .map(|line| displayed_line(line))
            .collect::<Vec<_>>()
    };
    let factory = method_docs(&file, "Api", "get");
    assert_eq!(
        result_lines(&factory),
        ["`.await` returns [`api::Result`]<[`u32`]>."]
    );
    assert_eq!(
        result_lines(&factory),
        result_lines(&found.endpoints["Get"])
    );
    assert_eq!(
        method_docs(&file, "api::Call<E>", "send"),
        [
            "Send the call. On a paged endpoint this returns one page.",
            "Preparation starts on the first poll; only the value is kept.",
            "",
            "# Panics",
            "",
            "Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.",
            "Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.",
        ]
    );
    assert_eq!(
        method_docs(&file, "api::Call<E>", "send_raw"),
        [
            "Send the call and get the [`reqwest::Response`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html) as it came.",
            "",
            "Concord passes a response with a status of 400 or above to the retry rules. It does not",
            "apply the endpoint's status rules or read the body. On the first poll, prepares exactly as for",
            "`send_full()`, then sends that preparation only as far as the head.",
            "",
            "# Panics",
            "",
            "Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.",
            "Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.",
        ]
    );
}

#[test]
fn c10_4_doc_rule_7_generated_prose_uses_caller_vocabulary() {
    let f = found("client Api { base \"https://types.test\"; GET get(); }");
    assert_eq!(
        f.methods["read_response"],
        [
            "Read a response that you obtained yourself using this endpoint's reader and status rules. No retry, call timeout, or idle timeout is started; pages are not advanced, and errors report zero attempts.",
        ]
    );
    assert!(!f
        .all
        .iter()
        .any(|l| l.contains("description") || l.contains("arrow")));
}
#[test]
fn c10_4_doc_rule_8_part_inputs_are_ordinary_options_without_part_identity() {
    let f = found(SOURCE);
    assert!(f.methods["items"].contains(
        &"`limit`: Accepts a [`u32`], a &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it."
            .into(),
    ));
    assert!(f.endpoints["Items"]
        .iter()
        .any(|line| line.contains("limit(impl Arg<Option<u32>>)")));
    assert!(!f.all.join("\n").contains("PART_NAME_MUST_NOT_LEAK"));
    assert!(!f.all.join("\n").contains("pagination"));
}
#[test]
fn c10_4_doc_rule_9_only_middleware_prose_reaches_the_client() {
    let f = found(SOURCE);
    assert!(f
        .all
        .contains(&"Every request goes through the client's middleware.".into()));
    assert!(f.all.contains(&"Signs every outgoing request.".into()));
    assert!(!f
        .all
        .join("\n")
        .contains("MIDDLEWARE_LITERAL_MUST_NOT_LEAK"));
    assert!(!f.all.join("\n").contains("Layer::new"));
}
#[test]
fn c10_4_c8_7_doc_rule_10_paged_calls_explain_pages_and_items() {
    let f = found(SOURCE);
    let line="`.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.";
    assert!(f.methods["items"].contains(&line.into()));
    assert_eq!(f.methods["pages"], [line]);
}

#[test]
fn c10_4_required_client_and_scope_input_prose_arrives_whole() {
    let f = found(
        r#"client Docs(#[doc="Client key prose."] key:String) {
        base "https://docs.test"; query {key;}
        scope things(#[doc="Scope route prose."] route:String) {path "{route}"; GET get();}
    }"#,
    );
    assert!(f.methods["new"].contains(&"Client key prose.".into()));
    assert!(f.methods["things"].contains(&"Scope route prose.".into()));
}
#[test]
fn c10_4_nested_options_explain_all_three_states() {
    let f = found(
        r#"client Docs {base "https://docs.test"; POST go(milestone?:Option<u64>) {body json {milestone;}} }"#,
    );
    let lines = &f.methods["milestone"];
    assert!(lines
        .iter()
        .any(|s| s.contains("an [`Option`]") && !s.contains("a [`Option`]")));
    assert!(lines
        .iter()
        .any(|s| s.contains("None::<u64>") && s.contains("null")));
    assert!(lines
        .iter()
        .any(|s| s.contains("None::<Option<u64>>") && s.contains("clears")));
    assert!(lines
        .iter()
        .any(|s| s.contains("bare `None`") && s.contains("type")));
}
#[test]
fn c10_4_request_names_queries_in_wire_order_without_values() {
    let f = found(
        r#"client Docs {base "https://docs.test"; query {"z"="MUST_NOT_PRINT";}
        GET get(count?:u32) {path "items"; query {count; override "z"="ALSO_SECRET";}}
    }"#,
    );
    assert!(f.methods["get"].contains(&"Request: `GET https://docs.test/items ?z&count`.".into()));
    assert!(!f.all.join("\n").contains("MUST_NOT_PRINT"));
}

#[test]
fn c1_11_c10_4_endpoint_types_have_no_variants_and_only_value_is_documented() {
    let file: syn::File = syn::parse2(crate::expand(super::description(include_str!(
        "../support/call_client.rs"
    ))))
    .unwrap();
    let module = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Mod(module) if module.ident == "gate" => Some(module),
            _ => None,
        })
        .unwrap();
    let items = &module.content.as_ref().unwrap().1;
    for endpoint_type in ["Plain", "Paged", "Post", "Delete"] {
        let endpoint = items
            .iter()
            .find_map(|item| match item {
                syn::Item::Enum(item) if item.ident == endpoint_type => Some(item),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing endpoint type {endpoint_type}"));
        assert!(matches!(endpoint.vis, syn::Visibility::Public(_)));
        assert!(
            endpoint.variants.is_empty(),
            "{endpoint_type} must have no value"
        );
    }
    let endpoint = items
        .iter()
        .find_map(|item| match item {
            syn::Item::Trait(item) if item.ident == "Endpoint" => Some(item),
            _ => None,
        })
        .unwrap();
    let visible: Vec<_> = endpoint
        .items
        .iter()
        .filter(|item| match item {
            syn::TraitItem::Type(item) => !hidden(&item.attrs),
            syn::TraitItem::Const(item) => !hidden(&item.attrs),
            syn::TraitItem::Fn(item) => !hidden(&item.attrs),
            syn::TraitItem::Macro(item) => !hidden(&item.attrs),
            _ => true,
        })
        .collect();
    assert!(matches!(visible.as_slice(), [syn::TraitItem::Type(value)] if value.ident == "Value"));
}

#[test]
fn c12_14_generated_edit_docs_explain_detection_limits_and_actual_body_ownership() {
    let file = tree("pub client Api { base \"https://edit.test\"; GET get(); }");
    let lines = method_docs(&file, "api::Call<E>", "edit_request");
    for line in [
        "Reading it, removing it or a detected replacement is a Build error.",
        "Wrappers and look-alikes cannot always be detected; edits must leave the body alone.",
        "If building succeeds, each attempt receives the declared repeatable body as a fresh stream.",
    ] {
        assert!(lines.iter().any(|actual| actual == line), "missing generated edit documentation: {line}");
    }
    assert!(!lines
        .iter()
        .any(|line| line.starts_with("replacing or removing it is a Build error.")));
}
