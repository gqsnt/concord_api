//! The I3 reader probes' Rust parser (`tools/desirability/probes/README.md`), on `syn` instead of
//! a hand-written lexer: `syn` tokenises strings, raw and byte strings, chars, lifetimes and
//! nested comments as rustc does. Two modes:
//!
//! - default: the R3 answer contract ("R3 answer contract"), below;
//! - `--type`: R2 answer and truth types ("Scoring", R2). stdin: one type per line, hex-encoded
//!   UTF-8; stdout: one JSON line per type, flushed after each, `{"ok": <tree>, "judge": <why or
//!   null>}` when the text parses as one Rust type (`syn::Type`) or `{"err": "<message>"}`.
//!   `judge` (fix round 6) says why rustc's identity check (the scorer compiles
//!   `same(PhantomData::<*mut (TRUTH)>, PhantomData::<*mut (ANSWER)>)` in the declaration's module) cannot judge
//!   the type, or is null: `impl Trait`, a `_` placeholder, a `'_` lifetime, a reference with an
//!   elided lifetime outside `fn(..)`/`Fn(..)` arguments (inferred in the check, so not told
//!   apart from `'static`), a `dyn` bound with a lifetime other than `'static`, `Self`, a qualified
//!   path `<T as Trait>::Name` (rustc would compute the type for the answer: the rustdoc of a plain
//!   `send P;` client shows `<P as SendWith<T>>::Output`, whose resolution is what R2 asks), a
//!   macro, an attribute, syntax `syn` does not model, or an expression other than an integer
//!   literal (an array length or const argument, which could hold code). The tree: `{"k":"path",
//!   "segs":[..],"args":[..]}` (a leading `::` is a first segment `""`; raw identifiers unprefixed;
//!   the arguments are the last segment's), `{"k":"ref","mut":b,"lifetime":s|null,"elem":t}`,
//!   `{"k":"tuple","elems":[..]}`, `{"k":"slice","elem":t}`, `{"k":"array","elem":t,"len":s}`,
//!   `{"k":"infer"}`, a generic argument `{"k":"lifetime","text":s}`, and `{"k":"raw","text":s}`
//!   (the token text) for what the R2 comparison keeps as text: `dyn`/`impl` bounds, `!`,
//!   pointers, `fn` pointers, macros, qualified paths `<A as B>::C`, paths with generic or
//!   parenthesized arguments before the last segment, and const, associated-type and constraint
//!   arguments. `( T )` is `T`.
//!
//! Answer mode. stdin: one answer per line, hex-encoded UTF-8. stdout: one line per answer,
//! `<lexes>\t<form>\t<cut>\t<reason>`:
//! - `lexes`: `1` when the text is a sequence of balanced Rust token trees, else `0`;
//! - `form`: `value` (the answer is one expression ending with `.send().await`), `ok` (ending
//!   with `.send().await?`, the compatibility path) or `refused`;
//! - `cut`: the byte length of the code to compile (the answer without its trailing `;`);
//! - `reason`: why the answer was refused (empty otherwise).
//!
//! The answer is parsed the way rustc reads it inside the scorer's wrapper `{ <answer> }`
//! (`Block::parse_within`) and must be exactly one statement: an expression, with at most one
//! trailing `;`. After one optional outer `( .. )` or `{ .. }` holding exactly one expression and
//! one optional `?`, the expression must be `<receiver>.send().await`, and the receiver must hold
//! none of: `return`, `break`, `continue`, `?`, loops, blocks with statements (outside closure
//! bodies, where they only affect the closure); `.await`, `async`, `unsafe`, `yield`, `try`
//! blocks, items, attributes, macros other than `vec!` and `format!` (bare, or as `std::`,
//! `::std::` or `alloc::` paths; their arguments must parse as expressions under the same rules)
//! and syntax `syn` does not model (anywhere).

use std::io::{self, BufRead, Write};
use std::str::FromStr;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::ext::IdentExt;
use syn::parse::{ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Block, Expr, GenericArgument, PathArguments, Stmt, Token, Type};

