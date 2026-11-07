//! The R3 answer contract of the I3 reader probes (`tools/desirability/probes/README.md`, "R3
//! answer contract"), checked with a Rust parser (`syn`) instead of a hand-written lexer: `syn`
//! tokenises strings, raw and byte strings, chars, lifetimes and nested comments as rustc does.
//!
//! stdin: one answer per line, hex-encoded UTF-8. stdout: one line per answer,
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
//! blocks, items, attributes, macros other than `vec!` and `format!` (whose arguments must parse as
//! expressions under the same rules) and syntax `syn` does not model (anywhere).

use std::io::{self, BufRead, Write};
use std::str::FromStr;

use proc_macro2::TokenStream;
use syn::parse::{ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Block, Expr, Stmt, Token};

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
        let name = if m.path.leading_colon.is_none() && m.path.segments.len() == 1 && m.path.segments[0].arguments.is_none() {
            m.path.segments[0].ident.to_string()
        } else {
            String::new()
        };
        let parsed = match name.as_str() {
            "vec" => m.parse_body_with(vec_body),
            "format" => m
                .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                .map(|p| p.into_iter().collect()),
            _ => {
                return self.refuse(format!("the macro `{}!` (only `vec!` and `format!` are allowed)", path_text(&m.path)));
            }
        };
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

fn decode_hex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

fn run() {
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
}
