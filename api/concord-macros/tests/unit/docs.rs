//! Whole-line checks for each rule of macro.md section 9.
use proc_macro2::{TokenStream, TokenTree};
use quote::{quote, ToTokens};
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
fn tree(source: &str) -> syn::File {
    syn::parse2(crate::expand(source.parse().unwrap())).unwrap()
}
#[derive(Default)]
struct Found {
    methods: BTreeMap<String, Vec<String>>,
    all: Vec<String>,
    signatures: Vec<String>,
}
impl<'ast> Visit<'ast> for Found {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac
            .path
            .segments
            .last()
            .is_some_and(|s| s.ident == "call_methods")
        {
            self.call_docs(mac.tokens.clone());
        }
    }
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        self.all.extend(docs(std::slice::from_ref(attr)));
    }
    fn visit_impl_item_fn(&mut self, method: &'ast syn::ImplItemFn) {
        if matches!(method.vis, syn::Visibility::Public(_)) {
            self.methods
                .insert(method.sig.ident.to_string(), docs(&method.attrs));
        }
        visit::visit_impl_item_fn(self, method);
    }
}
fn call_helper() -> TokenStream {
    let file = syn::parse_file(
        &std::fs::read_to_string(super::package().join("../concord-core/src/__private/call.rs"))
            .unwrap(),
    )
    .unwrap();
    let helper = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Macro(m) if m.ident.as_ref().is_some_and(|i| i == "call_methods") => {
                Some(m.mac.tokens.clone())
            }
            _ => None,
        })
        .unwrap();
    helper
}

