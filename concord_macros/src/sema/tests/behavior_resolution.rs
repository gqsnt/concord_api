use super::helpers::{analyze_ok, behavior_names, endpoint_by_name, single_endpoint};

#[test]
fn behavior_resolution_lowers_behavior_declarations() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                profile client_read {
                    retry off
                }

                profile scope_read {
                    retry off
                }

                profile endpoint_read {
                    retry off
                }

                default {
                    profile client_read
                }
            }

            scope users {
                path ["users"]
                profile scope_read

                GET Me
                    path ["me"]
                    profile endpoint_read
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(
        behavior_names(endpoint),
        &[
            "client_read".to_string(),
            "scope_read".to_string(),
            "endpoint_read".to_string(),
        ]
    );
}

#[test]
fn behavior_doc_names_are_deduped_in_stable_order() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                profile read {
                    retry off
                }

                profile match_read {
                    retry off
                }

                default {
                    profile read
                }
            }

            scope users {
                path ["users"]
                profile read

                GET Me
                    path ["me"]
                    profile match_read
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert_eq!(
        behavior_names(endpoint),
        &["read".to_string(), "match_read".to_string()]
    );
}

#[test]
fn duplicate_behavior_across_layers_remains_allowed() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                profile read {
                    retry off
                }

                default {
                    profile read
                }
            }

            scope users {
                path ["users"]
                profile read

                GET Me
                    path ["me"]
                    profile read
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(behavior_names(endpoint), &["read".to_string()]);
}
