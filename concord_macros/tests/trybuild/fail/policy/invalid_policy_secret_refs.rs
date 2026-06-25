use concord_macros::api;

api! {
    client InvalidPolicySecretHeaderApi {
        base "https://example.com"
        secret token: String
    }

    GET HeaderRef
        path ["header"]
        headers {
            "X-Token" = secret.token
        }
        -> Json<String>
}

api! {
    client InvalidPolicySecretQueryApi {
        base "https://example.com"
        secret token: String
    }

    GET QueryRef
        path ["query"]
        query {
            token = secret.token
        }
        -> Json<String>
}

api! {
    client InvalidPolicySecretTimeoutApi {
        base "https://example.com"
        secret token: String
    }

    GET TimeoutRef
        path ["timeout"]
        timeout: secret.token
        -> Json<String>
}

api! {
    client InvalidPolicySecretPaginationApi {
        base "https://example.com"
        secret token: String
    }

    GET PageRef(start: u64 = 0)
        path ["page"]
        query {
            start
        }
        paginate OffsetLimitPagination {
            offset = secret.token,
            limit = start
        }
        -> Json<Vec<String>>
}

fn main() {}
