//! `rows!()` expands to `R<Local>` the first time it is expanded and to `R<Other>` after: a
//! type written once in a description must be expanded once, or two parts of the generated
//! code see two types.
use proc_macro::TokenStream;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `R<Local>`, then `R<Other>` at every later expansion.
#[proc_macro]
pub fn rows(_: TokenStream) -> TokenStream {
    static EXPANSIONS: AtomicUsize = AtomicUsize::new(0);
    let problem = match EXPANSIONS.fetch_add(1, Ordering::SeqCst) {
        0 => "Local",
        _ => "Other",
    };
    let mut type_tokens = String::from("R<");
    type_tokens.push_str(problem);
    type_tokens.push('>');
    type_tokens.parse().unwrap_or_default()
}

/// Expands to the first group on the first use of a key and alternates thereafter.
///
/// Write `alternate!(key (u8) (u16))` in a type position, or give it two expressions.
#[proc_macro]
pub fn alternate(input: TokenStream) -> TokenStream {
    use proc_macro::TokenTree::{Group, Ident};
    use std::{collections::HashMap, sync::Mutex};

    static COUNTS: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
    let mut tokens = input.into_iter();
    let (Some(Ident(key)), Some(Group(first)), Some(Group(second)), None) =
        (tokens.next(), tokens.next(), tokens.next(), tokens.next())
    else {
        return "compile_error!(\"expected a key and two parenthesized alternatives\")"
            .parse()
            .unwrap_or_default();
    };
    let Ok(mut counts) = COUNTS.lock() else {
        return TokenStream::new();
    };
    let next = counts
        .get_or_insert_with(HashMap::new)
        .entry(key.to_string())
        .or_default();
    *next = !*next;
    if *next {
        first.stream()
    } else {
        second.stream()
    }
}
