use concord_macros::api;
use concord_core::prelude::Json;

api! {
  client UiMissingType {
    scheme: https,
    host: "example.com",
  }

  GET One "x/{id}" -> Json<()>;
}
fn main() {

}
