#![allow(non_upper_case_globals, dead_code)]
fn __status() -> u16 { 17 }
fn __headers() -> u16 { 18 }
fn __response() -> u16 { 19 }
fn __info() -> u16 { 20 }
fn __draft() -> u16 { 21 }
fn __value() -> u16 { 22 }
fn __body() -> u16 { 23 }
fn __client() -> u16 { 24 }
fn __arm() -> u16 { 25 }
fn __run() -> u16 { 26 }
fn __take() -> u16 { 27 }
fn __retry() -> u16 { 28 }
fn __timeout() -> u16 { 29 }
::concord::api! {
    client Hygienic {
        base "https://hygiene.test";
        read ::concord::Json;
        GET get() -> u16 { query { "value" = __value(); } status { 404 => __status() } }
    }
}
fn main() { assert_eq!(Hygienic::new().get().request().unwrap().url().query(), Some("value=22")); }
