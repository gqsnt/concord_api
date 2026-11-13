//! The description as written: no generated names, lookup, or inheritance.
//!
//! Fields intentionally await consumers in steps 2–5. Rust syntax retains its
//! own spans; DSL nodes carry an explicit span as well.
#![allow(dead_code)]

use proc_macro2::Span;
use syn::{Attribute, Expr, ExprPath, Ident, LitStr, Token, Type, Visibility};

/// One client, including the written order of its statements, parts and children.
#[derive(Debug)]
pub(crate) struct Client {
    /// The root declaration.
    pub(crate) node: Node,
}

/// A declaration at any level of the tree.
#[derive(Debug)]
pub(crate) struct Node {
    /// A failed local line makes this node's own facts unavailable to later steps.
    /// Children retain independent marks, so sound siblings remain available.
    pub(crate) parse_failed: bool,
    /// Recovery discarded a declaration header, so its part spreads are unknown.
    pub(crate) signatures_uncertain: bool,
    /// The declaration keyword's span (the method word for an endpoint).
    pub(crate) span: Span,
    /// The declaration's kind; no HTTP semantics are inferred.
    pub(crate) kind: NodeKind,
    /// Written attributes, including desugared doc comments.
    pub(crate) attrs: Vec<Attribute>,
    /// Written visibility; only the client has one.
    pub(crate) visibility: Visibility,
    /// Written name and its original span.
    pub(crate) name: Ident,
    /// The signature, if parentheses were written.
    pub(crate) signature: Option<Signature>,
    /// The arrow and type, if written.
    pub(crate) output: Option<Output>,
    /// Statements in source order, without expansion of parts.
    pub(crate) statements: Vec<Statement>,
    /// Parts in source order.
    pub(crate) parts: Vec<Node>,
    /// Scopes and endpoints in source order.
    pub(crate) children: Vec<Node>,
    /// The written block, or `None` for an endpoint ending in `;`.
    pub(crate) braces: Option<syn::token::Brace>,
}

/// The four declaration forms.
#[derive(Debug)]
pub(crate) enum NodeKind {
    /// A client declaration.
    Client,
    /// A part declaration.
    Part,
    /// A scope declaration.
    Scope,
    /// An endpoint with its written method.
    Endpoint(Method),
}

/// The method token and, for `METHOD`, its string literal.
#[derive(Debug)]
pub(crate) struct Method {
    /// The standard method word or `METHOD`.
    pub(crate) keyword: Ident,
    /// The custom HTTP token, unmodified.
    pub(crate) custom: Option<LitStr>,
}

/// Parentheses distinguish an omitted signature from an explicitly empty one.
#[derive(Debug)]
pub(crate) struct Signature {
    /// The delimiters and their spans.
    pub(crate) parens: syn::token::Paren,
    /// Inputs, then part spreads, in written order.
    pub(crate) entries: Vec<Parameter>,
}

/// A signature element.
#[derive(Debug)]
pub(crate) enum Parameter {
    /// An input declaration.
    Input(Box<Input>),
    /// A part reference, not yet looked up.
    Spread(Spread),
}

/// An input's type is exactly the author's type, including an explicit `Option`.
#[derive(Debug)]
pub(crate) struct Input {
    /// The input name's span.
    pub(crate) span: Span,
    /// The written attributes.
    pub(crate) attrs: Vec<Attribute>,
    /// The written name.
    pub(crate) name: Ident,
    /// The optional mark, with its own span; no inference from the type.
    pub(crate) optional: Option<Token![?]>,
    /// The author's Rust type.
    pub(crate) ty: Type,
}

/// A written `..part` reference.
#[derive(Debug)]
pub(crate) struct Spread {
    /// The dots and their span.
    pub(crate) dots: Token![..],
    /// The name and its span.
    pub(crate) name: Ident,
}

/// An endpoint's declared result.
#[derive(Debug)]
pub(crate) struct Output {
    /// The arrow, retained separately for later diagnostics.
    pub(crate) arrow: Token![->],
    /// The unexamined Rust type.
    pub(crate) ty: Type,
}

/// One statement, with its keyword's original span.
#[derive(Debug)]
pub(crate) struct Statement {
    /// The statement keyword and span.
    pub(crate) keyword: Ident,
    /// A forbidden statement-level edit, retained for resolve's message 24.
    pub(crate) edit: Option<Ident>,
    /// Written attributes (docs are allowed on middleware).
    pub(crate) attrs: Vec<Attribute>,
    /// The right side, as written.
    pub(crate) kind: StatementKind,
}

