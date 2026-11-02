#![cfg(feature = "codecs")]
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use wire::{BodyError, Encode, ErrorKind, ToHeaderValue};
use wire::bytes::Bytes;
use wire::reqwest::{self, header::HeaderValue};

type Native = HeaderValue;
type Raw = String;
type Unit = ();
type WireText = wire::Text;
#[allow(non_camel_case_types)]
pub mod raw_types { #[allow(non_camel_case_types)] pub type r#json = String; }
static STATIC: HeaderValue = HeaderValue::from_static("static-data");
fn flagged(s: &'static str) -> HeaderValue {
    let mut v = HeaderValue::from_static(s);
    v.set_sensitive(true);
    v
}
fn client() -> reqwest::Client { reqwest::Client::builder().no_proxy().build().unwrap() }

// A single-request, raw-byte loopback server, independent of the repository test helper.
fn server(status: u16, body: &'static [u8]) -> (String, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut raw = Vec::new();
        let mut buf = [0u8; 2048];
        loop {
            let n = s.read(&mut buf).unwrap();
            assert_ne!(n, 0);
            raw.extend_from_slice(&buf[..n]);
            if let Some(end) = raw.windows(4).position(|x| x == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
                let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length: "))
                    .map(|v| v.parse().unwrap()).unwrap_or(0);
                if raw.len() >= end + 4 + len { break; }
            }
        }
        write!(s, "HTTP/1.1 {status} Probe\r\nContent-Length: {}\r\nContent-Type: application/nonsense\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        s.write_all(body).unwrap();
        raw
    });
    (format!("http://{addr}"), handle)
}
fn headers(raw: &[u8], key: &str) -> Vec<Vec<u8>> {
    let end = raw.windows(4).position(|x| x == b"\r\n\r\n").unwrap();
    raw[..end].split(|b| *b == b'\n').filter_map(|line| {
        let i = line.iter().position(|b| *b == b':')?;
        if !line[..i].eq_ignore_ascii_case(key.as_bytes()) { return None; }
        let mut v = &line[i+1..];
        if v.first() == Some(&b' ') { v = &v[1..]; }
        if v.last() == Some(&b'\r') { v = &v[..v.len()-1]; }
        Some(v.to_vec())
    }).collect()
}
fn no_secret(shown: impl std::fmt::Debug) {
    let s = format!("{shown:?}");
    assert!(!s.contains("NATIVE-SECRET") && !s.contains("DECLARED-SECRET"), "{s}");
}

