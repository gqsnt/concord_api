// Names that a module where `api!` is written may well declare: a trait whose methods every
// type has, named like those that generated code calls; `const client`, `const status`,
// `const outer`, `const etag`, `const retry`; `type str`; modules named `core`, `std`,
// `concord`, `clippy`, `diagnostic`; the names of the prelude. None of them reaches what the macro
// writes: the hostile module's client sends the same requests as the clean one. A client
// named `Diagnostic`, with a scope `diagnostic`, compiles too.
#![allow(dead_code, non_upper_case_globals, non_camel_case_types, unused_imports, unused)]

pub mod clean {
    ::concord::api! {
        pub client Probe(key: ::concord::Secret, region?: ::std::string::String) {
            base "https://api.test";
            headers { "authorization" = "Bearer {key}"; }
            query { region; }
            read ::concord::Json;
            scope outer(id: u64) {
                path "items/{id}";
                GET get(n: u32, tag?: ::std::string::String) -> ::std::vec::Vec<u32> {
                    query { n; tag; "client_region" = client.region; "id" = outer.id; }
                    headers { tag; }
                    status { 410 { status } => { let _ = status; ::std::vec::Vec::new() } }
                }
                POST create(count: u64, note?: ::std::string::String) { body ::concord::json { count; note; } }
                GET list(start?: u32) -> ::std::vec::Vec<u32> { query { start; } paginate ::concord::by_offset(start); }
                METHOD "PURGE" purge();
            }
        }
    }
}

pub mod hostile {
    pub trait Everything {
        fn adapt(self) -> u32 where Self: Sized { 7 }
        fn as_ref(self) -> u32 where Self: Sized { 7 }
        fn as_u16(self) -> u16 where Self: Sized { 7 }
        fn is_success(self) -> bool where Self: Sized { true }
        fn as_bytes(self) -> u32 where Self: Sized { 7 }
        fn map_err(self) -> u32 where Self: Sized { 7 }
        fn base(self) -> u32 where Self: Sized { 7 }
        fn base_from(self) -> u32 where Self: Sized { 7 }
        fn base_value(self) -> u32 where Self: Sized { 7 }
        fn body(self) -> u32 where Self: Sized { 7 }
        fn literal_path(self) -> u32 where Self: Sized { 7 }
        fn segment(self) -> u32 where Self: Sized { 7 }
        fn segment_value(self) -> u32 where Self: Sized { 7 }
        fn segments(self) -> u32 where Self: Sized { 7 }
        fn trailing_separator(self) -> u32 where Self: Sized { 7 }
        fn query(self) -> u32 where Self: Sized { 7 }
        fn query_value(self) -> u32 where Self: Sized { 7 }
        fn header(self) -> u32 where Self: Sized { 7 }
        fn header_value(self) -> u32 where Self: Sized { 7 }
        fn description_idle_timeout(self) -> u32 where Self: Sized { 7 }
        fn add_middleware(self) -> u32 where Self: Sized { 7 }
        fn debug_struct(self) -> u32 where Self: Sized { 7 }
        fn finish_non_exhaustive(self) -> u32 where Self: Sized { 7 }
        fn with_retry(self) -> u32 where Self: Sized { 7 }
        fn with_timeout(self) -> u32 where Self: Sized { 7 }
        fn send(self) -> u32 where Self: Sized { 7 }
        fn to_owned(&self) -> u32 { 7 }
        fn into_value(self) -> u32 where Self: Sized { 7 }
    }
    impl<T: ?Sized> Everything for T {}
    pub trait AlsoDefault {
        fn default() -> Self;
    }
    impl<T: ::core::default::Default> AlsoDefault for T {
        fn default() -> Self {
            <T as ::core::default::Default>::default()
        }
    }
    pub const client: u8 = 9;
    pub const status: u8 = 9;
    pub const outer: u8 = 9;
    pub const etag: u8 = 9;
    pub const retry: u8 = 9;
    pub type str = u32;
    pub mod clippy {}
    pub mod diagnostic {}
    pub mod concord {}
    pub mod core {}
    pub mod std {}
    pub type Option = ();
    pub type Result = ();
    pub type String = ();
    pub type Vec = ();
    pub type Box = ();
    pub struct Some;
    pub struct None;
    pub struct Ok;
    pub struct Err;
    pub trait Clone {}
    pub trait Default {}
    pub trait Send {}
    pub trait Sync {}
    pub trait Debug {}
    pub trait Future {}
    pub trait IntoFuture {}

