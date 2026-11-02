use crate::codec::Text as CustomText; use concord::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via CustomText) -> String via CustomText;
        POST optional(body b?: String via CustomText);
    }
}
