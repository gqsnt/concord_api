//! Typed diagnostic catalogue. Every hole is a required Rust argument.
//!
//! Constructors format their arguments once; inserted author text is never
//! interpreted as a template. Unused constructors belong to the later steps.
#![allow(dead_code)]

use proc_macro2::{Span, TokenStream, TokenTree};

/// The author's tokens, displayed using their source span when available.
#[derive(Clone, Debug)]
pub(crate) struct Written {
    tokens: TokenStream,
    edited: Option<String>,
}

impl Written {
    pub(crate) fn tokens(&self) -> TokenStream {
        self.tokens.clone()
    }
    /// Retain written tokens, rather than an example from the documentation.
    pub(crate) fn new(tokens: TokenStream) -> Self {
        Self {
            tokens,
            edited: None,
        }
    }

    /// Prefer the complete source range, falling back to printed tokens.
    pub(crate) fn text(&self) -> String {
        if let Some(text) = &self.edited {
            return text.clone();
        }
        let mut tokens = self.tokens.clone().into_iter();
        let Some(first) = tokens.next() else {
            return String::new();
        };
        let last = tokens.last().unwrap_or_else(|| first.clone());
        first
            .span()
            .join(last.span())
            .and_then(|span| span.source_text())
            // Reconstructed repairs can retain spans from the original input.
            // Source text is usable only when it still names these tokens.
            .filter(|source| {
                source
                    .parse::<TokenStream>()
                    .is_ok_and(|tokens| tokens.to_string() == self.tokens.to_string())
            })
            .unwrap_or_else(|| printed(self.tokens.clone()))
    }

    /// Repair only the first literal (the METHOD string), retaining the line.
    pub(crate) fn method_replacement(&self, replacement: &str) -> Self {
        let mut replaced = false;
        let mut edited = self.text();
        let tokens = self
            .tokens
            .clone()
            .into_iter()
            .map(|token| {
                if !replaced && matches!(token, TokenTree::Literal(_)) {
                    replaced = true;
                    let original = token
                        .span()
                        .source_text()
                        .unwrap_or_else(|| token.to_string());
                    let literal = syn::LitStr::new(replacement, token.span());
                    edited = edited.replacen(&original, &literal.token().to_string(), 1);
                    TokenTree::Literal(literal.token())
                } else {
                    token
                }
            })
            .collect();
        Self {
            tokens,
            edited: Some(edited),
        }
    }
}

/// Print DSL fragments with Rust's punctuation, even on stable proc-macro spans.
fn printed(tokens: TokenStream) -> String {
    let tokens: Vec<_> = tokens.into_iter().collect();
    let mut out = String::new();
    for (i, token) in tokens.iter().enumerate() {
        let previous = i.checked_sub(1).and_then(|i| tokens.get(i));
        let punct =
            |t: Option<&TokenTree>, ch| matches!(t, Some(TokenTree::Punct(p)) if p.as_char() == ch);
        let tight_before = matches!(token, TokenTree::Punct(p) if ";,:.?!><".contains(p.as_char()));
        let tight_after = matches!(previous, Some(TokenTree::Punct(p)) if ".!<&".contains(p.as_char()) || p.spacing() == proc_macro2::Spacing::Joint)
            || (punct(previous, ':')
                && i.checked_sub(2)
                    .and_then(|i| tokens.get(i))
                    .is_some_and(|t| punct(Some(t), ':')));
        let parens = matches!(token, TokenTree::Group(g) if g.delimiter() == proc_macro2::Delimiter::Parenthesis);
        if !out.is_empty() && !tight_before && !tight_after && !parens {
            out.push(' ');
        }
        match token {
            TokenTree::Group(group) => {
                let inner = printed(group.stream());
                use proc_macro2::Delimiter::*;
                match group.delimiter() {
                    Parenthesis => out.push_str(&format!("({inner})")),
                    Bracket => out.push_str(&format!("[{inner}]")),
                    Brace => out.push_str(&format!("{{ {inner} }}")),
                    None => out.push_str(&inner),
                }
            }
            _ => out.push_str(&token.to_string()),
        }
    }
    out
}

/// A completely bound message; its private fields prohibit partial messages.
#[derive(Debug)]
pub(crate) struct Message {
    number: u8,
    error: String,
    label: String,
    help: String,
    secondary: Option<(Span, String)>,
}

impl Message {
    /// A39: replace an illustrative help with a repair justified by this input.
    pub(crate) fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }
    /// Contextual wording without changing the catalogue's primary message.
    pub(crate) fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
    /// The catalogue number, including contextual variants of that number.
    pub(crate) fn number(&self) -> u8 {
        self.number
    }
    /// Stable Rust displays the catalogue's three lines as one text.
    pub(crate) fn render(&self) -> String {
        format!("{}\nlabel: {}\nhelp: {}", self.error, self.label, self.help)
    }
    pub(crate) fn secondary_text(mut self, text: &str) -> Self {
        if let Some((_, label)) = &mut self.secondary {
            *label = text.into();
        }
        self
    }
    fn first(mut self, span: Span, text: String) -> Self {
        self.secondary = Some((span, text));
        self
    }
}

