//! A downstream consumer that writes the input as a struct literal. Disallowed on purpose: the
//! input is `#[non_exhaustive]`, so this does not compile (E0639) against any SDK version.
use g1_evo_sdk::ShopListingListInput;

fn main() {
    let input = ShopListingListInput {
        q: None,
        tags: None,
        limit: 20,
    };
    let _ = input;
}
