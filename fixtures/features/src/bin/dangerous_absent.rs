#[cfg(feature = "fixture-absence")]
use runtime::http::HttpConfigBuilder;
#[cfg(feature = "fixture-absence")]
use runtime::{Bytes, api};

#[cfg(feature = "fixture-absence")]
api! {
    client DangerousAbsentApi {
        base "http://dangerous-absent.example.test/";
    }

    GET read() -> Bytes;
}

#[cfg(feature = "fixture-absence")]
fn configure(builder: HttpConfigBuilder) -> HttpConfigBuilder {
    builder.dangerous_custom_retry(())
}

fn main() {
    #[cfg(feature = "fixture-absence")]
    {
        let _client = DangerousAbsentApi::new().expect("client construction");
    }
}