fn message(number: u8, error: String, label: String, help: String) -> Message {
    Message {
        number,
        error,
        label,
        help,
        secondary: None,
    }
}

/// Message 1: duplicate child.
pub(crate) fn m01_duplicate_child(replacement: &str, name: &str, first: Span) -> Message {
    message(
        1,
        format!("duplicate child name `{name}`"),
        "already declared in this block".to_owned(),
        format!("rename this endpoint to `{replacement}`"),
    )
    .first(first, "the first endpoint is here".to_owned())
}

/// Message 2: method conflict.
pub(crate) fn m02_method_conflict(name: &str, replacement: &str, first: Span) -> Message {
    message(
        2,
        format!("two declarations generate method `{name}`"),
        "conflicts with the optional input".to_owned(),
        format!("rename this child to `{replacement}`"),
    )
    .first(first, "the optional input is declared here".to_owned())
}

/// Message 3: reserved input.
pub(crate) fn m03_reserved_input(replacement: &str, name: &str) -> Message {
    message(
        3,
        format!("`{name}` is reserved by Concord"),
        "conflicts with a generated method".to_owned(),
        format!("rename it to `{replacement}`; write `query {{ \"{name}\" = {replacement}; }}`"),
    )
}

/// Message 4: keyword input.
pub(crate) fn m04_keyword_input(replacement: &str, ty: &str, key: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name inputs".into(),
        "choose another Rust name".into(),
        format!("use `{replacement}: {ty}` and `query {{ \"{key}\" = {replacement}; }}`"),
    )
}

/// Message 5: invalid name.
pub(crate) fn m05_invalid_name(replacement: &str) -> Message {
    message(
        5,
        "invalid API name".to_owned(),
        "expected lower snake case".to_owned(),
        format!("write `{replacement}`"),
    )
}

/// Message 6: invalid client.
pub(crate) fn m06_invalid_client(replacement: &str) -> Message {
    message(
        6,
        "invalid client name".to_owned(),
        "expected upper camel case with a non-keyword module name".to_owned(),
        format!("write `client {replacement}`"),
    )
}

/// Message 7: owner scope.
pub(crate) fn m07_owner_scope(replacement: &str, first: Span) -> Message {
    message(
        7,
        "ambiguous input owner".to_owned(),
        "this owner name is already in use".to_owned(),
        format!("rename this scope to `{replacement}`"),
    )
    .first(first, "this owner is declared here".to_owned())
}

/// Message 8: generated collision.
pub(crate) fn m08_generated_collision(
    second: &str,
    first: &str,
    generated: &str,
    replacement: &str,
    earlier: Span,
) -> Message {
    message(
        8,
        format!("`{first}` and `{second}` both generate the type `{generated}`"),
        "generated names collide".into(),
        format!("rename `{second}` to `{replacement}`"),
    )
    .first(earlier, "the first generated name is here".into())
}

/// Message 9: unused input.
pub(crate) fn m09_unused_input(name: &str) -> Message {
    message(
        9,
        format!("input `{name}` is never used"),
        "no placement or behaviour uses this input".to_owned(),
        format!("add `query {{ {name}; }}` or remove the input"),
    )
}

/// Message 10: borrowed input.
pub(crate) fn m10_borrowed_input(name: &str, owned_type: &str) -> Message {
    message(
        10,
        "input types must own their data".to_owned(),
        "borrowed input type".to_owned(),
        format!("write `{name}: {owned_type}`; callers may still pass references"),
    )
}

/// Message 11: unknown input.
pub(crate) fn m11_unknown_input(name: &str, ty: &str) -> Message {
    message(
        11,
        format!("unknown input `{name}`"),
        "no input with this name".to_owned(),
        format!("declare `{name}: {ty}` in this signature"),
    )
}

/// Message 12: ancestor hole.
pub(crate) fn m12_ancestor_hole(qualified: &str, name: &str, owner: &str) -> Message {
    message(
        12,
        "ancestor inputs require their owner's name".to_owned(),
        format!("`{name}` belongs to {owner}"),
        format!("write `{{{qualified}}}`"),
    )
}

/// Message 13: optional url hole.
pub(crate) fn m13_optional_url_hole(name: &str, ty: &str, first: Span) -> Message {
    message(
        13,
        "this URL input must supply one required value".to_owned(),
        "an optional input cannot fill this hole".to_owned(),
        format!("declare `{name}: {ty}`, without `?`"),
    )
    .first(first, "this input is marked optional here".to_owned())
}

/// Message 14: input attribute.
pub(crate) fn m14_input_attribute(name: &str, key: &str) -> Message {
    message(
        14,
        "unsupported attribute on an input".into(),
        "an input takes doc comments only".into(),
        format!("use `query {{ \"{key}\" = {name}; }}`"),
    )
}

