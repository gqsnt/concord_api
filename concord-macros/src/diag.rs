//! Typed diagnostic catalogue. Every hole is a required Rust argument.
//!
//! Constructors format their arguments once; inserted author text is never
//! interpreted as a template. Constructors are shared by parsing, resolution, and validation.
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
    /// Replace an illustrative help with a repair justified by this input.
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
pub(crate) fn duplicate_child(_replacement: &str, name: &str, first: Span) -> Message {
    message(
        1,
        format!("duplicate child name `{name}`"),
        "already declared in this block".to_owned(),
        "give this endpoint a distinct name, or remove the duplicate declaration".into(),
    )
    .first(first, "the first child is here".to_owned())
}

/// Message 2: method conflict.
pub(crate) fn method_conflict(name: &str, replacement: &str, first: Span) -> Message {
    message(
        2,
        format!("two declarations generate method `{name}`"),
        "conflicts with the optional input".to_owned(),
        format!("rename this child to `{replacement}`"),
    )
    .first(first, "the optional input is declared here".to_owned())
}

/// Message 3: reserved input.
pub(crate) fn reserved_input(_replacement: &str, name: &str) -> Message {
    message(3, format!("`{name}` is reserved by Concord"), "reserved for generated methods".into(),
        "choose a distinct valid input name that conflicts with no owner, input or generated method, and update its existing references without changing wire names".into())
}

/// Message 4: keyword input.
pub(crate) fn keyword_input(replacement: &str, ty: &str, key: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name inputs".into(),
        "a Rust keyword or raw identifier".into(),
        format!("use `{replacement}: {ty}` and `query {{ \"{key}\" = {replacement}; }}`"),
    )
}

/// Message 5: invalid name.
pub(crate) fn invalid_name(replacement: &str) -> Message {
    message(
        5,
        "invalid API name".to_owned(),
        "expected lower snake case".to_owned(),
        format!(
            "rename it to `{replacement}` and update its references, retaining existing wire names"
        ),
    )
}

/// Message 6: invalid client.
pub(crate) fn invalid_client(replacement: &str) -> Message {
    message(
        6,
        "invalid client name".to_owned(),
        "expected upper camel case with a non-keyword module name".to_owned(),
        format!("write `client {replacement}`"),
    )
}

/// Message 7: owner scope.
pub(crate) fn owner_scope(_replacement: &str, first: Span) -> Message {
    message(
        7,
        "name conflicts with an input owner".into(),
        "the client or an enclosing scope already has this name".into(),
        "choose a distinct scope name and update every qualified reference to this scope's inputs"
            .into(),
    )
    .first(first, "this owner is declared here".into())
}

