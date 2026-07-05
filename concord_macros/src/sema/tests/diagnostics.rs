use super::helpers::{analyze_err, assert_error_contains};
use crate::ast::{RawItem, RawScope};
use proc_macro2::Span;

fn nested_raw_scope(depth: usize) -> RawScope {
    let items = if depth == 0 {
        Vec::new()
    } else {
        vec![RawItem::Layer(Box::new(nested_raw_scope(depth - 1)))]
    };
    RawScope {
        span: Span::call_site(),
        scope_span: Span::call_site(),
        body_span: Span::call_site(),
        scope_name: Some(syn::Ident::new(
            &format!("scope_{depth}"),
            Span::call_site(),
        )),
        host_route: None,
        path_route: None,
        params: Vec::new(),
        policy: Default::default(),
        behavior_uses: Vec::new(),
        auth_uses: Vec::new(),
        retry: None,
        rate_limit: None,
        rate_limit_keys: Vec::new(),
        items,
    }
}

#[test]
fn generated_public_name_collisions_are_rejected() {
    let err = analyze_err(
        r#"
        client Collision {
            base "https://example.com"
            var debug_level: u8
        }

        GET Ping
            path ["ping"]
            -> Json<String>
        "#,
    );

    assert_error_contains(&err, "set_debug_level");
    assert_error_contains(&err, "generated client method");
}

#[test]
fn synthetic_raw_scope_tree_hits_depth_limit_during_normalization() {
    let mut raw = super::helpers::parse_raw(
        r#"
        client Api {
            base "https://example.com"
        }
        "#,
    );
    raw.items = vec![RawItem::Layer(Box::new(nested_raw_scope(65)))];

    let err = super::super::normalize_api(raw).expect_err("over-depth tree should fail");
    assert!(
        err.to_string()
            .contains("DSL scope nesting exceeds maximum supported depth of 64"),
        "{err}"
    );
    assert!(!err.to_string().contains("LEAK_SENTINEL"));
}
