//! Preserve explicit authored lexical paths after moving code into support
//! modules. Traverse Rust syntax rather than editing token text; qualified
//! paths and paths nested in generic arguments have independent roots.

use syn::visit_mut::{self, VisitMut};

struct Relocate {
    depth: usize,
}

impl VisitMut for Relocate {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        visit_mut::visit_path_mut(self, path);
        if path.leading_colon.is_some() {
            return;
        }
        let Some(first) = path.segments.first() else {
            return;
        };
        let is_self = first.ident == "self";
        if !is_self && first.ident != "super" {
            return;
        }
        let mut segments = syn::punctuated::Punctuated::new();
        for _ in 0..self.depth {
            segments.push(syn::PathSegment::from(syn::Ident::new(
                "super",
                first.ident.span(),
            )));
        }
        for (index, segment) in path.segments.iter().cloned().enumerate() {
            if index != 0 || !is_self {
                segments.push(segment);
            }
        }
        path.segments = segments;
    }

    fn visit_type_path_mut(&mut self, node: &mut syn::TypePath) {
        let before = node.path.segments.len();
        visit_mut::visit_type_path_mut(self, node);
        if let Some(qself) = &mut node.qself {
            if qself.position > 0 {
                qself.position += node.path.segments.len() - before;
            }
        }
    }

    fn visit_expr_path_mut(&mut self, node: &mut syn::ExprPath) {
        let before = node.path.segments.len();
        visit_mut::visit_expr_path_mut(self, node);
        if let Some(qself) = &mut node.qself {
            if qself.position > 0 {
                qself.position += node.path.segments.len() - before;
            }
        }
    }
}

pub(super) fn ty(authored: &syn::Type, depth: usize) -> syn::Type {
    let mut ty = authored.clone();
    Relocate { depth }.visit_type_mut(&mut ty);
    ty
}

pub(super) fn path(authored: &syn::Path, depth: usize) -> syn::Path {
    let mut path = authored.clone();
    Relocate { depth }.visit_path_mut(&mut path);
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    #[test]
    fn relocation_traverses_nested_qualified_function_and_const_generic_paths() {
        for (authored, expected) in [
            ("self::T", "super::super::T"),
            ("super::super::T", "super::super::super::super::T"),
            ("crate::T", "crate::T"),
            (
                "::std::vec::Vec<self::T>",
                "::std::vec::Vec<super::super::T>",
            ),
            (
                "<self::T as super::Trait<self::U>>::Assoc",
                "<super::super::T as super::super::super::Trait<super::super::U>>::Assoc",
            ),
            ("<self::T>::Assoc", "<super::super::T>::Assoc"),
            (
                "fn(self::T) -> super::U",
                "fn(super::super::T) -> super::super::super::U",
            ),
            ("[u8; self::N]", "[u8; super::super::N]"),
            (
                "Wrapper<{ <u32 as self::Trait>::N }>",
                "Wrapper<{ <u32 as super::super::Trait>::N }>",
            ),
            ("BareImported", "BareImported"),
        ] {
            let relocated = ty(&syn::parse_str(authored).unwrap(), 2);
            let expected: syn::Type = syn::parse_str(expected).unwrap();
            assert_eq!(
                relocated.to_token_stream().to_string(),
                expected.to_token_stream().to_string(),
                "{authored}"
            );
        }
    }
}
