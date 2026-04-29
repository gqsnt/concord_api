use concord_macros::api;

api! {
    client QueryShorthandUnknownApi {
        base https "example.com"
    }

    GET Search
        query {
            missing
        }
        -> Json<()>
}

fn main() {}
