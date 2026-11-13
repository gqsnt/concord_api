//! The authored bindings of each hand-written answer, compared with flat origins.
use std::collections::BTreeSet;

use syn::visit::{self, Visit};

use super::{parse_source, path, tour_description};
use crate::{flat, name, resolve};

fn binding_names(origin: &flat::Origin<'_>) -> Vec<String> {
    origin
        .owners
        .iter()
        .map(|owner| flat::owner_name(owner.node))
        .chain(
            origin
                .locals
                .iter()
                .map(|input| input.input.name.to_string()),
        )
        .collect()
}

fn pattern_name(pattern: &syn::Pat) -> String {
    match pattern {
        syn::Pat::Ident(ident) => ident.ident.to_string(),
        syn::Pat::Type(typed) => pattern_name(&typed.pat),
        _ => panic!("unexpected hand-written binding: {}", flat::tokens(pattern)),
    }
}

fn call_name(call: &syn::ExprCall) -> String {
    let syn::Expr::Path(path) = &*call.func else {
        return String::new();
    };
    path.path.segments.last().unwrap().ident.to_string()
}

fn status_key(arm: &syn::Arm) -> Option<String> {
    if let syn::Pat::Path(path) = &arm.pat {
        return Some(
            match path
                .path
                .segments
                .last()
                .unwrap()
                .ident
                .to_string()
                .as_str()
            {
                "CREATED" => "201",
                "ACCEPTED" => "202",
                "NO_CONTENT" => "204",
                "NOT_FOUND" => "404",
                "CONFLICT" => "409",
                name => panic!("unexpected hand-written status constant {name}"),
            }
            .into(),
        );
    }
    if let Some((_, guard)) = &arm.guard {
        if let syn::Expr::MethodCall(method) = &**guard {
            return Some(
                match method.method.to_string().as_str() {
                    "is_success" => "2xx",
                    "is_redirection" => "3xx",
                    "is_client_error" => "4xx",
                    name => panic!("unexpected hand-written status guard {name}"),
                }
                .into(),
            );
        }
    }
    None
}

struct Answer<'a, 'tree> {
    endpoint: &'a flat::Endpoint<'tree>,
    bindings: Vec<String>,
    reader: Option<&'a flat::Line<'tree, syn::Expr>>,
    key: Option<String>,
    readers: usize,
    arms: Vec<String>,
}

impl<'ast> Visit<'ast> for Answer<'_, '_> {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let outer = self.bindings.len();
        for statement in &block.stmts {
            if let syn::Stmt::Local(local) = statement {
                let name = pattern_name(&local.pat);
                if name == "__arm" {
                    let key = self.key.as_ref().unwrap();
                    let expected = self
                        .endpoint
                        .arms
                        .iter()
                        .find(|arm| arm.value.key.to_string() == *key)
                        .unwrap();
                    assert_eq!(
                        self.bindings,
                        binding_names(&expected.origin),
                        "{} arm {key} writer bindings",
                        self.endpoint.path
                    );
                    let syn::Expr::Closure(closure) = &*local.init.as_ref().unwrap().expr else {
                        panic!("arm must be a closure");
                    };
                    let actual: Vec<_> = closure.inputs.iter().map(pattern_name).collect();
                    let expected: Vec<_> = expected
                        .value
                        .fields
                        .as_ref()
                        .into_iter()
                        .flat_map(|fields| &fields.fields)
                        .map(|field| field.binding.as_ref().unwrap_or(&field.name).to_string())
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "{} arm {key} field bindings",
                        self.endpoint.path
                    );
                    self.arms.push(key.clone());
                }
                visit::visit_local(self, local);
                if !name.starts_with("__") {
                    self.bindings.push(name);
                }
            } else {
                visit::visit_stmt(self, statement);
            }
        }
        self.bindings.truncate(outer);
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let previous = self.key.replace(status_key(arm).unwrap_or_default());
        visit::visit_arm(self, arm);
        self.key = previous;
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let previous = self.reader;
        match call_name(call).as_str() {
            "read_body" => self.reader = self.endpoint.reader.as_ref(),
            "refused" => self.reader = self.endpoint.problem_reader.as_ref(),
            "reader" => {
                let expected = self.reader.unwrap();
                assert_eq!(
                    self.bindings,
                    binding_names(&expected.origin),
                    "{} reader writer bindings",
                    self.endpoint.path
                );
                assert_eq!(
                    flat::tokens(call.args.first().unwrap()),
                    flat::tokens(expected.value),
                    "{} reader expression",
                    self.endpoint.path
                );
                self.readers += 1;
            }
            _ => {}
        }
        visit::visit_expr_call(self, call);
        self.reader = previous;
    }
}