// Read the engine's real helper tokens as well as the procedural expansion.
// This keeps the whole-line assertions attached to the emitted documentation
// after A60 delegates methods to macro_rules, without a second prose catalogue.
impl Found {
    fn call_docs(&mut self, tokens: TokenStream) {
        use syn::{parse::Parser, Token};
        let (args, result_doc, conditional, pages) = (|p: syn::parse::ParseStream<'_>| {
            let _: syn::Type = p.parse()?;
            p.parse::<Token![,]>()?;
            let value: syn::Type = p.parse()?;
            p.parse::<Token![,]>()?;
            let problem: syn::Type = p.parse()?;
            p.parse::<Token![,]>()?;
            let result: syn::Path = p.parse()?;
            p.parse::<Token![,]>()?;
            let doc: syn::LitStr = p.parse()?;
            p.parse::<Token![,]>()?;
            let _: syn::Ident = p.parse()?;
            p.parse::<Token![,]>()?;
            let conditional: syn::Ident = p.parse()?;
            let pages = if !p.is_empty() {
                p.parse::<Token![,]>()?;
                let name: syn::Ident = p.parse()?;
                assert_eq!(name, "pages");
                let body;
                syn::braced!(body in p);
                Some(body.parse::<syn::ImplItemFn>()?)
            } else {
                None
            };
            Ok((
                super::surface::Arguments {
                    value: value.to_token_stream(),
                    shown: !matches!(&problem,syn::Type::Tuple(t) if t.elems.is_empty()),
                    problem: problem.to_token_stream(),
                    result: result.to_token_stream(),
                },
                doc,
                conditional == "conditional",
                pages,
            ))
        })
        .parse2(tokens)
        .unwrap();
        let helper = call_helper();
        let mut signatures = Vec::new();
        super::surface::helper_functions(helper.clone(), &args, conditional, &mut signatures);
        self.signatures
            .extend(signatures.iter().map(|s| s.to_token_stream().to_string()));
        self.helper_docs(helper, &result_doc);
        if let Some(pages) = pages {
            self.visit_impl_item_fn(&pages);
        }
    }
    fn helper_docs(&mut self, tokens: TokenStream, result_doc: &syn::LitStr) {
        use syn::{parse::Parser, Token};
        fn substituted(tokens: TokenStream, doc: &syn::LitStr) -> TokenStream {
            let mut out = TokenStream::new();
            let mut tokens = tokens.into_iter().peekable();
            while let Some(token) = tokens.next() {
                match token {
                    TokenTree::Punct(p)
                        if p.as_char() == '$'
                            && matches!(tokens.peek(),Some(TokenTree::Ident(i)) if i=="await_type") =>
                    {
                        tokens.next();
                        out.extend(quote!(#doc));
                    }
                    TokenTree::Group(g) => out.extend([TokenTree::Group(proc_macro2::Group::new(
                        g.delimiter(),
                        substituted(g.stream(), doc),
                    ))]),
                    token => out.extend([token]),
                }
            }
            out
        }
        fn text(expr: &syn::Expr) -> String {
            match expr {
                syn::Expr::Lit(l) => match &l.lit {
                    syn::Lit::Str(s) => s.value(),
                    _ => panic!("non-string doc"),
                },
                syn::Expr::Macro(m)
                    if m.mac
                        .path
                        .segments
                        .last()
                        .is_some_and(|s| s.ident == "concat") =>
                {
                    syn::punctuated::Punctuated::<syn::Expr, Token![,]>::parse_terminated
                        .parse2(m.mac.tokens.clone())
                        .unwrap()
                        .iter()
                        .map(text)
                        .collect()
                }
                _ => panic!(
                    "unsupported documentation expression: {}",
                    expr.to_token_stream()
                ),
            }
        }
        let input: Vec<_> = tokens.into_iter().collect();
        let mut attrs = Vec::<syn::Attribute>::new();
        let mut i = 0;
        while i < input.len() {
            if matches!(&input[i],TokenTree::Punct(p) if p.as_char()=='#')
                && matches!(input.get(i+1),Some(TokenTree::Group(g)) if g.delimiter()==proc_macro2::Delimiter::Bracket)
            {
                let tokens: TokenStream = input[i..i + 2].iter().cloned().collect();
                attrs.extend(
                    syn::Attribute::parse_outer
                        .parse2(substituted(tokens, result_doc))
                        .unwrap(),
                );
                i += 2;
                continue;
            }
            if matches!(&input[i],TokenTree::Ident(id) if id=="pub")
                && matches!(input.get(i+1),Some(TokenTree::Ident(id)) if id=="fn")
            {
                let name = input[i + 2].to_string();
                if !name.starts_with("__") {
                    let lines = attrs
                        .iter()
                        .filter_map(|a| match &a.meta {
                            syn::Meta::NameValue(n) if a.path().is_ident("doc") => {
                                Some(text(&n.value).trim_start().to_owned())
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    self.all.extend(lines.iter().cloned());
                    self.methods.insert(name, lines);
                }
                attrs.clear();
            } else if let TokenTree::Group(g) = &input[i] {
                self.helper_docs(g.stream(), result_doc);
                attrs.clear();
            } else {
                attrs.clear();
            }
            i += 1;
        }
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
        "`.send().await` gives [`docs::Result`]<[`Vec`]<[`Item`]>>.",
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
    assert!(f.methods["upload"].contains(&"This endpoint may not be repeated.".into()));
    assert!(!f.all.join("\n").contains("HEADER_VALUE_MUST_NOT_LEAK"));
    let f = found("client Api { base \"https://repeat.test\"; POST once(); POST safe() { idempotent; } GET get(); }");
    assert!(f.methods["once"].contains(&"This endpoint may not be repeated.".into()));
    assert!(!f.methods["safe"].contains(&"This endpoint may not be repeated.".into()));
    assert!(!f.methods["get"].contains(&"This endpoint may not be repeated.".into()));
}
#[test]
fn c10_4_doc_rule_2_links_keep_caller_paths_and_disambiguate_data() {
    let f=found("pub client Api { base \"https://types.test\"; read Json; scope account { GET get() -> Account; } }");
    assert!(f.methods["get"].contains(&"`.await` gives [`api::Result`]<[`self::Account`]>.".into()));
    assert!(!f.all.join("\n").contains("crate::api::Result"));
    let f = found(SOURCE);
    assert!(f.methods["items"]
        .contains(&"`.send().await` gives [`docs::Result`]<[`Vec`]<[`Item`]>>.".into()));
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
    assert!(file.contains("Sending < Vec < Item > >"));
    let f = found("client Empty { base \"https://empty.test\"; read Json; problem Problem; }");
    assert!(f.all.contains(
        &"The error of each call: [`concord::Error`]<[`Problem`](super::Problem)>.".into()
    ));
}
#[test]
fn c10_4_doc_rule_4_setters_preserve_spelling_and_state_accepted_values() {
    let f = found(SOURCE);
    assert_eq!(
        f.methods["limit"],
        [
            "Page size.",
            "Accepts a [`u32`], a &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it."
        ]
    );
    let f = found(
        "client Api { base \"https://types.test\"; GET get(text?: String) { query { text; } } }",
    );
    assert_eq!(f.methods["text"], ["Accepts a [`String`], a &[`String`], or an [`Option`]<[`String`]>. `None` clears it. Text and list conversions: [`concord::arg::Arg`]."]);
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
    struct Hidden {
        traits: usize,
        slots: usize,
        impls: usize,
    }
    fn hidden(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|a| {
            a.path().is_ident("doc")
                && matches!(&a.meta,syn::Meta::List(l) if l.tokens.to_string()=="hidden")
        })
    }
    impl<'ast> Visit<'ast> for Hidden {
        fn visit_macro(&mut self, mac: &'ast syn::Macro) {
            if mac
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident == "call_methods")
            {
                self.helper_impls(call_helper());
            }
        }
        fn visit_impl_item_fn(&mut self, method: &'ast syn::ImplItemFn) {
            use quote::ToTokens;
            if matches!(method.vis, syn::Visibility::Public(_)) && !hidden(&method.attrs) {
                assert!(!method.sig.to_token_stream().to_string().contains("__Types"));
            }
        }
        fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
            if t.ident == "__Types" {
                assert!(hidden(&t.attrs));
                self.traits += 1;
            }
        }
        fn visit_item_struct(&mut self, s: &'ast syn::ItemStruct) {
            if s.ident.to_string().starts_with("__Slot") {
                assert!(hidden(&s.attrs));
                self.slots += 1;
            }
        }
        fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
            if i.trait_.as_ref().is_some_and(|(_, p, _)| {
                p.segments
                    .last()
                    .is_some_and(|s| s.ident == "Call" || s.ident == "Slot")
            }) {
                assert!(hidden(&i.attrs));
                self.impls += 1;
            }
            visit::visit_item_impl(self, i);
        }
    }
    impl Hidden {
        fn helper_impls(&mut self, tokens: TokenStream) {
            let input: Vec<_> = tokens.into_iter().collect();
            for (i, token) in input.iter().enumerate() {
                if let TokenTree::Group(group) = token {
                    self.helper_impls(group.stream());
                }
                if !matches!(token,TokenTree::Ident(id) if id=="impl") {
                    continue;
                }
                let Some(end) = input[i..].iter().position(|t|matches!(t,TokenTree::Group(g) if g.delimiter()==proc_macro2::Delimiter::Brace)).map(|n|n+i) else {continue};
                let header: TokenStream = input[i..end].iter().cloned().collect();
                if !header
                    .to_string()
                    .replace(' ', "")
                    .contains("::Callfor$call")
                {
                    continue;
                }
                let mut start = i;
                let mut hidden = false;
                while start >= 2
                    && matches!(&input[start-2],TokenTree::Punct(p) if p.as_char()=='#')
                {
                    let TokenTree::Group(attr) = &input[start - 1] else {
                        break;
                    };
                    hidden |= attr.stream().to_string().replace(' ', "") == "doc(hidden)";
                    start -= 2;
                }
                assert!(
                    hidden,
                    "a pager/engine trait implementation leaked from call_methods!"
                );
                self.impls += 1;
            }
        }
    }
    let mut h = Hidden {
        traits: 0,
        slots: 0,
        impls: 0,
    };
    h.visit_file(&tree(SOURCE));
    h.visit_file(&tree(
        &SOURCE.replace("read Json;", "read Json; problem Problem;"),
    ));
    assert_eq!(h.traits, 2);
    assert!(h.slots > 0 && h.impls > 0);
    let arg = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../concord-core/src/arg.rs"),
    )
    .unwrap();
    assert!(arg.lines().any(|line| line == "mod contract {"));
    assert!(arg.contains("pub use contract::Arg;"));
    assert!(arg.contains("#[doc(hidden)]\n        impl<$($g)*> Arg"));
}
#[test]
fn c1_16_opus_9_send_names_its_public_result() {
    let f = found("client Api { base \"https://types.test\"; read Json; GET get() -> u32; }");
    assert_eq!(
        f.methods["send"],
        [
            "Send the call. On a paged endpoint this gives one page.",
            "Preparation starts on the first poll; only the value is kept.",
            "`.send().await` gives [`api::Result`]<[`u32`]>.",
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
            "Read an answer obtained yourself using this endpoint's reader and status rules.",
            "No retry or clock is applied, and pages are not advanced. Errors report zero tries."
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
        &"- `limit`: Accepts a [`u32`], a &[`u32`], or an [`Option`]<[`u32`]>. `None` clears it."
            .into()
    ));
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
    let line="`.send().await` gives one page; `.pages()` gives them all, and `.pages().items()` each item.";
    assert!(f.methods["items"].contains(&line.into()));
    assert_eq!(f.methods["pages"], [line]);
}

#[test]
fn c10_4_fable_7_required_client_and_scope_input_prose_arrives_whole() {
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
fn c10_4_fable_8_nested_options_explain_all_three_states() {
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
fn c10_4_fable_9_request_names_queries_in_wire_order_without_values() {
    let f = found(
        r#"client Docs {base "https://docs.test"; query {"z"="MUST_NOT_PRINT";}
        GET get(count?:u32) {path "items"; query {count; override "z"="ALSO_SECRET";}}
    }"#,
    );
    assert!(f.methods["get"].contains(&"Request: `GET https://docs.test/items ?z&count`.".into()));
    assert!(!f.all.join("\n").contains("MUST_NOT_PRINT"));
}
