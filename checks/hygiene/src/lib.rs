//! One `api!` invocation receives `$crate::KEY` from two crates: identical tokens that name two
//! different constants (`x-public` and `authorization`). A proc-macro cannot see `macro_rules`
//! hygiene, so Concord never treats equal tokens as one header: each occurrence is its own key,
//! compared by `HeaderName` when the request is built. Found by the final executable review (R1).

macro_rules! finish {
    ($a:path; $b:path) => {
        concord::api! {
            /// Two keys spelled `$crate::KEY`.
            pub client Hygiene {
                base "http://hygiene.invalid";
                headers { append $a = "PUBLIC"; append $b = "SECRET"; }
                /// The only endpoint.
                GET get();
            }
        }
    };
}

concord_hygiene_key_a::start!(concord_hygiene_key_b::next);
