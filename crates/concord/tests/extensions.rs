//! Public extension contracts (§17) implemented downstream through the facade.

mod extensions {
    pub(crate) mod contracts;
    mod oauth2;
    mod rate_limit_composition;
}