    ::concord::api! {
        pub client Probe(key: ::concord::Secret, region?: ::std::string::String) {
            base "https://api.test";
            headers { "authorization" = "Bearer {key}"; }
            query { region; }
            read ::concord::Json;
            scope outer(id: u64) {
                path "items/{id}";
                GET get(n: u32, tag?: ::std::string::String) -> ::std::vec::Vec<u32> {
                    query { n; tag; "client_region" = client.region; "id" = outer.id; }
                    headers { tag; }
                    status { 410 { status } => { let _ = status; ::std::vec::Vec::new() } }
                }
                POST create(count: u64, note?: ::std::string::String) { body ::concord::json { count; note; } }
                GET list(start?: u32) -> ::std::vec::Vec<u32> { query { start; } paginate ::concord::by_offset(start); }
                METHOD "PURGE" purge();
            }
        }
    }
}

pub mod named {
    concord::api! {
        pub client Diagnostic {
            base "https://api.test";
            GET ping();
            scope diagnostic { GET ping(); }
        }
    }
}

fn sent(request: concord::reqwest::Request) -> String {
    let mut headers: Vec<String> = request
        .headers()
        .iter()
        .map(|(k, v)| format!("{k}={v:?}"))
        .collect();
    headers.sort();
    let body = request
        .body()
        .and_then(|b| b.as_bytes())
        .map(|b| String::from_utf8_lossy(b).into_owned());
    format!("{} {} {headers:?} {body:?}", request.method(), request.url())
}

macro_rules! both {
    (|$client:ident| $call:expr) => {{
        let clean = {
            use clean::Probe as $client;
            $call
        };
        let hostile = {
            use hostile::Probe as $client;
            $call
        };
        (clean, hostile)
    }};
}

fn main() {
    let requests = [
        both!(|P| sent(P::new("k").region("eu").outer(5).get(3).tag("x").request().unwrap())),
        both!(|P| sent(P::new("k").outer(5).create(2).note("n").request().unwrap())),
        both!(|P| sent(P::new("k").outer(5).list().start(4).request().unwrap())),
        both!(|P| sent(P::new("k").outer(5).purge().request().unwrap())),
    ];
    for (clean, hostile) in requests {
        assert_eq!(clean, hostile);
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    for status_code in [200, 410] {
        let (clean, hostile) = both!(|P| {
            let call = P::new("k").outer(5).get(3);
            let read = call.read_response(concord::reply(status_code, "[1, 2]"));
            format!("{:?}", runtime.block_on(read).map_err(|e| e.to_string()))
        });
        assert_eq!(clean, hostile);
    }
    let (clean, hostile) = both!(|P| format!("{:?} {:?}", P::new("k"), P::new("k").outer(5)));
    assert_eq!(clean, hostile);
    let _ = sent(named::Diagnostic::new().diagnostic().ping().request().unwrap());
    let _ = std::future::IntoFuture::into_future(named::Diagnostic::new().ping());
    // Awaiting, pages and a conditional send are written for both.
    let _ = std::future::IntoFuture::into_future(hostile::Probe::new("k").outer(5).get(3));
    let _ = hostile::Probe::new("k").outer(5).list().pages();
    let _ = hostile::Probe::new("k").outer(5).get(3).send_if_none_match("\"v\"");
}
