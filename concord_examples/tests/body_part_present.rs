// Path: concord_macros/tests/ex15_body_part_present.rs
use concord_core::internal::BodyPart;
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }

    path "posts" {
        POST Create "" body JsonEncoding<NewPost> -> TextEncoding<String>;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewPost {
    pub title: String,
    pub body: String,
    #[serde(rename = "userId")]
    pub user_id: u32,
}

#[test]
fn body_part_returns_reference_to_endpoint_body() {
    let payload = NewPost {
        title: "t".to_string(),
        body: "b".to_string(),
        user_id: 10,
    };

    let ep = client::endpoints::Create::new(payload.clone());

    type E = client::endpoints::Create;
    let body = <<E as Endpoint<client::ClientCx>>::Body as BodyPart<E>>::body(&ep).unwrap();

    assert_eq!(body, &payload);
    assert_eq!(
        <E as Endpoint<client::ClientCx>>::METHOD,
        http::Method::POST
    );
}