/// Message 8: generated collision.
pub(crate) fn generated_collision(
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
pub(crate) fn unused_input(name: &str) -> Message {
    message(9, format!("input `{name}` is never used"), "nothing in the description uses this input".into(),
        "use this input in the intended `path`, `base`, `query`, `headers`, `body`, or expression, or remove its declaration".into())
}

/// Message 10: borrowed input.
pub(crate) fn borrowed_input(name: &str, owned_type: &str) -> Message {
    message(
        10,
        "input types must own their data".to_owned(),
        "borrowed input type".to_owned(),
        format!("write `{name}: {owned_type}`; callers may still pass references"),
    )
}

/// Message 11: unknown input.
pub(crate) fn unknown_input(name: &str, _ty: &str) -> Message {
    message(11, format!("unknown input `{name}`"), "no input with this name".into(),
        "correct this input name, or declare the intended input in this signature and use every declared input".into())
}

/// Message 12: ancestor hole.
pub(crate) fn ancestor_hole(qualified: &str, name: &str, owner: &str) -> Message {
    message(
        12,
        "ancestor inputs require their owner's name".into(),
        owner_label(name, owner),
        format!("write `{{{qualified}}}`"),
    )
}

/// Message 13: optional url hole.
pub(crate) fn optional_url_hole(name: &str, ty: &str, first: Span) -> Message {
    message(
        13,
        "a URL input cannot be optional".to_owned(),
        "an optional input cannot fill this placeholder".to_owned(),
        format!("declare `{name}: {ty}`, without `?`"),
    )
    .first(first, "this input is marked optional here".to_owned())
}

/// Message 14: input attribute.
pub(crate) fn input_attribute(name: &str, key: &str) -> Message {
    message(
        14,
        "unsupported attribute on an input".into(),
        "an input takes doc comments only".into(),
        format!("use `query {{ \"{key}\" = {name}; }}`"),
    )
}

/// Message 15: missing method.
pub(crate) fn missing_method(line: &Written) -> Message {
    let line = line.text();
    message(
        15,
        "expected an HTTP method before the endpoint name".to_owned(),
        "endpoint method is missing".to_owned(),
        format!("write `GET {line}`"),
    )
}

/// Message 16: after child.
pub(crate) fn after_child(line: &Written) -> Message {
    let line = line.text();
    message(
        16,
        "statements must precede children".to_owned(),
        "statement appears after a child".to_owned(),
        format!("move `{line}` before the first part or child"),
    )
}

/// Message 17: duplicate statement.
pub(crate) fn duplicate_statement(line: &Written, statement: &str, first: Span) -> Message {
    let _ = line;
    message(
        17,
        format!("duplicate `{statement}` statement"),
        format!("this node already declares `{statement}`"),
        format!("keep one `{statement}` declaration with its intended value"),
    )
    .first(first, format!("the first `{statement}` statement is here"))
}

/// Message 18: requires endpoint.
pub(crate) fn requires_endpoint(_line: &Written, statement: &str, node: &str) -> Message {
    message(18, format!("`{statement}` requires an endpoint"), format!("`{statement}` cannot be declared on a {node}"),
        format!("move `{statement}` to an endpoint, and combine it with any `{statement}` declared there or supplied by a part"))
}

/// Message 19: invalid method.
pub(crate) fn invalid_method(line: &Written) -> Message {
    let line = line.text();
    message(
        19,
        "invalid method name".to_owned(),
        "a method name is an HTTP token, with no space".to_owned(),
        format!("write `{line}`"),
    )
}

/// Message 20: problem placement.
pub(crate) fn problem_placement(_line: &Written) -> Message {
    message(
        20,
        "`problem` is allowed only on the client".into(),
        "problem types are declared on the client".into(),
        "keep one `problem` declaration, with the intended type, in the client block".into(),
    )
}

/// Message 21: inherited header.
pub(crate) fn inherited_header(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        21,
        format!("header `{key}` is already set by an ancestor"),
        "replacement requires `override`".into(),
        format!("write `override {}`", replacement.text()),
    )
    .first(first, "the inherited header is set here".into())
}

/// Message 22: override header.
pub(crate) fn override_header(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no inherited header `{key}` to override"),
        "no inherited value for this key".into(),
        format!("write `{}`", replacement.text()),
    )
}

/// Message 23: remove arm.
pub(crate) fn remove_arm(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited status arm `{key}` to remove"),
        "this arm is absent".into(),
        format!("delete `{}`", removal.text()),
    )
}

/// Message 24: edit path.
pub(crate) fn edit_path(line: &Written) -> Message {
    let line = line.text();
    message(
        24,
        "`override` and `remove` cannot be used on `path`".to_owned(),
        "each path adds segments".to_owned(),
        format!("write `{line}` and change the ancestor's path if needed"),
    )
}

/// Message 25: empty path.
pub(crate) fn empty_path(_replacement: &Written) -> Message {
    message(25, "path segments must be nonempty".into(), "empty path or a leading or doubled slash".into(),
        "remove leading or repeated slashes while keeping the path's segments and placeholders; omit an empty path statement".into())
}

/// Message 26: dot path.
pub(crate) fn dot_path(segment: &str, _replacement: &Written) -> Message {
    message(26, "dot segments are forbidden in paths".into(), format!("`{segment}` is a dot segment"),
        "write the intended path without dot segments, preserving the other segments and placeholders".into())
}

/// Message 27: path character.
pub(crate) fn path_character(_declaration: &Written, _replacement: &Written) -> Message {
    message(27, "forbidden character in fixed path text".into(), "this character is not allowed in fixed text".into(),
        "move the forbidden fixed text into an input placeholder at this position; keep the other path text and placeholders".into())
}