/// Message 15: missing method.
pub(crate) fn m15_missing_method(line: &Written) -> Message {
    let line = line.text();
    message(
        15,
        "expected an HTTP method before the endpoint name".to_owned(),
        "endpoint method is missing".to_owned(),
        format!("write `GET {line}`"),
    )
}

/// Message 16: after child.
pub(crate) fn m16_after_child(line: &Written) -> Message {
    let line = line.text();
    message(
        16,
        "statements must precede children".to_owned(),
        "statement appears after a child".to_owned(),
        format!("move `{line}` before the first child"),
    )
}

/// Message 17: duplicate statement.
pub(crate) fn m17_duplicate_statement(line: &Written, statement: &str, first: Span) -> Message {
    let line = line.text();
    message(
        17,
        format!("duplicate `{statement}` statement"),
        format!("this node already declares `{statement}`"),
        format!("keep one `{line}` statement"),
    )
    .first(first, format!("the first `{statement}` statement is here"))
}

/// Message 18: requires endpoint.
pub(crate) fn m18_requires_endpoint(line: &Written, statement: &str, node: &str) -> Message {
    let line = line.text();
    message(
        18,
        format!("`{statement}` requires an endpoint"),
        format!("`{statement}` stands on an endpoint, not on a {node}"),
        format!("move `{line}` into the endpoint block"),
    )
}

/// Message 19: invalid method.
pub(crate) fn m19_invalid_method(line: &Written) -> Message {
    let line = line.text();
    message(
        19,
        "invalid method name".to_owned(),
        "a method name is an HTTP token, with no space".to_owned(),
        format!("write `{line}`"),
    )
}

/// Message 20: problem placement.
pub(crate) fn m20_problem_placement(line: &Written) -> Message {
    let line = line.text();
    message(
        20,
        "`problem` requires the client".to_owned(),
        "problem types are declared on the client".to_owned(),
        format!("move `{line}` into the client block"),
    )
}

/// Message 21: inherited header.
pub(crate) fn m21_inherited_header(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        21,
        format!("header `{key}` is already set above"),
        "replacement requires `override`".into(),
        format!("write `override {}`", replacement.text()),
    )
    .first(first, "the inherited header is set here".into())
}

/// Message 22: override header.
pub(crate) fn m22_override_header(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no header `{key}` above to override"),
        "nothing is set above for this key".into(),
        format!("write `{}`", replacement.text()),
    )
}

/// Message 23: remove arm.
pub(crate) fn m23_remove_arm(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited status arm `{key}` to remove"),
        "this arm is absent".into(),
        format!("delete `{}`", removal.text()),
    )
}

/// Message 24: edit path.
pub(crate) fn m24_edit_path(line: &Written) -> Message {
    let line = line.text();
    message(
        24,
        "paths cannot be overridden or removed".to_owned(),
        "each path adds segments".to_owned(),
        format!("write `{line}` and change the ancestor's path if needed"),
    )
}

/// Message 25: empty path.
pub(crate) fn m25_empty_path(replacement: &Written) -> Message {
    message(
        25,
        "path segments must be nonempty".into(),
        "leading or doubled slash".into(),
        format!(
            "write `{}`; omit an empty path statement",
            replacement.text()
        ),
    )
}

/// Message 26: dot path.
pub(crate) fn m26_dot_path(segment: &str, replacement: &Written) -> Message {
    message(
        26,
        "dot segments are forbidden in paths".into(),
        format!("`{segment}` would navigate upwards"),
        format!(
            "write the intended path directly, such as `{}`",
            replacement.text()
        ),
    )
}

/// Message 27: path character.
pub(crate) fn m27_path_character(declaration: &Written, replacement: &Written) -> Message {
    message(
        27,
        "forbidden character in fixed path text".into(),
        "this character must come from an encoded value".into(),
        format!(
            "declare `{}` and write `{}`",
            declaration.text(),
            replacement.text()
        ),
    )
}

/// Message 28: path query.
pub(crate) fn m28_path_query(replacement: &Written) -> Message {
    message(
        28,
        "a path cannot contain a query".into(),
        "query begins here".into(),
        format!("write `{}`", replacement.text()),
    )
}

/// Message 29: star segment.
pub(crate) fn m29_star_segment(name: &str, adjacent: &str, replacement: &Written) -> Message {
    message(
        29,
        "a `{*…}` hole must be a whole segment".into(),
        format!("`{{*{name}}}` shares its segment with `{adjacent}`"),
        format!(
            "write `{}`; a `{{{name}}}` hole may share its segment",
            replacement.text()
        ),
    )
}

/// Message 30: header name.
pub(crate) fn m30_header_name(replacement: &Written) -> Message {
    message(
        30,
        "invalid header name".into(),
        "spaces are forbidden in header names".into(),
        format!("use a token such as `{}`", replacement.text()),
    )
}

