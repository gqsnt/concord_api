use concord_macros::api;

api! {
  client UiDup {
    scheme: https,
    host: "example.com",
  }

  // same rust field name, different types => must fail
  GET A "x" / {id:u32} query {id:String} -> Json<()>;
}

fn main() {

}