/// Message 27 for forbidden fixed text in a base.
pub(crate) fn base_character() -> Message {
    message(27, "forbidden character in fixed base text".into(), "this character is not allowed in fixed text".into(),
        "move the forbidden fixed text into an input placeholder at this position; keep the rest of this base".into())
}

/// Message 28: path query.
pub(crate) fn path_query(replacement: &Written) -> Message {
    message(
        28,
        "a path cannot contain a query".into(),
        "query begins here".into(),
        format!("write `{}`", replacement.text()),
    )
}

/// Message 29: star segment.
pub(crate) fn star_segment(name: &str, adjacent: &str, replacement: &Written) -> Message {
    message(
        29,
        "a `{*…}` placeholder must be a whole path segment".into(),
        format!("`{{*{name}}}` shares its segment with `{adjacent}`"),
        format!(
            "write `{}`; a `{{{name}}}` placeholder may share its segment",
            replacement.text()
        ),
    )
}

/// Message 30: header name.
pub(crate) fn header_name(_replacement: &Written) -> Message {
    message(30, "invalid header name".into(), "expected a nonempty ASCII HTTP token".into(),
        "choose the intended nonempty ASCII HTTP header name, preserving this entry's value or removal".into())
}

/// Message 31: head body.
pub(crate) fn head_body(_replacement: &Written) -> Message {
    message(31, "a HEAD response has no readable body".into(), "this arm binds `body`".into(),
        "remove the `body` binding and update this arm's expression so it no longer uses the response body".into())
}

/// Message 32: head success.
pub(crate) fn head_success(_replacement: &Written) -> Message {
    message(32, "a HEAD endpoint with a return type requires a success status arm".into(), "no arm matches a 2xx status".into(),
        "add a `status` block with a success arm that returns the declared type without binding `body`".into())
}

/// Message 33: missing reader.
pub(crate) fn missing_reader(_replacement: &Written, _import: &Written) -> Message {
    message(
        33,
        "no reader applies to this response body".into(),
        "no `read` is declared here or inherited".into(),
        "declare the intended reader with `read` on this endpoint or an ancestor".into(),
    )
}

/// Message 34: arm field.
pub(crate) fn arm_field(field: &str, _replacement: &Written, _binding: &Written) -> Message {
    message(
        34,
        format!("unknown field `{field}` in a status arm"),
        "expected `body`, `headers`, or `status`".into(),
        "choose the intended field and keep the binding that this arm's expression uses".into(),
    )
}

/// Message 35: unknown part.
pub(crate) fn unknown_part(name: &str, _replacement: &str) -> Message {
    message(
        35,
        format!("no part named `{name}`"),
        "not declared in the client block".into(),
        "declare the intended part in the client block, or correct this part name".into(),
    )
}

/// Message 36: unused part.
pub(crate) fn unused_part(name: &str) -> Message {
    message(
        36,
        format!("part `{name}` is never used"),
        format!("no scope or endpoint names `..{name}`"),
        "apply this part to a compatible scope or endpoint, or remove its declaration".into(),
    )
}

/// Message 37: part statement.
pub(crate) fn part_statement(statement: &str, _line: &Written) -> Message {
    let (label, help) = match statement {
        "path" => ("`path` is declared on the client, a scope, or an endpoint".into(),
            "move `path` to the node that needs it, and combine it with any `path` already there"),
        "problem" => ("`problem` is declared on the client".into(),
            "keep one `problem` declaration, with the intended type, in the client block"),
        _ => (format!("`{statement}` is declared on the client"),
            "construct this middleware in the client block using required client inputs; qualify or move the values it needs"),
    };
    message(
        37,
        format!("a part cannot contain `{statement}`"),
        label,
        help.into(),
    )
}

/// Message 38: part conflict.
pub(crate) fn part_conflict(statement: &str, part: &str, node: &str, first: Span) -> Message {
    message(
        38,
        format!("`{statement}` is declared both in part `{part}` and in this {node}"),
        "the statements of a part count as this node's own".to_owned(),
        "keep one declaration with the intended value or body function and every input placement; use compatible parts, or remove inputs that become unused".into(),
    )
    .first(first, format!("part `{part}` declares it here"))
}

