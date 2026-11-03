// `{Client}Config` is generated for a client with required values; a scope whose generated type
// would have the same name is rejected at the scope.
concord::api! {
    pub client Api(key: String) {
        base "http://x.test";
        query { "key" = key; }
        scope config() {
            GET item();
        }
    }
}
fn main() {}
