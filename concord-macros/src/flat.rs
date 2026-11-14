//! Resolved, source-preserving input to the Rust code generator.
//!
//! Entries are stored in evaluation order. `position` is their independent wire
//! order, including a key's first position across replacement and removal.
#![allow(dead_code)]

use proc_macro2::Span;
use quote::{quote, ToTokens};

use crate::ast::*;

/// An input after expansion; storage belongs to `owner`, syntax to `written`.
#[derive(Clone, Debug)]
pub(crate) struct InputLine<'a> {
    /// Original input, including its optional mark, docs, type and spans.
    pub(crate) input: &'a Input,
    /// Expanded node that stores the input and supplies its setter or argument.
    pub(crate) owner: &'a Node,
    /// Original writer, which may be a reusable part.
    pub(crate) written: &'a Node,
}

/// An owner view available before an author's expression.
#[derive(Clone, Debug)]
pub(crate) struct Owner<'a> {
    /// Original tree node; its attributes and declaration spans are retained.
    pub(crate) node: &'a Node,
    /// Inputs in root-to-leaf order, own signature before part signatures.
    pub(crate) inputs: Vec<InputLine<'a>>,
}

/// Authorship and lexical bindings, independent of the endpoint receiving them.
#[derive(Clone, Debug)]
pub(crate) struct Origin<'a> {
    /// Original writer, which may be a reusable part.
    pub(crate) written: &'a Node,
    /// Tree node at which this writer was expanded.
    pub(crate) placed: &'a Node,
    /// Complete generated endpoint or placement path, separated by dots.
    pub(crate) path: String,
    /// Owner views visible to this writer; parts see only the client view.
    pub(crate) owners: Vec<Owner<'a>>,
    /// Bare input bindings visible to this writer.
    pub(crate) locals: Vec<InputLine<'a>>,
    /// Original token range of this fact.
    pub(crate) span: Span,
    /// Number of bases in reach at the placement node, including its own.
    pub(crate) base_generation: usize,
    /// Whether damaged syntax makes absence checks inconclusive.
    pub(crate) tainted: bool,
}

/// A value and the exact place and environment in which it was written.
#[derive(Debug)]
pub(crate) struct Line<'a, T> {
    /// Original syntax, copied unchanged by the writer.
    pub(crate) value: &'a T,
    /// Writer, placement, bindings and span for this fact.
    pub(crate) origin: Origin<'a>,
}

impl<T> Clone for Line<'_, T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            origin: self.origin.clone(),
        }
    }
}

/// One surviving keyed value, in evaluation order.
#[derive(Clone, Debug)]
pub(crate) struct EntryLine<'a> {
    /// Original syntax and its lexical origin.
    pub(crate) line: Line<'a, Entry>,
    /// Decoded key, folded to ASCII lowercase for headers only.
    pub(crate) key: String,
    /// Independent wire position assigned when the key was first written.
    pub(crate) position: usize,
    /// Evaluation turn in tree order, with parts before node statements.
    pub(crate) sequence: usize,
    /// Code generation applies the across-base adapter gate to this value or piece.
    pub(crate) crosses_base: bool,
}

/// A whole path statement, with a gate on each piece (including literal pieces).
#[derive(Clone, Debug)]
pub(crate) struct PathLine<'a> {
    /// Original syntax and its lexical origin.
    pub(crate) line: Line<'a, Template>,
    /// Code generation applies the across-base adapter gate to this value or piece.
    pub(crate) crosses_base: bool,
}

/// A body expression, or the joined entries of identically spelled functions.
#[derive(Clone, Debug)]
pub(crate) enum BodyLine<'a> {
    Expression(Line<'a, syn::Expr>),
    Entries {
        function: Line<'a, syn::ExprPath>,
        entries: Vec<EntryLine<'a>>,
    },
}

/// The closed syntactic success wrappers; aliases remain ordinary Rust types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Wrapper {
    Option,
    Result,
}

