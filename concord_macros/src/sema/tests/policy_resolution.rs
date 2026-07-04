use super::helpers::{analyze_ok, client_policy, header_ops, query_ops, single_endpoint};
use crate::model::SetOp;
use crate::sema::{
    FmtResolvedPiece, FmtVarSource, KeyResolved, PolicyOp, PolicySetValue, PublicValueKind,
};

#[test]
fn resolved_query_and_header_ops_preserve_order_and_optional_conditions() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                var trace_id: String
            }

            GET Search(q: String, maybe?: String)
                path ["search"]
                query {
                    q,
                    "tag" += q,
                    "maybe" = maybe,
                    -"old"
                }
                headers {
                    "x-trace" = vars.trace_id,
                    -"x-old"
                }
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);
    let endpoint_policy = &endpoint.policy.endpoint;

    assert_eq!(query_ops(endpoint_policy).len(), 4);
    assert_eq!(header_ops(endpoint_policy).len(), 2);

    match &query_ops(endpoint_policy)[0] {
        PolicyOp::Set {
            key: KeyResolved::Ident(key),
            value: PolicySetValue::Value(PublicValueKind::EpField(field)),
            op: SetOp::Set,
        } => {
            assert_eq!(key.to_string(), "q");
            assert_eq!(field.to_string(), "q");
        }
        other => panic!("unexpected query shorthand lowering: {other:?}"),
    }

    match &query_ops(endpoint_policy)[1] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            value: PolicySetValue::Value(PublicValueKind::EpField(field)),
            op: SetOp::Push,
        } => {
            assert_eq!(key.value(), "tag");
            assert_eq!(field.to_string(), "q");
        }
        other => panic!("unexpected query push lowering: {other:?}"),
    }

    match &query_ops(endpoint_policy)[2] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            value: PolicySetValue::OptionalEpField(field),
            op: SetOp::Set,
        } => {
            assert_eq!(key.value(), "maybe");
            assert_eq!(field.to_string(), "maybe");
        }
        other => panic!("unexpected optional endpoint field lowering: {other:?}"),
    }

    assert!(matches!(
        &query_ops(endpoint_policy)[3],
        PolicyOp::Remove {
            key: KeyResolved::Static(key),
        } if key.value() == "old"
    ));

    match &header_ops(endpoint_policy)[0] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            value: PolicySetValue::Value(PublicValueKind::CxField(field)),
            op: SetOp::Set,
        } => {
            assert_eq!(key.value(), "x-trace");
            assert_eq!(field.to_string(), "trace_id");
        }
        other => panic!("unexpected client-var header lowering: {other:?}"),
    }

    assert!(matches!(
        &header_ops(endpoint_policy)[1],
        PolicyOp::Remove {
            key: KeyResolved::Static(key),
        } if key.value() == "x-old"
    ));
}

#[test]
fn policy_resolution_lowers_client_and_endpoint_public_values() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                var trace_id: String

                headers {
                    "x-trace" = vars.trace_id,
                    "x-static" = "literal"
                }
            }

            GET Search(q: String)
                path ["search"]
                query {
                    q,
                    "calc" = 1 + 2,
                }
                -> Json<()>
        }
        "#,
    );

    match &client_policy(&api).headers[0] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            value: PolicySetValue::Value(PublicValueKind::CxField(field)),
            ..
        } => {
            assert_eq!(key.value(), "x-trace");
            assert_eq!(field.to_string(), "trace_id");
        }
        other => panic!("unexpected client vars lowering: {other:?}"),
    }

    match &client_policy(&api).headers[1] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            value: PolicySetValue::Value(PublicValueKind::LitStr(lit)),
            ..
        } => {
            assert_eq!(key.value(), "x-static");
            assert_eq!(lit.value(), "literal");
        }
        other => panic!("unexpected client string literal lowering: {other:?}"),
    }

    let endpoint = single_endpoint(&api);
    match &endpoint.policy.endpoint.query[0] {
        PolicyOp::Set {
            key: KeyResolved::Ident(key),
            value: PolicySetValue::Value(PublicValueKind::EpField(field)),
            ..
        } => {
            assert_eq!(key.to_string(), "q");
            assert_eq!(field.to_string(), "q");
        }
        other => panic!("unexpected endpoint shorthand lowering: {other:?}"),
    }

    match &endpoint.policy.endpoint.query[1] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            value: PolicySetValue::Value(PublicValueKind::OtherExpr(_)),
            ..
        } => {
            assert_eq!(key.value(), "calc");
        }
        other => panic!("unexpected public expression lowering: {other:?}"),
    }
}

#[test]
fn policy_resolution_lowers_fmt_values() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                var trace_id: String
            }

            GET Search(q: String, maybe?: String)
                path ["search"]
                headers {
                    "x-trace" = fmt["Bearer ", vars.trace_id, " / ", q, " / ", maybe]
                }
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    match &endpoint.policy.endpoint.headers[0] {
        PolicyOp::Set {
            key: KeyResolved::Static(key),
            value: PolicySetValue::Value(PublicValueKind::Fmt(fmt)),
            ..
        } => {
            assert_eq!(key.value(), "x-trace");
            assert!(fmt.require_all);
            match fmt.pieces.as_slice() {
                [
                    FmtResolvedPiece::Lit(prefix),
                    FmtResolvedPiece::Var {
                        source: FmtVarSource::Cx,
                        field: cx_field,
                        optional: false,
                    },
                    FmtResolvedPiece::Lit(sep1),
                    FmtResolvedPiece::Var {
                        source: FmtVarSource::Ep,
                        field: ep_field,
                        optional: false,
                    },
                    FmtResolvedPiece::Lit(sep2),
                    FmtResolvedPiece::Var {
                        source: FmtVarSource::Ep,
                        field: maybe_field,
                        optional: true,
                    },
                ] => {
                    assert_eq!(prefix.value(), "Bearer ");
                    assert_eq!(cx_field.to_string(), "trace_id");
                    assert_eq!(sep1.value(), " / ");
                    assert_eq!(ep_field.to_string(), "q");
                    assert_eq!(sep2.value(), " / ");
                    assert_eq!(maybe_field.to_string(), "maybe");
                }
                other => panic!("unexpected fmt lowering: {other:?}"),
            }
        }
        other => panic!("unexpected fmt policy lowering: {other:?}"),
    }
}
