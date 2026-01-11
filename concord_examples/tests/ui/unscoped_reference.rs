use concord_macros::api;

api! {
  client UiUnscoped {
    scheme: https,
    host: "example.com",
    headers {
      "x" = token // ERROR: unscoped
    }
  }
}
fn main() {

}
