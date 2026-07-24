use proc_macro2::TokenStream;
use quote::quote;

pub(super) const GENERATED_CONTRACT_VERSION: u32 = 25;

pub(super) fn generated_contract_assertion() -> TokenStream {
    let version = GENERATED_CONTRACT_VERSION;
    let max_auth_placements = crate::MAX_AUTH_PLACEMENTS;
    quote! {
        const _: () = __core::__private::assert_generated_contract(#version);
        const _: () = assert!(
            __core::auth::MAX_AUTH_PLACEMENTS == #max_auth_placements,
            "Concord macro/core authentication placement limits differ",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_the_exact_generated_contract_check() {
        let output = generated_contract_assertion().to_string();

        assert!(output.contains("__core :: __private :: assert_generated_contract"));
        assert!(output.contains(&GENERATED_CONTRACT_VERSION.to_string()));
        assert!(output.contains("__core :: auth :: MAX_AUTH_PLACEMENTS"));
        assert!(output.contains(&crate::MAX_AUTH_PLACEMENTS.to_string()));
    }
}