/// Message 39: no arrow.
pub(crate) fn no_arrow(_arrow: &Written, _removal: &Written, first: Span) -> Message {
    message(39, "a status arm reads the body, but this endpoint has no return type".into(), "no return type (`->`) is declared".into(),
        "declare the intended return type with `->`, or stop the arms that apply here from binding `body`".into())
        .first(first, "this arm reads the body".into())
}

/// Message 40: optional middleware.
pub(crate) fn optional_middleware(name: &str, ty: &str) -> Message {
    message(
        40,
        format!("middleware cannot use the optional client input `{name}`"),
        "middleware is constructed in `new`, before optional inputs are set".to_owned(),
        format!("declare `{name}: {ty}`, without `?`"),
    )
}

/// Message 41: short header.
pub(crate) fn short_header(name: &str, key: &str) -> Message {
    message(
        41,
        "a short entry in `headers` cannot use a name with `_`".to_owned(),
        format!("this entry would send a header named `{name}`"),
        format!("write `\"{key}\" = {name};`"),
    )
}

/// Message 42: host header.
pub(crate) fn host_header(_replacement: &Written) -> Message {
    message(
        42,
        "`host` cannot be set in `headers`".into(),
        "the host comes from the `base`".into(),
        "remove this host header and put its intended host in the base".into(),
    )
}

/// Message 43: missing base.
pub(crate) fn missing_base(_replacement: &Written) -> Message {
    message(
        43,
        "no `base` applies to this endpoint".into(),
        "no `base` is declared here or inherited".into(),
        "set the intended base URL on this endpoint or an ancestor".into(),
    )
}

/// Message 44: class override.
pub(crate) fn class_override(
    class: &str,
    key: &str,
    replacement: &Written,
    first: Span,
) -> Message {
    message(
        44,
        format!("`override {class}` leaves the inherited arm `{key}` in place"),
        format!("an inherited exact arm still matches `{key}`"),
        format!(
            "also write `override {}`, or `remove {key}`",
            replacement.text()
        ),
    )
    .first(first, format!("the inherited `{key}` arm is here"))
}

/// Message 45: status key.
pub(crate) fn status_key(key: &str, _exact: &str, _class: &str) -> Message {
    message(
        45,
        format!("invalid status key `{key}`"),
        "a key is 100 to 599, or 1xx to 5xx".into(),
        "choose the intended exact status from 100 to 599 or class from 1xx to 5xx".into(),
    )
}

/// Message 46: option arm.
pub(crate) fn option_arm(replacement: &Written, first: Span) -> Message {
    option_exact_arm("2xx", replacement, first)
}

/// Message 10's separate case: Self cannot refer to the enclosing type.
pub(crate) fn self_type() -> Message {
    message(
        10,
        "`Self` has no meaning in a description".into(),
        "`Self` is not the enclosing type here".into(),
        "replace `Self` with the path of the intended type".into(),
    )
}

