use proc_macro2::TokenStream;
use quote::quote;

pub(super) const GENERATED_CONTRACT_VERSION: u32 = 33;

pub(super) fn generated_contract_assertion() -> TokenStream {
    let version = GENERATED_CONTRACT_VERSION;
    let max_auth_placements = crate::MAX_AUTH_PLACEMENTS;
    let max_auth_placement_name_bytes = crate::MAX_AUTH_PLACEMENT_NAME_BYTES;
    let max_authorization_scheme_bytes = crate::MAX_AUTHORIZATION_SCHEME_BYTES;
    let max_request_head_components = crate::MAX_REQUEST_HEAD_COMPONENTS;
    let max_request_url_bytes = crate::MAX_REQUEST_URL_BYTES;
    let max_request_header_count = crate::MAX_REQUEST_HEADER_COUNT;
    let max_request_header_bytes = crate::MAX_REQUEST_HEADER_BYTES;
    let max_rate_limit_id_bytes = crate::MAX_RATE_LIMIT_ID_BYTES;
    let max_rate_limit_key_part_name_bytes = crate::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES;
    let max_rate_limit_key_parts = crate::MAX_RATE_LIMIT_KEY_PARTS;
    let max_rate_limit_windows_per_rule = crate::MAX_RATE_LIMIT_WINDOWS_PER_POLICY;
    let max_rate_limit_policies_per_plan = crate::MAX_RATE_LIMIT_POLICIES_PER_PLAN;
    quote! {
        const _: () = __core::__private::assert_generated_contract(#version);
        const _: () = assert!(
            __core::auth::MAX_AUTH_PLACEMENTS == #max_auth_placements,
            "Concord macro/core authentication placement limits differ",
        );
        const _: () = assert!(
            __core::auth::MAX_AUTH_PLACEMENT_NAME_BYTES == #max_auth_placement_name_bytes,
            "Concord macro/core authentication placement-name limits differ",
        );
        const _: () = assert!(
            __core::auth::MAX_AUTHORIZATION_SCHEME_BYTES == #max_authorization_scheme_bytes,
            "Concord macro/core Authorization scheme limits differ",
        );
        const _: () = assert!(
            __core::endpoint::MAX_REQUEST_HEAD_COMPONENTS == #max_request_head_components,
            "Concord macro/core request-head component limits differ",
        );
        const _: () = assert!(
            __core::endpoint::MAX_REQUEST_URL_BYTES == #max_request_url_bytes,
            "Concord macro/core request URL limits differ",
        );
        const _: () = assert!(
            __core::endpoint::MAX_REQUEST_HEADER_COUNT == #max_request_header_count,
            "Concord macro/core request-header count limits differ",
        );
        const _: () = assert!(
            __core::endpoint::MAX_REQUEST_HEADER_BYTES == #max_request_header_bytes,
            "Concord macro/core request-header byte limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_ID_BYTES == #max_rate_limit_id_bytes,
            "Concord macro/core rate-limit ID limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES
                == #max_rate_limit_key_part_name_bytes,
            "Concord macro/core rate-limit key-part-name limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_KEY_PARTS == #max_rate_limit_key_parts,
            "Concord macro/core rate-limit key-part limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY
                == #max_rate_limit_windows_per_rule,
            "Concord macro/core rate-limit window limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN == #max_rate_limit_policies_per_plan,
            "Concord macro/core rate-limit plan limits differ",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_contract_and_structural_limit_alignment_checks() {
        let output = generated_contract_assertion().to_string();

        assert!(output.contains("__core :: __private :: assert_generated_contract"));
        assert!(output.contains(&GENERATED_CONTRACT_VERSION.to_string()));
        assert!(output.contains("__core :: auth :: MAX_AUTH_PLACEMENTS"));
        assert!(output.contains(&crate::MAX_AUTH_PLACEMENTS.to_string()));
        assert!(output.contains("__core :: auth :: MAX_AUTH_PLACEMENT_NAME_BYTES"));
        assert!(output.contains("__core :: auth :: MAX_AUTHORIZATION_SCHEME_BYTES"));
        assert!(output.contains("__core :: endpoint :: MAX_REQUEST_HEAD_COMPONENTS"));
        assert!(output.contains("__core :: endpoint :: MAX_REQUEST_URL_BYTES"));
        assert!(output.contains("__core :: endpoint :: MAX_REQUEST_HEADER_COUNT"));
        assert!(output.contains("__core :: endpoint :: MAX_REQUEST_HEADER_BYTES"));
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_ID_BYTES"));
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_KEY_PART_NAME_BYTES"));
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_KEY_PARTS"));
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_WINDOWS_PER_POLICY"));
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_POLICIES_PER_PLAN"));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_ID_BYTES.to_string()));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES.to_string()));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_KEY_PARTS.to_string()));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_WINDOWS_PER_POLICY.to_string()));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_POLICIES_PER_PLAN.to_string()));
    }
}