const NOT_SEND: &str = "its value is not the `send().await` result (it must end with `.send().await`)";

/// A block that is one expression: empty, or a single expression statement without `;`.
fn one_expression(stmts: &[Stmt]) -> bool {
    match stmts {
        [] | [Stmt::Expr(_, None)] => true,
        [Stmt::Macro(m)] => m.semi_token.is_none(),
        _ => false,
    }
}

/// `vec![]`, `vec![a, b, ..]` or `vec![x; n]` as expressions.
fn vec_body(input: ParseStream) -> syn::Result<Vec<Expr>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let first: Expr = input.parse()?;
    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        let n: Expr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after `vec![x; n]`"));
        }
        return Ok(vec![first, n]);
    }
    let mut out = vec![first];
    while !input.is_empty() {
        input.parse::<Token![,]>()?;
        if input.is_empty() {
            break;
        }
        out.push(input.parse()?);
    }
    Ok(out)
}

fn path_text(p: &syn::Path) -> String {
    let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
    format!("{}{}", if p.leading_colon.is_some() { "::" } else { "" }, segs.join("::"))
}

/// `vec` or `format` when the macro path is one of the allowed spellings: bare (`vec!`), or the
/// standard library's (`std::vec!`, `::std::vec!`, `alloc::vec!`); None otherwise.
fn allowed_macro(p: &syn::Path) -> Option<&'static str> {
    if p.segments.iter().any(|s| !s.arguments.is_none()) {
        return None;
    }
    let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
    let name = match segs.last().map(String::as_str) {
        Some("vec") => "vec",
        Some("format") => "format",
        _ => return None,
    };
    let owner: Vec<&str> = segs[..segs.len() - 1].iter().map(String::as_str).collect();
    match (p.leading_colon.is_some(), owner.as_slice()) {
        (false, []) | (_, ["std"]) | (false, ["alloc"]) => Some(name),
        _ => None,
    }
}

/// The receiver rules. `closures` counts the closure bodies around the node being visited; every
/// distinct violation found in the whole receiver is reported, in source order.
struct Check {
    closures: usize,
    reasons: Vec<String>,
}

impl Check {
    fn refuse(&mut self, why: impl Into<String>) {
        let why = why.into();
        if !self.reasons.contains(&why) {
            self.reasons.push(why);
        }
    }
}

impl<'ast> Visit<'ast> for Check {
    fn visit_expr(&mut self, e: &'ast Expr) {
        let outside = self.closures == 0;
        let why = match e {
            Expr::Await(_) => Some("an `.await` other than the final `.send().await`"),
            Expr::Async(_) => Some("an `async` block"),
            Expr::Unsafe(_) => Some("an `unsafe` block"),
            Expr::Yield(_) => Some("`yield`"),
            Expr::TryBlock(_) => Some("a `try` block"),
            Expr::Verbatim(_) => Some("syntax the parser does not model"),
            Expr::Return(_) if outside => Some("`return` outside a closure"),
            Expr::Break(_) if outside => Some("`break` outside a closure"),
            Expr::Continue(_) if outside => Some("`continue` outside a closure"),
            Expr::Try(_) if outside => Some("a `?` other than an optional final one (outside a closure)"),
            Expr::Loop(_) | Expr::While(_) | Expr::ForLoop(_) if outside => Some("a loop outside a closure"),
            Expr::Closure(c) => {
                if c.asyncness.is_some() {
                    self.refuse("an `async` closure");
                }
                self.closures += 1;
                visit::visit_expr_closure(self, c);
                self.closures -= 1;
                return;
            }
            _ => None,
        };
        if let Some(w) = why {
            self.refuse(w);
        }
        visit::visit_expr(self, e); // the children too, so that every violation is reported
    }

