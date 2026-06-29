use concord_macros::api;

pub struct Mapped;

api! {
    client ReservedNoContentMapApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> NoContent
        map Mapped {
            ()
        }
}

fn main() {}
