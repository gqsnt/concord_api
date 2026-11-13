#![allow(non_upper_case_globals, dead_code)]
const __status: u16 = 17;
const __headers: u16 = 18;
const __response: u16 = 19;
const __info: u16 = 20;
const __draft: u16 = 21;
const __value: u16 = 22;
const __body: u16 = 23;
const __client: u16 = 24;
const __arm: u16 = 25;
const __run: u16 = 26;
const __take: u16 = 27;
const __retry: u16 = 28;
const __timeout: u16 = 29;
::concord::api! {
    client Hygienic {
        base "https://hygiene.test";
        read ::concord::Json;
        GET get() -> u16 { query { "value" = __value; } status { 404 => __status } }
    }
}
fn main() { assert_eq!(Hygienic::new().get().request().unwrap().url().query(), Some("value=22")); }