    fn visit_block(&mut self, b: &'ast Block) {
        if self.closures == 0 && !one_expression(&b.stmts) {
            self.refuse("a block with statements outside a closure (the answer must be one expression)");
        }
        visit::visit_block(self, b);
    }

    fn visit_stmt(&mut self, s: &'ast Stmt) {
        if let Stmt::Expr(Expr::Verbatim(t), Some(_)) = s {
            if t.is_empty() {
                return; // a stray `;` in a closure body
            }
        }
        visit::visit_stmt(self, s);
    }

    fn visit_item(&mut self, _: &'ast syn::Item) {
        self.refuse("an item (`fn`, `impl`, `struct`, `use`, ..)");
    }

    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
        self.refuse("an attribute");
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let parsed = match allowed_macro(&m.path) {
            Some("vec") => m.parse_body_with(vec_body),
            Some("format") => m
                .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                .map(|p| p.into_iter().collect()),
            _ => {
                return self.refuse(format!(
                    "the macro `{}!` (only `vec!` and `format!` are allowed, bare or as `std::`, `::std::` or `alloc::` paths)",
                    path_text(&m.path)
                ));
            }
        };
        let name = path_text(&m.path);
        match parsed {
            Ok(exprs) => {
                for e in &exprs {
                    self.visit_expr(e);
                }
            }
            Err(err) => self.refuse(format!("`{name}!` arguments that do not parse as expressions ({err})")),
        }
    }
}

/// Finds any attribute (`#[..]`, also on the outer layers the receiver check does not visit).
struct Attrs(bool);

impl<'ast> Visit<'ast> for Attrs {
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
        self.0 = true;
    }
}

/// (form, byte length of the code to compile) or the refusal.
fn form(code: &str) -> Result<(&'static str, usize), String> {
    let stmts = Block::parse_within
        .parse_str(code)
        .map_err(|e| format!("does not parse as Rust (one expression): {e}"))?;
    let (e, semi) = match stmts.as_slice() {
        [Stmt::Expr(e, semi)] => (e, semi),
        [] => return Err("no expression".into()),
        [Stmt::Local(_)] => return Err("a `let` statement, not one expression".into()),
        [Stmt::Item(_)] => return Err("an item, not one expression".into()),
        [Stmt::Macro(_)] => return Err(NOT_SEND.into()),
        _ => return Err("more than one statement (the answer must be one expression)".into()),
    };
    if let Expr::Verbatim(_) = e {
        return Err("no expression".into());
    }
    let mut attrs = Attrs(false);
    attrs.visit_expr(e);
    if attrs.0 {
        return Err("an attribute".into());
    }
    let cut = match semi {
        Some(s) => s.spans[0].byte_range().start,
        None => code.len(),
    };
    // one optional outer ( .. ) or { .. } holding exactly one expression, one optional `?`
    let (mut cur, mut wrapped, mut tried) = (e, false, false);
    loop {
        match cur {
            Expr::Paren(p) if !wrapped => {
                wrapped = true;
                cur = &p.expr;
            }
            Expr::Block(b) if !wrapped && b.label.is_none() => match b.block.stmts.as_slice() {
                [Stmt::Expr(inner, None)] => {
                    wrapped = true;
                    cur = inner;
                }
                _ => return Err("an outer block that does not hold exactly one expression".into()),
            },
            Expr::Try(t) if !tried => {
                tried = true;
                cur = &t.expr;
            }
            _ => break,
        }
    }
    let Expr::Await(a) = cur else { return Err(NOT_SEND.into()) };
    let Expr::MethodCall(m) = &*a.base else { return Err(NOT_SEND.into()) };
    if m.method != "send" || !m.args.is_empty() || m.turbofish.is_some() {
        return Err(NOT_SEND.into());
    }
    let mut check = Check { closures: 0, reasons: Vec::new() };
    check.visit_expr(&m.receiver);
    if check.reasons.is_empty() {
        Ok((if tried { "ok" } else { "value" }, cut))
    } else {
        Err(check.reasons.join("; "))
    }
}