/// Collected errors from all compiler passes; there is no numeric or unbound raise API.
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
    /// Whether any compiler pass has refused the description.
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
pub(crate) fn duplicate_part(name: &str, _replacement: &str, first: Span) -> Message {
    message(
        1,
        format!("duplicate part name `{name}`"),
        "already declared in this block".into(),
        "keep one declaration of this part and retain its intended inputs and statements".into(),
    )
    .first(first, "the first part is here".into())
}
/// Message 1 for an input written twice in one signature.
pub(crate) fn duplicate_input(name: &str, _replacement: &str, first: Span) -> Message {
    message(
        1,
        format!("duplicate input name `{name}`"),
        "already declared in this signature".into(),
        "remove the repeated input declaration, keeping its existing placements".into(),
    )
    .first(first, "the first input is here".into())
}
/// Message 3 for a child whose name would generate a reserved method.
pub(crate) fn reserved_child(name: &str, replacement: &str) -> Message {
    message(
        3,
        format!("`{name}` is reserved by Concord"),
        "reserved for generated methods".into(),
        format!("rename this child to `{replacement}`"),
    )
}
/// Message 4 for a keyword or raw scope name.
pub(crate) fn keyword_scope(line: &Written, replacement: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name scopes".into(),
        "a Rust keyword or raw identifier".into(),
        format!("rename the scope `{}` to `{replacement}`", line.text()),
    )
}
/// Message 4 for a keyword or raw endpoint name.
pub(crate) fn keyword_endpoint(line: &Written, replacement: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name endpoints".into(),
        "a Rust keyword or raw identifier".into(),
        format!("rename the endpoint `{}` to `{replacement}`", line.text()),
    )
}
/// Message 4 for a part name: parts use the grammar's same `name` production.
pub(crate) fn keyword_part(line: &Written, replacement: &str) -> Message {
    message(
        4,
        "keywords and raw identifiers cannot name parts".into(),
        "a Rust keyword or raw identifier".into(),
        format!("rename the part `{}` to `{replacement}`", line.text()),
    )
}
/// Message 7 for an input that hides an owner, retaining its wire key.
pub(crate) fn owner_input(_key: &str, _replacement: &str, first: Span) -> Message {
    message(7, "name conflicts with an input owner".into(),
        "this name is already used by the client or a scope".into(),
        "choose a distinct valid input name that conflicts with no owner, input or generated method, and update its existing references without changing wire names".into())
        .first(first, "this owner is declared here".into())
}

/// Message 11 for a reference to a scope's input from inside a part.
pub(crate) fn scope_input_in_part(name: &str, _declaration: &Written) -> Message {
    message(11, format!("unknown input `{name}`"), "a part sees its own inputs and client inputs only".into(),
        "move this placement to the scope or endpoint that owns the input; parts can use only their own inputs and client inputs".into())
}

/// Message 12 for an unqualified ancestor input used as a short entry.
pub(crate) fn short_entry(owner: &str, name: &str) -> Message {
    message(
        12,
        "ancestor inputs require their owner's name".into(),
        "a short entry names an input of this node".into(),
        format!("write `\"{name}\" = {owner}.{name};`"),
    )
}
/// Message 12 for an unqualified base reference.
pub(crate) fn ancestor_base(owner: &str, name: &str) -> Message {
    message(
        12,
        "ancestor inputs require their owner's name".into(),
        owner_label(name, owner),
        format!("write `base {owner}.{name};`"),
    )
}

/// Message 13 for an optional input named directly by base.
pub(crate) fn optional_base(declaration: &Written, first: Span) -> Message {
    message(
        13,
        "a URL input cannot be optional".into(),
        "an optional input cannot supply the base".into(),
        format!("declare `{}`, without `?`", declaration.text()),
    )
    .first(first, "this input is marked optional here".into())
}
/// Message 14 for an unsupported part attribute.
pub(crate) fn part_attribute(attribute: &Written, destination: &str) -> Message {
    message(
        14,
        "unsupported attribute on a part".into(),
        "a part takes doc comments only".into(),
        format!("put `{}` on {destination}", attribute.text()),
    )
}
/// Message 14 for an unsupported client attribute.
pub(crate) fn client_attribute(attribute: &Written, destination: &str) -> Message {
    message(
        14,
        "unsupported attribute on the client".into(),
        "a client accepts only doc comments and `#[cfg]`".into(),
        format!("put `{}` on {destination}", attribute.text()),
    )
}
/// Message 14 for an unsupported scope attribute.
pub(crate) fn scope_attribute(attribute: &Written) -> Message {
    message(
        14,
        "unsupported attribute on a scope".into(),
        "a scope accepts only doc comments, `#[cfg]`, `#[deprecated]`, and `#[doc(hidden)]`".into(),
        format!("remove `{}` from this scope", attribute.text()),
    )
}
/// Message 14 for an unsupported endpoint attribute.
pub(crate) fn endpoint_attribute(attribute: &Written) -> Message {
    message(
        14,
        "unsupported attribute on an endpoint".into(),
        "an endpoint accepts only doc comments, `#[cfg]`, `#[deprecated]`, and `#[doc(hidden)]`"
            .into(),
        format!("remove `{}` from this endpoint", attribute.text()),
    )
}
/// Message 14 for an attribute on a statement other than middleware docs.
pub(crate) fn statement_attribute(attribute: &Written, statement: &str) -> Message {
    message(
        14,
        format!("unsupported attribute on `{statement}`"),
        if statement == "middleware" {
            "`middleware` accepts only doc comments"
        } else {
            "this statement takes no attributes"
        }
        .into(),
        format!("remove `{}` from this statement", attribute.text()),
    )
}
/// Message 16's distinct ordering case: a statement after a part.
pub(crate) fn after_part(line: &Written) -> Message {
    message(
        16,
        "statements must precede parts".into(),
        "statement appears after a part".into(),
        format!("move `{}` before the first part", line.text()),
    )
}
/// Message 16 for a part after a child declaration.
pub(crate) fn part_after_child(line: &Written) -> Message {
    message(
        16,
        "parts must precede children".into(),
        "part appears after a child".into(),
        format!("move `{}` before the first child", line.text()),
    )
}
/// Message 18 at a spread whose part holds an endpoint-only statement.
pub(crate) fn part_requires_endpoint(statement: &str, _spread: &Written) -> Message {
    message(18, format!("`{statement}` requires an endpoint"), format!("a scope cannot name a part that contains `{statement}`"),
        "apply this part to the endpoints that need it, and preserve any inputs or placements needed by their scopes".into())
}

