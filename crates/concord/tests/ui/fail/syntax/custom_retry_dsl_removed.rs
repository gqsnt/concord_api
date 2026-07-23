use concord::api;

fn retry_factory() {}

api! {
    client LegacyRetryApi {
        base "http://retry.example.test/";
        retry custom crate::retry_factory;
    }

    GET read() -> NoContent;
}

fn main() {}
