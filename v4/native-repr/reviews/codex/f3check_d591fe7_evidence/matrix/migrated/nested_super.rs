pub type Text = crate::codec::Text;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(super::Text b: String) -> super::Text String;
        POST optional(super::Text b?: String);
    }
}
