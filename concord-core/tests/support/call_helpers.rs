//! Tests for the preparation and paging helpers used by generated calls.
use super::{Call, ConditionalAccess, ConditionalCall, Core, Options, Prepared, Run};
use crate::__private::{pages_with_pager, prepare_call, prepare_conditional, Draft, Idempotent};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone)]
struct TestCall {
    core: Core<()>,
    options: Options<()>,
    page: u32,
    reads: Arc<AtomicUsize>,
}

impl TestCall {
    const PATH: &'static str = "test.get";

    fn new(core: Core<()>) -> Self {
        Self {
            core,
            options: Options::default(),
            page: 0,
            reads: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn draft(&self) -> Draft {
        let url = if self.page == 0 {
            "https://example.test/first"
        } else {
            "https://example.test/next"
        };
        Draft::from_literal(http::Method::GET, url)
    }
}

impl crate::page::Call for TestCall {
    type Value = u32;
    type Problem = ();

    fn at_url(mut self, url: &str) -> Self {
        self.options.at_url(crate::page::fragmentless(url));
        self
    }
}

impl Call for TestCall {
    fn request(&self) -> Result<reqwest::Request, crate::Error> {
        self.draft().finish(&self.core, &self.options, Self::PATH)
    }

    fn answered_from(mut self, url: reqwest::Url) -> Self {
        self.options.answered_from(url);
        self
    }

    fn prepare(self) -> Result<Prepared<u32, ()>, crate::Error> {
        let run = Run::start(&self.core, &self.options, Self::PATH);
        prepare_call(
            self.draft(),
            run,
            self.core.clone(),
            self.options.clone(),
            Idempotent::ByMethod,
            self,
            |call, response, info| async move {
                if !response.status().is_success() {
                    return Err(crate::__private::refused_plain(response, &info).await);
                }
                call.reads.fetch_add(1, Ordering::SeqCst);
                Ok(crate::Answer::new(
                    response.status(),
                    response.headers().clone().into(),
                    call.page + 7,
                ))
            },
        )
    }

    fn edit_request(
        mut self,
        edit: impl Fn(&mut reqwest::Request) + Send + Sync + 'static,
    ) -> Self {
        self.options.edit_request(edit);
        self
    }
}

impl ConditionalCall for TestCall {
    fn set_if_none_match(&mut self, _: ConditionalAccess<Self>, etag: &str) {
        self.options.if_none_match(etag);
    }
}

#[tokio::test(start_paused = true)]
async fn c10_10_prepare_call_retains_the_call_for_its_response_reader() {
    for status in [200, 304] {
        let call = TestCall::new(Core::default().with_transport(crate::fake(move |request| {
            assert_eq!(request.headers().get("if-none-match").unwrap(), "tag");
            Ok(crate::reply(status, ""))
        })));
        let reads = call.reads.clone();
        let mut call = call;
        call.page = 1;
        let prepared = prepare_conditional(call, "tag").unwrap();
        assert_eq!(prepared.request.url().path(), "/next");
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        let answer = prepared.send().await.unwrap();
        assert_eq!(*answer.value(), (status == 200).then_some(8));
        assert_eq!(reads.load(Ordering::SeqCst), usize::from(status == 200));
    }
}

#[tokio::test(start_paused = true)]
async fn c8_4_pages_with_pager_builds_one_factory_and_reads_each_call() {
    let call =
        TestCall::new(Core::default().with_transport(crate::fake(|_| Ok(crate::reply(200, "")))));
    let reads = call.reads.clone();
    let factories = AtomicUsize::new(0);
    let mut pages = pages_with_pager(call, || {
        factories.fetch_add(1, Ordering::SeqCst);
        move |mut call: TestCall, answer: &crate::Answer<u32>| {
            assert_eq!(*answer.value(), call.page + 7);
            if call.page == 0 {
                call.page = 1;
                Some(call)
            } else {
                None
            }
        }
    });
    assert_eq!(factories.load(Ordering::SeqCst), 1);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(pages.try_next().await.unwrap(), Some(7));
    assert_eq!(pages.try_next().await.unwrap(), Some(8));
    assert!(pages.try_next().await.unwrap().is_none());
    assert_eq!(reads.load(Ordering::SeqCst), 2);
}
