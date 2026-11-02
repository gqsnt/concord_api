use crate::codec;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via codec::Text) -> String via codec::Text;
        POST optional(body b?: String via codec::Text);
    }
}
