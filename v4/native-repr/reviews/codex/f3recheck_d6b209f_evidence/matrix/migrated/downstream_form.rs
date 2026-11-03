use crate::codec::Form;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(Form b: String) -> Form String;
        POST optional(Form b?: String);
    }
}
