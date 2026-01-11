use concord_macros::api;
use concord_core::prelude::Json;

api! {
  client UiDup {
    scheme: https,
    host: "example.com",
  }

  // same rust field name, different types => must fail
  GET A "x/{id:u32}" -> Json<()>;
  GET B "y/{id:String}" -> Json<()>;
}

fn main() {

}
