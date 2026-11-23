use concord::{fake, reply, Json};

mod model {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    pub struct Option<T>(pub T);
}

type Payload = Vec<Option<Result<u32, String>>>;

macro_rules! grouped {
    ($output:ty) => {
        concord::api! {
            client Grouped {
                base "https://types.test";
                read Json;
                GET get() -> $output;
            }
        }
    };
}

grouped!(::std::option::Option<u32>);

concord::api! {
    client Arrows {
        base "https://types.test";
        read Json;
        GET optional() -> std::option::Option<u32>;
        GET domain() -> ::core::result::Result<u32, String>;
        GET custom() -> model::Option<u32>;
        GET nested() -> Payload { path "nested"; }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let response = |request: &concord::reqwest::Request| {
        Ok(match request.url().path() {
            "/nested" => reply(200, r#"[null,{"Ok":7}]"#),
            _ => reply(200, "7"),
        })
    };
    let client = Arrows::new().with_transport(fake(response));
    assert_eq!(client.optional().await.unwrap(), Some(7));
    assert_eq!(client.domain().await.unwrap(), Ok(7));
    assert_eq!(client.custom().await.unwrap(), model::Option(7));
    assert_eq!(client.nested().await.unwrap(), vec![None, Some(Ok(7))]);
    assert_eq!(Grouped::new().with_transport(fake(response)).get().await.unwrap(), Some(7));
}
