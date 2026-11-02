use crate::codec::Text as CustomText; use concord::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(CustomText b: String) -> CustomText String;
        POST optional(CustomText b?: String);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(CustomText b: String) -> CustomText String;
        POST optional(CustomText b?: String);
    }
}