/// Message 31: head body.
pub(crate) fn m31_head_body(replacement: &Written) -> Message {
    message(
        31,
        "a HEAD response has no readable body".into(),
        "this arm asks for the body".into(),
        format!("write the arm without it, such as `{}`", replacement.text()),
    )
}

/// Message 32: head success.
pub(crate) fn m32_head_success(replacement: &Written) -> Message {
    message(
        32,
        "this HEAD result needs a success arm".into(),
        "no arm supplies the declared result".into(),
        format!("add `{}`", replacement.text()),
    )
}

/// Message 33: missing reader.
pub(crate) fn m33_missing_reader(replacement: &Written, import: &Written) -> Message {
    message(
        33,
        "nothing says how this answer is read".into(),
        "no `read` here or above".into(),
        format!(
            "write `{}` on the client, with `{}`",
            replacement.text(),
            import.text()
        ),
    )
}

/// Message 34: arm field.
pub(crate) fn m34_arm_field(replacement: &Written, binding: &Written) -> Message {
    message(
        34,
        "an arm binds body, headers or status".into(),
        "no such part of an answer".into(),
        format!(
            "write `{}`; give it your own name with `{}`",
            replacement.text(),
            binding.text()
        ),
    )
}

/// Message 35: unknown part.
pub(crate) fn m35_unknown_part(name: &str, replacement: &str) -> Message {
    message(
        35,
        format!("no part named `{name}`"),
        "not declared in the client block".to_owned(),
        format!("write `..{replacement}`, or declare `part {name}(…) {{ … }}` in the client"),
    )
}

/// Message 36: unused part.
pub(crate) fn m36_unused_part(name: &str) -> Message {
    message(
        36,
        format!("part `{name}` is never used"),
        format!("no scope or endpoint names `..{name}`"),
        format!("add `..{name}` to a signature, or remove the part"),
    )
}

/// Message 37: part statement.
pub(crate) fn m37_part_statement(statement: &str, line: &Written) -> Message {
    message(
        37,
        format!("a part cannot hold `{statement}`"),
        "`path`, `problem`, `middleware` and children belong to the tree".into(),
        format!("move `{}` into the scope that names the part", line.text()),
    )
}

/// Message 38: part conflict.
pub(crate) fn m38_part_conflict(statement: &str, part: &str, node: &str, first: Span) -> Message {
    message(
        38,
        format!("`{statement}` is written in the part `{part}` and in this {node}"),
        "the statements of a part count as this node's own".to_owned(),
        format!("remove this `{statement}`, or do not name `..{part}`"),
    )
    .first(first, format!("the part `{part}` writes it here"))
}

/// Message 39: no arrow.
pub(crate) fn m39_no_arrow(arrow: &Written, removal: &Written, first: Span) -> Message {
    message(
        39,
        "an arm in reach reads a body, and this endpoint has no arrow".into(),
        "the body would be read into nothing".into(),
        format!(
            "add an arrow, such as `{}` (a type of yours that reads anything), or `{}`",
            arrow.text(),
            removal.text()
        ),
    )
    .first(first, "this arm reads the body".into())
}

/// Message 40: optional middleware.
pub(crate) fn m40_optional_middleware(name: &str, ty: &str) -> Message {
    message(
        40,
        format!("`{name}` is optional, and a middleware is made in `new`"),
        "only the required inputs of the client are set when `new` runs".to_owned(),
        format!("declare `{name}: {ty}`, without `?`"),
    )
}

/// Message 41: short header.
pub(crate) fn m41_short_header(name: &str, key: &str) -> Message {
    message(
        41,
        "a header name cannot be written with `_`".to_owned(),
        format!("this entry would send a header named `{name}`"),
        format!("write `\"{key}\" = {name};`"),
    )
}

/// Message 42: host header.
pub(crate) fn m42_host_header(replacement: &Written) -> Message {
    message(
        42,
        "`host` cannot be set in `headers`".into(),
        "the host comes from the `base`".into(),
        format!(
            "put the host in the `base`, such as `{}`",
            replacement.text()
        ),
    )
}

/// Message 43: missing base.
pub(crate) fn m43_missing_base(replacement: &Written) -> Message {
    message(
        43,
        "no `base` in reach of this endpoint".into(),
        "no `base` here or above".into(),
        format!("write `{}` on the client", replacement.text()),
    )
}

/// Message 44: class override.
pub(crate) fn m44_class_override(
    class: &str,
    key: &str,
    replacement: &Written,
    first: Span,
) -> Message {
    message(
        44,
        format!("`override {class}` leaves the inherited arm `{key}` in place"),
        format!("an exact arm from above still takes `{key}`"),
        format!(
            "also write `override {}`, or `remove {key}`",
            replacement.text()
        ),
    )
    .first(first, format!("the inherited `{key}` arm is here"))
}

