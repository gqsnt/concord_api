wire::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b:String via codec::Text);
    }
}
