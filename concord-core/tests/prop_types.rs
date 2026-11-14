//! C1.8/C10.2 compiler assertions for Concord's public types and engine futures.
#![allow(clippy::unwrap_used)]
use concord_core::{
    __private::{self, Core, Draft, Idempotent, Options, Piece, Prepared, Run},
    answer::{BodyTimeout, KnownKind, KnownOutcome},
    base::BaseError,
    body::{BodyStream, Entries},
    error::{Class, Details, Kept, Kind},
    http::{Method, StatusCode},
    page,
    read::{Incoming, Place, ReadError, StreamError},
    retry::{Decision, Outcome, Tried},
    secret::Exposed,
    transport::{TransportError, TryInfo},
    Answer, Base, Body, ByteStream, Error, Fail, Headers, Http, Items, Next, Pages, Retry, Secret,
    Sending,
};
fn shared<T: Send + Sync + Unpin + 'static>() {}
fn copyable<T: Clone + Send + Sync + Unpin + 'static>() {}
fn send<T: Send + Unpin + 'static>() {}
fn plain<T: Unpin + 'static>() {}
fn send_future<T: std::future::Future + Send + 'static>(_: T) {}
fn borrowed_future<T: std::future::Future + Send>(_: T) {}

#[derive(Clone, Debug, Default)]
struct TestCall {
    core: Core<()>,
    options: Options<()>,
}
impl page::Call for TestCall {
    type Value = Vec<u8>;
    type Problem = ();
    fn at_url(mut self, url: &str) -> Self {
        self.options.at_url(url);
        self
    }
}
impl TestCall {
    fn draft() -> Draft {
        let mut d = Draft::new(Method::GET);
        d.base(&[Piece::Lit("https://example.test")]);
        d
    }
}
impl __private::Call for TestCall {
    fn request(&self) -> Result<concord_core::reqwest::Request, Error> {
        Self::draft().finish(&self.core, &self.options, "props.types")
    }
    fn answered_from(mut self, url: concord_core::reqwest::Url) -> Self {
        self.options.answered_from(url);
        self
    }
    fn prepare(self) -> Result<Prepared<Vec<u8>, ()>, Error> {
        Self::draft().prepare(
            Run::start(&self.core, &self.options, "props.types"),
            self.core,
            self.options,
            Idempotent::ByMethod,
            |response, _| async move {
                Ok(Answer::new(
                    response.status(),
                    response.headers().clone().into(),
                    Vec::new(),
                ))
            },
        )
    }
    fn edit_request(
        mut self,
        edit: impl Fn(&mut concord_core::reqwest::Request) + Send + Sync + 'static,
    ) -> Self {
        self.options.edit_request(edit);
        self
    }
}
#[test]
/// Also proves C10.2.
fn c1_8_all_public_owned_types_hold_their_thread_and_move_contracts() {
    copyable::<Base>();
    copyable::<BaseError>();
    copyable::<Body>();
    copyable::<Secret>();
    shared::<Exposed>();
    copyable::<Headers>();
    copyable::<Retry<String>>();
    copyable::<TryInfo>();
    copyable::<Http>();
    copyable::<BodyTimeout>();
    copyable::<KnownKind>();
    copyable::<Kind>();
    copyable::<Class>();
    copyable::<Kept>();
    copyable::<Place>();
    copyable::<Next<'static>>();
    shared::<Answer<String>>();
    shared::<Error<String>>();
    shared::<Fail<String>>();
    shared::<Details<'static, String>>();
    shared::<TransportError>();
    shared::<ReadError>();
    shared::<StreamError>();
    shared::<Pages<String, String>>();
    shared::<Items<String, String>>();
    shared::<KnownOutcome<'static>>();
    shared::<Outcome<'static, String>>();
    shared::<Tried<'static, String>>();
    copyable::<Decision>();
    send::<Sending<String, String>>();
    send::<ByteStream>();
    send::<BodyStream>();
    send::<Incoming>();
    // Entries intentionally permits a borrowed !Send Serialize value; only its move/lifetime is asserted.
    plain::<Entries<'static>>();
    copyable::<Core<String>>();
    copyable::<Options<String>>();
    copyable::<TestCall>();
    shared::<Prepared<String, String>>();
    shared::<Draft>();
    copyable::<__private::RequestSecrets>();
    copyable::<__private::SecretUrl>();
    copyable::<Idempotent>();
    copyable::<Run>();
    shared::<__private::Written>();
    shared::<__private::ValueRef<String>>();
    shared::<__private::SecretValue<'static, Secret>>();
    shared::<__private::BodySecretValue<'static, Secret>>();
    shared::<__private::CheckedReader<String, Dummy>>();
    // Piece borrows a dyn Value whose trait has no thread bound; no Send/Sync promise is invented.
    plain::<Piece<'static>>();
    let fake = concord_core::fake(|_| Ok(concord_core::reply(200, "ok")));
    fn check_fake<T: Send + Sync + Unpin + 'static>(_: &T) {}
    check_fake(&fake);
    use __private::Call as _;
    send_future(TestCall::default().send_full());
    let mut pages = __private::pages(
        TestCall::default(),
        |_, _: &Answer<Vec<u8>>| None::<TestCall>,
    );
    borrowed_future(pages.try_next());
    send_future(async move { pages.try_next().await });
    let mut items = __private::pages(
        TestCall::default(),
        |_, _: &Answer<Vec<u8>>| None::<TestCall>,
    )
    .items();
    borrowed_future(items.try_next());
    send_future(async move { items.try_next().await });
}
struct Dummy;
impl concord_core::Reader<String> for Dummy {
    async fn read(&self, _: Incoming) -> Result<String, ReadError> {
        Ok(String::new())
    }
}
#[test]
fn c10_2_futures_of_transport_and_read_are_send() {
    fn require_send<T: std::future::Future + Send>(_: T) {}
    let h = Http::new().unwrap();
    let r =
        concord_core::reqwest::Request::new(Method::GET, "https://example.test".parse().unwrap());
    require_send(concord_core::Transport::send(
        &h,
        r,
        TryInfo::new("props.types", 1),
    ));
    require_send(__private::read_body::<String, (), _>(
        &Dummy,
        concord_core::reply(200, "ok"),
        &TryInfo::new("props.types", 1),
    ));
    let _ = StatusCode::OK;
}