/// Message 45: status key.
pub(crate) fn m45_status_key(key: &str, exact: &str, class: &str) -> Message {
    message(
        45,
        format!("invalid status key `{key}`"),
        "a key is 100 to 599, or 1xx to 5xx".into(),
        format!("write an exact status such as `{exact}`, or a class such as `{class}`"),
    )
}

/// Message 46: option arm.
pub(crate) fn m46_option_arm(replacement: &Written, first: Span) -> Message {
    m46_option_exact_arm("2xx", replacement, first)
}

/// Message 10's separate case: Self cannot refer to the enclosing type.
pub(crate) fn m10_self_type() -> Message {
    message(
        10,
        "`Self` has no meaning in a description".into(),
        "`Self` is not the enclosing type here".into(),
        "write the path of the type".into(),
    )
}

/// Collected errors from all steps; there is no numeric or unbound raise API.
#[derive(Default)]
pub(crate) struct Diagnostics {
    errors: Option<syn::Error>,
    primary_spans: Vec<Span>,
    #[cfg(test)]
    numbers: Vec<u8>,
    #[cfg(test)]
    messages: Vec<String>,
}
impl Diagnostics {
    /// Add the parser's own words or an already spanned error.
    pub(crate) fn push(&mut self, error: syn::Error) {
        self.primary_spans.push(error.span());
        self.combine(error);
    }
    fn combine(&mut self, error: syn::Error) {
        match &mut self.errors {
            Some(errors) => errors.combine(error),
            None => self.errors = Some(error),
        }
    }
    /// Raise a fully bound catalogue message and its context-specific second place.
    pub(crate) fn raise(&mut self, span: Span, message: Message) {
        #[cfg(test)]
        self.numbers.push(message.number());
        #[cfg(test)]
        self.messages.push(message.render());
        self.push(syn::Error::new(span, message.render()));
        if let Some((span, text)) = message.secondary {
            self.combine(syn::Error::new(span, text));
        }
    }
    /// Raise over a complete written Rust type or multi-token DSL location.
    pub(crate) fn raise_spanned(&mut self, tokens: impl quote::ToTokens, message: Message) {
        #[cfg(test)]
        self.numbers.push(message.number());
        #[cfg(test)]
        self.messages.push(message.render());
        self.push(syn::Error::new_spanned(tokens, message.render()));
        if let Some((span, text)) = message.secondary {
            self.combine(syn::Error::new(span, text));
        }
    }
    /// Catalogue numbers observed by semantic regression tests.
    #[cfg(test)]
    pub(crate) fn numbers(&self) -> &[u8] {
        &self.numbers
    }
    #[cfg(test)]
    pub(crate) fn messages(&self) -> &[String] {
        &self.messages
    }
    /// Whether any step has refused the description.
    pub(crate) fn is_empty(&self) -> bool {
        self.errors.is_none()
    }
    /// Primary refusals, excluding notes on the earlier sound declaration.
    pub(crate) fn primary_spans(&self) -> &[Span] {
        &self.primary_spans
    }
    /// Emit all errors, preserving each author's span.
    pub(crate) fn into_tokens(self) -> TokenStream {
        self.errors
            .map_or_else(TokenStream::new, |error| error.into_compile_error())
    }
}

