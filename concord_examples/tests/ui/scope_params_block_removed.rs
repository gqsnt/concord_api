use concord_macros::api;

api! {
    client UiScopeParamsBlockRemoved {
        scheme: https,
        host: "example.com",
    }

    scope platform {
        params {
            region: String
        }
        host[region, "api"]

        GET Ping
        -> Json<()>
        {
            path["ping"]
        }
    }
}

fn main() {}
