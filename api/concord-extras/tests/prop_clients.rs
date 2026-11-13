//! C10.2/C10.7 assertions on the paper's handwritten generated-code model.
#![cfg(all(feature = "json", feature = "form"))]
#![allow(clippy::unwrap_used)]
extern crate self as concord;
#[doc(hidden)]
pub use concord_core::*;
#[doc(hidden)]
pub use concord_extras::{
    body::{form, json},
    page::{by_cursor, by_link, by_next_url, by_offset, by_offset_limit, by_page},
    read::{Binary, Json, Stream, Text},
};
/// The core reader API plus shipped readers, as in the facade.
pub mod read {
    pub use concord_core::read::*;
    pub use concord_extras::read::*;
}
// Reuse the checked-in model without copying it or adding a dependency cycle.
#[path = "../../tour/src/data.rs"]
mod data;
pub use data::*;
include!("../../tour/src/declarations.rs");
include!("../../tour/src/tasks.rs");
include!("../../tour/src/riot.rs");
include!("../../tour/src/files.rs");
include!("../../tour/src/grant.rs");
include!("../../tour/src/feed.rs");
include!("../../tour/src/shapes.rs");

use proptest::prelude::*;
use std::sync::{Arc, Mutex};
#[derive(Debug, PartialEq)]
struct Wire {
    method: http::Method,
    url: String,
    headers: http::HeaderMap,
    body: Option<Vec<u8>>,
}
fn wire(r: &reqwest::Request) -> Wire {
    Wire {
        method: r.method().clone(),
        url: r.url().to_string(),
        headers: r.headers().clone(),
        body: r.body().and_then(|b| b.as_bytes().map(<[u8]>::to_vec)),
    }
}
fn diagnostics(error: &(dyn std::error::Error + 'static)) -> String {
    let mut out = String::new();
    let mut error = Some(error);
    for _ in 0..32 {
        match error {
            None => return out,
            Some(e) => {
                out.push_str(&format!("{e} {e:?}"));
                error = e.source();
            }
        }
    }
    assert!(error.is_none());
    out
}
proptest! {
    #![proptest_config(ProptestConfig{failure_persistence:None,..Default::default()})]
    #[test]
    /// Also proves C10.10.
    fn c2_3_inspected_request_is_exactly_the_request_sent(
        puuid in ".{0,80}",secret in ".{0,80}",start in prop::option::of(any::<u32>()),count in prop::option::of(any::<u32>()),
    ){
        let sent = Arc::new(Mutex::new(Vec::new()));
        let captured = sent.clone();
        let client = Riot::new(Secret::new(secret)).with_transport(fake(move |request| {
            captured.lock().unwrap().push(wire(request));
            Ok(reply_with(
                200,
                &[("content-type", "application/json")],
                "[]",
            ))
        }));
        let call = client
            .matches(MatchRoute::Europe)
            .ids(Puuid::from(puuid.as_str()))
            .start(start)
            .count(count);
        let expected = call.request();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let answer = rt.block_on(call.send());
        match expected {
            Ok(r) => {
                prop_assert!(answer.is_ok(), "{:?}", answer);
                assert_eq!(sent.lock().unwrap().as_slice(), &[wire(&r)]);
            }
            Err(e) => {
                prop_assert_eq!(e.kind(), error::Kind::Build);
                prop_assert_eq!(answer.unwrap_err().kind(), error::Kind::Build);
                prop_assert!(sent.lock().unwrap().is_empty());
            }
        }
    }
    #[test]
    /// Also proves C10.7.
    fn c6_1_clients_handles_calls_and_engine_errors_hide_secrets(
        payload in ".{0,100}",status in prop::sample::select(vec![200u16,400,401,429,500,503]),
        class in prop::sample::select(vec![error::Class::Connect,error::Class::Timeout,error::Class::Body,error::Class::Blocked,error::Class::Other]),
    ){
        let secret = format!("SECRET_BEGIN{payload}SECRET_END");
        let shared = Secret::new(secret.clone());
        let client = Riot::new(shared.clone());
        let handle = client.account(AccountRoute::Europe);
        let call = handle.me(shared.clone());
        assert!(!format!("{client:?} {handle:?} {call:?}").contains(&secret));
        let accepted = call.request().is_ok();
        let files = Files::new(shared.clone());
        assert!(
            !format!("{files:?} {:?}", files.grant("grant", "id", shared.clone())).contains(&secret)
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let bad_base = client
                .clone()
                .with_base("not a base")
                .account(AccountRoute::Europe)
                .me(shared.clone())
                .send()
                .await
                .unwrap_err();
            assert_eq!(bad_base.kind(), error::Kind::Build);
            assert!(!diagnostics(&bad_base).contains(&secret));
            let failed = client
                .clone()
                .with_transport(fake(move |_| Err(transport::TransportError::new(class))))
                .account(AccountRoute::Europe)
                .me(shared.clone())
                .send()
                .await
                .unwrap_err();
            assert_eq!(
                failed.kind(),
                if accepted {
                    error::Kind::Transport
                } else {
                    error::Kind::Build
                }
            );
            assert!(!diagnostics(&failed).contains(&secret));
            let body = secret.clone();
            let response = client
                .with_transport(fake(move |_| Ok(reply(status, body.clone()))))
                .account(AccountRoute::Europe)
                .me(shared)
                .send()
                .await
                .unwrap_err();
            assert_eq!(
                response.kind(),
                if !accepted {
                    error::Kind::Build
                } else if status == 200 {
                    error::Kind::Mismatch
                } else {
                    error::Kind::Refused
                }
            );
            assert!(!diagnostics(&response).contains(&secret));
        });
    }
}
#[test]
fn c10_2_every_model_client_handle_call_and_send_future_holds_contracts() {
    fn typed<T: Clone + Send + Sync + Unpin + 'static>() {}
    typed::<Tasks>();
    typed::<Riot>();
    typed::<Files>();
    typed::<Feed>();
    typed::<tasks::List>();
    typed::<tasks::Create>();
    typed::<tasks::Get>();
    typed::<tasks::Delete>();
    typed::<riot::Matches>();
    typed::<riot::Account>();
    typed::<riot::matches::Ids>();
    typed::<riot::matches::Get>();
    typed::<riot::account::ByRiotId>();
    typed::<riot::account::Me>();
    typed::<files::Grant>();
    typed::<files::Exists>();
    typed::<files::Readme>();
    typed::<files::Download>();
    typed::<files::Upload>();
    typed::<files::Search>();
    typed::<feed::List>();
    typed::<feed::ByCursor>();
    typed::<feed::Linked>();
    fn future<T: std::future::Future + Send + 'static>(_: T) {}
    future(Tasks::new().list().send());
    future(
        Tasks::new()
            .create(NewTask {
                title: String::new(),
            })
            .send(),
    );
    future(Tasks::new().get(1u64).send());
    future(Tasks::new().delete(1u64).send());
    let riot = Riot::new("key");
    future(
        riot.matches(MatchRoute::Europe)
            .ids(Puuid::from("p"))
            .send(),
    );
    future(
        riot.matches(MatchRoute::Europe)
            .get(MatchId::from("id"))
            .send(),
    );
    future(riot.account(AccountRoute::Europe).me("key").send());
    future(
        riot.account(AccountRoute::Europe)
            .by_riot_id("name", "tag")
            .send(),
    );
    let files = Files::new("key");
    future(files.grant("g", "id", "key").send());
    future(files.exists(DocId::from("id")).send());
    future(files.readme("repo").send());
    future(files.download(FileId::from("id")).send());
    future(files.upload("name", Body::from("bytes")).send());
    future(files.search("query").send());
    future(Feed::new().list().send());
    future(Feed::new().by_cursor().send());
    future(Feed::new().linked().send());
    let mut pages = Feed::new().list().pages();
    fn borrowed<T: std::future::Future + Send>(_: T) {}
    borrowed(pages.try_next());
    future(async move { pages.try_next().await });
}
