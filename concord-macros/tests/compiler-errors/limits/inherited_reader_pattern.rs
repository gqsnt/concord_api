mod patterns {
    #[allow(non_camel_case_types)]
    pub struct zone;
}

use patterns::*;

concord::api! {
    client Service(key: u8) {
        base "https://example.invalid";
        read (|zone: &u8| concord::Json)(key);

        scope zone {
            GET get() -> u8 { status { 2xx { body } => { body } } }
        }
    }
}

fn main() {}
