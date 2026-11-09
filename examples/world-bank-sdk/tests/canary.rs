//! No secret inputs: the public, read-only API has no #[sensitive] values or credentials.
//! A canary in the ordinary country path string is the delivery control. Client, response
//! heads/metadata and errors are checked across seven scenarios, including range traversal.
//! Country/indicator/date handles, Prepared URLs and provider/model text are documented clear
//! data and deliberately not absence-checked with the control canary. Excludes native reqwest and explicit unredacted accessors. No exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error<D: 'static>(c: &mut Canaries, e: &concord::Error<D>) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) {
    // Ordinary request values are printable in Prepared; this is not a secret input.
    let _ = format!("{prepared:?} {prepared:#?}");
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use concord_sdk_world_bank::concord_pagination::Resume;
use concord_sdk_world_bank::{Client, Error};
use std::num::NonZeroU32;

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    c.assert_debug_absent("SDK error metadata", &e.meta());
    if let Error::Concord { source, .. } = e {
        concord_error(c, source);
    }
}

#[tokio::test]
async fn request_control_never_enters_diagnostics() {
    let mut c = Canaries::new("world-bank");
    let country = c.issue();
    for case in 0..7 {
        let echo = country.clone();
        let server = TestServer::start(move |_: &Recorded| {
            reply(
                case,
                "application/json",
                include_str!("../fixtures/year.json"),
                include_str!("../fixtures/error.json"),
                &echo,
            )
        })
        .await;
        let api = Client::new(http())
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        c.assert_debug_absent("client", &api);
        let op = api.indicator(&country, "SP.POP.TOTL", "2006");
        // Generated handles and Prepared deliberately print ordinary path/query values.
        assert!(format!("{op:?} {op:#?}").contains(&country));
        receive(&mut c, op.clone().prepare().unwrap()).await;
        let result = op.send().await;
        c.assert_debug_absent("indicator Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(page) => {
                c.assert_debug_absent("indicator page", &page);
                c.assert_debug_absent("provider paging metadata", &page.metadata);
                c.assert_debug_absent("HTTP metadata", &page.response);
            }
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    assert!(e.to_string().contains("400"));
                    assert_eq!(e.meta().unwrap().status().as_u16(), 400);
                }
            }
        }
        let mut walk = api
            .range(
                &country,
                "SP.POP.TOTL",
                2006,
                2006,
                NonZeroU32::new(50).unwrap(),
                Resume::Start,
            )
            .unwrap();
        c.assert_debug_absent("indicator traversal", &walk);
        let page = walk.next_page().await;
        c.assert_debug_absent("traversal page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &walk.checkpoint());
        c.assert_debug_absent("traversal progress", &walk.progress());
        c.assert_debug_absent("traversal report", &walk.finish());
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 150);
    println!(
        "world-bank: {} renderings; ordinary-string positive control passed",
        c.checked()
    );
}
