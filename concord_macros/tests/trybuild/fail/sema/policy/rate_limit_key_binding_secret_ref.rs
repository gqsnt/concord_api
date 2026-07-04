use concord_macros::api;

api! {
    client RateLimitKeySecretApi {
        base "https://example.com"
        secret token: String
    }

    GET Ping(tenant: String)
        path ["ping"]
        rate_limit key tenant_key = secret.token
        -> Json<String>
}

fn main() {}
