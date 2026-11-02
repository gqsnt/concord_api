use c4 as c4;
mod wire;
mod facade { pub use c4::{api, Json}; }
c4::api! {
    pub client Renamed { base "http://127.0.0.1";
        POST p(body b: String via c4::Json) -> String via c4::Json;
        POST absolute(body b: String via c4::Text) -> String via ::c4::Text;
    }
}
facade::api! { pub client Reexport { base "http://127.0.0.1";
    POST p(body b: String via facade::Json) -> String via facade::Json;
} }
#[tokio::test] async fn renamed_and_reexported() {
    let a = wire::exchange(|u| Renamed::new(wire::http()).with_base_url(u).unwrap().p("value").prepare().unwrap(), br#""reply""#).await;
    let b = wire::exchange(|u| Reexport::new(wire::http()).with_base_url(u).unwrap().p("value").prepare().unwrap(), br#""reply""#).await;
    println!("renamed: {a:?}; reexported: {b:?}");
    assert_eq!(a, b);
    assert_eq!(a.body, br#""value""#);
    assert_eq!(a.content_types, ["application/json"]);
    assert_eq!(a.decoded, Ok("reply".to_owned()));
    let c = wire::exchange(|u| Renamed::new(wire::http()).with_base_url(u).unwrap().absolute("value").prepare().unwrap(), b"reply").await;
    assert_eq!(c.body, b"value"); assert_eq!(c.decoded, Ok("reply".to_owned()));
}
