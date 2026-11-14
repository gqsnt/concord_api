//! C2 request placement and Base round trips; no transport or socket is involved.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use concord_core::{
    __private::{AdaptValue as _, Core, Draft, Idempotent, Options, Piece, Run, ValueRef},
    error::Kind,
    http::{header::HeaderName, Method},
    reqwest::{Request, Url},
    Base, Secret,
};
use proptest::prelude::*;

fn text() -> BoxedStrategy<String> {
    prop_oneof![
        3 => proptest::collection::vec(any::<char>(), 0..64).prop_map(|c| c.into_iter().collect()),
        2 => "[a-zA-Z0-9 _~./%?&#=+\\\\-]{0,40}",
        1 => prop::sample::select(vec!["", ".", "..", "/", "//", "%2e", "%00", "?q=x#f", "\r\nx: y", "https://user:pass@else.test/a?x#f", "é🦀", "{}{:?}%s"])
            .prop_map(str::to_owned),
    ].boxed()
}

fn decode(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut bytes = text.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let h = (bytes.next().unwrap() as char).to_digit(16).unwrap();
            let l = (bytes.next().unwrap() as char).to_digit(16).unwrap();
            out.push((h * 16 + l) as u8);
        } else {
            out.push(b);
        }
    }
    out
}

fn base_text() -> BoxedStrategy<String> {
    prop_oneof![
        text(),
        (
            prop::sample::select(vec!["http", "HTTPS", "HtTp"]),
            prop::sample::select(vec![
                "example.test",
                "EXAMPLE.test",
                "127.1",
                "0x7f000001",
                "[2001:0DB8::1]",
                "éxample.test",
                "%65xample.test",
                "",
                "user@host"
            ]),
            prop::sample::select(vec!["", ":", ":80", ":443", ":00080", ":65535", ":65536"]),
            prop::sample::select(vec![
                "",
                "/",
                "/v1/",
                "/café/%FF",
                "/a%41",
                "/%2E",
                "/a/../b",
                "/a%2fb",
                "/%25",
                "/a\\b",
                "/?q=x",
                "/#f",
                "//a",
                "/a//",
                "/a ",
                "/\0"
            ])
        )
            .prop_map(|(s, h, p, path)| format!("{s}://{h}{p}{path}")),
    ]
    .boxed()
}

fn check_base(written: &str, base: &Base) {
    let parsed = Url::parse(&base.to_string()).unwrap();
    let (scheme, rest) = written.split_once("://").unwrap();
    let split = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..split];
    let path = if split == rest.len() {
        "/"
    } else {
        &rest[split..]
    };
    let host_end = if authority.starts_with('[') {
        authority.find(']').unwrap() + 1
    } else {
        authority.find(':').unwrap_or(authority.len())
    };
    let host = Url::parse(&format!("{scheme}://{}/", &authority[..host_end])).unwrap();
    assert!(scheme.eq_ignore_ascii_case(parsed.scheme()));
    assert_eq!(host.host(), parsed.host());
    match &authority[host_end..] {
        "" | ":" => assert_eq!(parsed.port(), None),
        port => assert_eq!(
            parsed.port_or_known_default(),
            Some(port[1..].parse::<u16>().unwrap())
        ),
    }
    assert_eq!(decode(path), decode(parsed.path()));
    assert_eq!(parsed.username(), "");
    assert_eq!(parsed.password(), None);
    assert_eq!(parsed.query(), None);
    assert_eq!(parsed.fragment(), None);
    assert!(!written.bytes().any(|b| b.is_ascii_control() || b == b'\\'));
    for part in path
        .trim_start_matches('/')
        .strip_suffix('/')
        .unwrap_or(path.trim_start_matches('/'))
        .split('/')
    {
        let part = decode(part);
        assert!(!matches!(part.as_slice(), b"." | b".."));
        assert!(!part
            .iter()
            .any(|b| b.is_ascii_control() || b" /\\?#%".contains(b)));
    }
    assert_eq!(base.to_string().parse::<Base>().unwrap(), *base);
}