/// Message 20 for middleware outside the client, excluding parts (message 37).
pub(crate) fn middleware_placement(line: &Written) -> Message {
    message(
        20,
        "`middleware` is allowed only on the client".into(),
        "middleware is declared on the client".into(),
        format!("move `{}` into the client block", line.text()),
    )
}
/// Message 20 for a part declaration outside the client.
pub(crate) fn part_placement(line: &Written) -> Message {
    message(
        20,
        "`part` is allowed only on the client".into(),
        "parts are declared on the client".into(),
        format!("move `{}` into the client block", line.text()),
    )
}
/// Message 21 for an inherited query pair.
pub(crate) fn inherited_query(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        21,
        format!("query pair `{key}` is already set by an ancestor"),
        "replacement requires `override`".into(),
        format!("write `override {}`", replacement.text()),
    )
    .first(first, "the inherited query pair is set here".into())
}
/// Message 21 for a status arm, including an exact key under a class.
pub(crate) fn inherited_arm(key: &str, replacement: &Written, first: Span) -> Message {
    message(
        21,
        format!("an inherited arm already matches `{key}`"),
        "replacement requires `override`".into(),
        format!("write `override {}`", replacement.text()),
    )
    .first(first, "the inherited status arm is here".into())
}
/// Message 22 for an override without an inherited query pair.
pub(crate) fn override_query(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no inherited query pair `{key}` to override"),
        "no inherited value for this key".into(),
        format!("write `{}`", replacement.text()),
    )
}
/// Message 22 for an override without an inherited arm.
pub(crate) fn override_arm(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no inherited status arm `{key}` to override"),
        "no inherited arm matches this key".into(),
        format!("write `{}`", replacement.text()),
    )
}
/// Message 22 for an edit in a body, which never inherits entries.
pub(crate) fn override_body(key: &str, replacement: &Written) -> Message {
    message(
        22,
        format!("no inherited body entry `{key}` to override"),
        "a body inherits no entries".into(),
        format!("write `{}`", replacement.text()),
    )
}
/// Message 23 for removal of an absent header entry.
pub(crate) fn remove_header(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited header `{key}` to remove"),
        "this header is absent".into(),
        format!("delete `{}`", removal.text()),
    )
}
/// Message 23 for removal of an absent query entry.
pub(crate) fn remove_query(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited query pair `{key}` to remove"),
        "this query pair is absent".into(),
        format!("delete `{}`", removal.text()),
    )
}
/// Message 23 for removal in a body, which never inherits entries.
pub(crate) fn remove_body(key: &str, removal: &Written) -> Message {
    message(
        23,
        format!("no inherited body entry `{key}` to remove"),
        "a body inherits no entries".into(),
        format!("delete `{}`", removal.text()),
    )
}
/// Message 23 when an exact removal was intended to exclude a class member.
pub(crate) fn remove_class_member(
    key: &str,
    inherited_class: &str,
    _replacement: &Written,
) -> Message {
    message(23, format!("no inherited status arm `{key}` to remove"),
        format!("status `{key}` is covered by the inherited `{inherited_class}` arm"),
        format!("write an `override` arm for `{key}` with the intended fields and expression; an exact removal cannot exclude a status from a class"))
}

