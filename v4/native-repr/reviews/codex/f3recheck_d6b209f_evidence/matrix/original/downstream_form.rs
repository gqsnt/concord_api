use crate::codec::Form;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via Form) -> String via Form;
        POST optional(body b?: String via Form);
    }
}
