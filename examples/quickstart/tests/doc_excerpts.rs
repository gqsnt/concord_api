//! Keeps the code in the documentation compiled.
//!
//! Rules for `README.md`, `examples/native/README.md` and `docs/*.md`:
//!
//! - A code block whose label (the text between the previous code block and this one) contains
//!   ``Excerpt of [`name`](PATH)`` must appear verbatim in `PATH` (a link relative to the
//!   document; ``Excerpt of `PATH` `` without a link is relative to the repository root). The
//!   excerpt may be uniformly less indented than its source.
//! - In a document that is not compiled as doctests, every Rust block must be such an excerpt.
//! - In a doctested document (`DOCTESTED`, included by `src/lib.rs`), every block must name its
//!   language, and a Rust block that is not compiled must say why: `rust,ignore` after a label
//!   containing "Excerpt of" or "does not compile".

use std::fs;
use std::path::{Path, PathBuf};

const DOCTESTED: &[&str] = &["docs/AUTHOR_GUIDE.md", "docs/MIGRATION.md"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn documents() -> Vec<String> {
    let mut docs = vec![
        "README.md".to_owned(),
        "examples/native/README.md".to_owned(),
    ];
    for entry in fs::read_dir(root().join("docs")).expect("docs/ exists") {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.ends_with(".md") {
            docs.push(format!("docs/{name}"));
        }
    }
    docs.sort();
    docs
}

struct Block {
    line: usize,
    info: String,
    label: String,
    body: Vec<String>,
}

impl Block {
    fn is_rust(&self) -> bool {
        let lang = self.info.split(',').next().unwrap_or_default().trim();
        lang.is_empty() || lang == "rust"
    }

    /// The quoted file: the first link target after "Excerpt of" (relative to the document's
    /// directory), or else the first backticked path (relative to the repository root).
    fn source(&self, doc_dir: &Path) -> Option<PathBuf> {
        let (_, rest) = self.label.split_once("Excerpt of")?;
        if let Some((_, target)) = rest.split_once("](") {
            let (target, _) = target.split_once(')')?;
            return Some(doc_dir.join(target));
        }
        rest.split('`').nth(1).map(|path| root().join(path))
    }
}

fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut label = String::new();
    let mut lines = text.lines().enumerate();
    while let Some((n, line)) = lines.next() {
        let trimmed = line.trim_start();
        let Some(info) = trimmed.strip_prefix("```") else {
            label.push_str(line);
            label.push('\n');
            continue;
        };
        let indent = line.len() - trimmed.len();
        let mut body = Vec::new();
        for (_, inner) in lines.by_ref() {
            if inner.trim_start().starts_with("```") {
                break;
            }
            let strip = indent.min(inner.len() - inner.trim_start().len());
            body.push(inner[strip..].to_owned());
        }
        out.push(Block {
            line: n + 1,
            info: info.trim().to_owned(),
            label: std::mem::take(&mut label),
            body,
        });
    }
    out
}

/// Whether `excerpt` occurs in `source` as consecutive lines, all shifted by the same indentation.
fn occurs(source: &str, excerpt: &[String]) -> bool {
    let src: Vec<&str> = source.lines().collect();
    let Some(first) = excerpt.first() else {
        return false;
    };
    (0..src.len()).any(|start| {
        let Some(indent) = src[start].strip_suffix(first.as_str()) else {
            return false;
        };
        indent.chars().all(|c| c == ' ')
            && start + excerpt.len() <= src.len()
            && excerpt.iter().enumerate().all(|(j, line)| {
                let s = src[start + j];
                if line.trim().is_empty() {
                    s.trim().is_empty()
                } else {
                    s.strip_prefix(indent) == Some(line.as_str())
                }
            })
    })
}

#[test]
fn documentation_code_is_compiled_or_a_checked_excerpt() {
    let mut problems = Vec::new();
    let mut excerpts = 0;
    for doc in documents() {
        let text = fs::read_to_string(root().join(&doc)).unwrap();
        let doctested = DOCTESTED.contains(&doc.as_str());
        let doc_dir = root().join(&doc).parent().unwrap().to_path_buf();
        for block in blocks(&text) {
            let at = format!("{doc}:{}", block.line);
            if let Some(source) = block.source(&doc_dir) {
                excerpts += 1;
                let shown = source.display();
                match fs::read_to_string(&source) {
                    Ok(source_text) if occurs(&source_text, &block.body) => {}
                    Ok(_) => problems.push(format!("{at}: excerpt not found verbatim in {shown}")),
                    Err(e) => problems.push(format!("{at}: cannot read {shown}: {e}")),
                }
                if doctested && block.is_rust() && !block.info.contains("ignore") {
                    problems.push(format!(
                        "{at}: an excerpt in a doctested file needs `rust,ignore`"
                    ));
                }
                continue;
            }
            if doctested {
                if block.info.is_empty() {
                    problems.push(format!(
                        "{at}: name the block's language (rustdoc compiles unnamed blocks)"
                    ));
                } else if block.is_rust()
                    && block.info.contains("ignore")
                    && !block.label.contains("does not compile")
                {
                    problems.push(format!(
                        "{at}: an ignored block must say why it does not compile"
                    ));
                }
            } else if block.is_rust() {
                problems.push(format!(
                    "{at}: Rust block is neither compiled (doctested) nor a labelled excerpt"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        excerpts > 0,
        "no excerpts found: is the label format still `Excerpt of`?"
    );
}

#[test]
fn the_checker_rejects_drift() {
    let source = "fn f() {\n    let a = 1;\n\n    let b = 2;\n}\n";
    let lines = |s: &[&str]| s.iter().map(|l| (*l).to_owned()).collect::<Vec<_>>();
    assert!(occurs(source, &lines(&["let a = 1;", "", "let b = 2;"])));
    assert!(occurs(source, &lines(&["    let a = 1;"])));
    assert!(!occurs(source, &lines(&["let a = 1;", "let b = 2;"])));
    assert!(!occurs(source, &lines(&["let a = 2;"])));
    assert!(!occurs(source, &lines(&["a = 1;"])));
}
