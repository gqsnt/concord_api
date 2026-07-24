use proc_macro2::TokenStream;
use quote::quote;

pub(super) const GENERATED_CONTRACT_VERSION: u32 = 25;

pub(super) fn generated_contract_assertion() -> TokenStream {
    let version = GENERATED_CONTRACT_VERSION;
    let max_auth_placements = crate::MAX_AUTH_PLACEMENTS;
    let max_rate_limit_id_bytes = crate::MAX_RATE_LIMIT_ID_BYTES;
    let max_rate_limit_windows_per_rule = crate::MAX_RATE_LIMIT_WINDOWS_PER_RULE;
    let max_rate_limit_rules_per_plan = crate::MAX_RATE_LIMIT_RULES_PER_PLAN;
    quote! {
        const _: () = __core::__private::assert_generated_contract(#version);
        const _: () = assert!(
            __core::auth::MAX_AUTH_PLACEMENTS == #max_auth_placements,
            "Concord macro/core authentication placement limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_ID_BYTES == #max_rate_limit_id_bytes,
            "Concord macro/core rate-limit ID limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_RULE
                == #max_rate_limit_windows_per_rule,
            "Concord macro/core rate-limit window limits differ",
        );
        const _: () = assert!(
            __core::rate_limit::MAX_RATE_LIMIT_RULES_PER_PLAN == #max_rate_limit_rules_per_plan,
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
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_ID_BYTES"));
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_WINDOWS_PER_RULE"));
        assert!(output.contains("__core :: rate_limit :: MAX_RATE_LIMIT_RULES_PER_PLAN"));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_ID_BYTES.to_string()));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_WINDOWS_PER_RULE.to_string()));
        assert!(output.contains(&crate::MAX_RATE_LIMIT_RULES_PER_PLAN.to_string()));
    }
}