/// The twelve statement forms.
#[derive(Debug)]
pub(crate) enum StatementKind {
    /// A template or input reference.
    Base(Base),
    /// A path template.
    Path(Template),
    /// A query block.
    Query(Entries),
    /// A headers block.
    Headers(Entries),
    /// A body expression or function with entries.
    Body(Body),
    /// A reader expression.
    Read(Expr),
    /// A status block.
    Status(Arms),
    /// A problem type and its optional reader.
    Problem {
        ty: Box<Type>,
        read: Option<(Ident, Expr)>,
    },
    /// A middleware expression.
    Middleware(Expr),
    /// A pager expression.
    Paginate(Expr),
    /// A written expression, or the bare statement (not a synthesized `true`).
    Idempotent(Option<Expr>),
    /// A silence clock expression.
    IdleTimeout(Expr),
}

/// The two syntactic forms of `base`.
#[derive(Debug)]
pub(crate) enum Base {
    /// A decoded template, retaining its literal.
    Template(Template),
    /// An input reference.
    Reference(Reference),
}

/// The two syntactic forms of a body.
#[derive(Debug)]
pub(crate) enum Body {
    /// An ordinary Rust expression.
    Expression(Expr),
    /// A function path and an ordered block of entries.
    Entries {
        function: ExprPath,
        entries: Entries,
    },
}

/// An ordered block of entries, retaining its delimiters.
#[derive(Debug)]
pub(crate) struct Entries {
    /// At least one entry failed to parse; later steps must skip this block.
    pub(crate) parse_failed: bool,
    /// The block's delimiter spans.
    pub(crate) braces: syn::token::Brace,
    /// Entries in written order, without key comparison.
    pub(crate) entries: Vec<Entry>,
}

/// An entry, with its leading token's span.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The leading token's span.
    pub(crate) span: Span,
    /// The optional edit keyword and span.
    pub(crate) edit: Option<Ident>,
    /// The written entry form.
    pub(crate) kind: EntryKind,
}

/// Short, valued and removed entries are kept distinct.
#[derive(Debug)]
pub(crate) enum EntryKind {
    /// A short entry; qualified references are retained for later message 12.
    Short(Reference),
    /// A string key and its value.
    Value { key: LitStr, value: Value },
    /// A removed string key.
    Remove(LitStr),
}

/// A whole expression is classified as a template only when it is a bare string.
#[derive(Debug)]
pub(crate) enum Value {
    /// A bare string literal.
    Template(Template),
    /// All other Rust expressions, including parenthesized strings.
    Expression(Box<Expr>),
}

/// A status block, retaining its delimiters and arm order.
#[derive(Debug)]
pub(crate) struct Arms {
    /// At least one arm failed to parse; later steps must skip this block.
    pub(crate) parse_failed: bool,
    /// The block's delimiter spans.
    pub(crate) braces: syn::token::Brace,
    /// The arms, not compared or resolved.
    pub(crate) arms: Vec<Arm>,
}

/// A written status arm or removal.
#[derive(Debug)]
pub(crate) struct Arm {
    /// The leading token's span.
    pub(crate) span: Span,
    /// An optional `override` or `remove` token.
    pub(crate) edit: Option<Ident>,
    /// The key token, whose spelling is checked in step 4 (message 45).
    pub(crate) key: proc_macro2::TokenTree,
    /// The fields, if braces were written.
    pub(crate) fields: Option<Fields>,
    /// No expression for a removal.
    pub(crate) expression: Option<Expr>,
}

/// The fields written between an arm's key and arrow.
#[derive(Debug)]
pub(crate) struct Fields {
    /// Delimiter spans.
    pub(crate) braces: syn::token::Brace,
    /// Fields and aliases, unexamined (message 34 belongs to step 4).
    pub(crate) fields: Vec<Field>,
}

/// A response field, optionally renamed.
#[derive(Debug)]
pub(crate) struct Field {
    /// The field name and span.
    pub(crate) name: Ident,
    /// The binding's written name, if explicitly renamed.
    pub(crate) binding: Option<Ident>,
}

/// A name or an `owner.name`; no owner is looked up in the AST.
#[derive(Debug)]
pub(crate) struct Reference {
    /// The first token, or the entire literal for a template hole.
    pub(crate) span: Span,
    /// The optional written owner.
    pub(crate) owner: Option<Ident>,
    /// The written input name.
    pub(crate) name: Ident,
}

/// A decoded template and its ordered text and holes.
#[derive(Debug)]
pub(crate) struct Template {
    /// The original literal, including its span and raw-string spelling.
    pub(crate) literal: LitStr,
    /// Text (with escaped braces decoded) and holes.
    pub(crate) pieces: Vec<TemplatePiece>,
}

/// A template piece; stable Rust locates every piece at its entire literal.
#[derive(Debug)]
pub(crate) enum TemplatePiece {
    /// Fixed decoded text and the literal's span.
    Text { text: String, span: Span },
    /// An input hole, with the literal's span on every identifier.
    Hole {
        reference: Reference,
        star: bool,
        span: Span,
    },
}
