#![allow(dead_code)]
struct Option;
struct Result;
struct Some;
struct None;
struct Ok;
struct Err;
struct String;
struct Vec;
struct Box;
mod concord {}
mod core {}
mod std {}

fn drop<T>(_: T) {
    panic!("the macro resolved the author's drop");
}

#[allow(unused_macros)]
macro_rules! concat {
    ($($tokens:tt)*) => {
        compile_error!("the macro resolved the author's concat");
    };
}

::concord::api! {
    client Shadow {
        base "https://shadow.test";
        read ::concord::Json;
        GET get() -> u32;
        HEAD head() -> bool { status { 204 => true } }
        POST post();
        GET page(start?: u32) -> ::std::vec::Vec<u32> {
            path "page";
            query { start; }
            paginate ::concord::by_offset(start);
        }
        GET stream() -> ::concord::ByteStream { path "stream"; read ::concord::read::Stream; }
    }
}

fn main() {
    ::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = Shadow::new().with_transport(::concord::fake(|request| {
                let response = match request.method().as_str() {
                    "HEAD" => ::concord::reply(204, ""),
                    "POST" => ::concord::reply(200, ""),
                    _ => {
                        let url = request.url();
                        if let ::core::option::Option::Some(etag) =
                            request.headers().get("if-none-match")
                        {
                            assert_eq!(etag, "tag");
                        }
                        if url.path().ends_with("/stream") {
                            ::concord::reply(200, "body")
                        } else if url.path().ends_with("/page") {
                            ::concord::reply(
                                200,
                                if url.query().is_some() { "[]" } else { "[1,2]" },
                            )
                        } else if request.headers().contains_key("if-none-match") {
                            ::concord::reply(304, "")
                        } else {
                            ::concord::reply(200, "7")
                        }
                    }
                };
                ::core::result::Result::Ok(response)
            }));

            assert_eq!(
                client.get().request().unwrap().method(),
                ::concord::http::Method::GET
            );
            assert!(client.get().into_request().is_ok());
            assert_eq!(client.get().await.unwrap(), 7);
            assert_eq!(
                client
                    .get()
                    .read_response(::concord::reply(200, "7"))
                    .await
                    .unwrap(),
                7,
            );
            assert_eq!(client.get().send_raw().await.unwrap().status(), 200);
            assert!(client
                .get()
                .send_if_none_match("tag")
                .await
                .unwrap()
                .value()
                .is_none());
            assert_eq!(
                client
                    .get()
                    .with_retry(::concord::Retry::none())
                    .with_timeout(::std::time::Duration::from_secs(5))
                    .with_idle_timeout(::std::time::Duration::from_secs(1))
                    .edit_request(|_| {})
                    .send_full()
                    .await
                    .unwrap()
                    .into_value(),
                7,
            );
            assert_eq!(
                client.head().request().unwrap().method(),
                ::concord::http::Method::HEAD
            );
            assert_eq!(
                client
                    .head()
                    .send_if_none_match("tag")
                    .await
                    .unwrap()
                    .status(),
                204
            );
            assert!(client.head().await.unwrap());
            client.post().await.unwrap();
            let mut pages = client.page().pages();
            assert_eq!(pages.try_next().await.unwrap().unwrap(), [1, 2]);
            assert!(pages.try_next().await.unwrap().unwrap().is_empty());
            assert!(pages.try_next().await.unwrap().is_none());
            let mut stream = client.stream().await.unwrap();
            assert_eq!(stream.try_next().await.unwrap().unwrap(), "body");
            assert!(stream.try_next().await.unwrap().is_none());
            let mut conditional = client
                .stream()
                .send_if_none_match("tag")
                .await
                .unwrap()
                .into_value()
                .unwrap();
            assert_eq!(conditional.try_next().await.unwrap().unwrap(), "body");
        });
}