const KEYS: [&str; 6] = ["q", "", "a&b", "é", "a=b#c", "q"];
const HEADERS: [&str; 5] = ["x-one", "X-TWO", "x-one", "x-empty", "X-Two"];

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c2_2_any_accepted_base_has_the_written_components(written in base_text()) {
        if let Ok(base) = written.parse::<Base>() {
            check_base(&written, &base);
        }
    }

    #[test]
    /// Also proves C2.3, C2.5, C2.7.
    fn c2_1_accepted_request_round_trips_every_placement(
        scheme in prop::sample::select(vec!["http", "https"]),
        prefix in prop::sample::select(vec!["", "/v1", "/café/%FF", "/v1/"]),
        host in "[a-zA-Z][a-zA-Z0-9-]{0,20}[a-zA-Z0-9]",
        pieces in proptest::collection::vec(text(), 0..6),
        splat in text(), trailing in any::<bool>(),
        query in proptest::collection::vec((0usize..KEYS.len(), proptest::collection::vec(text(), 0..5)), 0..12),
        headers in proptest::collection::vec((0usize..HEADERS.len(), proptest::collection::vec("[ -~\\t]{0,40}", 0..4)), 0..8),
    ) {
        let base = format!("{scheme}://{host}.example.test{prefix}");
        let base: Base = match base.parse() {
            Ok(base) => base,
            Err(_) => {
                // The ASCII generator can produce malformed A-labels (Xn--0).
                // This property round-trips accepted bases; rejected generated
                // bases must also be invalid to the underlying URL parser.
                prop_assert!(Url::parse(&base).is_err(), "a valid generated base was rejected: {}", base);
                return Ok(());
            }
        };
        let mut draft = Draft::new(Method::GET);
        draft.base_from(&base);
        let mut path = Vec::new();
        for piece in &pieces {
            draft.segment(&[Piece::Lit("p"), Piece::Val("piece", piece)]);
            path.push(format!("p{piece}").into_bytes());
        }
        draft.segments("splat", &splat);
        path.extend(splat.split('/').map(|s| s.as_bytes().to_vec()));
        if trailing {
            draft.segment(&[Piece::Lit("tail/")]);
            path.push(b"tail".to_vec());
            path.push(Vec::new());
        }
        let mut expected_query: Vec<(&str, Vec<String>)> = Vec::new();
        for (k, values) in &query {
            let key = KEYS[*k];
            draft.query(key, &[Piece::Val("value", values)]);
            if let Some((_, old)) = expected_query.iter_mut().find(|(k, _)| *k == key) {
                *old = values.clone();
            } else {
                expected_query.push((key, values.clone()));
            }
        }
        let mut expected_headers: Vec<(String, Vec<String>)> = Vec::new();
        for (k, values) in &headers {
            let key = HEADERS[*k];
            draft.header(key, &[Piece::Val("value", values)]);
            if let Some((_, old)) = expected_headers
                .iter_mut()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
            {
                *old = values.clone();
            } else {
                expected_headers.push((key.to_ascii_lowercase(), values.clone()));
            }
        }
        let result = draft.finish::<()>(&Core::default(), &Options::default(), "props.request");
        if pieces.iter().any(String::is_empty)
            || splat.is_empty()
            || splat.split('/').any(|s| s == "." || s == "..")
        {
            prop_assert_eq!(result.unwrap_err().kind(), Kind::Build);
        } else {
            let request = result.unwrap();
            let url = Url::parse(request.url().as_str()).unwrap();
            let expected_host = format!("{}.example.test", host.to_ascii_lowercase());
            prop_assert_eq!(url.host_str(), Some(expected_host.as_str()));
            prop_assert_eq!(url.scheme(), scheme);
            prop_assert_eq!(url.port(), None);
            prop_assert_eq!(url.username(), "");
            prop_assert_eq!(url.password(), None);
            let base_url = Url::parse(&base.to_string()).unwrap();
            let mut expected_path: Vec<_> = base_url
                .path()
                .strip_suffix('/')
                .unwrap_or(base_url.path())
                .split('/')
                .skip(1)
                .map(decode)
                .collect();
            expected_path.extend(path);
            prop_assert_eq!(
                url.path()
                    .split('/')
                    .skip(1)
                    .map(decode)
                    .collect::<Vec<_>>(),
                expected_path
            );
            let pairs: Vec<_> = expected_query
                .into_iter()
                .flat_map(|(k, vs)| vs.into_iter().map(move |v| (k.to_owned(), v)))
                .collect();
            prop_assert_eq!(
                url.query_pairs()
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect::<Vec<_>>(),
                pairs.clone()
            );
            prop_assert_eq!(url.query().is_none(), pairs.is_empty());
            prop_assert_eq!(url.fragment(), None);
            let mut count = 0;
            for (key, values) in expected_headers {
                count += values.len();
                prop_assert_eq!(
                    request
                        .headers()
                        .get_all(key)
                        .iter()
                        .map(|v| v.to_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    values
                );
            }
            prop_assert_eq!(request.headers().len(), count);
        }
    }

    #[test]
    fn c2_1_any_draft_is_a_request_or_build_error_never_a_panic(
        base in base_text(), host in text(), path in text(), splat in text(), value in text(),
        header in prop::sample::select(vec!["", "host", "x-ok", "\r\nx", "é", "content-length", "content-type"]),
        use_host in any::<bool>(),
    ) {
        let mut d = Draft::new(Method::POST);
        if use_host {
            d.base(&[
                Piece::Lit("https://"),
                Piece::Val("host", &host),
                Piece::Lit(".test/"),
            ]);
        } else if let Ok(base) = base.parse::<Base>() {
            d.base_from(&base);
        }
        d.segment(&[Piece::Lit(""), Piece::Val("path", &path)]);
        d.segments("splat", &splat);
        d.query("a&b", &[Piece::Val("query", &value)]);
        d.header(header, &[Piece::Val("header", &value)]);
        if let Err(e) = d.finish::<()>(&Core::default(), &Options::default(), "props.any") {
            prop_assert_eq!(e.kind(), Kind::Build);
        }
    }

    #[test]
    fn c2_1_host_hole_stays_in_one_host_label(host in text()) {
        let mut d = Draft::new(Method::GET);
        d.base(&[
            Piece::Lit("https://"),
            Piece::Val("host", &host),
            Piece::Lit(".example.test/v1"),
        ]);
        if let Ok(r) = d.finish::<()>(&Core::default(), &Options::default(), "props.host") {
            let expected_host = format!("{}.example.test", host.to_ascii_lowercase());
            prop_assert_eq!(r.url().host_str(), Some(expected_host.as_str()));
            prop_assert_eq!(r.url().path(), "/v1");
            prop_assert_eq!(r.url().query(), None);
            prop_assert_eq!(r.url().fragment(), None);
            prop_assert_eq!(r.url().username(), "");
        }
    }
}

