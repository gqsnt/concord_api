use crate::codec::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(self::Text b: String) -> self::Text String;
        POST optional(self::Text b?: String);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(self::Text b: String) -> self::Text String;
        POST optional(self::Text b?: String);
    }
}