wire::api! {
    pub client NativeOwners(header("x-root") root: Native,
        header("x-default") default: Native = &STATIC,
        header("x-many") each many: Vec<Native> = Vec::new()) {
        base "http://127.0.0.1/root";
        request json;
        response json;
        headers { "x-literal" = "literal"; }
        scope scoped(header("x-scope") scoped: Native) {
            POST go(header("x-call") call: Native,
                #[sensitive] header("x-declared") declared: Native, body data: Raw);
            GET fallback(v?: Native) { headers { override "x-root" = v else inherit; } }
            GET appended(v: Native) { headers { append "x-root" = v; } }
            GET removed() { headers { remove "x-root"; } }
        }
    }
}

#[tokio::test]
async fn native_owners_clone_snapshots_flags_bytes_and_wire() {
    let (url, server) = server(204, b"");
    let cfg = NativeOwnersConfig { root: flagged("NATIVE-SECRET-root") };
    no_secret(&cfg);
    let original = NativeOwners::new(client(), cfg).with_base_url(&url).unwrap()
        .set_default(&STATIC)
        .set_many(vec![flagged("NATIVE-SECRET-list"), Native::from_static("public")]);
    no_secret(&original);
    let scope = original.scoped(flagged("NATIVE-SECRET-scope"));
    no_secret(&scope);
    let changed = original.set_root(Native::from_static("replacement"));
    assert_eq!(changed.scoped(Native::from_static("s")).fallback().prepare().unwrap().request().headers()["x-root"], "replacement");
    let raw = Native::from_bytes(b"a\x80\xffz").unwrap();
    let request = scope.go(raw.clone(), Native::from_static("DECLARED-SECRET"), "payload");
    no_secret(&request);
    let cloned = request.clone();
    let p = request.prepare().unwrap();
    no_secret(&p);
    let hs = p.request().headers();
    assert_eq!(hs["x-root"].as_bytes(), b"NATIVE-SECRET-root");
    for key in ["x-root", "x-scope", "x-declared"] { assert!(hs[key].is_sensitive(), "{key}"); }
    assert!(!raw.is_sensitive());
    assert_eq!(hs["x-call"].as_bytes(), raw.as_bytes());
    assert_eq!(hs["x-default"].as_bytes().as_ptr(), STATIC.as_bytes().as_ptr());
    assert_eq!(hs.get_all("x-many").iter().map(|v| v.is_sensitive()).collect::<Vec<_>>(), [true,false]);
    let c = cloned.prepare().unwrap();
    assert_eq!(c.request().body().unwrap().as_bytes(), p.request().body().unwrap().as_bytes());
    assert!(c.request().headers()["x-root"].is_sensitive());
    p.send().await.unwrap();
    let raw_wire = server.join().unwrap();
    assert_eq!(headers(&raw_wire,"x-call"), [b"a\x80\xffz".to_vec()]);
    assert_eq!(headers(&raw_wire,"x-many"), [b"NATIVE-SECRET-list".to_vec(),b"public".to_vec()]);
    assert_eq!(headers(&raw_wire,"x-declared"), [b"DECLARED-SECRET".to_vec()]);
    let p = scope.appended(Native::from_static("public")).prepare().unwrap();
    assert_eq!(p.request().headers().get_all("x-root").iter().map(|v|v.is_sensitive()).collect::<Vec<_>>(),[true,false]);
    assert!(!scope.removed().prepare().unwrap().request().headers().contains_key("x-root"));
}

pub struct NoMedia;
impl Encode<String> for NoMedia {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(v: String) -> Result<reqwest::Body,BodyError> { Ok(v.into()) }
}
pub struct InvalidMedia;
impl Encode<String> for InvalidMedia {
    const CONTENT_TYPE: Option<&'static str> = Some("invalid\nconstant");
    fn encode(v: String) -> Result<reqwest::Body,BodyError> { Ok(v.into()) }
}
pub struct FailEncode;
impl Encode<String> for FailEncode {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: String) -> Result<reqwest::Body,BodyError> { Err(BodyError::new("intentional encoder failure")) }
}
pub mod json {
    pub type Codec = wire::Text;
}
#[allow(non_camel_case_types)]
pub type csv = wire::Text;
wire::api! {
    pub client Formats {
        base "http://127.0.0.1";
        request json("application/vnd.default+json");
        response json;
        GET nothing();
        POST null(body value: Option<u8>);
            POST empty(text note: Raw);
        POST empty_bytes(bytes b: Bytes);
        POST local(json data: Raw);
        POST media(body("application/custom") data: Raw);
        POST no_media(NoMedia data: String);
        POST replaced(InvalidMedia("text/plain") data: String);
        POST bad(header("x-bad") h: String, FailEncode b: String);
        GET unit() -> ();
        GET unit_alias() -> Unit;
        GET unit_explicit() -> json ();
        GET json_string() -> Raw;
        GET raw() -> text Raw;
        GET alias() -> WireText Raw;
        GET raw_identifier() -> text raw_types::r#json;
        POST lexical(json: String, json query: Raw) { query { "json" = json; } }
        POST custom_path(json::Codec v: Raw) -> self::csv Raw;
        scope nested {
            request text;
            GET typed() -> Raw;
            POST text_body(body x: Raw);
            scope deep {
                response bytes;
                POST both(body x: Raw) -> Bytes;
            }
        }
    }
}

fn body<T>(p: &wire::Prepared<T>) -> Option<&[u8]> { p.request().body().and_then(|b| b.as_bytes()) }
fn ct<T>(p: &wire::Prepared<T>) -> Option<&[u8]> { p.request().headers().get("content-type").map(|v|v.as_bytes()) }

