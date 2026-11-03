use crate::codec::Text; // use concord::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via Text) -> String via Text;
        POST optional(body b?: String via Text);
    }
}
