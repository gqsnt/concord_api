use crate::codec as concord;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via concord::Text) -> String via concord::Text;
        POST optional(body b?: String via concord::Text);
    }
}
