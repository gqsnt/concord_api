mod facade { pub use crate::c4::{api, Text}; }
facade::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(facade::Text b: String) -> facade::Text String;
        POST optional(facade::Text b?: String);
    }
}
facade::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(facade::Text b: String) -> facade::Text String;
        POST optional(facade::Text b?: String);
    }
}