/// One endpoint, ready for generation. No expressions have been evaluated.
#[derive(Clone, Debug)]
pub(crate) struct Endpoint<'a> {
    /// Original tree node; its attributes and declaration spans are retained.
    pub(crate) node: &'a Node,
    /// Complete generated endpoint or placement path, separated by dots.
    pub(crate) path: String,
    /// Inputs in root-to-leaf order, own signature before part signatures.
    pub(crate) inputs: Vec<InputLine<'a>>,
    /// Nearest description clock, evaluated first for every send.
    pub(crate) idle_timeout: Option<Line<'a, syn::Expr>>,
    /// Endpoint mark, evaluated second; None means use the HTTP method.
    pub(crate) idempotent: Option<Line<'a, Option<syn::Expr>>>,
    /// Nearest base, with its writer and bindings.
    pub(crate) base: Option<Line<'a, Base>>,
    /// Every path, root first; each template piece carries its path gate.
    pub(crate) paths: Vec<PathLine<'a>>,
    /// Surviving query entries in evaluation order, each with a wire position.
    pub(crate) query: Vec<EntryLine<'a>>,
    /// Surviving headers in evaluation order, each with a wire position.
    pub(crate) headers: Vec<EntryLine<'a>>,
    /// Endpoint expression or joined part-and-node entry body.
    pub(crate) body: Option<BodyLine<'a>>,
    /// Nearest reader; evaluating it is required only for an actual body read.
    pub(crate) reader: Option<Line<'a, syn::Expr>>,
    /// Resolved arms in asking order: exact keys before class keys.
    pub(crate) arms: Vec<Line<'a, Arm>>,
    /// Client problem declaration, if any.
    pub(crate) problem: Option<Line<'a, Statement>>,
    /// Problem-line reader, or the client reader at the same lexical level.
    pub(crate) problem_reader: Option<Line<'a, syn::Expr>>,
    /// Endpoint pager, evaluated once when pages are constructed.
    pub(crate) pager: Option<Line<'a, syn::Expr>>,
    /// Simple success type, after removing a recognized success wrapper.
    pub(crate) read_type: Option<&'a syn::Type>,
    /// Closed syntactic Option or Result spelling, without alias inference.
    pub(crate) wrapper: Option<Wrapper>,
    /// Whether damaged syntax makes absence checks inconclusive.
    pub(crate) tainted: bool,
}

/// Each expanded node's own statements, also retained for checks before pruning.
#[derive(Clone, Debug)]
pub(crate) struct Level<'a> {
    /// Original tree node; its attributes and declaration spans are retained.
    pub(crate) node: &'a Node,
    /// Inputs in root-to-leaf order, own signature before part signatures.
    pub(crate) inputs: Vec<InputLine<'a>>,
    /// Expanded local statements in part-before-node order, before pruning.
    pub(crate) statements: Vec<Line<'a, Statement>>,
    /// Whether damaged syntax makes absence checks inconclusive.
    pub(crate) tainted: bool,
}

/// Middleware is constructed once by the client, outside the endpoint lists.
#[derive(Debug, Default)]
pub(crate) struct Client<'a> {
    /// Resolved endpoints in their tree declaration order.
    pub(crate) endpoints: Vec<Endpoint<'a>>,
    /// Expanded nodes for input checks and generated handle declarations.
    pub(crate) levels: Vec<Level<'a>>,
    /// Client expressions in construction order, outside endpoint execution.
    pub(crate) middleware: Vec<Line<'a, syn::Expr>>,
}

/// Stable token spelling; it never depends on Debug implementations or addresses.
pub(crate) fn tokens(value: &impl ToTokens) -> String {
    value.to_token_stream().to_string()
}

/// Status keys have exactly the lexical spelling required by C1.20.
pub(crate) fn status(key: &proc_macro2::TokenTree) -> Option<(u16, bool)> {
    let text = key.to_string();
    let bytes = text.as_bytes();
    if bytes.len() != 3 || !matches!(bytes.first(), Some(b'1'..=b'5')) {
        return None;
    }
    if text.ends_with("xx") {
        return text.get(..1)?.parse::<u16>().ok().map(|n| (n * 100, true));
    }
    if bytes.iter().all(u8::is_ascii_digit) {
        return text.parse().ok().map(|n| (n, false));
    }
    None
}

/// A field, rather than an expression's spelling, is what requests a body.
pub(crate) fn body_field(arm: &Arm) -> Option<&Field> {
    arm.fields
        .as_ref()?
        .fields
        .iter()
        .find(|field| field.name == "body")
}

