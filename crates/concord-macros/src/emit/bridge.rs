use proc_macro2::TokenStream;
use quote::quote;

pub(super) const GENERATED_CONTRACT_VERSION: u32 = 24;

pub(super) fn generated_contract_assertion() -> TokenStream {
    let version = GENERATED_CONTRACT_VERSION;
    quote! {
        const _: () = __core::__private::assert_generated_contract(#version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_the_exact_generated_contract_check() {
        let output = generated_contract_assertion().to_string();

        assert!(output.contains("__core :: __private :: assert_generated_contract"));
        assert!(output.contains("24"));
    }
}
