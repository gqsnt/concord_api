
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via super::codec::Text) -> String via super::codec::Text;
        POST optional(body b?: String via super::codec::Text);
    }
}
