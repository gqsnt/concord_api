use concord_macros::api;

api! {
    client PublicTypeCollisionApi {
        base "https://example.com"
    }

    GET FooBar
        path ["foo-bar"]
        -> Json<String>

    scope Foo {
        path ["foo"]

        GET Bar
            path ["bar"]
            -> Json<String>
    }
}

fn main() {}
