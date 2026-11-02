#![allow(dead_code,unused_imports)]
mod facade { pub use c4::{api,Json}; }
mod absolute_body {
c4::api! { pub client A { base "http://127.0.0.1"; POST p(body b: String via ::c4::Text); } }

}
mod absolute_output {
c4::api! { pub client A { base "http://127.0.0.1"; GET p() -> String via ::c4::Text; } }

}
mod absolute_custom_body {
extern crate c4 as codec;
c4::api! { pub client A { base "http://127.0.0.1"; POST p(body b: String via ::codec::Text); } }

}
mod absolute_alias_output {
extern crate c4 as codec;
c4::api! { pub client A { base "http://127.0.0.1"; GET p() -> String via ::codec::Text; } }

}
mod renamed_body { use crate::facade; c4::api! { pub client A { base "http://127.0.0.1"; POST p(body b: String via c4::Json); } } }
mod renamed_output { use crate::facade; c4::api! { pub client A { base "http://127.0.0.1"; GET p() -> String via c4::Json; } } }
mod reexported_body { use crate::facade; c4::api! { pub client A { base "http://127.0.0.1"; POST p(body b: String via facade::Json); } } }
mod reexported_output { use crate::facade; c4::api! { pub client A { base "http://127.0.0.1"; GET p() -> String via facade::Json; } } }
