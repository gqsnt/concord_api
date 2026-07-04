use concord_macros::api;

api! {
    client PathBaseApi {
        base "https://example.com/api"
    }
}

api! {
    client QueryBaseApi {
        base "https://example.com?x=1"
    }
}

api! {
    client FragmentBaseApi {
        base "https://example.com#frag"
    }
}

fn main() {}
