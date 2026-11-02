#![allow(missing_docs)]
concord::api! {
    pub client Typed(#[sensitive] header("x-api-key") key: String) {
        base "https://api.example.com/v1";
        scope s0(owner: String) {
            path "s0/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s1(owner: String) {
            path "s1/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s2(owner: String) {
            path "s2/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s3(owner: String) {
            path "s3/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s4(owner: String) {
            path "s4/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s5(owner: String) {
            path "s5/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s6(owner: String) {
            path "s6/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s7(owner: String) {
            path "s7/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s8(owner: String) {
            path "s8/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s9(owner: String) {
            path "s9/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s10(owner: String) {
            path "s10/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s11(owner: String) {
            path "s11/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s12(owner: String) {
            path "s12/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s13(owner: String) {
            path "s13/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s14(owner: String) {
            path "s14/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s15(owner: String) {
            path "s15/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s16(owner: String) {
            path "s16/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s17(owner: String) {
            path "s17/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s18(owner: String) {
            path "s18/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
        scope s19(owner: String) {
            path "s19/{owner}";
            GET e0(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e1(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e2(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e3(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e4(id: u64) -> String via concord::Text { path "items/{id}"; }
            GET e5(id: u64) -> String via concord::Text { path "items/{id}"; }
            PUT e6(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            PUT e7(id: u64, body note: String via concord::Text) -> String via concord::Text { path "items/{id}"; }
            GET e8(id: u64) -> Vec<u8> via concord::Binary { path "items/{id}"; }
            PUT e9(id: u64, body data: Vec<u8> via concord::Binary) { path "items/{id}"; }
        }
    }
}