/// Message 1 for two parts with the same name.
pub(crate) fn m01_duplicate_part(name: &str, replacement: &str, first: Span) -> Message {
    message(
        1,
        format!("duplicate part name `{name}`"),
        "already declared in this block".into(),
        format!("rename this part to `{replacement}`"),
    )
    .first(first, "the first part is here".into())
}
/// Message 1 for an input written twice in one signature.
pub(crate) fn m01_duplicate_input(name: &str, replacement: &str, first: Span) -> Message {
    message(
        1,
        format!("duplicate input name `{name}`"),
        "already declared in this signature".into(),
        format!("rename this input to `{replacement}`"),
    )
    .first(first, "the first input is here".into())
}
/// Message 3 for a child whose name would generate a reserved method.
pub(crate) fn m03_reserved_child(name: &str, replacement: &str) -> Message {
    message(
        3,
        format!("`{name}` is reserved by Concord"),
        "conflicts with a generated method".into(),
        format!("rename this child to `{replacement}`"),
    )
}
/// Message 4 for a keyword or raw scope name.
pub(crate) fn m04_keyword_scope(line: &Written, replacement: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name scopes".into(),
        "choose another Rust name".into(),
        format!("rename the scope in `{}` to `{replacement}`", line.text()),
    )
}
/// Message 4 for a keyword or raw endpoint name.
pub(crate) fn m04_keyword_endpoint(line: &Written, replacement: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name endpoints".into(),
        "choose another Rust name".into(),
        format!(
            "rename the endpoint in `{}` to `{replacement}`",
            line.text()
        ),
    )
}
/// Message 4 for a part name: parts use the grammar's same `name` production.
pub(crate) fn m04_keyword_part(line: &Written, replacement: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name parts".into(),
        "choose another Rust name".into(),
        format!("rename the part in `{}` to `{replacement}`", line.text()),
    )
}
/// Message 7 for an input that hides an owner, retaining its wire key.
pub(crate) fn m07_owner_input(key: &str, replacement: &str, first: Span) -> Message {
    message(
        7,
        "ambiguous input owner".into(),
        "this owner name is already in use".into(),
        format!(
            "rename this input to `{replacement}`; write `query {{ \"{key}\" = {replacement}; }}`"
        ),
    )
    .first(first, "this owner is declared here".into())
}
/// Message 11 for a reference to a scope's input from inside a part.
pub(crate) fn m11_scope_input_in_part(name: &str, declaration: &Written) -> Message {
    message(
        11,
        format!("unknown input `{name}`"),
        "a part sees its own inputs and client inputs only".into(),
        format!("declare `{}` in this part's signature", declaration.text()),
    )
}
/// Message 12 for a qualified name used as a short entry.
pub(crate) fn m12_short_entry(owner: &str, name: &str) -> Message {
    message(
        12,
        "ancestor inputs require their owner's name".into(),
        "a short entry names an input of this node".into(),
        format!("write `\"{name}\" = {owner}.{name};`"),
    )
}
/// Message 12 for an unqualified base reference.
pub(crate) fn m12_ancestor_base(owner: &str, name: &str) -> Message {
    message(
        12,
        "ancestor inputs require their owner's name".into(),
        format!("`{name}` belongs to {owner}"),
        format!("write `base {owner}.{name};`"),
    )
}
/// Message 13 for an optional input named directly by base.
pub(crate) fn m13_optional_base(declaration: &Written, first: Span) -> Message {
    message(
        13,
        "this URL input must supply one required value".into(),
        "an optional input cannot supply the base".into(),
        format!("declare `{}`, without `?`", declaration.text()),
    )
    .first(first, "this input is marked optional here".into())
}
/// Message 14 for an unsupported part attribute.
pub(crate) fn m14_part_attribute(attribute: &Written, destination: &str) -> Message {
    message(
        14,
        "unsupported attribute on a part".into(),
        "a part takes doc comments only".into(),
        format!("put `{}` on {destination}", attribute.text()),
    )
}
/// Message 14 for an unsupported client attribute.
pub(crate) fn m14_client_attribute(attribute: &Written, destination: &str) -> Message {
    message(
        14,
        "unsupported attribute on the client".into(),
        "a client takes doc comments and #[cfg] only".into(),
        format!("put `{}` on {destination}", attribute.text()),
    )
}
/// Message 14 for an unsupported scope attribute.
pub(crate) fn m14_scope_attribute(attribute: &Written) -> Message {
    message(
        14,
        "unsupported attribute on a scope".into(),
        "a scope takes doc comments, #[cfg], #[deprecated] and #[doc(hidden)] only".into(),
        format!("remove `{}` from this scope", attribute.text()),
    )
}
/// Message 14 for an unsupported endpoint attribute.
pub(crate) fn m14_endpoint_attribute(attribute: &Written) -> Message {
    message(
        14,
        "unsupported attribute on an endpoint".into(),
        "an endpoint takes doc comments, #[cfg], #[deprecated] and #[doc(hidden)] only".into(),
        format!("remove `{}` from this endpoint", attribute.text()),
    )
}
/// Message 14 for an attribute on a statement other than middleware docs.
pub(crate) fn m14_statement_attribute(attribute: &Written, statement: &str) -> Message {
    message(
        14,
        format!("unsupported attribute on `{statement}`"),
        "this statement takes no attributes".into(),
        format!("remove `{}` from this statement", attribute.text()),
    )
}
/// Message 16's distinct ordering case: a statement after a part.
pub(crate) fn m16_after_part(line: &Written) -> Message {
    message(
        16,
        "statements must precede parts".into(),
        "statement appears after a part".into(),
        format!("move `{}` before the first part", line.text()),
    )
}
/// Message 16 for a part after a child declaration.
pub(crate) fn m16_part_after_child(line: &Written) -> Message {
    message(
        16,
        "statements must precede children".into(),
        "part appears after a child".into(),
        format!("move `{}` before the first child", line.text()),
    )
}
/// Message 18 at a spread whose part holds an endpoint-only statement.
pub(crate) fn m18_part_requires_endpoint(statement: &str, spread: &Written) -> Message {
    message(
        18,
        format!("`{statement}` requires an endpoint"),
        format!("a scope cannot name a part holding `{statement}`"),
        format!("move `{}` into the endpoint signature", spread.text()),
    )
}
/// Message 20 for middleware outside the client, excluding parts (message 37).
pub(crate) fn m20_middleware_placement(line: &Written) -> Message {
    message(
        20,
        "`middleware` requires the client".into(),
        "middlewares are declared on the client".into(),
        format!("move `{}` into the client block", line.text()),
    )
}
/// Message 20 for a part declaration outside the client.
pub(crate) fn m20_part_placement(line: &Written) -> Message {
    message(
        20,
        "`part` requires the client".into(),
        "parts are declared on the client".into(),
        format!("move `{}` into the client block", line.text()),
    )
}
/// Message 21 for an inherited query pair.
pub(crate) fn m21_inherited_query(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        21,
        format!("query pair `{key}` is already set above"),
        "replacement requires `override`".into(),
        format!("write `override {}`", replacement.text()),
    )
    .first(first, "the inherited query pair is set here".into())
}
/// Message 21 for a status arm, including an exact key under a class.
pub(crate) fn m21_inherited_arm(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        21,
        format!("status arm `{key}` is already set above"),
        "replacement requires `override`".into(),
        format!("write `override {}`", replacement.text()),
    )
    .first(first, "the inherited status arm is here".into())
}
/// Message 22 for an override without an inherited query pair.
pub(crate) fn m22_override_query(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no query pair `{key}` above to override"),
        "nothing is set above for this key".into(),
        format!("write `{}`", replacement.text()),
    )
}
/// Message 22 for an override without an inherited arm.
pub(crate) fn m22_override_arm(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no status arm `{key}` above to override"),
        "nothing is set above for this key".into(),
        format!("write `{}`", replacement.text()),
    )
}
/// Message 22 for an edit in a body, which never inherits entries.
pub(crate) fn m22_override_body(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no body entry `{key}` above to override"),
        "a body inherits no entries".into(),
        format!("write `{}`", replacement.text()),
    )
}
/// Message 23 for removal of an absent header entry.
pub(crate) fn m23_remove_header(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited header `{key}` to remove"),
        "this header is absent".into(),
        format!("delete `{}`", removal.text()),
    )
}
/// Message 23 for removal of an absent query entry.
pub(crate) fn m23_remove_query(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited query pair `{key}` to remove"),
        "this query pair is absent".into(),
        format!("delete `{}`", removal.text()),
    )
}
/// Message 23 for removal in a body, which never inherits entries.
pub(crate) fn m23_remove_body(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited body entry `{key}` to remove"),
        "a body inherits no entries".into(),
        format!("delete `{}`", removal.text()),
    )
}
/// Message 23 when an exact removal was intended to exclude a class member.
pub(crate) fn m23_remove_class_member(
    key: &str,
    inherited_class: &str,
    replacement: &Written,
) -> Message {
    message(
        23,
        format!("no inherited status arm `{key}` to remove"),
        format!("only the `{inherited_class}` class arm is set above"),
        format!("write `override {}`", replacement.text()),
    )
}
/// Message 24 for edits before a whole statement or block other than path.
pub(crate) fn m24_edited_statement(statement: &str, replacement: &Written) -> Message {
    message(
        24,
        format!("`{statement}` cannot be overridden or removed"),
        "`override` and `remove` stand inside a block, before an entry or an arm".into(),
        format!("write `{}`", replacement.text()),
    )
}
/// Message 29 for a star hole outside a path.
pub(crate) fn m29_star_outside_path(name: &str, replacement: &Written) -> Message {
    message(
        29,
        "a `{*…}` hole must be a whole segment".into(),
        format!("`{{*{name}}}` stands outside a path"),
        format!("write `{}` with a `{{{name}}}` hole", replacement.text()),
    )
}
/// Message 31 for a body-binding arm written by a part of a HEAD endpoint.
pub(crate) fn m31_head_part_body(part: &str, replacement: &Written, first: Span) -> Message {
    message(
        31,
        "a HEAD response has no readable body".into(),
        format!("the part `{part}` asks for the body"),
        format!("write `{}` without a body binding", replacement.text()),
    )
    .first(first, format!("the part `{part}` binds the body here"))
}
/// Message 33 for a problem type with no available reader.
pub(crate) fn m33_problem_reader(line: &Written, reader: &str) -> Message {
    message(
        33,
        "nothing says how this answer is read".into(),
        "no `read` on the client or the problem line".into(),
        format!("add `read {reader}` to `{}`", line.text()),
    )
}
/// Message 34 for a repeated answer field.
pub(crate) fn m34_duplicate_field(field: &str, line: &Written, first: Span) -> Message {
    message(
        34,
        "an arm binds body, headers or status".into(),
        format!("field `{field}` is written twice"),
        format!("write `{}` with each field once", line.text()),
    )
    .first(first, format!("the first `{field}` field is here"))
}
/// Message 34 for two fields given the same local binding.
pub(crate) fn m34_duplicate_binding(binding: &str, line: &Written, first: Span) -> Message {
    message(
        34,
        "an arm binds body, headers or status".into(),
        format!("name `{binding}` is bound twice"),
        format!("write `{}` with distinct binding names", line.text()),
    )
    .first(first, format!("the first `{binding}` binding is here"))
}
/// Message 37 for a child inside a part.
pub(crate) fn m37_part_child(child: &Written) -> Message {
    message(
        37,
        "a part cannot hold a child".into(),
        "`path`, `problem`, `middleware` and children belong to the tree".into(),
        format!("move `{}` into the scope that names the part", child.text()),
    )
}
/// Message 37 for a spread inside a part's signature.
pub(crate) fn m37_part_spread(spread: &Written) -> Message {
    message(
        37,
        "a part cannot hold a part spread".into(),
        "a part names no other part".into(),
        format!(
            "move `{}` into a scope or endpoint signature",
            spread.text()
        ),
    )
}
/// Message 37 for a spread in the client's signature.
pub(crate) fn m37_client_spread(spread: &Written) -> Message {
    message(
        37,
        "the client cannot name a part".into(),
        "parts are named by scopes and endpoints only".into(),
        format!(
            "move `{}` into a scope or endpoint signature",
            spread.text()
        ),
    )
}
/// Message 38 when two named parts contribute the same statement or key.
pub(crate) fn m38_two_parts(
    item: &str,
    first_part: &str,
    second_part: &str,
    first: Span,
) -> Message {
    message(38, format!("`{item}` is written in both parts `{first_part}` and `{second_part}`"), "the statements of a part count as this node's own".into(), format!("remove `{item}` from one part, or do not name both `..{first_part}` and `..{second_part}`"))
        .first(first, format!("the part `{first_part}` writes it here"))
}
/// Message 38 for a key contributed by a part and repeated by the node.
pub(crate) fn m38_part_key(kind: &str, key: &str, part: &str, first: Span) -> Message {
    message(
        38,
        format!("{kind} `{key}` is written in the part `{part}` and in this node"),
        "the statements of a part count as this node's own".into(),
        format!("remove this {kind}, or do not name `..{part}`"),
    )
    .first(first, format!("the part `{part}` writes `{key}` here"))
}
/// Message 38 for a key repeated in the same block, without an intervening part.
pub(crate) fn m38_duplicate_key(kind: &str, key: &str, line: &Written, first: Span) -> Message {
    message(
        38,
        format!("duplicate {kind} `{key}` in this block"),
        "a key is written once in one node".into(),
        format!("keep one `{}` entry", line.text()),
    )
    .first(first, format!("the first {kind} `{key}` is here"))
}
/// Message 38 for an input contributed by two different parts.
pub(crate) fn m38_two_part_inputs(
    name: &str,
    first_part: &str,
    second_part: &str,
    first: Span,
) -> Message {
    message(
        38,
        format!("input `{name}` is declared in both parts `{first_part}` and `{second_part}`"),
        "the inputs of a part count as this node's own".into(),
        format!(
            "rename one `{name}` input, or do not name both `..{first_part}` and `..{second_part}`"
        ),
    )
    .first(
        first,
        format!("the part `{first_part}` declares `{name}` here"),
    )
}
/// Message 38 for a part input colliding with the node's input.
pub(crate) fn m38_part_input(name: &str, part: &str, first: Span) -> Message {
    message(
        38,
        format!("input `{name}` is declared in the part `{part}` and in this node"),
        "the inputs of a part count as this node's own".into(),
        format!("rename one `{name}` input, or do not name `..{part}`"),
    )
    .first(first, format!("the part `{part}` declares this input here"))
}
/// Message 38 for a part named twice by one node.
pub(crate) fn m38_duplicate_spread(part: &str, first: Span) -> Message {
    message(
        38,
        format!("part `{part}` is named twice in this signature"),
        "a part is named once by one node".into(),
        format!("keep one `..{part}` spread"),
    )
    .first(first, format!("the first `..{part}` is here"))
}
/// Message 38 for body forms/functions that cannot be joined.
pub(crate) fn m38_body_conflict(part: &str, line: &Written, first: Span) -> Message {
    message(
        38,
        format!("`body` is written in the part `{part}` and in this endpoint"),
        "entry bodies join only when they name the same function".into(),
        format!("remove `{}`, or do not name `..{part}`", line.text()),
    )
    .first(first, format!("the part `{part}` writes its body here"))
}
/// Message 46 for Result, whose inherited arm bypasses success reading.
pub(crate) fn m46_result_arm(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        46,
        "an arm from above reads the body for a `Result` after the arrow".into(),
        "an inherited success arm bypasses the Result success reading rule".into(),
        format!(
            "write the arm here: `status {{ override {} }}`, or `status {{ remove {key} }}`",
            replacement.text()
        ),
    )
    .first(first, "this inherited success arm reads the body".into())
}
/// Message 46 for an exact Option success arm, rather than the 2xx class.
pub(crate) fn m46_option_exact_arm(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        46,
        "an arm from above reads the body for an `Option` after the arrow".into(),
        "a `null` in the body would be a `None`, with no word".into(),
        format!(
            "write the arm here: `status {{ override {} }}`, or `status {{ remove {key} }}`",
            replacement.text()
        ),
    )
    .first(
        first,
        format!("the inherited `{key}` arm reads the body here"),
    )
}
