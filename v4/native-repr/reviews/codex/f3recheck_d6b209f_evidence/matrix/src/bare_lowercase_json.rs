type json = crate::codec::Json;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(self::json b: String) -> self::json String;
        POST optional(self::json b?: String);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(self::json b: String) -> self::json String;
        POST optional(self::json b?: String);
    }
}
