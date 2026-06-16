#[test]
fn current_trybuild_fixtures_match_expected_results() {
    set_unique_trybuild_target();

    for path in [
        "tests/dsl/pass/pass_endpoint_stanza.rs",
        "tests/dsl/pass/pass_fmt.rs",
        "tests/dsl/pass/behavior_scope.rs",
        "tests/dsl/pass/behavior_endpoint_override.rs",
        "tests/dsl/pass/behavior_extends.rs",
        "tests/dsl/pass/behavior_list.rs",
        "tests/dsl/pass/behavior_rate_limit_merge.rs",
        "tests/dsl/pass/behavior_rate_limit_key_binding.rs",
        "tests/dsl/pass/rate_limit_duplicate_across_layers_allowed.rs",
        "tests/dsl/pass/behavior_default.rs",
        "tests/dsl/pass/behavior_default_override.rs",
        "tests/dsl/pass/behavior_group.rs",
        "tests/dsl/pass/behavior_group_mixed.rs",
        "tests/dsl/pass/auth_group.rs",
        "tests/dsl/pass/auth_group_mixed.rs",
        "tests/dsl/pass/policies_group.rs",
        "tests/dsl/pass/policies_group_mixed.rs",
        "tests/dsl/pass/defaults_alias.rs",
        "tests/dsl/pass/pass_query_shorthand.rs",
        "tests/dsl/pass/pass_retry_default.rs",
        "tests/usage/pass/custom_codec_body.rs",
        "tests/usage/pass/custom_codec_response.rs",
        "tests/usage/pass/custom_codec_body_and_response.rs",
        "tests/usage/pass/custom_pagination_controller.rs",
        "tests/usage/pass/pass_client_config.rs",
        "tests/usage/pass/pass_execution_pagination_auth.rs",
        "tests/usage/pass/pass_facade_navigation.rs",
        "tests/usage/pass/pass_param_builders.rs",
    ]
    .into_iter()
    {
        trybuild::TestCases::new().pass(path);
    }

    for path in [
        "tests/dsl/fail/fail_duplicate_default.rs",
        "tests/dsl/fail/fail_base_malformed_url.rs",
        "tests/dsl/fail/fail_base_split_http.rs",
        "tests/dsl/fail/fail_base_split_https.rs",
        "tests/dsl/fail/fail_duplicate_behavior.rs",
        "tests/dsl/fail/fail_unknown_behavior_use.rs",
        "tests/dsl/fail/fail_unknown_behavior_parent.rs",
        "tests/dsl/fail/fail_behavior_self_extends.rs",
        "tests/dsl/fail/fail_behavior_cycle.rs",
        "tests/dsl/fail/fail_behavior_invalid_item.rs",
        "tests/dsl/fail/fail_behavior_empty_list_scope.rs",
        "tests/dsl/fail/fail_behavior_empty_list_defaults.rs",
        "tests/dsl/fail/fail_behavior_empty_list_endpoint.rs",
        "tests/dsl/fail/fail_behavior_duplicate_cache.rs",
        "tests/dsl/fail/fail_behavior_duplicate_retry.rs",
        "tests/dsl/fail/fail_behavior_duplicate_rate_limit.rs",
        "tests/dsl/fail/fail_rate_limit_duplicate_list_endpoint.rs",
        "tests/dsl/fail/fail_rate_limit_duplicate_list_defaults.rs",
        "tests/dsl/fail/fail_rate_limit_duplicate_list_behavior.rs",
        "tests/dsl/fail/fail_unknown_default_behavior.rs",
        "tests/dsl/fail/fail_behaviors_invalid_item.rs",
        "tests/dsl/fail/fail_duplicate_behavior_flat_and_group.rs",
        "tests/dsl/fail/fail_auth_group_invalid_item.rs",
        "tests/dsl/fail/fail_auth_group_rejects_auth_use.rs",
        "tests/dsl/fail/fail_policies_invalid_item.rs",
        "tests/dsl/fail/fail_policies_rejects_default_retry.rs",
        "tests/dsl/fail/fail_policies_rejects_default_cache.rs",
        "tests/dsl/fail/fail_policies_rejects_default_rate_limit.rs",
        "tests/dsl/fail/fail_policies_rejects_behavior.rs",
        "tests/dsl/fail/fail_duplicate_default_and_defaults.rs",
        "tests/dsl/fail/fail_duplicate_defaults.rs",
        "tests/dsl/fail/fail_endpoint_duplicate_response.rs",
        "tests/dsl/fail/fail_endpoint_missing_response.rs",
        "tests/dsl/fail/fail_fmt_empty.rs",
        "tests/dsl/fail/fail_fmt_secret_in_path.rs",
        "tests/dsl/fail/fail_map_before_response.rs",
        "tests/dsl/fail/fail_max_attempts_zero.rs",
        "tests/dsl/fail/fail_query_unknown.rs",
        "tests/usage/fail/body_codec_missing_trait.rs",
        "tests/usage/fail/custom_pagination_block.rs",
        "tests/usage/fail/custom_pagination_missing_default.rs",
        "tests/usage/fail/fail_collect_pages.rs",
        "tests/usage/fail/fail_duplicate_alias.rs",
        "tests/usage/fail/fail_maybe_field.rs",
        "tests/usage/fail/fail_missing_required_param.rs",
        "tests/usage/fail/fail_non_credential_acquire_as.rs",
        "tests/usage/fail/fail_non_paginated_paginate.rs",
        "tests/usage/fail/fail_reset_field.rs",
        "tests/usage/fail/fail_with_configure.rs",
        "tests/usage/fail/response_codec_missing_trait.rs",
    ]
    .into_iter()
    {
        trybuild::TestCases::new().compile_fail(path);
    }
}

fn set_unique_trybuild_target() {
    let target_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("trybuild-current")
        .join(std::process::id().to_string());

    // The test binary has one test, so mutating the process environment here is
    // bounded to this harness. A run-local target dir avoids stale Windows
    // locks from interrupted trybuild pass executables.
    unsafe {
        std::env::set_var("CARGO_TARGET_DIR", target_dir);
    }
}