/// Entry keys are decoded; only headers compare without ASCII case.
pub(crate) fn key(entry: &Entry, header: bool) -> String {
    let key = match &entry.kind {
        EntryKind::Short(reference) => reference.name.to_string(),
        EntryKind::Value { key, .. } | EntryKind::Remove(key) => key.value(),
    };
    if header {
        key.to_ascii_lowercase()
    } else {
        key
    }
}

/// A concrete line for diagnostics, omitting an edit when suggesting plain use.
pub(crate) fn entry_tokens(entry: &Entry) -> proc_macro2::TokenStream {
    match &entry.kind {
        EntryKind::Short(reference) => {
            let name = &reference.name;
            quote!(#name;)
        }
        EntryKind::Value { key, value } => {
            let value = match value {
                Value::Template(template) => template.literal.to_token_stream(),
                Value::Expression(expr) => expr.to_token_stream(),
            };
            quote!(#key = #value;)
        }
        EntryKind::Remove(key) => quote!(remove #key;),
    }
}

/// A plain keyed form that can legally be prefixed with `override`.
pub(crate) fn entry_replacement(entry: &Entry) -> proc_macro2::TokenStream {
    if let EntryKind::Short(reference) = &entry.kind {
        let name = &reference.name;
        let key = syn::LitStr::new(&name.to_string(), name.span());
        quote!(#key = #name;)
    } else {
        entry_tokens(entry)
    }
}

/// Preserve an arm's fields and expression when suggesting its edited form.
pub(crate) fn arm_tokens(arm: &Arm) -> proc_macro2::TokenStream {
    let key = &arm.key;
    let fields = arm.fields.as_ref().map(|fields| {
        let fields = fields.fields.iter().map(|f| {
            let name = &f.name;
            let alias = f.binding.as_ref().map(|binding| quote!(: #binding));
            quote!(#name #alias)
        });
        quote!({ #(#fields),* })
    });
    let expr = arm
        .expression
        .as_ref()
        .map_or_else(|| quote!(()), ToTokens::to_token_stream);
    quote!(#key #fields => #expr)
}

/// Ignore parentheses and leading `::`, but no user path or alias.
pub(crate) fn arrow(ty: &syn::Type) -> (Option<Wrapper>, &syn::Type) {
    let mut bare = ty;
    while let syn::Type::Paren(paren) = bare {
        bare = &paren.elem;
    }
    let syn::Type::Path(path) = bare else {
        return (None, ty);
    };
    if path.qself.is_some() {
        return (None, ty);
    }
    let spelling = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    let wrapper = match spelling.as_str() {
        "Option" | "std::option::Option" | "core::option::Option" => Wrapper::Option,
        "Result" | "std::result::Result" | "core::result::Result" => Wrapper::Result,
        _ => return (None, ty),
    };
    let Some(segment) = path.path.segments.last() else {
        return (None, ty);
    };
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return (None, ty);
    };
    let Some(syn::GenericArgument::Type(inner)) = args.args.first() else {
        return (None, ty);
    };
    (Some(wrapper), inner)
}

fn source(origin: &Origin<'_>) -> String {
    let start = origin.span.start();
    let part = if matches!(origin.written.kind, NodeKind::Part) {
        format!(" part={}", origin.written.name)
    } else {
        String::new()
    };
    format!(
        "@{}{}:{}:{}",
        origin.path,
        part,
        start.line,
        start.column + 1
    )
}

impl Client<'_> {
    /// One line per fact, in construction/send/answer/pager order.
    pub(crate) fn print(&self) -> String {
        let mut out = String::new();
        for middleware in &self.middleware {
            out.push_str(&format!(
                "middleware {} {}\n",
                tokens(middleware.value),
                source(&middleware.origin)
            ));
        }
        for endpoint in &self.endpoints {
            out.push_str(&endpoint.print());
        }
        out
    }
}

impl Endpoint<'_> {
    /// Deterministic plain text for review and checked-in golden files.
    pub(crate) fn print(&self) -> String {
        let method = match &self.node.kind {
            NodeKind::Endpoint(method) => method
                .custom
                .as_ref()
                .map_or_else(|| method.keyword.to_string(), syn::LitStr::value),
            _ => String::new(),
        };
        let result = self
            .node
            .output
            .as_ref()
            .map_or_else(|| "()".into(), |o| tokens(&o.ty));
        let mut out = format!("endpoint {} {} -> {}\n", self.path, method, result);
        for input in &self.inputs {
            let start = input.input.span.start();
            out.push_str(&format!(
                "input {}.{}{}: {} @{}:{}:{}\n",
                owner_name(input.owner),
                input.input.name,
                if input.input.optional.is_some() {
                    "?"
                } else {
                    ""
                },
                tokens(&input.input.ty),
                input.written.name,
                start.line,
                start.column + 1
            ));
        }
        let mut add = |kind: &str, value: String, origin: &Origin<'_>| {
            out.push_str(&format!("{kind} {value} {}\n", source(origin)));
        };
        if let Some(line) = &self.idle_timeout {
            add("idle_timeout", tokens(line.value), &line.origin);
        }
        if let Some(line) = &self.idempotent {
            add(
                "idempotent",
                line.value.as_ref().map_or_else(|| "true".into(), tokens),
                &line.origin,
            );
        }
        if let Some(line) = &self.base {
            let value = match line.value {
                Base::Template(t) => tokens(&t.literal),
                Base::Reference(r) => reference_text(r),
            };
            add("base", value, &line.origin);
        }
        for path in &self.paths {
            for piece in &path.line.value.pieces {
                let value = match piece {
                    TemplatePiece::Text { text, .. } => format!("text={text:?}"),
                    TemplatePiece::Hole {
                        reference, star, ..
                    } => format!(
                        "hole={{{}{}}}",
                        if *star { "*" } else { "" },
                        reference_text(reference)
                    ),
                };
                add(
                    "path",
                    format!("{value} cross_base={}", path.crosses_base),
                    &path.line.origin,
                );
            }
        }
        for (kind, entries) in [("query", &self.query), ("header", &self.headers)] {
            for entry in entries {
                add(
                    kind,
                    format!(
                        "wire={} {} cross_base={}",
                        entry.position,
                        entry_tokens(entry.line.value),
                        entry.crosses_base
                    ),
                    &entry.line.origin,
                );
            }
        }
        match &self.body {
            Some(BodyLine::Expression(line)) => add("body", tokens(line.value), &line.origin),
            Some(BodyLine::Entries { function, entries }) => {
                add("body_function", tokens(function.value), &function.origin);
                for entry in entries {
                    add(
                        "body_entry",
                        entry_tokens(entry.line.value).to_string(),
                        &entry.line.origin,
                    );
                }
            }
            None => {}
        }
        if let Some(reader) = &self.reader {
            add("read", tokens(reader.value), &reader.origin);
        }
        for arm in &self.arms {
            let fields = arm
                .value
                .fields
                .as_ref()
                .map_or_else(String::new, |fields| {
                    fields
                        .fields
                        .iter()
                        .map(|f| {
                            f.binding
                                .as_ref()
                                .map_or_else(|| f.name.to_string(), |b| format!("{}: {b}", f.name))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                });
            let expr = arm
                .value
                .expression
                .as_ref()
                .map_or_else(String::new, tokens);
            add(
                "arm",
                format!("{} {{{fields}}} => {expr}", arm.value.key),
                &arm.origin,
            );
        }
        if let Some(problem) = &self.problem {
            if let StatementKind::Problem { ty, .. } = &problem.value.kind {
                add("problem", tokens(ty), &problem.origin);
            }
        }
        if let Some(reader) = &self.problem_reader {
            add("problem_read", tokens(reader.value), &reader.origin);
        }
        if let Some(pager) = &self.pager {
            add("paginate", tokens(pager.value), &pager.origin);
        }
        out
    }
}

/// The owner's DSL name (`client` is independent of the generated module name).
pub(crate) fn owner_name(node: &Node) -> String {
    if matches!(node.kind, NodeKind::Client) {
        "client".into()
    } else {
        node.name.to_string()
    }
}

/// A hole's name for diagnostics and build errors.
pub(crate) fn reference_text(reference: &Reference) -> String {
    reference.owner.as_ref().map_or_else(
        || reference.name.to_string(),
        |owner| format!("{owner}.{}", reference.name),
    )
}