#[test]
fn format_presence_aliases_independent_defaults_lexical_and_media() {
    let api = Formats::new(client());
    let p = api.nothing().prepare().unwrap();
    assert!(body(&p).is_none() && ct(&p).is_none());
    let p = api.null(None).prepare().unwrap();
    assert_eq!(body(&p), Some(&b"null"[..]));
    assert_eq!(ct(&p), Some(&b"application/vnd.default+json"[..]));
    for present in [false,true] {
        for header in [false,true] {
            // Local format below avoids the explicit-default conflict by using a separate fixture.
            let api = FourCases::new(client());
            let p = api.maybe().maybe_ct(header.then(|| flagged("NATIVE-SECRET-ct")))
                .maybe_data(present.then_some(None)).prepare().unwrap();
            assert_eq!(body(&p).is_some(),present);
            let expected = if header {Some(&b"NATIVE-SECRET-ct"[..])} else if present {Some(&b"application/json"[..])} else {None};
            assert_eq!(ct(&p),expected);
            if header {assert!(p.request().headers()["content-type"].is_sensitive()); no_secret(&p);}
        }
    }
    let p = api.empty("").prepare().unwrap();
    assert_eq!(body(&p),Some(&b""[..])); assert_eq!(ct(&p),Some(&b"text/plain; charset=utf-8"[..]));
    let p = api.empty_bytes(Bytes::new()).prepare().unwrap();
    assert_eq!(body(&p),Some(&b""[..])); assert_eq!(ct(&p),Some(&b"application/octet-stream"[..]));
    let p = api.local("s").prepare().unwrap();
    assert_eq!(body(&p),Some(&br#""s""#[..])); assert_eq!(ct(&p),Some(&b"application/json"[..]));
    let p = api.media("s").prepare().unwrap(); assert_eq!(ct(&p),Some(&b"application/custom"[..]));
    let p = api.no_media("s").prepare().unwrap(); assert!(ct(&p).is_none());
    let p = api.replaced("s").prepare().unwrap(); assert_eq!(ct(&p),Some(&b"text/plain"[..]));
    let error = api.bad("bad\nheader","x").prepare().unwrap_err();
    assert_eq!(error.kind(),ErrorKind::Build);
    assert!(error.to_string().contains("intentional encoder failure"));
    let p = api.nested().text_body("raw").prepare().unwrap(); assert_eq!(body(&p),Some(&b"raw"[..]));
    let p = api.lexical("q","s").prepare().unwrap();
    assert_eq!(p.request().url().query(),Some("json=q")); assert_eq!(body(&p),Some(&br#""s""#[..]));
    let p = api.custom_path("custom").prepare().unwrap(); assert_eq!(body(&p),Some(&b"custom"[..]));
}

wire::api! {
    pub client FourCases {
        base "http://127.0.0.1";
        request json;
        response json;
        POST maybe(header("content-type") ct?: Native, body data?: Option<u8>);
        scope many(header("content-type") each cts: Vec<Native>) {
            POST go(body data?: Option<u8>);
        }
        scope classified(#[sensitive] header("content-type") ct?: Native) {
            POST inserted(body data: u8) { headers { remove "content-type"; } }
            POST explicit(json("application/custom") data: u8) { headers { remove "content-type"; } }
        }
    }
}

#[tokio::test]
async fn composed_content_type_and_sensitive_insertions_reach_wire() {
    for present in [false,true] { for header in [false,true] {
        let (url,s) = server(204,b"");
        let p = FourCases::new(client()).with_base_url(&url).unwrap().maybe()
            .maybe_ct(header.then(|| Native::from_static("application/custom")))
            .maybe_data(present.then_some(None)).prepare().unwrap();
        assert!(!p.request().headers().contains_key("accept"));
        p.send().await.unwrap();
        let raw = s.join().unwrap();
        let expected = if header {vec![b"application/custom".to_vec()]} else if present {vec![b"application/json".to_vec()]} else {vec![]};
        assert_eq!(headers(&raw,"content-type"),expected);
        assert_eq!(headers(&raw,"accept"),[b"*/*".to_vec()]); // reqwest client default
    }}
    let api=FourCases::new(client());
    let p=api.many(vec![]).go().set_data(None).prepare().unwrap(); assert_eq!(ct(&p),Some(&b"application/json"[..]));
    let p=api.many(vec![Native::from_static("")]).go().prepare().unwrap(); assert_eq!(ct(&p),Some(&b""[..]));
    let p=api.many(vec![Native::from_static("a"),flagged("NATIVE-SECRET")]).go().prepare().unwrap();
    assert_eq!(p.request().headers().get_all("content-type").iter().count(),2);
    for explicit in [false,true] {
        let (url,s)=server(204,b"");
        let api=FourCases::new(client()).with_base_url(&url).unwrap();
        let p=if explicit {api.classified().explicit(7).prepare().unwrap()} else {api.classified().inserted(7).prepare().unwrap()};
        assert!(p.request().headers()["content-type"].is_sensitive());
        assert!(!format!("{p:?}").contains(if explicit {"application/custom"}else{"application/json"}));
        p.send().await.unwrap();
        assert_eq!(headers(&s.join().unwrap(),"content-type").len(),1);
    }
}

#[tokio::test]
async fn drain_typed_unit_string_alias_status_and_native_decoder() {
    for status in [200,204,299] {
        let (url,s)=server(status,b"");
        Formats::new(client()).with_base_url(&url).unwrap().nothing().send().await.unwrap(); s.join().unwrap();
    }
    for which in 0..3 { for bytes in [b"".as_slice(),b"null".as_slice()] {
        let (url,s)=server(if bytes.is_empty(){204}else{200},bytes);
        let api=Formats::new(client()).with_base_url(&url).unwrap();
        let result=match which {0=>api.unit().send().await,1=>api.unit_alias().send().await,_=>api.unit_explicit().send().await};
        if bytes.is_empty() {assert_eq!(result.unwrap_err().kind(),ErrorKind::Decode);} else {result.unwrap();}
        s.join().unwrap();
    }}
    for which in 0..5 {
        let (url,s)=server(200,if which==0 || which==4 {br#""hi""#} else {b"hi"});
        let api=Formats::new(client()).with_base_url(&url).unwrap();
        let p=match which {0=>api.json_string().prepare(),1=>api.raw().prepare(),2=>api.alias().prepare(),3=>api.raw_identifier().prepare(),_=>api.nested().typed().prepare()}.unwrap();
        let (http,mut request,decoder)=p.into_parts();
        request.headers_mut().insert("x-native",HeaderValue::from_static("retained"));
        let response=http.execute(request).await.unwrap();
        assert_eq!(response.status().as_u16(),200);
        assert_eq!(decoder.decode(response).await.unwrap(),"hi");
        assert_eq!(headers(&s.join().unwrap(),"x-native"),[b"retained".to_vec()]);
    }
    let (url,s)=server(400,b"unread-response");
    let e=Formats::new(client()).with_base_url(&url).unwrap().nothing().send().await.unwrap_err();
    assert_eq!(e.kind(),ErrorKind::Status);
    assert_eq!(e.into_response().unwrap().bytes().await.unwrap().as_ref(),b"unread-response"); s.join().unwrap();
    let (url,s)=server(418,b"unchecked");
    let response=Formats::new(client()).with_base_url(&url).unwrap().json_string().prepare().unwrap().send_raw().await.unwrap();
    assert_eq!(response.status().as_u16(),418); assert_eq!(response.bytes().await.unwrap().as_ref(),b"unchecked"); s.join().unwrap();
}

#[derive(Clone,serde::Serialize,serde::Deserialize,Debug,PartialEq)]
pub struct FormInput { label:String, count:u8 }
wire::api! {
    pub client FormApi {
        base "http://127.0.0.1";
        request form;
        response json;
        POST submit(body input: FormInput) -> FormInput;
        GET decoded() -> form FormInput;
    }
}
#[tokio::test]
async fn form_input_json_output_and_form_decode() {
    let (url,s)=server(200,br#"{"label":"a b+","count":7}"#);
    let input=FormInput{label:"a b+".into(),count:7};
    let out=FormApi::new(client()).with_base_url(&url).unwrap().submit(input.clone()).send().await.unwrap();
    assert_eq!(out,input);
    let raw=s.join().unwrap();
    assert_eq!(headers(&raw,"content-type"),[b"application/x-www-form-urlencoded".to_vec()]);
    assert!(raw.ends_with(b"label=a+b%2B&count=7"));
    let (url,s)=server(200,b"label=a+b%2B&count=7");
    assert_eq!(FormApi::new(client()).with_base_url(&url).unwrap().decoded().send().await.unwrap(),input); s.join().unwrap();
}

// Direct dual-use forwarding: the application performs no explicit byte-to-string conversion.
#[derive(Clone)]
pub struct Dual(Native);
impl ToHeaderValue for Dual { fn to_header_value(&self)->Option<Native>{Some(self.0.clone())} }
impl std::fmt::Display for Dual {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str(self.0.to_str().unwrap()) }
}
wire::api! {
    pub client DualUse {
        base "http://127.0.0.1";
        GET go(v: Dual) { path "{v}"; query { "q" = v; } headers { "x-dual" = v; } }
    }
}
#[test]
fn dual_use_prepared_must_withhold_native_flag() {
    let request=DualUse::new(client()).go(Dual(flagged("NATIVE-SECRET")));
    no_secret(&request);
    let p=request.prepare().unwrap();
    assert!(p.request().headers()["x-dual"].is_sensitive());
    no_secret(&p);
}
