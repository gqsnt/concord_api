#![cfg(feature = "codecs")]
mod codec {
    pub struct Text;
    impl wire::Encode<String> for Text {
        const CONTENT_TYPE:Option<&'static str>=Some("application/x-custom");
        fn encode(v:String)->Result<wire::reqwest::Body,wire::BodyError>{Ok(format!("CUSTOM:{v}").into())}
    }
}
wire::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(codec::Text b:String);
    }
}
include!("../../migration-output.rs");
#[test]
fn migration_preserves_the_downstream_codec() {
    let h=wire::reqwest::Client::builder().no_proxy().build().unwrap();
    let expected=Expected::new(h.clone()).p("value").prepare().unwrap();
    let migrated=Migrated::new(h).p("value").prepare().unwrap();
    assert_eq!(expected.request().body().unwrap().as_bytes(),Some(&b"CUSTOM:value"[..]));
    println!("expected body: {:?}; migrated body: {:?}",expected.request().body().unwrap().as_bytes(),migrated.request().body().unwrap().as_bytes());
    assert_eq!(migrated.request().body().unwrap().as_bytes(),expected.request().body().unwrap().as_bytes());
}