/// (lexes, form, cut, reason) of one answer.
fn verdict(code: &str) -> (bool, &'static str, usize, String) {
    let lexes = TokenStream::from_str(code).is_ok();
    match form(code) {
        Ok((f, cut)) => (lexes, f, cut, String::new()),
        Err(why) => (lexes, "refused", code.len(), why),
    }
}

// ---------------------------------------------------------------- `--type`: R2 types

/// A JSON string literal.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn raw(t: &impl ToTokens) -> String {
    format!(r#"{{"k":"raw","text":{}}}"#, json_str(&t.to_token_stream().to_string()))
}

/// The JSON tree of a type (the module documentation lists the node kinds).
fn type_tree(t: &Type) -> String {
    match t {
        Type::Paren(p) => type_tree(&p.elem),
        Type::Group(g) => type_tree(&g.elem),
        Type::Path(p) if p.qself.is_none() => path_tree(&p.path).unwrap_or_else(|| raw(t)),
        Type::Reference(r) => format!(
            r#"{{"k":"ref","mut":{},"lifetime":{},"elem":{}}}"#,
            r.mutability.is_some(),
            r.lifetime.as_ref().map_or("null".to_string(), |l| json_str(&l.to_string())),
            type_tree(&r.elem)
        ),
        Type::Tuple(tp) => {
            let elems: Vec<String> = tp.elems.iter().map(type_tree).collect();
            format!(r#"{{"k":"tuple","elems":[{}]}}"#, elems.join(","))
        }
        Type::Slice(s) => format!(r#"{{"k":"slice","elem":{}}}"#, type_tree(&s.elem)),
        Type::Array(a) => format!(
            r#"{{"k":"array","elem":{},"len":{}}}"#,
            type_tree(&a.elem),
            json_str(&a.len.to_token_stream().to_string())
        ),
        Type::Infer(_) => r#"{"k":"infer"}"#.to_string(),
        _ => raw(t), // `dyn`/`impl` bounds, `!`, pointers, `fn` pointers, macros, `<A as B>::C`
    }
}

/// A path whose generic arguments, if any, are on its last segment; None otherwise (kept as text).
fn path_tree(p: &syn::Path) -> Option<String> {
    let n = p.segments.len();
    let mut segs: Vec<String> = Vec::new();
    if p.leading_colon.is_some() {
        segs.push(json_str(""));
    }
    let mut args: Vec<String> = Vec::new();
    for (i, s) in p.segments.iter().enumerate() {
        segs.push(json_str(&s.ident.unraw().to_string()));
        match &s.arguments {
            PathArguments::None => {}
            PathArguments::AngleBracketed(a) if i + 1 == n => {
                args = a.args.iter().map(arg_tree).collect();
            }
            _ => return None,
        }
    }
    Some(format!(r#"{{"k":"path","segs":[{}],"args":[{}]}}"#, segs.join(","), args.join(",")))
}

fn arg_tree(a: &GenericArgument) -> String {
    match a {
        GenericArgument::Type(t) => type_tree(t),
        GenericArgument::Lifetime(l) => format!(r#"{{"k":"lifetime","text":{}}}"#, json_str(&l.to_string())),
        other => raw(other), // const, associated type or const, constraint
    }
}

/// What rustc's identity check cannot judge in a type (fix round 6; the module documentation
/// lists the cases). The check compiles `same(PhantomData::<*mut (TRUTH)>, PhantomData::<*mut
/// (ANSWER)>)` inside a function body, where `_`, `'_` and elided reference lifetimes are inferred
/// (so they would match anything, even behind the invariant marker), and where an array length or
/// const argument is code.
#[derive(Default)]
struct Judge {
    why: Vec<&'static str>,
    late_bound: usize, // depth inside `fn(..)` / `Fn(..)` arguments, where elided lifetimes are late-bound
}

impl Judge {
    fn refuse(&mut self, why: &'static str) {
        if !self.why.contains(&why) {
            self.why.push(why);
        }
    }
}

impl<'ast> Visit<'ast> for Judge {
    fn visit_type(&mut self, t: &'ast Type) {
        match t {
            Type::ImplTrait(_) => self.refuse("`impl Trait`"),
            Type::Infer(_) => self.refuse("a `_` placeholder (inferred)"),
            Type::Macro(_) => self.refuse("a macro"),
            Type::Verbatim(_) => self.refuse("syntax syn does not model"),
            Type::Reference(r) if r.lifetime.is_none() && self.late_bound == 0 => {
                self.refuse("a reference with an elided lifetime (inferred)")
            }
            _ => {}
        }
        visit::visit_type(self, t);
    }

    fn visit_type_trait_object(&mut self, o: &'ast syn::TypeTraitObject) {
        if o.bounds.iter().any(|b| matches!(b, syn::TypeParamBound::Lifetime(l) if l.ident != "static")) {
            self.refuse("a `dyn` bound with a lifetime other than `'static`");
        }
        visit::visit_type_trait_object(self, o);
    }

    fn visit_type_param_bound(&mut self, b: &'ast syn::TypeParamBound) {
        if matches!(b, syn::TypeParamBound::Verbatim(_)) {
            self.refuse("syntax syn does not model");
        }
        visit::visit_type_param_bound(self, b);
    }

    fn visit_type_fn_ptr(&mut self, f: &'ast syn::TypeFnPtr) {
        self.late_bound += 1;
        visit::visit_type_fn_ptr(self, f);
        self.late_bound -= 1;
    }

    fn visit_parenthesized_generic_arguments(&mut self, p: &'ast syn::ParenthesizedGenericArguments) {
        self.late_bound += 1;
        visit::visit_parenthesized_generic_arguments(self, p);
        self.late_bound -= 1;
    }

    fn visit_qself(&mut self, q: &'ast syn::QSelf) {
        self.refuse("a qualified path `<T as Trait>::Name` (rustc would compute the type)");
        visit::visit_qself(self, q);
    }

    fn visit_lifetime(&mut self, l: &'ast syn::Lifetime) {
        if l.ident == "_" {
            self.refuse("a `'_` lifetime (inferred)");
        }
    }

    fn visit_ident(&mut self, i: &'ast proc_macro2::Ident) {
        if i == "Self" {
            self.refuse("`Self`");
        }
    }

    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
        self.refuse("an attribute");
    }

    fn visit_expr(&mut self, e: &'ast Expr) {
        // array lengths, const arguments: an integer literal, nothing that runs (not visited further)
        let literal = matches!(e, Expr::Lit(l) if l.attrs.is_empty() && matches!(l.lit, syn::Lit::Int(_)));
        if !literal {
            self.refuse("an expression other than an integer literal");
        }
    }
}

/// Why rustc's identity check cannot judge `t`, or None.
fn judge(t: &Type) -> Option<String> {
    let mut j = Judge::default();
    j.visit_type(t);
    if j.why.is_empty() { None } else { Some(j.why.join("; ")) }
}

/// One JSON line for a type text: `{"ok": <tree>, "judge": <why or null>}` or `{"err": "<why>"}`.
fn type_verdict(text: &str) -> String {
    match syn::parse_str::<Type>(text) {
        Ok(t) => format!(r#"{{"ok":{},"judge":{}}}"#, type_tree(&t), judge(&t).map_or("null".to_string(), |w| json_str(&w))),
        Err(e) => format!(r#"{{"err":{}}}"#, json_str(&format!("does not parse as a Rust type: {e}"))),
    }
}

fn decode_hex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

fn run_types() {
    let stdin = io::stdin();
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.expect("reading stdin");
        let verdict = match decode_hex(line.trim()) {
            Some(text) => type_verdict(&text),
            None => format!(r#"{{"err":{}}}"#, json_str("the type is not hex-encoded UTF-8")),
        };
        writeln!(out, "{verdict}").expect("writing stdout");
        out.flush().expect("writing stdout"); // one reply per request: the caller keeps the process
    }
}

fn run() {
    if std::env::args().nth(1).as_deref() == Some("--type") {
        return run_types();
    }
    let stdin = io::stdin();
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.expect("reading stdin");
        let (lexes, form, cut, why) = match decode_hex(line.trim()) {
            Some(code) => verdict(&code),
            None => (false, "refused", 0, "the answer is not hex-encoded UTF-8".to_string()),
        };
        writeln!(out, "{}\t{}\t{}\t{}", u8::from(lexes), form, cut, why.replace(['\t', '\n', '\r'], " "))
            .expect("writing stdout");
    }
    out.flush().expect("writing stdout");
}

fn main() {
    // deeply nested answers recurse deeply in the parser: give it a large stack
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)
        .expect("starting the checker thread")
        .join()
        .expect("the checker thread panicked");
}

#[cfg(test)]
mod tests {
    use super::verdict;

    fn form(code: &str) -> &'static str {
        verdict(code).1
    }

    #[test]
    fn review5_raw_byte_string_early_return_is_refused() {
        let code = r####"(br#"""#, api.holidays("x&y=z",24).prepare()?.send_raw().await?, return Ok(()), br#"""#, api).4.holidays("x&y=z",24).send().await"####;
        let (lexes, f, _, why) = verdict(code);
        assert!(lexes);
        assert_eq!(f, "refused", "{why}");
        assert!(why.contains("`return` outside a closure"), "{why}");
        assert!(why.contains("a `?` other than"), "{why}");
        assert!(why.contains("an `.await` other than"), "{why}");
    }

    #[test]
    fn contract_forms() {
        assert_eq!(form(r#"api.holidays("x&y=z", 24).send().await"#), "value");
        assert_eq!(form(r#"api.holidays("x&y=z", 24).send().await;"#), "value");
        assert_eq!(form(r#"api.holidays("x&y=z", 24).send().await?"#), "ok");
        assert_eq!(form(r#"(api.holidays("x", 24).send().await)"#), "value");
        assert_eq!(form(r#"{ api.holidays("x", 24).send().await }"#), "value");
        assert_eq!(form("(std::ops::Range { start: 0usize, end: vec![0u8; 2].len() }, api).1.list().send().await"), "value");
        assert_eq!(form("(move || -> Option<_> { let n: usize = \"2\".parse().ok()?; if n == 0 { return None; } Some(api) })().unwrap().list().send().await"), "value");
        assert_eq!(form(r#"api.search(format!("{}-{}", 1, 2)).send().await"#), "value");
    }

    #[test]
    fn refusals() {
        for code in [
            "let r = api.list().send().await; r",
            "api.list().send().await.ok()",
            "api.list().prepare()?.send_raw().await?",
            "api.list().prepare()?.send().await",
            "api.list().send().await?; return Ok(()); api.list().send().await",
            "match 1 { _ => api.list().send().await }",
            "(api.list(), return Ok(())).0.send().await",
            "api.list(loop { break 1 }).send().await",
            "api.list({ let x = 1; x }).send().await",
            "api.list(async { 1 }).send().await",
            "api.list(unsafe { 1 }).send().await",
            "api.list(todo!()).send().await",
            "api.list(vec![return Ok(()); 1]).send().await",
            "api.list(|| async { 1 }).send().await",
            "api.list(|| x.await).send().await",
            "api.list(|| { fn f() {} }).send().await",
            "api.list(#[cfg(any())] 1).send().await",
            "(api.list().send().await, 1)",
            "((api.list().send().await))",
            "api.list().send().await }",
            "api.list().send().await;;",
            "",
        ] {
            assert_eq!(form(code), "refused", "{code}");
        }
        assert!(!verdict("api.list().send().await }").0);
        assert!(!verdict(r#"api.list("unterminated).send().await"#).0);
    }

    #[test]
    fn standard_library_macro_paths() {
        // review 6, finding 3: reader-compliant spellings of `vec!` and `format!`
        for code in [
            r#"api.search(std::format!("{}-{}", 1, 2)).send().await"#,
            r#"api.search(::std::format!("{}", 1)).send().await"#,
            r#"api.search(alloc::format!("{}", 1)).send().await"#,
            "api.ids(std::vec![1, 2]).send().await",
            "api.ids(::std::vec![0u8; 2]).send().await",
            "api.ids(alloc::vec![1]).send().await",
            r#"(std::format!("{}", 1), ::std::vec![0u8; 2], api).2.list().send().await"#,
        ] {
            assert_eq!(form(code), "value", "{code}");
        }
        for code in [
            r#"api.search(core::format!("{}", 1)).send().await"#,
            r#"api.search(std::fmt::format!("{}", 1)).send().await"#,
            "api.ids(std::vec::vec![1]).send().await",
            "api.ids(::alloc::vec![1]).send().await",
            "api.ids(made_up::vec![1]).send().await",
            r#"api.search(std::println!("{}", 1)).send().await"#,
            "api.ids(std::vec![return Ok(()); 1]).send().await", // same argument checks
            "api.ids(alloc::vec![x.await]).send().await",
        ] {
            assert_eq!(form(code), "refused", "{code}");
        }
        let why = verdict(r#"api.search(std::println!("{}", 1)).send().await"#).3;
        assert!(why.contains("the macro `std::println!`"), "{why}");
    }

    use super::type_verdict;

    #[test]
    fn type_trees() {
        assert_eq!(
            type_verdict("Result<HolidayResponse, Error>"),
            r#"{"ok":{"k":"path","segs":["Result"],"args":[{"k":"path","segs":["HolidayResponse"],"args":[]},{"k":"path","segs":["Error"],"args":[]}]},"judge":null}"#
        );
        assert_eq!(
            type_verdict("::std::result::Result<(), concord::Error,>"),
            r#"{"ok":{"k":"path","segs":["","std","result","Result"],"args":[{"k":"tuple","elems":[]},{"k":"path","segs":["concord","Error"],"args":[]}]},"judge":null}"#
        );
        assert_eq!(
            type_verdict("&'static mut [ (u8) ; 4 ]"),
            r#"{"ok":{"k":"ref","mut":true,"lifetime":"'static","elem":{"k":"array","elem":{"k":"path","segs":["u8"],"args":[]},"len":"4"}},"judge":null}"#
        );
        assert_eq!(
            type_verdict("(Bytes, ResponseMeta)"),
            r#"{"ok":{"k":"tuple","elems":[{"k":"path","segs":["Bytes"],"args":[]},{"k":"path","segs":["ResponseMeta"],"args":[]}]},"judge":null}"#
        );
        assert_eq!(type_verdict("(T,)"), r#"{"ok":{"k":"tuple","elems":[{"k":"path","segs":["T"],"args":[]}]},"judge":null}"#);
        assert_eq!(type_verdict("r#type::X<'a, _>"), r#"{"ok":{"k":"path","segs":["type","X"],"args":[{"k":"lifetime","text":"'a"},{"k":"infer"}]},"judge":"a `_` placeholder (inferred)"}"#);
        assert_eq!(type_verdict("Box<dyn Error + Send>"), r#"{"ok":{"k":"path","segs":["Box"],"args":[{"k":"raw","text":"dyn Error + Send"}]},"judge":null}"#);
        assert_eq!(type_verdict("<A as B>::C"), r#"{"ok":{"k":"raw","text":"< A as B > :: C"},"judge":"a qualified path `<T as Trait>::Name` (rustc would compute the type)"}"#);
        assert_eq!(type_verdict("Foo<T>::Bar"), r#"{"ok":{"k":"raw","text":"Foo < T > :: Bar"},"judge":null}"#);
        assert_eq!(type_verdict("Iter<Item = u8>"), r#"{"ok":{"k":"path","segs":["Iter"],"args":[{"k":"raw","text":"Item = u8"}]},"judge":null}"#);
    }

    #[test]
    fn malformed_types_are_refused() {
        // review 6, finding 2: the hand-written parser repaired these
        for text in [
            "Result<HolidayResponse Error>",   // a missing comma
            "Result<HolidayResponse, Error>>", // an extra `>`
            "Result<HolidayResponse, 'a Error>", // a stray token
            "(Bytes ResponseMeta)",
            "Result<HolidayResponse, Error",
            "Result<HolidayResponse,, Error>",
            "concord::::Error",
            "Error;",
            "`Error`",
            "Error x",
            "",
        ] {
            assert!(type_verdict(text).starts_with(r#"{"err":"does not parse as a Rust type: "#), "{text}: {}", type_verdict(text));
        }
    }

    #[test]
    fn judgeable_types() {
        // what the R2 rustc identity check can judge: paths, aliases, tuples, `'static`, `dyn`
        // without a lifetime bound or with `'static`, elided lifetimes in `fn(..)`/`Fn(..)` arguments
        for text in [
            "Result<HolidayResponse, Error>",
            "core::result::Result<Response<alloc::vec::Vec<core::primitive::u8>>, concord::Error>",
            "(Bytes, ResponseMeta)",
            "&'static str",
            "[u8; 4]",
            "Box<dyn std::error::Error + Send + Sync>",
            "Box<dyn std::error::Error + 'static>",
            "Box<dyn Fn(&str) -> &str>",
            "fn(&u8) -> &u8",
            "Result<'static, HolidayResponse, Error>", // judged (and refused) by rustc: E0107
            "Error<'static>",
            "DeploymentList<made_up::Wrong>",
            "self::Error",
            "super::model::DeploymentList",
        ] {
            assert!(type_verdict(text).ends_with(r#","judge":null}"#), "{text}: {}", type_verdict(text));
        }
    }

    #[test]
    fn unjudgeable_types_are_refused() {
        for (text, why) in [
            ("_", "a `_` placeholder (inferred)"),
            ("Result<_, Error>", "a `_` placeholder (inferred)"),
            ("impl std::error::Error", "`impl Trait`"),
            ("Box<dyn std::error::Error + 'a>", "a `dyn` bound with a lifetime other than `'static`"),
            ("Box<dyn std::error::Error + '_>", "a `dyn` bound with a lifetime other than `'static`"),
            ("Error<'_>", "a `'_` lifetime (inferred)"),
            ("&str", "a reference with an elided lifetime (inferred)"),
            ("Vec<&str>", "a reference with an elided lifetime (inferred)"),
            ("Self", "`Self`"),
            ("Result<Self, Error>", "`Self`"),
            ("<Self as Trait>::Output", "`Self`"),
            ("<SendPolicy as concord::SendWith<Rate>>::Output", "a qualified path `<T as Trait>::Name` (rustc would compute the type)"),
            ("Result<<Json as Decode<T>>::Output, Error>", "a qualified path `<T as Trait>::Name` (rustc would compute the type)"),
            ("<Vec<u8>>::Item", "a qualified path `<T as Trait>::Name` (rustc would compute the type)"),
            ("my_types!()", "a macro"),
            ("Result<concord::ty!(), Error>", "a macro"),
            ("[u8; { fn f() {} 3 }]", "an expression other than an integer literal"),
            ("[u8; N]", "an expression other than an integer literal"),
            ("[u8; include_str!(\"/etc/passwd\").len()]", "an expression other than an integer literal"),
            ("Foo<{ 1 + 2 }>", "an expression other than an integer literal"),
        ] {
            let v = type_verdict(text);
            let judge = v.split(r#","judge":"#).nth(1).unwrap_or("");
            assert!(v.starts_with(r#"{"ok":"#) && judge.starts_with('"') && judge.contains(why), "{text}: {v}");
        }
    }
}
