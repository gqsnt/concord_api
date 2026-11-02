type text = crate::codec::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via text) -> String via text;
        POST optional(body b?: String via text);
    }
}
