use concord_macros::api;

api! {
    client InvalidExposeSecretHeaderApi {
        base "https://example.com"
        var trace_id: String
    }

    GET ExposeHeader
        path ["header"]
        headers {
            "X-Trace" = vars.trace_id.expose()
        }
        -> Json<String>
}

api! {
    client InvalidExposeSecretQueryApi {
        base "https://example.com"
        var trace_id: String
    }

    GET ExposeQuery
        path ["query"]
        query {
            trace = vars.trace_id.expose_secret()
        }
        -> Json<String>
}

fn main() {}
