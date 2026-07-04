use super::helpers::{analyze_ok, behavior_names, endpoint_by_name, single_endpoint};

#[test]
fn behavior_resolution_lowers_behavior_declarations() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"

                behavior client_read {
                    retry off
                }

                behavior scope_read {
                    retry off
                }

                behavior endpoint_read {
                    retry off
                }

                defaults {
                    behavior client_read
                }
            }

            scope users {
                path ["users"]
                behavior scope_read

                GET Me
                    path ["me"]
                    behavior endpoint_read
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

                behavior read {
                    retry off
                }

                behavior match_read {
                    retry off
                }

                defaults {
                    behavior read
                }
            }

            scope users {
                path ["users"]
                behavior read

                GET Me
                    path ["me"]
                    behavior match_read
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

                behavior read {
                    retry off
                }

                defaults {
                    behavior read
                }
            }

            scope users {
                path ["users"]
                behavior read

                GET Me
                    path ["me"]
                    behavior read
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(behavior_names(endpoint), &["read".to_string()]);
}
