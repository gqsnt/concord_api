use concord_macros::api;

api! {
  client UiUnknownCx {
    scheme: https,
    host: "example.com",
    headers {
      "x" = cx.missing; // ERROR: unknown cx var
    }
  }
}
fn main() {

}