struct Tour<'a, 'tree> {
    model: &'a flat::Client<'tree>,
    checked: BTreeSet<String>,
    readers: usize,
    arms: usize,
    conditional: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for Tour<'_, '_> {
    fn visit_item_impl(&mut self, implementation: &'ast syn::ItemImpl) {
        let Some(endpoint) = self.model.endpoints.iter().find(|endpoint| {
            let (module, name) = endpoint.path.rsplit_once('.').unwrap();
            let expected = format!("{}::{}", module.replace('.', "::"), name::type_name(name));
            flat::tokens(&implementation.self_ty).replace(' ', "") == expected
        }) else {
            return;
        };
        for item in &implementation.items {
            if let syn::ImplItem::Fn(method) = item {
                if method.sig.ident != "__read_answer" {
                    continue;
                }
                let mut answer = Answer {
                    endpoint,
                    bindings: Vec::new(),
                    reader: None,
                    key: None,
                    readers: 0,
                    arms: Vec::new(),
                };
                answer.visit_block(&method.block);
                let expected: Vec<_> = endpoint
                    .arms
                    .iter()
                    .map(|arm| arm.value.key.to_string())
                    .collect();
                assert_eq!(answer.arms, expected, "{} arm asking order", endpoint.path);
                self.readers += answer.readers;
                self.arms += answer.arms.len();
                assert!(
                    self.checked.insert(endpoint.path.clone()),
                    "duplicate answer {}",
                    endpoint.path
                );
            }
        }
    }
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if !invocation
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "call_methods")
        {
            return;
        }
        use syn::parse::Parser as _;
        let (call, conditional) = (|input: syn::parse::ParseStream<'_>| {
            let call: syn::Type = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            let _: syn::Type = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            let _: syn::Type = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            let _: syn::Path = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            let _: syn::LitStr = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            let _: syn::Ident = input.parse()?;
            input.parse::<syn::Token![,]>()?;
            let conditional: syn::Ident = input.parse()?;
            let _: proc_macro2::TokenStream = input.parse()?;
            Ok((flat::tokens(&call).replace(' ', ""), conditional))
        })
        .parse2(invocation.tokens.clone())
        .unwrap();
        let endpoint = self
            .model
            .endpoints
            .iter()
            .find(|endpoint| {
                let (module, name) = endpoint.path.rsplit_once('.').unwrap();
                call == format!("{}::{}", module.replace('.', "::"), name::type_name(name))
            })
            .unwrap();
        let crate::ast::NodeKind::Endpoint(method) = &endpoint.node.kind else {
            panic!();
        };
        let method = method
            .custom
            .as_ref()
            .map_or_else(|| method.keyword.to_string(), syn::LitStr::value);
        assert_eq!(
            conditional == "conditional",
            matches!(method.as_str(), "GET" | "HEAD"),
            "{}: method supplies __prepare_if_none_match eligibility",
            endpoint.path
        );
        assert!(self.conditional.insert(endpoint.path.clone()));
    }
}

#[test]
fn c1_16_all_five_tour_clients_bind_each_reader_and_arm_at_its_writer() {
    let mut endpoints = 0;
    let mut readers = 0;
    let mut arms = 0;
    for client in ["tasks", "riot", "files", "feed", "shapes"] {
        let source = tour_description(client);
        let mut parsed = parse_source(&source);
        let model = resolve::resolve(parsed.client.as_ref().unwrap(), &mut parsed.diagnostics);
        assert!(
            parsed.diagnostics.is_empty(),
            "{}",
            parsed.diagnostics.into_tokens()
        );
        let mut visitor = Tour {
            model: &model,
            checked: BTreeSet::new(),
            readers: 0,
            arms: 0,
            conditional: BTreeSet::new(),
        };
        for file in std::iter::once(client).chain((client == "files").then_some("grant")) {
            let rust =
                std::fs::read_to_string(path().join(format!("../tour/src/{file}.rs"))).unwrap();
            visitor.visit_file(&syn::parse_file(&rust).unwrap());
        }
        assert_eq!(
            visitor.checked.len(),
            model.endpoints.len(),
            "{client}: every answer compared"
        );
        assert_eq!(
            visitor.conditional, visitor.checked,
            "{client}: every conditional preparation compared"
        );
        endpoints += visitor.checked.len();
        readers += visitor.readers;
        arms += visitor.arms;
    }
    assert_eq!(endpoints, 55); // Four guide clients (16), grant, and all 38 Shapes endpoints.
    assert!(readers > endpoints);
    assert!(arms > 10);
}

#[test]
fn c1_17_part_readers_and_arms_keep_part_bindings_beside_node_bindings() {
    let mut parsed = parse_source(
        r#"client Bound(root: R) {
        base "https://bindings.example"; read root; problem E;
        part response(reader: R) {
            read reader;
            status { 404 { headers } => f(client.root, reader, headers) }
        }
        scope region(zone: String) {
            path "{zone}";
            GET fetch(local: u8, ..response) -> u8 { query { local; } }
        }
    }"#,
    );
    let model = resolve::resolve(parsed.client.as_ref().unwrap(), &mut parsed.diagnostics);
    assert!(
        parsed.diagnostics.is_empty(),
        "{}",
        parsed.diagnostics.into_tokens()
    );
    let endpoint = &model.endpoints[0];
    let reader = endpoint.reader.as_ref().unwrap();
    let arm = &endpoint.arms[0];
    for origin in [&reader.origin, &arm.origin] {
        assert_eq!(origin.written.name, "response");
        assert_eq!(origin.placed.name, "fetch");
        assert_eq!(binding_names(origin), ["client", "reader"]);
        assert_eq!(origin.locals[0].owner.name, "fetch");
        assert_eq!(origin.locals[0].written.name, "response");
    }
    assert_eq!(
        binding_names(&endpoint.query[0].line.origin),
        ["client", "region", "local", "reader"]
    );
    assert_eq!(
        binding_names(&endpoint.problem_reader.as_ref().unwrap().origin),
        ["client", "root"]
    );
}
