#[cfg(all())] use crate::codec::Text; #[cfg(any())] use concord::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(Text b: String) -> Text String;
        POST optional(Text b?: String);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(Text b: String) -> Text String;
        POST optional(Text b?: String);
    }
}
