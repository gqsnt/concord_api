use crate::codec::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via self::Text) -> String via self::Text;
        POST optional(body b?: String via self::Text);
    }
}