fn diagnostics(error: &(dyn std::error::Error + 'static)) -> String {
    let mut result = format!("{error} {error:?}");
    let mut source = error.source();
    for _ in 0..32 {
        let Some(e) = source else {
            return result;
        };
        result.push_str(&format!(" {e} {e:?}"));
        source = e.source();
    }
    panic!("cyclic error source");
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    /// Also proves C10.7.
    fn c6_1_secrets_are_redacted_and_placements_are_marked(raw in text()) {
        // Delimit the payload: even an empty payload or "e" is distinguishable from diagnostic prose.
        let raw = format!("SECRET_BEGIN{raw}SECRET_END");
        let secret = Secret::new(raw.clone());
        let core = Core::<()>::default();
        let options = Options::default();
        assert!(!format!("{secret:?} {:?} {core:?} {options:?}", secret.expose()).contains(&raw));
        let mut d = Draft::new(Method::GET);
        d.base(&[Piece::Lit("https://example.test")]);
        d.segment(&[Piece::Lit(""), Piece::Val("path", &secret)]);
        d.query("token", &[Piece::Val("token", &secret)]);
        d.header("x-secret", &[Piece::Val("token", &secret)]);
        assert!(!format!("{d:?}").contains(&raw));
        let run = Run::start(&core, &options, "props.secret");
        let prepared = d.prepare::<(), (), _, _>(
            run,
            core.clone(),
            options.clone(),
            Idempotent::ByMethod,
            |_, _| async {
                Ok(concord_core::Answer::new(
                    concord_core::http::StatusCode::OK,
                    concord_core::Headers::default(),
                    (),
                ))
            },
        );
        match prepared {
            Ok(p) => {
                assert!(!format!("{p:?} {:?}", p.secrets).contains(&raw));
                prop_assert!(p.request.headers()[HeaderName::from_static("x-secret")].is_sensitive());
                prop_assert_eq!(&p.secrets.query, &vec!["token"]);
                prop_assert!(!p.secrets.url.as_ref().unwrap().path.is_empty());
            }
            Err(e) => {
                prop_assert_eq!(e.kind(), Kind::Build);
                prop_assert!(!diagnostics(&e).contains(&raw));
            }
        }
        // Independently put the exact arbitrary payload in every permitted URL/header place.
        for place in 0..6 {
            let mut d = Draft::new(Method::GET);
            if place == 0 {
                d.base(&[
                    Piece::Lit("https://"),
                    Piece::Val("host", &secret),
                    Piece::Lit(".test"),
                ]);
            } else if place == 1 {
                d.base(&[
                    Piece::Lit("https://example.test:"),
                    Piece::Val("port", &secret),
                ]);
            } else {
                d.base(&[Piece::Lit("https://example.test")]);
            }
            match place {
                2 => d.segment(&[Piece::Lit(""), Piece::Val("path", &secret)]),
                3 => d.segments("path", &secret),
                4 => d.query("token", &[Piece::Val("token", &secret)]),
                5 => d.header(
                    "x-secret",
                    &[Piece::Lit("Bearer "), Piece::Val("token", &secret)],
                ),
                _ => {}
            }
            // Trigger a later engine Build error even when the secret itself is accepted.
            d.header("invalid header", &[Piece::Lit("x")]);
            let e = d.finish::<()>(&core, &options, "props.secret").unwrap_err();
            prop_assert!(!diagnostics(&e).contains(&raw));
        }
        let body = concord_core::Body::from(raw.clone());
        assert!(!format!("{body:?}").contains(&raw));
        let mut entries = concord_core::body::Entries::new();
        entries.__push("value", raw.clone());
        assert!(!format!("{entries:?}").contains(&raw));
        let e: concord_core::Error = concord_core::__private::build_error(
            "props.source",
            Box::new(std::io::Error::other("author failure")),
        );
        prop_assert!(!diagnostics(&e).contains(&raw));
    }
}

#[test]
fn c2_7_header_names_are_case_insensitive() {
    let mut d = Draft::new(Method::GET);
    d.base(&[Piece::Lit("https://example.test")]);
    d.header("X-A", &[Piece::Lit("old")]);
    d.header("x-a", &[Piece::Lit("new")]);
    let r: Request = d
        .finish::<()>(&Core::default(), &Options::default(), "props.header")
        .unwrap();
    assert_eq!(r.headers().len(), 1);
    assert_eq!(r.headers()["x-a"], "new");
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c4_1_one_shot_body_is_never_repeated(bytes in proptest::collection::vec(any::<u8>(),0..1024), tries in 2u32..100, status in prop::sample::select(vec![408u16,429,502,503,504])) {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let count = Arc::new(AtomicUsize::new(0));
        let sent = count.clone();
        let core = Core::<()>::default()
            .with_retry(
                concord_core::Retry::standard()
                    .tries(tries)
                    .not_applied_on(status)
                    .when(|_| Some(std::time::Duration::ZERO)),
            )
            .with_transport(concord_core::fake(move |_| {
                sent.fetch_add(1, Ordering::SeqCst);
                Ok(concord_core::reply(status, ""))
            }));
        let options = Options::default();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut d = Draft::new(Method::POST);
            d.base(&[Piece::Lit("https://example.test")]);
            let inner = concord_core::Body::from(bytes).into_stream().unwrap();
            d.body(concord_core::Body::stream(inner));
            let p = d
                .prepare::<(), (), _, _>(
                    Run::start(&core, &options, "props.oneshot"),
                    core,
                    options,
                    Idempotent::Yes,
                    |response, info| async move {
                        Err(concord_core::__private::refused_plain(response, &info).await)
                    },
                )
                .unwrap();
            assert_eq!(p.send().await.unwrap_err().kind(), Kind::Refused);
        });
        prop_assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    /// Also proves C8.4.
    fn c2_7_secret_marks_cover_each_accepted_placement(raw in "[a-z][a-z0-9]{0,20}"){
        let secret = Secret::new(raw.clone());
        let optional = ValueRef(Some(secret));
        let adapted = (&optional).adapt();
        let absent = ValueRef(None::<Secret>);
        let absent = (&absent).adapt();
        let core = Core::<()>::default();
        let options = Options::default();
        let mut d = Draft::new(Method::GET);
        d.base(&[
            Piece::Lit("https://"),
            Piece::Val("host", &adapted),
            Piece::Lit(".example.test/base/"),
            Piece::Val("prefix", &adapted),
        ]);
        d.segment(&[Piece::Lit("x"), Piece::Val("path", &adapted)]);
        d.segments("splat", &adapted);
        d.query(
            "secret",
            &[Piece::Lit("prefix:"), Piece::Val("pair", &adapted)],
        );
        d.header(
            "x-secret",
            &[Piece::Lit("Bearer "), Piece::Val("header", &adapted)],
        );
        d.query("absent", &[Piece::Val("absent", &absent)]);
        d.header("x-absent", &[Piece::Val("absent", &absent)]);
        let p = d
            .prepare::<(), (), _, _>(
                Run::start(&core, &options, "props.marks"),
                core,
                options,
                Idempotent::ByMethod,
                |_, _| async {
                    Ok(concord_core::Answer::new(
                        concord_core::http::StatusCode::OK,
                        concord_core::Headers::default(),
                        (),
                    ))
                },
            )
            .unwrap();
        prop_assert!(!p.request.headers().contains_key("x-absent"));
        prop_assert!(p.request.headers()["x-secret"].is_sensitive());
        prop_assert_eq!(p.secrets.query, vec!["secret"]);
        let marks = p.secrets.url.unwrap();
        prop_assert_eq!(marks.host.len(), 1);
        prop_assert_eq!(marks.path.len(), 3);
        let host = p.request.url().host_str().unwrap();
        for range in marks.host {
            prop_assert_eq!(&host[range], raw.as_str());
        }
        for range in marks.path {
            prop_assert_eq!(&p.request.url().path()[range], raw.as_str());
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn c2_5_arbitrary_query_names_preserve_pairs_and_override_positions(
        entries in proptest::collection::vec((text(),proptest::collection::vec(text(),0..4)),0..12),
    ) {
        let mut d = Draft::new(Method::GET);
        d.base(&[Piece::Lit("https://example.test/root")]);
        let mut expected: Vec<(String, Vec<String>)> = Vec::new();
        for (key, values) in entries {
            // Draft's program names are static. Leak only the generated test names,
            // bounded by 12 names per case and the finite case count.
            let name: &'static str = Box::leak(key.clone().into_boxed_str());
            d.query(name, &[Piece::Val("values", &values)]);
            if let Some((_, old)) = expected.iter_mut().find(|(k, _)| *k == key) {
                *old = values;
            } else {
                expected.push((key, values));
            }
        }
        let r = d
            .finish::<()>(&Core::default(), &Options::default(), "props.names")
            .unwrap();
        let expected: Vec<_> = expected
            .into_iter()
            .flat_map(|(k, vs)| vs.into_iter().map(move |v| (k.clone(), v)))
            .collect();
        prop_assert_eq!(
            r.url()
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect::<Vec<_>>(),
            expected
        );
        prop_assert_eq!(r.url().path(), "/root");
        prop_assert_eq!(r.url().fragment(), None);
        prop_assert_eq!(r.url().host_str(), Some("example.test"));
    }
    #[test]
    fn c2_7_arbitrary_header_names_are_checked_without_changing_values(
        key in prop_oneof![text(),"[A-Za-z][A-Za-z0-9!#$%&'*+.^_`|~-]{0,32}"],
        values in proptest::collection::vec(prop_oneof![text(),"[ -~\\t]{0,40}"],0..5),
    ) {
        let mut d = Draft::new(Method::GET);
        d.base(&[Piece::Lit("https://example.test/root")]);
        let name: &'static str = Box::leak(key.clone().into_boxed_str());
        d.header(name, &[Piece::Val("values", &values)]);
        match d.finish::<()>(&Core::default(), &Options::default(), "props.headers") {
            Err(e) => prop_assert_eq!(e.kind(), Kind::Build),
            Ok(r) => {
                prop_assert_eq!(r.url().as_str(), "https://example.test/root");
                let name = HeaderName::from_bytes(key.as_bytes()).unwrap();
                prop_assert_eq!(
                    r.headers()
                        .get_all(name)
                        .iter()
                        .map(|v| v.to_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    values
                );
            }
        }
    }
}
