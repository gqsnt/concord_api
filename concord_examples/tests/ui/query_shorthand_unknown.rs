use concord_macros::api;

api! {
    client QueryShorthandUnknownApi {
        base https "example.com"
    }

    GET Search -> Json<()> {
        query {
            missing
        }
    }
}

fn main() {}