/// Message 24 for edits before a whole statement or block other than path.
pub(crate) fn edited_statement(statement: &str, _replacement: &Written) -> Message {
    let help = if matches!(statement, "query" | "headers" | "status") {
        "put `override` or `remove` on individual entries or status arms, resolving inherited keys and preserving the intended values and expressions".into()
    } else {
        format!("declare `{statement}` with the intended value, without `override` or `remove`; change ancestor declarations if needed")
    };
    message(
        24,
        format!("`override` and `remove` cannot be used on `{statement}`"),
        "edit keyword on a whole statement".into(),
        help,
    )
}

/// Message 29 for a star hole outside a path.
pub(crate) fn star_outside_path(name: &str, replacement: &Written) -> Message {
    message(
        29,
        "a `{*…}` placeholder is allowed only in `path`".into(),
        format!("`{{*{name}}}` is outside a `path` statement"),
        format!(
            "write `{}`, with a `{{{name}}}` placeholder",
            replacement.text()
        ),
    )
}
/// Message 33 for a problem type with no available reader.
pub(crate) fn problem_reader(_line: &Written, _reader: &str) -> Message {
    message(33, "no reader applies to this response body".into(), "no `read` on the client or in this `problem` declaration".into(),
        "declare a reader with `read` on the client, or after the problem type, before the semicolon".into())
}

/// Message 34 for a repeated answer field.
pub(crate) fn duplicate_field(field: &str, _line: &Written, first: Span) -> Message {
    message(
        34,
        format!("duplicate field `{field}` in a status arm"),
        format!("`{field}` is already bound in this arm"),
        "bind each field once; keep the bindings that this arm's expression uses".into(),
    )
    .first(first, format!("the first `{field}` field is here"))
}

/// Message 34 for two fields given the same local binding.
pub(crate) fn duplicate_binding(binding: &str, _line: &Written, first: Span) -> Message {
    message(
        34,
        format!("duplicate binding `{binding}` in a status arm"),
        format!("name `{binding}` is bound twice"),
        "give these fields distinct binding names and update their uses in this arm's expression"
            .into(),
    )
    .first(first, format!("the first `{binding}` binding is here"))
}

/// Message 37 for a child inside a part.
pub(crate) fn part_child(_child: &Written) -> Message {
    message(37, "a part cannot contain a child".into(), "children are declared in the client or a scope".into(),
        "move this child declaration into the client or an appropriate scope, preserving its inputs and references".into())
}

/// Message 37 for a spread inside a part's signature.
pub(crate) fn part_spread(_spread: &Written) -> Message {
    message(37, "a part cannot name another part".into(), "only a scope or an endpoint can name a part".into(),
        "move this part reference to a compatible scope or endpoint signature, preserving its inputs and placements".into())
}

/// Message 37 for a spread in the client's signature.
pub(crate) fn client_spread(_spread: &Written) -> Message {
    message(37, "the client cannot name a part".into(), "parts are named by scopes and endpoints only".into(),
        "move this part reference to a compatible scope or endpoint signature, preserving its inputs and placements".into())
}

