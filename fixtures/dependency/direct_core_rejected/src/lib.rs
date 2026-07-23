use concord_core::prelude::Bytes;
use concord_macros::api;

api! {
    client DirectCoreApi {
        base "https://direct-core.example.test";
    }

    GET read() -> Bytes;
}
