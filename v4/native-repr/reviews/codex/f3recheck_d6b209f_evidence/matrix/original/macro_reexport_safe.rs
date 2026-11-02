mod facade { pub use crate::c4::{api, Text}; }
facade::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via facade::Text) -> String via facade::Text;
        POST optional(body b?: String via facade::Text);
    }
}