/// Message 38 when two named parts contribute the same statement or key.
pub(crate) fn two_parts(item: &str, first_part: &str, second_part: &str, first: Span) -> Message {
    message(38, format!("`{item}` is declared in both parts `{first_part}` and `{second_part}`"), "the statements of a part count as this node's own".into(), "keep one declaration with the intended value or body function and every input placement; use compatible parts, or remove inputs that become unused".into())
        .first(first, format!("part `{first_part}` declares it here"))
}
/// Message 38 for a key contributed by a part and repeated by the node.
pub(crate) fn part_key(kind: &str, key: &str, part: &str, first: Span) -> Message {
    message(
        38,
        format!("{kind} `{key}` is declared both in part `{part}` and in this node"),
        "the statements of a part count as this node's own".into(),
        "keep one declaration with the intended value or body function and every input placement; use compatible parts, or remove inputs that become unused".into(),
    )
    .first(first, format!("part `{part}` declares `{key}` here"))
}
/// Message 38 for a key repeated in the same block, without an intervening part.
pub(crate) fn duplicate_key(kind: &str, key: &str, _line: &Written, first: Span) -> Message {
    message(
        38,
        format!("duplicate {kind} `{key}` in this block"),
        "a key is declared once in one node".into(),
        if kind == "status arm" { "keep one status arm or removal for this key; preserve needed input references and remove inputs that become unused" } else { "keep one entry or removal for this key; preserve needed input references and remove inputs that become unused" }.into(),
    )
    .first(first, format!("the first {kind} `{key}` is here"))
}
/// Message 38 for an input contributed by two different parts.
pub(crate) fn two_part_inputs(
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
pub(crate) fn part_input(name: &str, part: &str, first: Span) -> Message {
    message(
        38,
        format!("input `{name}` is declared in the part `{part}` and in this node"),
        "the inputs of a part count as this node's own".into(),
        format!("rename one `{name}` input, or do not name `..{part}`"),
    )
    .first(first, format!("the part `{part}` declares this input here"))
}
/// Message 38 for a part named twice by one node.
pub(crate) fn duplicate_spread(part: &str, first: Span) -> Message {
    message(
        38,
        format!("part `{part}` is named twice in this signature"),
        "a part is named once by one node".into(),
        format!("keep one `..{part}`"),
    )
    .first(first, format!("the first `..{part}` is here"))
}
/// Message 38 for body forms/functions that cannot be joined.
pub(crate) fn body_conflict(part: &str, _line: &Written, first: Span) -> Message {
    message(
        38,
        format!("`body` is declared both in part `{part}` and in this endpoint"),
        "only body entry blocks with the same function path can be merged".into(),
        "keep one declaration with the intended value or body function and every input placement; use compatible parts, or remove inputs that become unused".into(),
    )
    .first(first, format!("part `{part}` declares its body here"))
}
/// Message 46 for Result, whose inherited arm bypasses success reading.
pub(crate) fn result_arm(_key: &str, _replacement: &Written, first: Span) -> Message {
    message(
        46,
        "an inherited arm reads the body for a `Result` return type".into(),
        "this endpoint does not declare its own success arm".into(),
        "write the intended success arm on this endpoint with the wrapper the result needs; qualify its ancestor inputs with their owners and consolidate the existing status block".into(),
    )
    .first(first, "this inherited success arm reads the body".into())
}
/// Message 46 for an exact Option success arm, rather than the 2xx class.
pub(crate) fn option_exact_arm(key: &str, _replacement: &Written, first: Span) -> Message {
    message(
        46,
        "an inherited arm reads the body for an `Option` return type".into(),
        "`null` in the body could become `None` without an explicit arm on this endpoint".into(),
        "write the intended success arm on this endpoint with the wrapper the result needs; qualify its ancestor inputs with their owners and consolidate the existing status block".into(),
    )
    .first(
        first,
        format!("the inherited `{key}` arm reads the body here"),
    )
}

fn owner_label(name: &str, owner: &str) -> String {
    if owner == "client" {
        format!("`{name}` belongs to the client")
    } else {
        format!("`{name}` belongs to scope `{owner}`")
    }
}

/// Message 12 for an explicitly qualified short entry.
pub(crate) fn qualified_short_entry(owner: &str, name: &str) -> Message {
    message(
        12,
        "a short entry cannot name a qualified input".into(),
        "a short entry names an input of this node".into(),
        format!("write `\"{name}\" = {owner}.{name};`"),
    )
}

/// Message 31 for a part's body binding on a HEAD endpoint.
pub(crate) fn head_part_body(part: &str, replacement: &Written, first: Span) -> Message {
    head_body(replacement)
        .label(format!("part `{part}` binds `body`"))
        .first(first, format!("part `{part}` binds the body here"))
}
