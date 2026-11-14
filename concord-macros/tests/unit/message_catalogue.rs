//! Compare documented messages with the complete macro's diagnostics.
use super::*;

fn primary(tokens: TokenStream, out: &mut Vec<String>) {
    let mut tokens = tokens.into_iter().peekable();
    while let Some(token) = tokens.next() {
        match token {
            TokenTree::Ident(id) if id == "compile_error" => {
                assert!(matches!(tokens.next(),Some(TokenTree::Punct(p)) if p.as_char()=='!'));
                let Some(TokenTree::Group(g)) = tokens.next() else {
                    panic!()
                };
                let text = syn::parse2::<syn::LitStr>(g.stream()).unwrap().value();
                if text.contains("\nlabel: ") {
                    out.push(format!("error: {text}"));
                }
            }
            TokenTree::Group(g) => primary(g.stream(), out),
            _ => {}
        }
    }
}
#[test]
fn c1_18_every_messages_page_case_runs_the_whole_macro() {
    let page = fs::read_to_string(package().join("../docs/reference/messages.md")).unwrap();
    let mut number = 0;
    let mut blocks = Vec::new();
    let mut lines = page.lines();
    let mut tenth = 0;
    while let Some(line) = lines.next() {
        if line.starts_with("## 2.") {
            break;
        }
        if let Some((digits, _)) = line.strip_prefix("**").and_then(|s| s.split_once(".**")) {
            number = digits.parse::<u8>().unwrap();
        }
        if line == "```text" {
            let text = [
                lines.next().unwrap(),
                lines.next().unwrap(),
                lines.next().unwrap(),
            ]
            .join("\n");
            assert_eq!(lines.next(), Some("```"));
            let suffix = if number == 10 {
                tenth += 1;
                if tenth == 2 {
                    "_self"
                } else {
                    ""
                }
            } else {
                ""
            };
            blocks.push((number, format!("{number:02}{suffix}"), text));
        }
    }
    assert_eq!(blocks.len(), 47);
    let mut failures = Vec::new();
    for (number, stem, page_text) in blocks {
        let dir = package().join("tests/diagnostics/catalogue");
        let source = fs::read_to_string(dir.join(format!("{stem}.api"))).unwrap();
        let native = fs::read_to_string(dir.join(format!("{stem}.rs"))).unwrap();
        assert_eq!(
            native,
            format!("concord::api! {{\n{source}}}\nfn main() {{}}\n"),
            "native case drift: {stem}"
        );
        let mut actual = Vec::new();
        primary(crate::expand(source.parse().unwrap()), &mut actual);
        assert_eq!(actual.len(), 1, "message {number}: {actual:?}");
        let actual = actual.pop().unwrap();
        assert_eq!(
            actual.lines().next(),
            page_text.lines().next(),
            "message {number} first line"
        );
        if std::env::var_os("CONCORD_REVIEW_MESSAGES").is_some() {
            let target = package().join("../target/message-fixtures");
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join(format!("{stem}.txt")), format!("{actual}\n")).unwrap();
        }
        if actual.trim_end() != page_text.trim_end() {
            failures.push(format!(
                "message {number}\npage: {page_text}\nprinted: {actual}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
