type text = crate::codec::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(text b: String) -> text String;
        POST optional(text b?: String);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(self::text b: String) -> self::text String;
        POST optional(self::text b?: String);
    }
}
