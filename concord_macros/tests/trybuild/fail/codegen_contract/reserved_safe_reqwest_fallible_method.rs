use concord_macros::api;

api! {
    client ReservedSafeReqwestFallibleApi { base "https://example.com" }

    GET FallibleBuilder
        as new_with_safe_reqwest_builder_fallible
        path ["fallible-builder"]
        -> Json<String>
}

fn main() {}
