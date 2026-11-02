#![feature(prelude_import)]
extern crate std;
#[prelude_import]
use std::prelude::rust_2024::*;
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
pub mod raw_types {
    #[allow(non_camel_case_types)]
    pub type json = String;
}
static STATIC: HeaderValue = HeaderValue::from_static("static-data");
fn flagged(s: &'static str) -> HeaderValue {
    let mut v = HeaderValue::from_static(s);
    v.set_sensitive(true);
    v
}
fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

// A single-request, raw-byte loopback server, independent of the repository test helper.
fn server(status: u16, body: &'static [u8]) -> (String, JoinHandle<Vec<u8>>) {
    server_length(status, body, body.len())
}
fn server_length(status: u16, body: &'static [u8], length: usize)
    -> (String, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle =
        thread::spawn(move ||
                {
                    let (mut s, _) = listener.accept().unwrap();
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    let mut raw = Vec::new();
                    let mut buf = [0u8; 2048];
                    loop {
                        let n = s.read(&mut buf).unwrap();





                        // Local format below avoids the explicit-default conflict by using a separate fixture.


                        // reqwest client default

                        // A no-output endpoint consumes the response instead of returning immediately after headers.
                        // Raw text is invalid JSON string data under an explicit JSON default.


                        // Direct dual-use forwarding: the application performs no explicit byte-to-string conversion.


                        {
                            match (&n, &0) {
                                (left_val, right_val) => {
                                    if *left_val == *right_val {
                                        let kind = ::core::panicking::AssertKind::Ne;
                                        ::core::panicking::assert_failed(kind, &*left_val,
                                            &*right_val, ::core::option::Option::None);
                                    }
                                }
                            }
                        };
                        raw.extend_from_slice(&buf[..n]);
                        if let Some(end) =
                                raw.windows(4).position(|x| x == b"\r\n\r\n") {
                            let head =
                                String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
                            let len: usize =
                                head.lines().find_map(|l|
                                                l.strip_prefix("content-length: ")).map(|v|
                                            v.parse().unwrap()).unwrap_or(0);
                            if raw.len() >= end + 4 + len { break; }
                        }
                    }
                    s.write_fmt(format_args!("HTTP/1.1 {0} Probe\r\nContent-Length: {1}\r\nContent-Type: application/nonsense\r\nConnection: close\r\n\r\n",
                                status, length)).unwrap();
                    s.write_all(body).unwrap();
                    raw
                });
    (::alloc::__export::must_use({
                ::alloc::fmt::format(format_args!("http://{0}", addr))
            }), handle)
}
fn headers(raw: &[u8], key: &str) -> Vec<Vec<u8>> {
    let end = raw.windows(4).position(|x| x == b"\r\n\r\n").unwrap();
    raw[..end].split(|b|
                    *b ==
                        b'\n').filter_map(|line|
                {
                    let i = line.iter().position(|b| *b == b':')?;
                    if !line[..i].eq_ignore_ascii_case(key.as_bytes()) {
                        return None;
                    }
                    let mut v = &line[i + 1..];
                    if v.first() == Some(&b' ') { v = &v[1..]; }
                    if v.last() == Some(&b'\r') { v = &v[..v.len() - 1]; }
                    Some(v.to_vec())
                }).collect()
}
fn no_secret(shown: impl std::fmt::Debug) {
    let s =
        ::alloc::__export::must_use({
                ::alloc::fmt::format(format_args!("{0:?}", shown))
            });
    if !(!s.contains("NATIVE-SECRET") && !s.contains("DECLARED-SECRET")) {
        { ::core::panicking::panic_fmt(format_args!("{0}", s)); }
    };
}
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_0: ::concord::reqwest::header::HeaderName
    =
    ::concord::reqwest::header::HeaderName::from_static("x-root");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_1: ::concord::reqwest::header::HeaderName
    =
    ::concord::reqwest::header::HeaderName::from_static("x-default");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_2: ::concord::reqwest::header::HeaderName
    =
    ::concord::reqwest::header::HeaderName::from_static("x-many");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_3: ::concord::reqwest::header::HeaderName
    =
    ::concord::reqwest::header::HeaderName::from_static("x-literal");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_VALUE_0:
    ::concord::reqwest::header::HeaderValue =
    ::concord::reqwest::header::HeaderValue::from_static("literal");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_4: ::concord::reqwest::header::HeaderName
    =
    ::concord::reqwest::header::HeaderName::from_static("x-scope");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_5: ::concord::reqwest::header::HeaderName
    =
    ::concord::reqwest::header::HeaderName::from_static("x-call");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_NativeOwners_HEADER_6: ::concord::reqwest::header::HeaderName
    =
    ::concord::reqwest::header::HeaderName::from_static("x-declared");
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordNativeOwnersRequestFormat = ::concord::Json;
#[doc =
" The required values of [`NativeOwners`], passed to [`NativeOwners::new`]. Optional and initialized values are set afterwards with the client's `set_*` and `maybe_*` methods."]
pub struct NativeOwnersConfig {
    #[doc = " The client's required value `root`."]
    #[doc = ""]
    #[doc = " Sent as: header `x-root`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    pub root: Native,
}
#[automatically_derived]
impl ::core::clone::Clone for NativeOwnersConfig {
    #[inline]
    fn clone(&self) -> Self {
        NativeOwnersConfig { root: ::core::clone::Clone::clone(&self.root) }
    }
}
impl ::core::fmt::Debug for NativeOwnersConfig {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("NativeOwnersConfig");
        __d.field("root", &::concord::__private::DebugHeader(&self.root));
        __d.finish()
    }
}
#[doc = " Client for `NativeOwners`, generated by `concord::api!`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `json` (declared here); typed outputs `json` (declared here). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct NativeOwners {
    __s: ::std::sync::Arc<__ConcordNativeOwnersState>,
}
#[automatically_derived]
impl ::core::clone::Clone for NativeOwners {
    #[inline]
    fn clone(&self) -> Self {
        NativeOwners { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordNativeOwnersState {
    __http: ::concord::reqwest::Client,
    __base: ::concord::__private::Base,
    root: Native,
    default: Native,
    many: Vec<Native>,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordNativeOwnersState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordNativeOwnersState {
            __http: ::core::clone::Clone::clone(&self.__http),
            __base: ::core::clone::Clone::clone(&self.__base),
            root: ::core::clone::Clone::clone(&self.root),
            default: ::core::clone::Clone::clone(&self.default),
            many: ::core::clone::Clone::clone(&self.many),
        }
    }
}
impl NativeOwners {
    #[doc =
    r" Creates the client. `http` is used for every request; configure timeouts,"]
    #[doc =
    r" redirects, proxies, TLS and default headers on it. Builds no connection."]
    #[doc = r" Initializers of the client's values are evaluated here, once."]
    #[inline]
    pub fn new(http: ::concord::reqwest::Client, config: NativeOwnersConfig)
        -> Self {
        let NativeOwnersConfig { root } = config;
        #[allow(non_snake_case)]
        #[inline]
        fn __concord_default_default() -> Native {
            <_ as ::concord::IntoValue<Native>>::into_value(&STATIC)
        }
        #[allow(non_snake_case)]
        #[inline]
        fn __concord_default_many() -> Vec<Native> {
            <_ as ::concord::IntoValue<Vec<Native>>>::into_value(Vec::new())
        }
        Self {
            __s: ::std::sync::Arc::new(__ConcordNativeOwnersState {
                    __http: http,
                    __base: ::concord::__private::Base::from_static("http://127.0.0.1/root"),
                    root: ::concord::IntoValue::into_value(root),
                    default: __concord_default_default(),
                    many: __concord_default_many(),
                }),
        }
    }
    #[doc =
    r" Replaces the declared base URL (for example with a staging server or a"]
    #[doc =
    r" local test server). It must be an `http`/`https` URL with a host, no port 0"]
    #[doc =
    r" and no user information, query or fragment; anything else is an error."]
    #[doc =
    r" Declared paths are appended to its path. With `subdomain` labels the host"]
    #[doc =
    r" must be a domain name: to reach a local server, keep a domain and map it"]
    #[doc =
    r" with `reqwest::ClientBuilder::resolve`. Returns a new handle: scopes and"]
    #[doc = r" requests created from the old one keep the old base."]
    #[inline]
    pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>)
        -> ::core::result::Result<Self, ::concord::Error> {
        let base = ::concord::__private::Base::parse(url.as_ref())?;
        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
        ::core::result::Result::Ok(self)
    }
    #[doc = ""]
    #[doc = " Replaces `root`."]
    #[doc = " Required: supplied when the client is created."]
    #[doc = " Sent as: header `x-root`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_root(mut self, value: impl ::concord::IntoValue<Native>)
        -> Self {
        ::std::sync::Arc::make_mut(&mut self.__s).root =
            ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = ""]
    #[doc = " Replaces `default`."]
    #[doc =
    " Initially `&STATIC`, evaluated once when the client is created."]
    #[doc = " Sent as: header `x-default`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_default(mut self, value: impl ::concord::IntoValue<Native>)
        -> Self {
        ::std::sync::Arc::make_mut(&mut self.__s).default =
            ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = ""]
    #[doc = " Replaces `many`."]
    #[doc =
    " Initially `Vec::new()`, evaluated once when the client is created."]
    #[doc = " Sent as: header `x-many` (one occurrence per item)."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_many(mut self, value: impl ::concord::IntoValue<Vec<Native>>)
        -> Self {
        ::std::sync::Arc::make_mut(&mut self.__s).many =
            ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = " Enters scope `NativeOwners.scoped`."]
    #[doc = ""]
    #[doc =
    " Formats of the endpoints below: request bodies `json` (the default of the client `NativeOwners`); typed outputs `json` (the default of the client `NativeOwners`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `scoped`: required value."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn scoped(&self, scoped: impl ::concord::IntoValue<Native>)
        -> NativeOwnersScoped {
        NativeOwnersScoped {
            __s: ::std::sync::Arc::new(__ConcordNativeOwnersScopedState {
                    __parent: ::core::clone::Clone::clone(self),
                    scoped: ::concord::IntoValue::into_value(scoped),
                }),
        }
    }
}
impl ::core::fmt::Debug for NativeOwners {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("NativeOwners");
        __d.field("base", &__s.__base);
        __d.field("root", &::concord::__private::DebugHeader(&__s.root));
        __d.field("default",
            &::concord::__private::DebugHeader(&__s.default));
        __d.field("many", &::concord::__private::DebugHeaderEach(&__s.many));
        __d.finish()
    }
}
#[doc = " Scope `NativeOwners.scoped`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `json` (the default of the client `NativeOwners`); typed outputs `json` (the default of the client `NativeOwners`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct NativeOwnersScoped {
    __s: ::std::sync::Arc<__ConcordNativeOwnersScopedState>,
}
#[automatically_derived]
impl ::core::clone::Clone for NativeOwnersScoped {
    #[inline]
    fn clone(&self) -> Self {
        NativeOwnersScoped { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordNativeOwnersScopedState {
    __parent: NativeOwners,
    scoped: Native,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordNativeOwnersScopedState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordNativeOwnersScopedState {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            scoped: ::core::clone::Clone::clone(&self.scoped),
        }
    }
}
impl NativeOwnersScoped {
    #[doc = ""]
    #[doc = " Replaces `scoped`."]
    #[doc = " Required: supplied when this scope is created."]
    #[doc = " Sent as: header `x-scope`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_scoped(mut self, value: impl ::concord::IntoValue<Native>)
        -> Self {
        ::std::sync::Arc::make_mut(&mut self.__s).scoped =
            ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = " `POST` endpoint `NativeOwners.scoped.go`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1/root`."]
    #[doc =
    " Body (`data`): `Raw`, encoded with the format `json` (the `request` default of the client `NativeOwners`). `Content-Type: application/json` is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `NativeOwners` does not apply."]
    #[doc =
    " Headers (when present): `x-root`, `x-default`, `x-many`, `x-literal`, `x-scope`, `x-call`, `x-declared` (sensitive)."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `call`: required value."]
    #[doc = " - `declared`: required value."]
    #[doc = " - `data`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn go(&self, call: impl ::concord::IntoValue<Native>,
        declared: impl ::concord::IntoValue<Native>,
        data: impl ::concord::IntoValue<Raw>) -> NativeOwnersScopedGo {
        NativeOwnersScopedGo {
            __parent: ::core::clone::Clone::clone(self),
            call: ::concord::IntoValue::into_value(call),
            declared: ::concord::IntoValue::into_value(declared),
            data: ::concord::IntoValue::into_value(data),
        }
    }
    #[doc = " `GET` endpoint `NativeOwners.scoped.fallback`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1/root`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `NativeOwners` does not apply."]
    #[doc =
    " Headers (when present): `x-root`, `x-default`, `x-many`, `x-literal`, `x-scope`."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn fallback(&self) -> NativeOwnersScopedFallback {
        NativeOwnersScopedFallback {
            __parent: ::core::clone::Clone::clone(self),
            v: ::core::option::Option::None,
        }
    }
    #[doc = " `GET` endpoint `NativeOwners.scoped.appended`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1/root`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `NativeOwners` does not apply."]
    #[doc =
    " Headers (when present): `x-root`, `x-default`, `x-many`, `x-literal`, `x-scope`."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `v`: required value."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn appended(&self, v: impl ::concord::IntoValue<Native>)
        -> NativeOwnersScopedAppended {
        NativeOwnersScopedAppended {
            __parent: ::core::clone::Clone::clone(self),
            v: ::concord::IntoValue::into_value(v),
        }
    }
    #[doc = " `GET` endpoint `NativeOwners.scoped.removed`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1/root`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `NativeOwners` does not apply."]
    #[doc =
    " Headers (when present): `x-default`, `x-many`, `x-literal`, `x-scope`."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn removed(&self) -> NativeOwnersScopedRemoved {
        NativeOwnersScopedRemoved {
            __parent: ::core::clone::Clone::clone(self),
        }
    }
}
impl ::core::fmt::Debug for NativeOwnersScoped {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("NativeOwnersScoped");
        __d.field("scoped", &::concord::__private::DebugHeader(&__s.scoped));
        __d.finish()
    }
}
#[doc = " `POST` endpoint `NativeOwners.scoped.go`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct NativeOwnersScopedGo {
    __parent: NativeOwnersScoped,
    call: Native,
    declared: Native,
    data: Raw,
}
impl ::core::clone::Clone for NativeOwnersScopedGo where
    for<'__concord> Native: ::core::clone::Clone,
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            call: ::core::clone::Clone::clone(&self.call),
            declared: ::core::clone::Clone::clone(&self.declared),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl NativeOwnersScopedGo {
    #[doc = ""]
    #[doc = " Replaces `call`."]
    #[doc = " Required: supplied when this request is created."]
    #[doc = " Sent as: header `x-call`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_call(mut self, value: impl ::concord::IntoValue<Native>)
        -> Self {
        self.call = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = ""]
    #[doc = " Replaces `declared`."]
    #[doc = " Required: supplied when this request is created."]
    #[doc = " Sent as: header `x-declared`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[doc =
    " Sensitive: sent normally, withheld from Concord's `Debug` output and errors."]
    #[inline]
    pub fn set_declared(mut self, value: impl ::concord::IntoValue<Native>)
        -> Self {
        self.declared = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<Raw>) -> Self {
        self.data = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "NativeOwners.scoped.go",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::new::<__ConcordNativeOwnersRequestFormat>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.data, &__META)?)
            };
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_0),
                "header `x-root`", &__o0.root, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_1),
                "header `x-default`", &__o0.default, false)?;
        __p.header_each(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_2),
                "header `x-many`", &__o0.many, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_3),
                "header `x-literal`", &__CONCORD_NativeOwners_HEADER_VALUE_0,
                false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_4),
                "header `x-scope`", &__o1.scoped, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_5),
                "header `x-call`", &self.call, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_6),
                "header `x-declared`", &self.declared, true)?;
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for NativeOwnersScopedGo {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("NativeOwnersScopedGo");
        __d.field("call", &::concord::__private::DebugHeader(&self.call));
        __d.field("declared", &::concord::__private::Redacted);
        __d.field("data", &"..");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `NativeOwners.scoped.fallback`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct NativeOwnersScopedFallback {
    __parent: NativeOwnersScoped,
    v: ::core::option::Option<Native>,
}
impl ::core::clone::Clone for NativeOwnersScopedFallback where
    for<'__concord> Native: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            v: ::core::clone::Clone::clone(&self.v),
        }
    }
}
impl NativeOwnersScopedFallback {
    #[doc = ""]
    #[doc = " Sets `v` (present)."]
    #[doc = " Initially absent."]
    #[doc = " Sent as: header `x-root`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[doc =
    " When `v` is absent, the inherited `x-root` header is sent unchanged (`override ... else inherit` in `NativeOwners.scoped.fallback`)."]
    #[inline]
    pub fn set_v(mut self, value: impl ::concord::IntoValue<Native>) -> Self {
        self.v =
            ::core::option::Option::Some(::concord::IntoValue::into_value(value));
        self
    }
    #[doc = ""]
    #[doc =
    " Sets whether `v` is present: `Some(v)` is `set_v(v)`; `None` makes it absent."]
    #[doc = " Initially absent."]
    #[doc = " Sent as: header `x-root`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[doc =
    " When `v` is absent, the inherited `x-root` header is sent unchanged (`override ... else inherit` in `NativeOwners.scoped.fallback`)."]
    #[inline]
    pub fn maybe_v(mut self, value: ::core::option::Option<Native>) -> Self {
        self.v = value;
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "NativeOwners.scoped.fallback",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        if let ::core::option::Option::Some(__v) = &self.v {
            __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_0),
                    "header `x-root`", &*__v, false)?;
        } else {
            __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_0),
                    "header `x-root`", &__o0.root, false)?;
        }
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_1),
                "header `x-default`", &__o0.default, false)?;
        __p.header_each(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_2),
                "header `x-many`", &__o0.many, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_3),
                "header `x-literal`", &__CONCORD_NativeOwners_HEADER_VALUE_0,
                false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_4),
                "header `x-scope`", &__o1.scoped, false)?;
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for NativeOwnersScopedFallback {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("NativeOwnersScopedFallback");
        __d.field("v",
            &self.v.as_ref().map(::concord::__private::DebugHeader));
        __d.finish()
    }
}
#[doc = " `GET` endpoint `NativeOwners.scoped.appended`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct NativeOwnersScopedAppended {
    __parent: NativeOwnersScoped,
    v: Native,
}
impl ::core::clone::Clone for NativeOwnersScopedAppended where
    for<'__concord> Native: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            v: ::core::clone::Clone::clone(&self.v),
        }
    }
}
impl NativeOwnersScopedAppended {
    #[doc = ""]
    #[doc = " Replaces `v`."]
    #[doc = " Required: supplied when this request is created."]
    #[doc = " Sent as: header `x-root`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_v(mut self, value: impl ::concord::IntoValue<Native>) -> Self {
        self.v = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "NativeOwners.scoped.appended",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_0),
                "header `x-root`", &__o0.root, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_0),
                "header `x-root`", &self.v, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_1),
                "header `x-default`", &__o0.default, false)?;
        __p.header_each(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_2),
                "header `x-many`", &__o0.many, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_3),
                "header `x-literal`", &__CONCORD_NativeOwners_HEADER_VALUE_0,
                false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_4),
                "header `x-scope`", &__o1.scoped, false)?;
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for NativeOwnersScopedAppended {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("NativeOwnersScopedAppended");
        __d.field("v", &::concord::__private::DebugHeader(&self.v));
        __d.finish()
    }
}
#[doc = " `GET` endpoint `NativeOwners.scoped.removed`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct NativeOwnersScopedRemoved {
    __parent: NativeOwnersScoped,
}
impl ::core::clone::Clone for NativeOwnersScopedRemoved where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl NativeOwnersScopedRemoved {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "NativeOwners.scoped.removed",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_1),
                "header `x-default`", &__o0.default, false)?;
        __p.header_each(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_2),
                "header `x-many`", &__o0.many, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_3),
                "header `x-literal`", &__CONCORD_NativeOwners_HEADER_VALUE_0,
                false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_NativeOwners_HEADER_4),
                "header `x-scope`", &__o1.scoped, false)?;
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for NativeOwnersScopedRemoved {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("NativeOwnersScopedRemoved");
        __d.finish()
    }
}
extern crate test;
#[rustc_test_marker = "native_owners_clone_snapshots_flags_bytes_and_wire"]
#[doc(hidden)]
pub static native_owners_clone_snapshots_flags_bytes_and_wire:
    test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("native_owners_clone_snapshots_flags_bytes_and_wire"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 88usize,
            start_col: 10usize,
            end_line: 88usize,
            end_col: 60usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(native_owners_clone_snapshots_flags_bytes_and_wire())),
    };
fn native_owners_clone_snapshots_flags_bytes_and_wire() {
    let body =
        async {
            let (url, server) = server(204, b"");
            let cfg =
                NativeOwnersConfig { root: flagged("NATIVE-SECRET-root") };
            no_secret(&cfg);
            let original =
                NativeOwners::new(client(),
                                    cfg).with_base_url(&url).unwrap().set_default(&STATIC).set_many(::alloc::boxed::box_assume_init_into_vec_unsafe(::alloc::intrinsics::write_box_via_move(::alloc::boxed::Box::new_uninit(),
                            [flagged("NATIVE-SECRET-list"),
                                    Native::from_static("public")])));
            no_secret(&original);
            let scope = original.scoped(flagged("NATIVE-SECRET-scope"));
            no_secret(&scope);
            let changed =
                original.set_root(Native::from_static("replacement"));
            {
                match (&changed.scoped(Native::from_static("s")).fallback().prepare().unwrap().request().headers()["x-root"],
                        &"replacement") {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            let raw = Native::from_bytes(b"a\x80\xffz").unwrap();
            let request =
                scope.go(raw.clone(), Native::from_static("DECLARED-SECRET"),
                    "payload");
            no_secret(&request);
            let cloned = request.clone();
            let p = request.prepare().unwrap();
            no_secret(&p);
            let hs = p.request().headers();
            {
                match (&hs["x-root"].as_bytes(), &b"NATIVE-SECRET-root") {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            for key in ["x-root", "x-scope", "x-declared"] {
                if !hs[key].is_sensitive() {
                    { ::core::panicking::panic_fmt(format_args!("{0}", key)); }
                };
            }
            if !!raw.is_sensitive() {
                ::core::panicking::panic("assertion failed: !raw.is_sensitive()")
            };
            {
                match (&hs["x-call"].as_bytes(), &raw.as_bytes()) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            {
                match (&hs["x-default"].as_bytes().as_ptr(),
                        &STATIC.as_bytes().as_ptr()) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            {
                match (&hs.get_all("x-many").iter().map(|v|
                                        v.is_sensitive()).collect::<Vec<_>>(), &[true, false]) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            let c = cloned.prepare().unwrap();
            {
                match (&c.request().body().unwrap().as_bytes(),
                        &p.request().body().unwrap().as_bytes()) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            if !c.request().headers()["x-root"].is_sensitive() {
                ::core::panicking::panic("assertion failed: c.request().headers()[\"x-root\"].is_sensitive()")
            };
            p.send().await.unwrap();
            let raw_wire = server.join().unwrap();
            {
                match (&headers(&raw_wire, "x-call"),
                        &[b"a\x80\xffz".to_vec()]) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            {
                match (&headers(&raw_wire, "x-many"),
                        &[b"NATIVE-SECRET-list".to_vec(), b"public".to_vec()]) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            {
                match (&headers(&raw_wire, "x-declared"),
                        &[b"DECLARED-SECRET".to_vec()]) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            let p =
                scope.appended(Native::from_static("public")).prepare().unwrap();
            {
                match (&p.request().headers().get_all("x-root").iter().map(|v|
                                        v.is_sensitive()).collect::<Vec<_>>(), &[true, false]) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            if !!scope.removed().prepare().unwrap().request().headers().contains_key("x-root")
                {
                ::core::panicking::panic("assertion failed: !scope.removed().prepare().unwrap().request().headers().contains_key(\"x-root\")")
            };
        };
    let mut body = body;
    #[allow(unused_mut)]
    let mut body =
        unsafe { ::tokio::macros::support::Pin::new_unchecked(&mut body) };
    let body: ::core::pin::Pin<&mut dyn ::core::future::Future<Output = ()>> =
        body;

    #[allow(clippy :: expect_used, clippy :: diverging_sub_expression, clippy
    :: needless_return, clippy :: unwrap_in_result)]
    {
        use tokio::runtime::Builder;
        return Builder::new_current_thread().enable_all().build().expect("Failed building the Runtime").block_on(body);
    }
}
pub struct NoMedia;
impl Encode<String> for NoMedia {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(v: String) -> Result<reqwest::Body, BodyError> { Ok(v.into()) }
}
pub struct InvalidMedia;
impl Encode<String> for InvalidMedia {
    const CONTENT_TYPE: Option<&'static str> = Some("invalid\nconstant");
    fn encode(v: String) -> Result<reqwest::Body, BodyError> { Ok(v.into()) }
}
pub struct FailEncode;
impl Encode<String> for FailEncode {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: String) -> Result<reqwest::Body, BodyError> {
        Err(BodyError::new("intentional encoder failure"))
    }
}
pub mod json {
    pub type Codec = wire::Text;
}
#[allow(non_camel_case_types)]
pub type csv = wire::Text;
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_Formats_HEADER_0: ::concord::reqwest::header::HeaderName =
    ::concord::reqwest::header::HeaderName::from_static("x-bad");
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordFormatsRequestFormat = ::concord::Json;
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordFormatsResponseFormat = ::concord::Json;
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordFormatsNestedRequestFormat = ::concord::Text;
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordFormatsNestedDeepResponseFormat = ::concord::Binary;
#[doc = " Client for `Formats`, generated by `concord::api!`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `json` (declared here, media type `application/vnd.default+json`); typed outputs `json` (declared here). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct Formats {
    __s: ::std::sync::Arc<__ConcordFormatsState>,
}
#[automatically_derived]
impl ::core::clone::Clone for Formats {
    #[inline]
    fn clone(&self) -> Self {
        Formats { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordFormatsState {
    __http: ::concord::reqwest::Client,
    __base: ::concord::__private::Base,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordFormatsState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordFormatsState {
            __http: ::core::clone::Clone::clone(&self.__http),
            __base: ::core::clone::Clone::clone(&self.__base),
        }
    }
}
impl Formats {
    #[doc =
    r" Creates the client. `http` is used for every request; configure timeouts,"]
    #[doc =
    r" redirects, proxies, TLS and default headers on it. Builds no connection."]
    #[doc = r" Initializers of the client's values are evaluated here, once."]
    #[inline]
    pub fn new(http: ::concord::reqwest::Client) -> Self {
        Self {
            __s: ::std::sync::Arc::new(__ConcordFormatsState {
                    __http: http,
                    __base: ::concord::__private::Base::from_static("http://127.0.0.1"),
                }),
        }
    }
    #[doc =
    r" Replaces the declared base URL (for example with a staging server or a"]
    #[doc =
    r" local test server). It must be an `http`/`https` URL with a host, no port 0"]
    #[doc =
    r" and no user information, query or fragment; anything else is an error."]
    #[doc =
    r" Declared paths are appended to its path. With `subdomain` labels the host"]
    #[doc =
    r" must be a domain name: to reach a local server, keep a domain and map it"]
    #[doc =
    r" with `reqwest::ClientBuilder::resolve`. Returns a new handle: scopes and"]
    #[doc = r" requests created from the old one keep the old base."]
    #[inline]
    pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>)
        -> ::core::result::Result<Self, ::concord::Error> {
        let base = ::concord::__private::Base::parse(url.as_ref())?;
        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
        ::core::result::Result::Ok(self)
    }
    #[doc = " `GET` endpoint `Formats.nothing`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn nothing(&self) -> FormatsNothing {
        FormatsNothing { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `POST` endpoint `Formats.null`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`value`): `Option<u8>`, encoded with the format `json` (the `request` default of the client `Formats`). `Content-Type: application/vnd.default+json` (from the `request` default of the client `Formats`) is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `value`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn null(&self, value: impl ::concord::IntoValue<Option<u8>>)
        -> FormatsNull {
        FormatsNull {
            __parent: ::core::clone::Clone::clone(self),
            value: ::concord::IntoValue::into_value(value),
        }
    }
    #[doc = " `POST` endpoint `Formats.empty`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`note`): `Raw`, encoded with the format `text` (written in place). `Content-Type: text/plain; charset=utf-8` is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `note`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn empty(&self, note: impl ::concord::IntoValue<Raw>)
        -> FormatsEmpty {
        FormatsEmpty {
            __parent: ::core::clone::Clone::clone(self),
            note: ::concord::IntoValue::into_value(note),
        }
    }
    #[doc = " `POST` endpoint `Formats.empty_bytes`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`b`): `Bytes`, encoded with the format `bytes` (written in place). `Content-Type: application/octet-stream` is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `b`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn empty_bytes(&self, b: impl ::concord::IntoValue<Bytes>)
        -> FormatsEmptyBytes {
        FormatsEmptyBytes {
            __parent: ::core::clone::Clone::clone(self),
            b: ::concord::IntoValue::into_value(b),
        }
    }
    #[doc = " `POST` endpoint `Formats.local`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`data`): `Raw`, encoded with the format `json` (written in place). `Content-Type: application/json` is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `data`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn local(&self, data: impl ::concord::IntoValue<Raw>)
        -> FormatsLocal {
        FormatsLocal {
            __parent: ::core::clone::Clone::clone(self),
            data: ::concord::IntoValue::into_value(data),
        }
    }
    #[doc = " `POST` endpoint `Formats.media`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`data`): `Raw`, encoded with the format `json` (the `request` default of the client `Formats`). `Content-Type: application/custom` (declared on the body) is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `data`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn media(&self, data: impl ::concord::IntoValue<Raw>)
        -> FormatsMedia {
        FormatsMedia {
            __parent: ::core::clone::Clone::clone(self),
            data: ::concord::IntoValue::into_value(data),
        }
    }
    #[doc = " `POST` endpoint `Formats.no_media`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`data`): `String`, encoded with the format `NoMedia` (written in place). `Content-Type`: the format's `Encode::CONTENT_TYPE`, sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `data`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn no_media(&self, data: impl ::concord::IntoValue<String>)
        -> FormatsNoMedia {
        FormatsNoMedia {
            __parent: ::core::clone::Clone::clone(self),
            data: ::concord::IntoValue::into_value(data),
        }
    }
    #[doc = " `POST` endpoint `Formats.replaced`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`data`): `String`, encoded with the format `InvalidMedia` (written in place). `Content-Type: text/plain` (declared on the body) is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `data`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn replaced(&self, data: impl ::concord::IntoValue<String>)
        -> FormatsReplaced {
        FormatsReplaced {
            __parent: ::core::clone::Clone::clone(self),
            data: ::concord::IntoValue::into_value(data),
        }
    }
    #[doc = " `POST` endpoint `Formats.bad`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`b`): `String`, encoded with the format `FailEncode` (written in place). `Content-Type`: the format's `Encode::CONTENT_TYPE`, sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = " Headers (when present): `x-bad`."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `h`: required value."]
    #[doc = " - `b`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn bad(&self, h: impl ::concord::IntoValue<String>,
        b: impl ::concord::IntoValue<String>) -> FormatsBad {
        FormatsBad {
            __parent: ::core::clone::Clone::clone(self),
            h: ::concord::IntoValue::into_value(h),
            b: ::concord::IntoValue::into_value(b),
        }
    }
    #[doc = " `GET` endpoint `Formats.unit`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `()`, decoded from a 2xx response with the format `json` (the `response` default of the client `Formats`). `()` is decoded like any output type: the response body must be JSON `null`, so an empty response (such as a 204) is a decode error. An endpoint without `->` decodes no output and accepts any 2xx."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn unit(&self) -> FormatsUnit {
        FormatsUnit { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `GET` endpoint `Formats.unit_alias`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `Unit`, decoded from a 2xx response with the format `json` (the `response` default of the client `Formats`)."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn unit_alias(&self) -> FormatsUnitAlias {
        FormatsUnitAlias { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `GET` endpoint `Formats.unit_explicit`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `()`, decoded from a 2xx response with the format `json` (written in place). `()` is decoded like any output type: the response body must be JSON `null`, so an empty response (such as a 204) is a decode error. An endpoint without `->` decodes no output and accepts any 2xx."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn unit_explicit(&self) -> FormatsUnitExplicit {
        FormatsUnitExplicit { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `GET` endpoint `Formats.json_string`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `Raw`, decoded from a 2xx response with the format `json` (the `response` default of the client `Formats`)."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn json_string(&self) -> FormatsJsonString {
        FormatsJsonString { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `GET` endpoint `Formats.raw`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `Raw`, decoded from a 2xx response with the format `text` (written in place)."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn raw(&self) -> FormatsRaw {
        FormatsRaw { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `GET` endpoint `Formats.alias`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `Raw`, decoded from a 2xx response with the format `WireText` (written in place)."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn alias(&self) -> FormatsAlias {
        FormatsAlias { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `GET` endpoint `Formats.raw_identifier`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `raw_types::r#json`, decoded from a 2xx response with the format `text` (written in place)."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn raw_identifier(&self) -> FormatsRawIdentifier {
        FormatsRawIdentifier { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `POST` endpoint `Formats.lexical`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`query`): `Raw`, encoded with the format `json` (written in place). `Content-Type: application/json` is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = " Query (when present): `json`."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `json`: required value."]
    #[doc = " - `query`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn lexical(&self, json: impl ::concord::IntoValue<String>,
        query: impl ::concord::IntoValue<Raw>) -> FormatsLexical {
        FormatsLexical {
            __parent: ::core::clone::Clone::clone(self),
            json: ::concord::IntoValue::into_value(json),
            query: ::concord::IntoValue::into_value(query),
        }
    }
    #[doc = " `POST` endpoint `Formats.custom_path`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`v`): `Raw`, encoded with the format `json::Codec` (written in place). `Content-Type`: the format's `Encode::CONTENT_TYPE`, sent with the body."]
    #[doc =
    " Output: `Raw`, decoded from a 2xx response with the format `self::csv` (written in place)."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `v`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn custom_path(&self, v: impl ::concord::IntoValue<Raw>)
        -> FormatsCustomPath {
        FormatsCustomPath {
            __parent: ::core::clone::Clone::clone(self),
            v: ::concord::IntoValue::into_value(v),
        }
    }
    #[doc = " Enters scope `Formats.nested`."]
    #[doc = ""]
    #[doc =
    " Formats of the endpoints below: request bodies `text` (declared here); typed outputs `json` (the default of the client `Formats`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn nested(&self) -> FormatsNested {
        FormatsNested {
            __s: ::std::sync::Arc::new(__ConcordFormatsNestedState {
                    __parent: ::core::clone::Clone::clone(self),
                }),
        }
    }
}
impl ::core::fmt::Debug for Formats {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("Formats");
        __d.field("base", &__s.__base);
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.nothing`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsNothing {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsNothing where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsNothing {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.nothing",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsNothing {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsNothing");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.null`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsNull {
    __parent: Formats,
    value: Option<u8>,
}
impl ::core::clone::Clone for FormatsNull where
    for<'__concord> Option<u8>: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            value: ::core::clone::Clone::clone(&self.value),
        }
    }
}
impl FormatsNull {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_value(mut self, value: impl ::concord::IntoValue<Option<u8>>)
        -> Self {
        self.value = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.null",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Option<u8>>
                    =
                    ::concord::__private::BodyFormat::with_media_type::<__ConcordFormatsRequestFormat>(::concord::reqwest::header::HeaderValue::from_static("application/vnd.default+json"));
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.value, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsNull {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsNull");
        __d.field("value", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.empty`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsEmpty {
    __parent: Formats,
    note: Raw,
}
impl ::core::clone::Clone for FormatsEmpty where
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            note: ::core::clone::Clone::clone(&self.note),
        }
    }
}
impl FormatsEmpty {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_note(mut self, value: impl ::concord::IntoValue<Raw>) -> Self {
        self.note = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.empty",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::new::<::concord::Text>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.note, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsEmpty {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsEmpty");
        __d.field("note", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.empty_bytes`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsEmptyBytes {
    __parent: Formats,
    b: Bytes,
}
impl ::core::clone::Clone for FormatsEmptyBytes where
    for<'__concord> Bytes: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            b: ::core::clone::Clone::clone(&self.b),
        }
    }
}
impl FormatsEmptyBytes {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_b(mut self, value: impl ::concord::IntoValue<Bytes>) -> Self {
        self.b = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.empty_bytes",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Bytes> =
                    ::concord::__private::BodyFormat::new::<::concord::Binary>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.b, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsEmptyBytes {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsEmptyBytes");
        __d.field("b", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.local`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsLocal {
    __parent: Formats,
    data: Raw,
}
impl ::core::clone::Clone for FormatsLocal where
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FormatsLocal {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<Raw>) -> Self {
        self.data = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.local",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::new::<::concord::Json>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.data, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsLocal {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsLocal");
        __d.field("data", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.media`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsMedia {
    __parent: Formats,
    data: Raw,
}
impl ::core::clone::Clone for FormatsMedia where
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FormatsMedia {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<Raw>) -> Self {
        self.data = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.media",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::with_media_type::<__ConcordFormatsRequestFormat>(::concord::reqwest::header::HeaderValue::from_static("application/custom"));
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.data, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsMedia {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsMedia");
        __d.field("data", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.no_media`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsNoMedia {
    __parent: Formats,
    data: String,
}
impl ::core::clone::Clone for FormatsNoMedia where
    for<'__concord> String: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FormatsNoMedia {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<String>)
        -> Self {
        self.data = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.no_media",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<String> =
                    ::concord::__private::BodyFormat::new::<NoMedia>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.data, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsNoMedia {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsNoMedia");
        __d.field("data", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.replaced`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsReplaced {
    __parent: Formats,
    data: String,
}
impl ::core::clone::Clone for FormatsReplaced where
    for<'__concord> String: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FormatsReplaced {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<String>)
        -> Self {
        self.data = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.replaced",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<String> =
                    ::concord::__private::BodyFormat::with_media_type::<InvalidMedia>(::concord::reqwest::header::HeaderValue::from_static("text/plain"));
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.data, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsReplaced {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsReplaced");
        __d.field("data", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.bad`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsBad {
    __parent: Formats,
    h: String,
    b: String,
}
impl ::core::clone::Clone for FormatsBad where
    for<'__concord> String: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            h: ::core::clone::Clone::clone(&self.h),
            b: ::core::clone::Clone::clone(&self.b),
        }
    }
}
impl FormatsBad {
    #[doc = ""]
    #[doc = " Replaces `h`."]
    #[doc = " Required: supplied when this request is created."]
    #[doc = " Sent as: header `x-bad`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_h(mut self, value: impl ::concord::IntoValue<String>) -> Self {
        self.h = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_b(mut self, value: impl ::concord::IntoValue<String>) -> Self {
        self.b = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.bad",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<String> =
                    ::concord::__private::BodyFormat::new::<FailEncode>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.b, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.header(::core::clone::Clone::clone(&__CONCORD_Formats_HEADER_0),
                "header `x-bad`", &self.h, false)?;
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsBad {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsBad");
        __d.field("h", &::concord::__private::DebugHeader(&self.h));
        __d.field("b", &"..");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.unit`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsUnit {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsUnit where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsUnit {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.unit",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<__ConcordFormatsResponseFormat,
                    ()>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsUnit {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsUnit");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.unit_alias`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsUnitAlias {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsUnitAlias where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsUnitAlias {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<Unit>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.unit_alias",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<__ConcordFormatsResponseFormat,
                    Unit>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self)
        -> ::core::result::Result<Unit, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsUnitAlias {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsUnitAlias");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.unit_explicit`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsUnitExplicit {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsUnitExplicit where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsUnitExplicit {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.unit_explicit",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<::concord::Json, ()>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsUnitExplicit {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsUnitExplicit");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.json_string`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsJsonString {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsJsonString where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsJsonString {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<Raw>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.json_string",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<__ConcordFormatsResponseFormat,
                    Raw>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<Raw, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsJsonString {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsJsonString");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.raw`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsRaw {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsRaw where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsRaw {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<Raw>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.raw",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<::concord::Text, Raw>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<Raw, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsRaw {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsRaw");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.alias`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsAlias {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsAlias where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsAlias {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<Raw>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.alias",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<WireText, Raw>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<Raw, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsAlias {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsAlias");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.raw_identifier`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsRawIdentifier {
    __parent: Formats,
}
impl ::core::clone::Clone for FormatsRawIdentifier where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsRawIdentifier {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<raw_types::json>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.raw_identifier",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<::concord::Text,
                    raw_types::json>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self)
        -> ::core::result::Result<raw_types::json, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsRawIdentifier {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsRawIdentifier");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.lexical`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsLexical {
    __parent: Formats,
    json: String,
    query: Raw,
}
impl ::core::clone::Clone for FormatsLexical where
    for<'__concord> String: ::core::clone::Clone,
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            json: ::core::clone::Clone::clone(&self.json),
            query: ::core::clone::Clone::clone(&self.query),
        }
    }
}
impl FormatsLexical {
    #[doc = ""]
    #[doc = " Replaces `json`."]
    #[doc = " Required: supplied when this request is created."]
    #[doc = " Sent as: query parameter `json`."]
    #[inline]
    pub fn set_json(mut self, value: impl ::concord::IntoValue<String>)
        -> Self {
        self.json = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_query(mut self, value: impl ::concord::IntoValue<Raw>)
        -> Self {
        self.query = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.lexical",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::new::<::concord::Json>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.query, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.query("json", &self.json, "query parameter `json`")?;
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsLexical {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsLexical");
        __d.field("json", &::concord::__private::DebugDisplay(&self.json));
        __d.field("query", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.custom_path`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsCustomPath {
    __parent: Formats,
    v: Raw,
}
impl ::core::clone::Clone for FormatsCustomPath where
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            v: ::core::clone::Clone::clone(&self.v),
        }
    }
}
impl FormatsCustomPath {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_v(mut self, value: impl ::concord::IntoValue<Raw>) -> Self {
        self.v = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<Raw>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.custom_path",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::new::<json::Codec>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.v, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::decoder::<self::csv, Raw>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<Raw, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsCustomPath {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsCustomPath");
        __d.field("v", &"..");
        __d.finish()
    }
}
#[doc = " Scope `Formats.nested`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `text` (declared here); typed outputs `json` (the default of the client `Formats`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct FormatsNested {
    __s: ::std::sync::Arc<__ConcordFormatsNestedState>,
}
#[automatically_derived]
impl ::core::clone::Clone for FormatsNested {
    #[inline]
    fn clone(&self) -> Self {
        FormatsNested { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordFormatsNestedState {
    __parent: Formats,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordFormatsNestedState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordFormatsNestedState {
            __parent: ::core::clone::Clone::clone(&self.__parent),
        }
    }
}
impl FormatsNested {
    #[doc = " `GET` endpoint `Formats.nested.typed`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `Raw`, decoded from a 2xx response with the format `json` (the `response` default of the client `Formats`)."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn typed(&self) -> FormatsNestedTyped {
        FormatsNestedTyped { __parent: ::core::clone::Clone::clone(self) }
    }
    #[doc = " `POST` endpoint `Formats.nested.text_body`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`x`): `Raw`, encoded with the format `text` (the `request` default of scope `Formats.nested`). `Content-Type: text/plain; charset=utf-8` is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `Formats` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `x`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn text_body(&self, x: impl ::concord::IntoValue<Raw>)
        -> FormatsNestedTextBody {
        FormatsNestedTextBody {
            __parent: ::core::clone::Clone::clone(self),
            x: ::concord::IntoValue::into_value(x),
        }
    }
    #[doc = " Enters scope `Formats.nested.deep`."]
    #[doc = ""]
    #[doc =
    " Formats of the endpoints below: request bodies `text` (the default of scope `Formats.nested`); typed outputs `bytes` (declared here). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn deep(&self) -> FormatsNestedDeep {
        FormatsNestedDeep {
            __s: ::std::sync::Arc::new(__ConcordFormatsNestedDeepState {
                    __parent: ::core::clone::Clone::clone(self),
                }),
        }
    }
}
impl ::core::fmt::Debug for FormatsNested {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("FormatsNested");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `Formats.nested.typed`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsNestedTyped {
    __parent: FormatsNested,
}
impl ::core::clone::Clone for FormatsNestedTyped where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormatsNestedTyped {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<Raw>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.nested.typed",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<__ConcordFormatsResponseFormat,
                    Raw>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<Raw, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsNestedTyped {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsNestedTyped");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.nested.text_body`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsNestedTextBody {
    __parent: FormatsNested,
    x: Raw,
}
impl ::core::clone::Clone for FormatsNestedTextBody where
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            x: ::core::clone::Clone::clone(&self.x),
        }
    }
}
impl FormatsNestedTextBody {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_x(mut self, value: impl ::concord::IntoValue<Raw>) -> Self {
        self.x = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.nested.text_body",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::new::<__ConcordFormatsNestedRequestFormat>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.x, &__META)?)
            };
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsNestedTextBody {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsNestedTextBody");
        __d.field("x", &"..");
        __d.finish()
    }
}
#[doc = " Scope `Formats.nested.deep`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `text` (the default of scope `Formats.nested`); typed outputs `bytes` (declared here). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct FormatsNestedDeep {
    __s: ::std::sync::Arc<__ConcordFormatsNestedDeepState>,
}
#[automatically_derived]
impl ::core::clone::Clone for FormatsNestedDeep {
    #[inline]
    fn clone(&self) -> Self {
        FormatsNestedDeep { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordFormatsNestedDeepState {
    __parent: FormatsNested,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordFormatsNestedDeepState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordFormatsNestedDeepState {
            __parent: ::core::clone::Clone::clone(&self.__parent),
        }
    }
}
impl FormatsNestedDeep {
    #[doc = " `POST` endpoint `Formats.nested.deep.both`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`x`): `Raw`, encoded with the format `text` (the `request` default of scope `Formats.nested`). `Content-Type: text/plain; charset=utf-8` is sent with the body."]
    #[doc =
    " Output: `Bytes`, decoded from a 2xx response with the format `bytes` (the `response` default of scope `Formats.nested.deep`)."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `x`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn both(&self, x: impl ::concord::IntoValue<Raw>)
        -> FormatsNestedDeepBoth {
        FormatsNestedDeepBoth {
            __parent: ::core::clone::Clone::clone(self),
            x: ::concord::IntoValue::into_value(x),
        }
    }
}
impl ::core::fmt::Debug for FormatsNestedDeep {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("FormatsNestedDeep");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `Formats.nested.deep.both`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormatsNestedDeepBoth {
    __parent: FormatsNestedDeep,
    x: Raw,
}
impl ::core::clone::Clone for FormatsNestedDeepBoth where
    for<'__concord> Raw: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            x: ::core::clone::Clone::clone(&self.x),
        }
    }
}
impl FormatsNestedDeepBoth {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_x(mut self, value: impl ::concord::IntoValue<Raw>) -> Self {
        self.x = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<Bytes>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "Formats.nested.deep.both",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Raw> =
                    ::concord::__private::BodyFormat::new::<__ConcordFormatsNestedRequestFormat>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.x, &__META)?)
            };
        let __o2 = &*self.__parent.__s;
        let __o1 = &*__o2.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::decoder::<__ConcordFormatsNestedDeepResponseFormat,
                    Bytes>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self)
        -> ::core::result::Result<Bytes, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormatsNestedDeepBoth {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormatsNestedDeepBoth");
        __d.field("x", &"..");
        __d.finish()
    }
}
fn body<T>(p: &wire::Prepared<T>) -> Option<&[u8]> {
    p.request().body().and_then(|b| b.as_bytes())
}
fn ct<T>(p: &wire::Prepared<T>) -> Option<&[u8]> {
    p.request().headers().get("content-type").map(|v| v.as_bytes())
}
extern crate test;
#[rustc_test_marker =
"format_presence_aliases_independent_defaults_lexical_and_media"]
#[doc(hidden)]
pub static format_presence_aliases_independent_defaults_lexical_and_media:
    test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("format_presence_aliases_independent_defaults_lexical_and_media"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 185usize,
            start_col: 4usize,
            end_line: 185usize,
            end_col: 66usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(format_presence_aliases_independent_defaults_lexical_and_media())),
    };
fn format_presence_aliases_independent_defaults_lexical_and_media() {
    let api = Formats::new(client());
    let p = api.nothing().prepare().unwrap();
    if !(body(&p).is_none() && ct(&p).is_none()) {
        ::core::panicking::panic("assertion failed: body(&p).is_none() && ct(&p).is_none()")
    };
    let p = api.null(None).prepare().unwrap();
    {
        match (&body(&p), &Some(&b"null"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    {
        match (&ct(&p), &Some(&b"application/vnd.default+json"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    for present in [false, true] {
        for header in [false, true] {
            let api = FourCases::new(client());
            let p =
                api.maybe().maybe_ct(header.then(||
                                        flagged("NATIVE-SECRET-ct"))).maybe_data(present.then_some(None)).prepare().unwrap();
            {
                match (&body(&p).is_some(), &present) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            let expected =
                if header {
                    Some(&b"NATIVE-SECRET-ct"[..])
                } else if present {
                    Some(&b"application/json"[..])
                } else { None };
            {
                match (&ct(&p), &expected) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            if header {
                if !p.request().headers()["content-type"].is_sensitive() {
                    ::core::panicking::panic("assertion failed: p.request().headers()[\"content-type\"].is_sensitive()")
                };
                no_secret(&p);
            }
        }
    }
    let p = api.empty("").prepare().unwrap();
    {
        match (&body(&p), &Some(&b""[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    {
        match (&ct(&p), &Some(&b"text/plain; charset=utf-8"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    let p = api.empty_bytes(Bytes::new()).prepare().unwrap();
    {
        match (&body(&p), &Some(&b""[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    {
        match (&ct(&p), &Some(&b"application/octet-stream"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    let p = api.local("s").prepare().unwrap();
    {
        match (&body(&p), &Some(&br#""s""#[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    {
        match (&ct(&p), &Some(&b"application/json"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    let p = api.media("s").prepare().unwrap();
    {
        match (&ct(&p), &Some(&b"application/custom"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    let p = api.no_media("s").prepare().unwrap();
    if !ct(&p).is_none() {
        ::core::panicking::panic("assertion failed: ct(&p).is_none()")
    };
    let p = api.replaced("s").prepare().unwrap();
    {
        match (&ct(&p), &Some(&b"text/plain"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    let error = api.bad("bad\nheader", "x").prepare().unwrap_err();
    {
        match (&error.kind(), &ErrorKind::Build) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    if !error.to_string().contains("intentional encoder failure") {
        ::core::panicking::panic("assertion failed: error.to_string().contains(\"intentional encoder failure\")")
    };
    let p = api.nested().text_body("raw").prepare().unwrap();
    {
        match (&body(&p), &Some(&b"raw"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    let p = api.lexical("q", "s").prepare().unwrap();
    {
        match (&p.request().url().query(), &Some("json=q")) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    {
        match (&body(&p), &Some(&br#""s""#[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
    let p = api.custom_path("custom").prepare().unwrap();
    {
        match (&body(&p), &Some(&b"custom"[..])) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(kind, &*left_val,
                        &*right_val, ::core::option::Option::None);
                }
            }
        }
    };
}
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_FourCases_HEADER_0: ::concord::reqwest::header::HeaderName =
    ::concord::reqwest::header::HeaderName::from_static("content-type");
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordFourCasesRequestFormat = ::concord::Json;
#[doc = " Client for `FourCases`, generated by `concord::api!`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `json` (declared here); typed outputs `json` (declared here). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct FourCases {
    __s: ::std::sync::Arc<__ConcordFourCasesState>,
}
#[automatically_derived]
impl ::core::clone::Clone for FourCases {
    #[inline]
    fn clone(&self) -> Self {
        FourCases { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordFourCasesState {
    __http: ::concord::reqwest::Client,
    __base: ::concord::__private::Base,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordFourCasesState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordFourCasesState {
            __http: ::core::clone::Clone::clone(&self.__http),
            __base: ::core::clone::Clone::clone(&self.__base),
        }
    }
}
impl FourCases {
    #[doc =
    r" Creates the client. `http` is used for every request; configure timeouts,"]
    #[doc =
    r" redirects, proxies, TLS and default headers on it. Builds no connection."]
    #[doc = r" Initializers of the client's values are evaluated here, once."]
    #[inline]
    pub fn new(http: ::concord::reqwest::Client) -> Self {
        Self {
            __s: ::std::sync::Arc::new(__ConcordFourCasesState {
                    __http: http,
                    __base: ::concord::__private::Base::from_static("http://127.0.0.1"),
                }),
        }
    }
    #[doc =
    r" Replaces the declared base URL (for example with a staging server or a"]
    #[doc =
    r" local test server). It must be an `http`/`https` URL with a host, no port 0"]
    #[doc =
    r" and no user information, query or fragment; anything else is an error."]
    #[doc =
    r" Declared paths are appended to its path. With `subdomain` labels the host"]
    #[doc =
    r" must be a domain name: to reach a local server, keep a domain and map it"]
    #[doc =
    r" with `reqwest::ClientBuilder::resolve`. Returns a new handle: scopes and"]
    #[doc = r" requests created from the old one keep the old base."]
    #[inline]
    pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>)
        -> ::core::result::Result<Self, ::concord::Error> {
        let base = ::concord::__private::Base::parse(url.as_ref())?;
        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
        ::core::result::Result::Ok(self)
    }
    #[doc = " `POST` endpoint `FourCases.maybe`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (optional `data`): `Option<u8>`, encoded with the format `json` (the `request` default of the client `FourCases`). `Content-Type`: the declared `content-type` header when it is sent (with or without the body), otherwise `application/json` with the body. Absent: no body, and the format adds no `Content-Type`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `FourCases` does not apply."]
    #[doc = " Headers (when present): `content-type`."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn maybe(&self) -> FourCasesMaybe {
        FourCasesMaybe {
            __parent: ::core::clone::Clone::clone(self),
            ct: ::core::option::Option::None,
            data: ::core::option::Option::None,
        }
    }
    #[doc = " Enters scope `FourCases.many`."]
    #[doc = ""]
    #[doc =
    " Formats of the endpoints below: request bodies `json` (the default of the client `FourCases`); typed outputs `json` (the default of the client `FourCases`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `cts`: required value."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn many(&self, cts: impl ::concord::IntoValue<Vec<Native>>)
        -> FourCasesMany {
        FourCasesMany {
            __s: ::std::sync::Arc::new(__ConcordFourCasesManyState {
                    __parent: ::core::clone::Clone::clone(self),
                    cts: ::concord::IntoValue::into_value(cts),
                }),
        }
    }
    #[doc = " Enters scope `FourCases.classified`."]
    #[doc = ""]
    #[doc =
    " Formats of the endpoints below: request bodies `json` (the default of the client `FourCases`); typed outputs `json` (the default of the client `FourCases`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn classified(&self) -> FourCasesClassified {
        FourCasesClassified {
            __s: ::std::sync::Arc::new(__ConcordFourCasesClassifiedState {
                    __parent: ::core::clone::Clone::clone(self),
                    ct: ::core::option::Option::None,
                }),
        }
    }
}
impl ::core::fmt::Debug for FourCases {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("FourCases");
        __d.field("base", &__s.__base);
        __d.finish()
    }
}
#[doc = " `POST` endpoint `FourCases.maybe`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FourCasesMaybe {
    __parent: FourCases,
    ct: ::core::option::Option<Native>,
    data: ::core::option::Option<Option<u8>>,
}
impl ::core::clone::Clone for FourCasesMaybe where
    for<'__concord> Native: ::core::clone::Clone,
    for<'__concord> Option<u8>: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            ct: ::core::clone::Clone::clone(&self.ct),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FourCasesMaybe {
    #[doc = ""]
    #[doc = " Sets `ct` (present)."]
    #[doc = " Initially absent."]
    #[doc = " Sent as: header `content-type`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_ct(mut self, value: impl ::concord::IntoValue<Native>)
        -> Self {
        self.ct =
            ::core::option::Option::Some(::concord::IntoValue::into_value(value));
        self
    }
    #[doc = ""]
    #[doc =
    " Sets whether `ct` is present: `Some(v)` is `set_ct(v)`; `None` makes it absent."]
    #[doc = " Initially absent."]
    #[doc = " Sent as: header `content-type`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn maybe_ct(mut self, value: ::core::option::Option<Native>) -> Self {
        self.ct = value;
        self
    }
    #[doc = ""]
    #[doc = " Sets the request body. Initially absent: no body is sent."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<Option<u8>>)
        -> Self {
        self.data =
            ::core::option::Option::Some(::concord::IntoValue::into_value(value));
        self
    }
    #[doc = ""]
    #[doc =
    " Sets the request body to exactly `value`: `None` sends no body."]
    #[inline]
    pub fn maybe_data(mut self, value: ::core::option::Option<Option<u8>>)
        -> Self {
        self.data = value;
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "FourCases.maybe",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Option<u8>>
                    =
                    ::concord::__private::BodyFormat::new::<__ConcordFourCasesRequestFormat>();
                match self.data {
                    ::core::option::Option::Some(__b) =>
                        ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                                    __b, &__META)?),
                    ::core::option::Option::None =>
                        ::core::option::Option::None,
                }
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        if let ::core::option::Option::Some(__v) = &self.ct {
            __p.header(::core::clone::Clone::clone(&__CONCORD_FourCases_HEADER_0),
                    "header `content-type`", &*__v, false)?;
        }
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FourCasesMaybe {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FourCasesMaybe");
        __d.field("ct",
            &self.ct.as_ref().map(::concord::__private::DebugHeader));
        __d.field("data", &self.data.as_ref().map(|_| ".."));
        __d.finish()
    }
}
#[doc = " Scope `FourCases.many`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `json` (the default of the client `FourCases`); typed outputs `json` (the default of the client `FourCases`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct FourCasesMany {
    __s: ::std::sync::Arc<__ConcordFourCasesManyState>,
}
#[automatically_derived]
impl ::core::clone::Clone for FourCasesMany {
    #[inline]
    fn clone(&self) -> Self {
        FourCasesMany { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordFourCasesManyState {
    __parent: FourCases,
    cts: Vec<Native>,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordFourCasesManyState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordFourCasesManyState {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            cts: ::core::clone::Clone::clone(&self.cts),
        }
    }
}
impl FourCasesMany {
    #[doc = ""]
    #[doc = " Replaces `cts`."]
    #[doc = " Required: supplied when this scope is created."]
    #[doc = " Sent as: header `content-type` (one occurrence per item)."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_cts(mut self, value: impl ::concord::IntoValue<Vec<Native>>)
        -> Self {
        ::std::sync::Arc::make_mut(&mut self.__s).cts =
            ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = " `POST` endpoint `FourCases.many.go`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (optional `data`): `Option<u8>`, encoded with the format `json` (the `request` default of the client `FourCases`). `Content-Type`: the declared `content-type` header when it is sent (with or without the body), otherwise `application/json` with the body. Absent: no body, and the format adds no `Content-Type`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `FourCases` does not apply."]
    #[doc = " Headers (when present): `content-type`."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn go(&self) -> FourCasesManyGo {
        FourCasesManyGo {
            __parent: ::core::clone::Clone::clone(self),
            data: ::core::option::Option::None,
        }
    }
}
impl ::core::fmt::Debug for FourCasesMany {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("FourCasesMany");
        __d.field("cts", &::concord::__private::DebugHeaderEach(&__s.cts));
        __d.finish()
    }
}
#[doc = " `POST` endpoint `FourCases.many.go`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FourCasesManyGo {
    __parent: FourCasesMany,
    data: ::core::option::Option<Option<u8>>,
}
impl ::core::clone::Clone for FourCasesManyGo where
    for<'__concord> Option<u8>: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FourCasesManyGo {
    #[doc = ""]
    #[doc = " Sets the request body. Initially absent: no body is sent."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<Option<u8>>)
        -> Self {
        self.data =
            ::core::option::Option::Some(::concord::IntoValue::into_value(value));
        self
    }
    #[doc = ""]
    #[doc =
    " Sets the request body to exactly `value`: `None` sends no body."]
    #[inline]
    pub fn maybe_data(mut self, value: ::core::option::Option<Option<u8>>)
        -> Self {
        self.data = value;
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "FourCases.many.go",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<Option<u8>>
                    =
                    ::concord::__private::BodyFormat::new::<__ConcordFourCasesRequestFormat>();
                match self.data {
                    ::core::option::Option::Some(__b) =>
                        ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                                    __b, &__META)?),
                    ::core::option::Option::None =>
                        ::core::option::Option::None,
                }
            };
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.header_each(::core::clone::Clone::clone(&__CONCORD_FourCases_HEADER_0),
                "header `content-type`", &__o1.cts, false)?;
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FourCasesManyGo {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FourCasesManyGo");
        __d.field("data", &self.data.as_ref().map(|_| ".."));
        __d.finish()
    }
}
#[doc = " Scope `FourCases.classified`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `json` (the default of the client `FourCases`); typed outputs `json` (the default of the client `FourCases`). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct FourCasesClassified {
    __s: ::std::sync::Arc<__ConcordFourCasesClassifiedState>,
}
#[automatically_derived]
impl ::core::clone::Clone for FourCasesClassified {
    #[inline]
    fn clone(&self) -> Self {
        FourCasesClassified { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordFourCasesClassifiedState {
    __parent: FourCases,
    ct: ::core::option::Option<Native>,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordFourCasesClassifiedState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordFourCasesClassifiedState {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            ct: ::core::clone::Clone::clone(&self.ct),
        }
    }
}
impl FourCasesClassified {
    #[doc = ""]
    #[doc = " Sets `ct` (present)."]
    #[doc = " Initially absent."]
    #[doc = " Sent as: header `content-type`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[doc =
    " Sensitive: sent normally, withheld from Concord's `Debug` output and errors."]
    #[inline]
    pub fn set_ct(mut self, value: impl ::concord::IntoValue<Native>)
        -> Self {
        ::std::sync::Arc::make_mut(&mut self.__s).ct =
            ::core::option::Option::Some(::concord::IntoValue::into_value(value));
        self
    }
    #[doc = ""]
    #[doc =
    " Sets whether `ct` is present: `Some(v)` is `set_ct(v)`; `None` makes it absent."]
    #[doc = " Initially absent."]
    #[doc = " Sent as: header `content-type`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[doc =
    " Sensitive: sent normally, withheld from Concord's `Debug` output and errors."]
    #[inline]
    pub fn maybe_ct(mut self, value: ::core::option::Option<Native>) -> Self {
        ::std::sync::Arc::make_mut(&mut self.__s).ct = value;
        self
    }
    #[doc = " `POST` endpoint `FourCases.classified.inserted`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`data`): `u8`, encoded with the format `json` (the `request` default of the client `FourCases`). `Content-Type: application/json` is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `FourCases` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `data`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn inserted(&self, data: impl ::concord::IntoValue<u8>)
        -> FourCasesClassifiedInserted {
        FourCasesClassifiedInserted {
            __parent: ::core::clone::Clone::clone(self),
            data: ::concord::IntoValue::into_value(data),
        }
    }
    #[doc = " `POST` endpoint `FourCases.classified.explicit`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`data`): `u8`, encoded with the format `json` (written in place). `Content-Type: application/custom` (declared on the body) is sent with the body."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained; the `response` default `json` of the client `FourCases` does not apply."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `data`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn explicit(&self, data: impl ::concord::IntoValue<u8>)
        -> FourCasesClassifiedExplicit {
        FourCasesClassifiedExplicit {
            __parent: ::core::clone::Clone::clone(self),
            data: ::concord::IntoValue::into_value(data),
        }
    }
}
impl ::core::fmt::Debug for FourCasesClassified {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("FourCasesClassified");
        __d.field("ct",
            &__s.ct.as_ref().map(|_| ::concord::__private::Redacted));
        __d.finish()
    }
}
#[doc = " `POST` endpoint `FourCases.classified.inserted`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FourCasesClassifiedInserted {
    __parent: FourCasesClassified,
    data: u8,
}
impl ::core::clone::Clone for FourCasesClassifiedInserted where
    for<'__concord> u8: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FourCasesClassifiedInserted {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<u8>) -> Self {
        self.data = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "FourCases.classified.inserted",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: true,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<u8> =
                    ::concord::__private::BodyFormat::new::<__ConcordFourCasesRequestFormat>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.data, &__META)?)
            };
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FourCasesClassifiedInserted {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FourCasesClassifiedInserted");
        __d.field("data", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `FourCases.classified.explicit`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FourCasesClassifiedExplicit {
    __parent: FourCasesClassified,
    data: u8,
}
impl ::core::clone::Clone for FourCasesClassifiedExplicit where
    for<'__concord> u8: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            data: ::core::clone::Clone::clone(&self.data),
        }
    }
}
impl FourCasesClassifiedExplicit {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_data(mut self, value: impl ::concord::IntoValue<u8>) -> Self {
        self.data = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "FourCases.classified.explicit",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: true,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<u8> =
                    ::concord::__private::BodyFormat::with_media_type::<::concord::Json>(::concord::reqwest::header::HeaderValue::from_static("application/custom"));
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.data, &__META)?)
            };
        let __o1 = &*self.__parent.__s;
        let __o0 = &*__o1.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FourCasesClassifiedExplicit {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FourCasesClassifiedExplicit");
        __d.field("data", &"..");
        __d.finish()
    }
}
extern crate test;
#[rustc_test_marker =
"composed_content_type_and_sensitive_insertions_reach_wire"]
#[doc(hidden)]
pub static composed_content_type_and_sensitive_insertions_reach_wire:
    test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("composed_content_type_and_sensitive_insertions_reach_wire"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 239usize,
            start_col: 10usize,
            end_line: 239usize,
            end_col: 67usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(composed_content_type_and_sensitive_insertions_reach_wire())),
    };
fn composed_content_type_and_sensitive_insertions_reach_wire() {
    let body =
        async {
            for present in [false, true] {
                for header in [false, true] {
                    let (url, s) = server(204, b"");
                    let p =
                        FourCases::new(client()).with_base_url(&url).unwrap().maybe().maybe_ct(header.then(||
                                                Native::from_static("application/custom"))).maybe_data(present.then_some(None)).prepare().unwrap();
                    if !!p.request().headers().contains_key("accept") {
                        ::core::panicking::panic("assertion failed: !p.request().headers().contains_key(\"accept\")")
                    };
                    p.send().await.unwrap();
                    let raw = s.join().unwrap();
                    let expected =
                        if header {
                            ::alloc::boxed::box_assume_init_into_vec_unsafe(::alloc::intrinsics::write_box_via_move(::alloc::boxed::Box::new_uninit(),
                                    [b"application/custom".to_vec()]))
                        } else if present {
                            ::alloc::boxed::box_assume_init_into_vec_unsafe(::alloc::intrinsics::write_box_via_move(::alloc::boxed::Box::new_uninit(),
                                    [b"application/json".to_vec()]))
                        } else { ::alloc::vec::Vec::new() };
                    {
                        match (&headers(&raw, "content-type"), &expected) {
                            (left_val, right_val) => {
                                if !(*left_val == *right_val) {
                                    let kind = ::core::panicking::AssertKind::Eq;
                                    ::core::panicking::assert_failed(kind, &*left_val,
                                        &*right_val, ::core::option::Option::None);
                                }
                            }
                        }
                    };
                    {
                        match (&headers(&raw, "accept"), &[b"*/*".to_vec()]) {
                            (left_val, right_val) => {
                                if !(*left_val == *right_val) {
                                    let kind = ::core::panicking::AssertKind::Eq;
                                    ::core::panicking::assert_failed(kind, &*left_val,
                                        &*right_val, ::core::option::Option::None);
                                }
                            }
                        }
                    };
                }
            }
            let api = FourCases::new(client());
            let p =
                api.many(::alloc::vec::Vec::new()).go().set_data(None).prepare().unwrap();
            {
                match (&ct(&p), &Some(&b"application/json"[..])) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            let p =
                api.many(::alloc::boxed::box_assume_init_into_vec_unsafe(::alloc::intrinsics::write_box_via_move(::alloc::boxed::Box::new_uninit(),
                                        [Native::from_static("")]))).go().prepare().unwrap();
            {
                match (&ct(&p), &Some(&b""[..])) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            let p =
                api.many(::alloc::boxed::box_assume_init_into_vec_unsafe(::alloc::intrinsics::write_box_via_move(::alloc::boxed::Box::new_uninit(),
                                        [Native::from_static("a"),
                                                flagged("NATIVE-SECRET")]))).go().prepare().unwrap();
            {
                match (&p.request().headers().get_all("content-type").iter().count(),
                        &2) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            for explicit in [false, true] {
                let (url, s) = server(204, b"");
                let api =
                    FourCases::new(client()).with_base_url(&url).unwrap();
                let p =
                    if explicit {
                        api.classified().explicit(7).prepare().unwrap()
                    } else { api.classified().inserted(7).prepare().unwrap() };
                if !p.request().headers()["content-type"].is_sensitive() {
                    ::core::panicking::panic("assertion failed: p.request().headers()[\"content-type\"].is_sensitive()")
                };
                if !!::alloc::__export::must_use({
                                        ::alloc::fmt::format(format_args!("{0:?}", p))
                                    }).contains(if explicit {
                                    "application/custom"
                                } else { "application/json" }) {
                    ::core::panicking::panic("assertion failed: !format!(\"{p:?}\").contains(if explicit {\n            \"application/custom\"\n        } else { \"application/json\" })")
                };
                p.send().await.unwrap();
                {
                    match (&headers(&s.join().unwrap(), "content-type").len(),
                            &1) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(kind, &*left_val,
                                    &*right_val, ::core::option::Option::None);
                            }
                        }
                    }
                };
            }
        };
    let mut body = body;
    #[allow(unused_mut)]
    let mut body =
        unsafe { ::tokio::macros::support::Pin::new_unchecked(&mut body) };
    let body: ::core::pin::Pin<&mut dyn ::core::future::Future<Output = ()>> =
        body;

    #[allow(clippy :: expect_used, clippy :: diverging_sub_expression, clippy
    :: needless_return, clippy :: unwrap_in_result)]
    {
        use tokio::runtime::Builder;
        return Builder::new_current_thread().enable_all().build().expect("Failed building the Runtime").block_on(body);
    }
}
extern crate test;
#[rustc_test_marker =
"drain_typed_unit_string_alias_status_and_native_decoder"]
#[doc(hidden)]
pub static drain_typed_unit_string_alias_status_and_native_decoder:
    test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("drain_typed_unit_string_alias_status_and_native_decoder"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 269usize,
            start_col: 10usize,
            end_line: 269usize,
            end_col: 65usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(drain_typed_unit_string_alias_status_and_native_decoder())),
    };
fn drain_typed_unit_string_alias_status_and_native_decoder() {
    let body =
        async {
            for status in [200, 204, 299] {
                let (url, s) = server(status, b"");
                Formats::new(client()).with_base_url(&url).unwrap().nothing().send().await.unwrap();
                s.join().unwrap();
            }
            for which in 0..3 {
                for bytes in [b"".as_slice(), b"null".as_slice()] {
                    let (url, s) =
                        server(if bytes.is_empty() { 204 } else { 200 }, bytes);
                    let api =
                        Formats::new(client()).with_base_url(&url).unwrap();
                    let result =
                        match which {
                            0 => api.unit().send().await,
                            1 => api.unit_alias().send().await,
                            _ => api.unit_explicit().send().await,
                        };
                    if bytes.is_empty() {
                        {
                            match (&result.unwrap_err().kind(), &ErrorKind::Decode) {
                                (left_val, right_val) => {
                                    if !(*left_val == *right_val) {
                                        let kind = ::core::panicking::AssertKind::Eq;
                                        ::core::panicking::assert_failed(kind, &*left_val,
                                            &*right_val, ::core::option::Option::None);
                                    }
                                }
                            }
                        };
                    } else { result.unwrap(); }
                    s.join().unwrap();
                }
            }
            for which in 0..5 {
                let (url, s) =
                    server(200,
                        if which == 0 || which == 4 { br#""hi""# } else { b"hi" });
                let api = Formats::new(client()).with_base_url(&url).unwrap();
                let p =
                    match which {
                            0 => api.json_string().prepare(),
                            1 => api.raw().prepare(),
                            2 => api.alias().prepare(),
                            3 => api.raw_identifier().prepare(),
                            _ => api.nested().typed().prepare(),
                        }.unwrap();
                let (http, mut request, decoder) = p.into_parts();
                request.headers_mut().insert("x-native",
                    HeaderValue::from_static("retained"));
                let response = http.execute(request).await.unwrap();
                {
                    match (&response.status().as_u16(), &200) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(kind, &*left_val,
                                    &*right_val, ::core::option::Option::None);
                            }
                        }
                    }
                };
                {
                    match (&decoder.decode(response).await.unwrap(), &"hi") {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(kind, &*left_val,
                                    &*right_val, ::core::option::Option::None);
                            }
                        }
                    }
                };
                {
                    match (&headers(&s.join().unwrap(), "x-native"),
                            &[b"retained".to_vec()]) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(kind, &*left_val,
                                    &*right_val, ::core::option::Option::None);
                            }
                        }
                    }
                };
            }
            let (url, s) = server(400, b"unread-response");
            let e =
                Formats::new(client()).with_base_url(&url).unwrap().nothing().send().await.unwrap_err();
            {
                match (&e.kind(), &ErrorKind::Status) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            {
                match (&e.into_response().unwrap().bytes().await.unwrap().as_ref(),
                        &b"unread-response") {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            s.join().unwrap();
            let (url, s) = server(418, b"unchecked");
            let response =
                Formats::new(client()).with_base_url(&url).unwrap().json_string().prepare().unwrap().send_raw().await.unwrap();
            {
                match (&response.status().as_u16(), &418) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            {
                match (&response.bytes().await.unwrap().as_ref(),
                        &b"unchecked") {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            s.join().unwrap();
            let (url, s) = server_length(200, b"partial", 100);
            let error =
                Formats::new(client()).with_base_url(&url).unwrap().nothing().send().await.unwrap_err();
            {
                match (&error.kind(), &ErrorKind::Transport) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            s.join().unwrap();
            let (url, s) = server(200, b"raw-string");
            let error =
                Formats::new(client()).with_base_url(&url).unwrap().json_string().send().await.unwrap_err();
            {
                match (&error.kind(), &ErrorKind::Decode) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            s.join().unwrap();
        };
    let mut body = body;
    #[allow(unused_mut)]
    let mut body =
        unsafe { ::tokio::macros::support::Pin::new_unchecked(&mut body) };
    let body: ::core::pin::Pin<&mut dyn ::core::future::Future<Output = ()>> =
        body;

    #[allow(clippy :: expect_used, clippy :: diverging_sub_expression, clippy
    :: needless_return, clippy :: unwrap_in_result)]
    {
        use tokio::runtime::Builder;
        return Builder::new_current_thread().enable_all().build().expect("Failed building the Runtime").block_on(body);
    }
}
pub struct FormInput {
    label: String,
    count: u8,
}
#[automatically_derived]
impl ::core::clone::Clone for FormInput {
    #[inline]
    fn clone(&self) -> Self {
        FormInput {
            label: ::core::clone::Clone::clone(&self.label),
            count: ::core::clone::Clone::clone(&self.count),
        }
    }
}
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications,
clippy :: absolute_paths,)]
const _: () =
    {
        #[allow(unused_extern_crates, clippy :: useless_attribute)]
        extern crate serde as _serde;
        ;
        #[automatically_derived]
        impl _serde::Serialize for FormInput {
            fn serialize<__S>(&self, __serializer: __S)
                -> _serde::__private229::Result<__S::Ok, __S::Error> where
                __S: _serde::Serializer {
                let mut __serde_state =
                    _serde::Serializer::serialize_struct(__serializer,
                            "FormInput", false as usize + 1 + 1)?;
                _serde::ser::SerializeStruct::serialize_field(&mut __serde_state,
                        "label", &self.label)?;
                _serde::ser::SerializeStruct::serialize_field(&mut __serde_state,
                        "count", &self.count)?;
                _serde::ser::SerializeStruct::end(__serde_state)
            }
        }
    };
#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications,
clippy :: absolute_paths,)]
const _: () =
    {
        #[allow(unused_extern_crates, clippy :: useless_attribute)]
        extern crate serde as _serde;
        ;
        #[automatically_derived]
        impl<'de> _serde::Deserialize<'de> for FormInput {
            fn deserialize<__D>(__deserializer: __D)
                -> _serde::__private229::Result<Self, __D::Error> where
                __D: _serde::Deserializer<'de> {
                #[allow(non_camel_case_types)]
                #[doc(hidden)]
                enum __Field { __field0, __field1, __ignore, }
                #[doc(hidden)]
                struct __FieldVisitor;
                #[automatically_derived]
                impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                    type Value = __Field;
                    fn expecting(&self,
                        __formatter: &mut _serde::__private229::Formatter)
                        -> _serde::__private229::fmt::Result {
                        _serde::__private229::Formatter::write_str(__formatter,
                            "field identifier")
                    }
                    fn visit_u64<__E>(self, __value: u64)
                        -> _serde::__private229::Result<Self::Value, __E> where
                        __E: _serde::de::Error {
                        match __value {
                            0u64 => _serde::__private229::Ok(__Field::__field0),
                            1u64 => _serde::__private229::Ok(__Field::__field1),
                            _ => _serde::__private229::Ok(__Field::__ignore),
                        }
                    }
                    fn visit_str<__E>(self, __value: &str)
                        -> _serde::__private229::Result<Self::Value, __E> where
                        __E: _serde::de::Error {
                        match __value {
                            "label" => _serde::__private229::Ok(__Field::__field0),
                            "count" => _serde::__private229::Ok(__Field::__field1),
                            _ => { _serde::__private229::Ok(__Field::__ignore) }
                        }
                    }
                    fn visit_bytes<__E>(self, __value: &[u8])
                        -> _serde::__private229::Result<Self::Value, __E> where
                        __E: _serde::de::Error {
                        match __value {
                            b"label" => _serde::__private229::Ok(__Field::__field0),
                            b"count" => _serde::__private229::Ok(__Field::__field1),
                            _ => { _serde::__private229::Ok(__Field::__ignore) }
                        }
                    }
                }
                #[automatically_derived]
                impl<'de> _serde::Deserialize<'de> for __Field {
                    #[inline]
                    fn deserialize<__D>(__deserializer: __D)
                        -> _serde::__private229::Result<Self, __D::Error> where
                        __D: _serde::Deserializer<'de> {
                        _serde::Deserializer::deserialize_identifier(__deserializer,
                            __FieldVisitor)
                    }
                }
                #[doc(hidden)]
                struct __Visitor<'de> {
                    marker: _serde::__private229::PhantomData<FormInput>,
                    lifetime: _serde::__private229::PhantomData<&'de ()>,
                }
                #[automatically_derived]
                impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                    type Value = FormInput;
                    fn expecting(&self,
                        __formatter: &mut _serde::__private229::Formatter)
                        -> _serde::__private229::fmt::Result {
                        _serde::__private229::Formatter::write_str(__formatter,
                            "struct FormInput")
                    }
                    #[inline]
                    fn visit_seq<__A>(self, mut __seq: __A)
                        -> _serde::__private229::Result<Self::Value, __A::Error>
                        where __A: _serde::de::SeqAccess<'de> {
                        let __field0 =
                            match _serde::de::SeqAccess::next_element::<String>(&mut __seq)?
                                {
                                _serde::__private229::Some(__value) => __value,
                                _serde::__private229::None =>
                                    return _serde::__private229::Err(_serde::de::Error::invalid_length(0usize,
                                                &"struct FormInput with 2 elements")),
                            };
                        let __field1 =
                            match _serde::de::SeqAccess::next_element::<u8>(&mut __seq)?
                                {
                                _serde::__private229::Some(__value) => __value,
                                _serde::__private229::None =>
                                    return _serde::__private229::Err(_serde::de::Error::invalid_length(1usize,
                                                &"struct FormInput with 2 elements")),
                            };
                        _serde::__private229::Ok(FormInput {
                                label: __field0,
                                count: __field1,
                            })
                    }
                    #[inline]
                    fn visit_map<__A>(self, mut __map: __A)
                        -> _serde::__private229::Result<Self::Value, __A::Error>
                        where __A: _serde::de::MapAccess<'de> {
                        let mut __field0: _serde::__private229::Option<String> =
                            _serde::__private229::None;
                        let mut __field1: _serde::__private229::Option<u8> =
                            _serde::__private229::None;
                        while let _serde::__private229::Some(__key) =
                                _serde::de::MapAccess::next_key::<__Field>(&mut __map)? {
                            match __key {
                                __Field::__field0 => {
                                    if _serde::__private229::Option::is_some(&__field0) {
                                        return _serde::__private229::Err(<__A::Error as
                                                        _serde::de::Error>::duplicate_field("label"));
                                    }
                                    __field0 =
                                        _serde::__private229::Some(_serde::de::MapAccess::next_value::<String>(&mut __map)?);
                                }
                                __Field::__field1 => {
                                    if _serde::__private229::Option::is_some(&__field1) {
                                        return _serde::__private229::Err(<__A::Error as
                                                        _serde::de::Error>::duplicate_field("count"));
                                    }
                                    __field1 =
                                        _serde::__private229::Some(_serde::de::MapAccess::next_value::<u8>(&mut __map)?);
                                }
                                _ => {
                                    let _ =
                                        _serde::de::MapAccess::next_value::<_serde::de::IgnoredAny>(&mut __map)?;
                                }
                            }
                        }
                        let __field0 =
                            match __field0 {
                                _serde::__private229::Some(__field0) => __field0,
                                _serde::__private229::None =>
                                    _serde::__private229::de::missing_field("label")?,
                            };
                        let __field1 =
                            match __field1 {
                                _serde::__private229::Some(__field1) => __field1,
                                _serde::__private229::None =>
                                    _serde::__private229::de::missing_field("count")?,
                            };
                        _serde::__private229::Ok(FormInput {
                                label: __field0,
                                count: __field1,
                            })
                    }
                }
                #[doc(hidden)]
                const FIELDS: &'static [&'static str] = &["label", "count"];
                _serde::Deserializer::deserialize_struct(__deserializer,
                    "FormInput", FIELDS,
                    __Visitor {
                        marker: _serde::__private229::PhantomData::<FormInput>,
                        lifetime: _serde::__private229::PhantomData,
                    })
            }
        }
    };
#[automatically_derived]
impl ::core::fmt::Debug for FormInput {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        ::core::fmt::Formatter::debug_struct_field2_finish(f, "FormInput",
            "label", &self.label, "count", &&self.count)
    }
}
#[automatically_derived]
impl ::core::marker::StructuralPartialEq for FormInput { }
#[automatically_derived]
impl ::core::cmp::PartialEq for FormInput {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count && self.label == other.label
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordFormApiRequestFormat = ::concord::Form;
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordFormApiResponseFormat = ::concord::Json;
#[doc = " Client for `FormApi`, generated by `concord::api!`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `form` (declared here); typed outputs `json` (declared here). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct FormApi {
    __s: ::std::sync::Arc<__ConcordFormApiState>,
}
#[automatically_derived]
impl ::core::clone::Clone for FormApi {
    #[inline]
    fn clone(&self) -> Self {
        FormApi { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordFormApiState {
    __http: ::concord::reqwest::Client,
    __base: ::concord::__private::Base,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordFormApiState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordFormApiState {
            __http: ::core::clone::Clone::clone(&self.__http),
            __base: ::core::clone::Clone::clone(&self.__base),
        }
    }
}
impl FormApi {
    #[doc =
    r" Creates the client. `http` is used for every request; configure timeouts,"]
    #[doc =
    r" redirects, proxies, TLS and default headers on it. Builds no connection."]
    #[doc = r" Initializers of the client's values are evaluated here, once."]
    #[inline]
    pub fn new(http: ::concord::reqwest::Client) -> Self {
        Self {
            __s: ::std::sync::Arc::new(__ConcordFormApiState {
                    __http: http,
                    __base: ::concord::__private::Base::from_static("http://127.0.0.1"),
                }),
        }
    }
    #[doc =
    r" Replaces the declared base URL (for example with a staging server or a"]
    #[doc =
    r" local test server). It must be an `http`/`https` URL with a host, no port 0"]
    #[doc =
    r" and no user information, query or fragment; anything else is an error."]
    #[doc =
    r" Declared paths are appended to its path. With `subdomain` labels the host"]
    #[doc =
    r" must be a domain name: to reach a local server, keep a domain and map it"]
    #[doc =
    r" with `reqwest::ClientBuilder::resolve`. Returns a new handle: scopes and"]
    #[doc = r" requests created from the old one keep the old base."]
    #[inline]
    pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>)
        -> ::core::result::Result<Self, ::concord::Error> {
        let base = ::concord::__private::Base::parse(url.as_ref())?;
        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
        ::core::result::Result::Ok(self)
    }
    #[doc = " `POST` endpoint `FormApi.submit`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`input`): `FormInput`, encoded with the format `form` (the `request` default of the client `FormApi`). `Content-Type: application/x-www-form-urlencoded` is sent with the body."]
    #[doc =
    " Output: `FormInput`, decoded from a 2xx response with the format `json` (the `response` default of the client `FormApi`)."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `input`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn submit(&self, input: impl ::concord::IntoValue<FormInput>)
        -> FormApiSubmit {
        FormApiSubmit {
            __parent: ::core::clone::Clone::clone(self),
            input: ::concord::IntoValue::into_value(input),
        }
    }
    #[doc = " `GET` endpoint `FormApi.decoded`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1`."]
    #[doc =
    " Output: `FormInput`, decoded from a 2xx response with the format `form` (written in place)."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn decoded(&self) -> FormApiDecoded {
        FormApiDecoded { __parent: ::core::clone::Clone::clone(self) }
    }
}
impl ::core::fmt::Debug for FormApi {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("FormApi");
        __d.field("base", &__s.__base);
        __d.finish()
    }
}
#[doc = " `POST` endpoint `FormApi.submit`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormApiSubmit {
    __parent: FormApi,
    input: FormInput,
}
impl ::core::clone::Clone for FormApiSubmit where
    for<'__concord> FormInput: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            input: ::core::clone::Clone::clone(&self.input),
        }
    }
}
impl FormApiSubmit {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_input(mut self, value: impl ::concord::IntoValue<FormInput>)
        -> Self {
        self.input = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<FormInput>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "FormApi.submit",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<FormInput> =
                    ::concord::__private::BodyFormat::new::<__ConcordFormApiRequestFormat>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.input, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::decoder::<__ConcordFormApiResponseFormat,
                    FormInput>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self)
        -> ::core::result::Result<FormInput, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormApiSubmit {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormApiSubmit");
        __d.field("input", &"..");
        __d.finish()
    }
}
#[doc = " `GET` endpoint `FormApi.decoded`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct FormApiDecoded {
    __parent: FormApi,
}
impl ::core::clone::Clone for FormApiDecoded where  {
    #[inline]
    fn clone(&self) -> Self {
        Self { __parent: ::core::clone::Clone::clone(&self.__parent) }
    }
}
impl FormApiDecoded {
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<FormInput>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "FormApi.decoded",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::decoder::<::concord::Form,
                    FormInput>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self)
        -> ::core::result::Result<FormInput, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for FormApiDecoded {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("FormApiDecoded");
        __d.finish()
    }
}
extern crate test;
#[rustc_test_marker = "form_input_json_output_and_form_decode"]
#[doc(hidden)]
pub static form_input_json_output_and_form_decode: test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("form_input_json_output_and_form_decode"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 321usize,
            start_col: 10usize,
            end_line: 321usize,
            end_col: 48usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(form_input_json_output_and_form_decode())),
    };
fn form_input_json_output_and_form_decode() {
    let body =
        async {
            let (url, s) = server(200, br#"{"label":"a b+","count":7}"#);
            let input = FormInput { label: "a b+".into(), count: 7 };
            let out =
                FormApi::new(client()).with_base_url(&url).unwrap().submit(input.clone()).send().await.unwrap();
            {
                match (&out, &input) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            let raw = s.join().unwrap();
            {
                match (&headers(&raw, "content-type"),
                        &[b"application/x-www-form-urlencoded".to_vec()]) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            if !raw.ends_with(b"label=a+b%2B&count=7") {
                ::core::panicking::panic("assertion failed: raw.ends_with(b\"label=a+b%2B&count=7\")")
            };
            let (url, s) = server(200, b"label=a+b%2B&count=7");
            {
                match (&FormApi::new(client()).with_base_url(&url).unwrap().decoded().send().await.unwrap(),
                        &input) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(kind, &*left_val,
                                &*right_val, ::core::option::Option::None);
                        }
                    }
                }
            };
            s.join().unwrap();
        };
    let mut body = body;
    #[allow(unused_mut)]
    let mut body =
        unsafe { ::tokio::macros::support::Pin::new_unchecked(&mut body) };
    let body: ::core::pin::Pin<&mut dyn ::core::future::Future<Output = ()>> =
        body;

    #[allow(clippy :: expect_used, clippy :: diverging_sub_expression, clippy
    :: needless_return, clippy :: unwrap_in_result)]
    {
        use tokio::runtime::Builder;
        return Builder::new_current_thread().enable_all().build().expect("Failed building the Runtime").block_on(body);
    }
}
pub struct Dual(&'static str);
#[automatically_derived]
impl ::core::clone::Clone for Dual {
    #[inline]
    fn clone(&self) -> Self { Dual(::core::clone::Clone::clone(&self.0)) }
}
impl ToHeaderValue for Dual {
    fn to_header_value(&self) -> Option<Native> { Some(flagged(self.0)) }
}
impl std::fmt::Display for Dual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_DualUse_HEADER_0: ::concord::reqwest::header::HeaderName =
    ::concord::reqwest::header::HeaderName::from_static("x-dual");
#[doc(hidden)]
#[allow(non_upper_case_globals, dead_code)]
static __CONCORD_DualUse_HEADER_1: ::concord::reqwest::header::HeaderName =
    ::concord::reqwest::header::HeaderName::from_static("x-copy");
#[doc = " Client for `DualUse`, generated by `concord::api!`."]
pub struct DualUse {
    __s: ::std::sync::Arc<__ConcordDualUseState>,
}
#[automatically_derived]
impl ::core::clone::Clone for DualUse {
    #[inline]
    fn clone(&self) -> Self {
        DualUse { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordDualUseState {
    __http: ::concord::reqwest::Client,
    __base: ::concord::__private::Base,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordDualUseState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordDualUseState {
            __http: ::core::clone::Clone::clone(&self.__http),
            __base: ::core::clone::Clone::clone(&self.__base),
        }
    }
}
impl DualUse {
    #[doc =
    r" Creates the client. `http` is used for every request; configure timeouts,"]
    #[doc =
    r" redirects, proxies, TLS and default headers on it. Builds no connection."]
    #[doc = r" Initializers of the client's values are evaluated here, once."]
    #[inline]
    pub fn new(http: ::concord::reqwest::Client) -> Self {
        Self {
            __s: ::std::sync::Arc::new(__ConcordDualUseState {
                    __http: http,
                    __base: ::concord::__private::Base::from_static("http://127.0.0.1"),
                }),
        }
    }
    #[doc =
    r" Replaces the declared base URL (for example with a staging server or a"]
    #[doc =
    r" local test server). It must be an `http`/`https` URL with a host, no port 0"]
    #[doc =
    r" and no user information, query or fragment; anything else is an error."]
    #[doc =
    r" Declared paths are appended to its path. With `subdomain` labels the host"]
    #[doc =
    r" must be a domain name: to reach a local server, keep a domain and map it"]
    #[doc =
    r" with `reqwest::ClientBuilder::resolve`. Returns a new handle: scopes and"]
    #[doc = r" requests created from the old one keep the old base."]
    #[inline]
    pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>)
        -> ::core::result::Result<Self, ::concord::Error> {
        let base = ::concord::__private::Base::parse(url.as_ref())?;
        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
        ::core::result::Result::Ok(self)
    }
    #[doc = " `GET` endpoint `DualUse.go`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1/{v}`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained."]
    #[doc = " Query (when present): `q`."]
    #[doc = " Headers (when present): `x-dual`, `x-copy`."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `v`: required value."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn go(&self, v: impl ::concord::IntoValue<Dual>) -> DualUseGo {
        DualUseGo {
            __parent: ::core::clone::Clone::clone(self),
            v: ::concord::IntoValue::into_value(v),
        }
    }
    #[doc = " `GET` endpoint `DualUse.declared`."]
    #[doc = ""]
    #[doc = " Request: `GET http://127.0.0.1/{v}`."]
    #[doc =
    " Output: none (`()`): any 2xx succeeds and the response body is drained."]
    #[doc = " Query (when present): `q` (sensitive)."]
    #[doc =
    " Headers (when present): `x-dual` (sensitive), `x-copy` (sensitive)."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `v`: required value."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn declared(&self, v: impl ::concord::IntoValue<Dual>)
        -> DualUseDeclared {
        DualUseDeclared {
            __parent: ::core::clone::Clone::clone(self),
            v: ::concord::IntoValue::into_value(v),
        }
    }
}
impl ::core::fmt::Debug for DualUse {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("DualUse");
        __d.field("base", &__s.__base);
        __d.finish()
    }
}
#[doc = " `GET` endpoint `DualUse.go`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct DualUseGo {
    __parent: DualUse,
    v: Dual,
}
impl ::core::clone::Clone for DualUseGo where
    for<'__concord> Dual: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            v: ::core::clone::Clone::clone(&self.v),
        }
    }
}
impl DualUseGo {
    #[doc = ""]
    #[doc = " Replaces `v`."]
    #[doc = " Required: supplied when this request is created."]
    #[doc =
    " Sent as: path segment `{v}`; query parameter `q`; header `x-dual`; header `x-copy`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[inline]
    pub fn set_v(mut self, value: impl ::concord::IntoValue<Dual>) -> Self {
        self.v = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "DualUse.go",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 1usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        {
            let mut __c = ::std::string::String::new();
            ::concord::__private::encode_segment(&mut __c, &self.v, &__META,
                    "path segment `{v}`")?;
            __p.segment(__c, "path segment `{v}`")?;
        }
        __p.query("q", &self.v, "query parameter `q`")?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_DualUse_HEADER_0),
                "header `x-dual`", &self.v, false)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_DualUse_HEADER_1),
                "header `x-copy`",
                &::concord::__private::format_template(format_args!("{0}",
                                ::concord::__private::text(&self.v)), &__META,
                            "header `x-copy`")?, false)?;
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for DualUseGo {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("DualUseGo");
        __d.field("v", &::concord::__private::DebugTextHeader(&self.v));
        __d.finish()
    }
}
#[doc = " `GET` endpoint `DualUse.declared`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct DualUseDeclared {
    __parent: DualUse,
    v: Dual,
}
impl ::core::clone::Clone for DualUseDeclared where
    for<'__concord> Dual: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            v: ::core::clone::Clone::clone(&self.v),
        }
    }
}
impl DualUseDeclared {
    #[doc = ""]
    #[doc = " Replaces `v`."]
    #[doc = " Required: supplied when this request is created."]
    #[doc =
    " Sent as: path segment `{v}`; query parameter `q`; header `x-dual`; header `x-copy`."]
    #[doc =
    " As a header value it goes through `concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity flag; text must be visible ASCII, space or tab; integers and `bool` are written in decimal / `true`/`false`."]
    #[doc =
    " Sensitive: sent normally, withheld from Concord's `Debug` output and errors."]
    #[inline]
    pub fn set_v(mut self, value: impl ::concord::IntoValue<Dual>) -> Self {
        self.v = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        -> ::core::result::Result<::concord::Prepared<()>, ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "DualUse.declared",
                method: "GET",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 1usize,
                sensitive_segments: &[0usize],
                sensitive_query: &["q"],
                sensitive_content_type: false,
            };
        let __body = ::core::option::Option::None;
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        {
            let mut __c = ::std::string::String::new();
            ::concord::__private::encode_segment(&mut __c, &self.v, &__META,
                    "path segment `{v}`")?;
            __p.segment(__c, "path segment `{v}`")?;
        }
        __p.query("q", &self.v, "query parameter `q`")?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_DualUse_HEADER_0),
                "header `x-dual`", &self.v, true)?;
        __p.header(::core::clone::Clone::clone(&__CONCORD_DualUse_HEADER_1),
                "header `x-copy`",
                &::concord::__private::format_template(format_args!("{0}",
                                ::concord::__private::text(&self.v)), &__META,
                            "header `x-copy`")?, true)?;
        __p.finish(::concord::reqwest::Method::GET, &__o0.__http, __body,
            ::concord::__private::drain(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self) -> ::core::result::Result<(), ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for DualUseDeclared {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("DualUseDeclared");
        __d.field("v", &::concord::__private::Redacted);
        __d.finish()
    }
}
extern crate test;
#[rustc_test_marker = "declared_sensitive_dual_use_is_protected"]
#[doc(hidden)]
pub static declared_sensitive_dual_use_is_protected: test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("declared_sensitive_dual_use_is_protected"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 349usize,
            start_col: 4usize,
            end_line: 349usize,
            end_col: 44usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(declared_sensitive_dual_use_is_protected())),
    };
fn declared_sensitive_dual_use_is_protected() {
    let p =
        DualUse::new(client()).declared(Dual("DECLARED-SECRET")).prepare().unwrap();
    no_secret(&p);
    if !p.request().headers()["x-dual"].is_sensitive() {
        ::core::panicking::panic("assertion failed: p.request().headers()[\"x-dual\"].is_sensitive()")
    };
    if !p.request().headers()["x-copy"].is_sensitive() {
        ::core::panicking::panic("assertion failed: p.request().headers()[\"x-copy\"].is_sensitive()")
    };
}
pub mod custom {
    pub struct Codec;
    impl wire::Encode<String> for Codec {
        const CONTENT_TYPE: Option<&'static str> =
            Some("application/x-independent");
        fn encode(v: String) -> Result<wire::reqwest::Body, wire::BodyError> {
            Ok(::alloc::__export::must_use({
                            ::alloc::fmt::format(format_args!("custom:{0}", v))
                        }).into())
        }
    }
    impl wire::Decode<String> for Codec {
        fn decode(v: wire::bytes::Bytes) -> Result<String, wire::BodyError> {
            let text =
                std::str::from_utf8(&v).map_err(|_|
                            wire::BodyError::new("custom invalid utf8"))?;
            text.strip_prefix("custom:").map(str::to_owned).ok_or(wire::BodyError::new("custom missing prefix"))
        }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordCustomApiRequestFormat = custom::Codec;
#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
type __ConcordCustomApiResponseFormat = custom::Codec;
#[doc = " Client for `CustomApi`, generated by `concord::api!`."]
#[doc = ""]
#[doc =
" Formats of the endpoints below: request bodies `custom::Codec` (declared here); typed outputs `custom::Codec` (declared here). A format written on a body or after `->` replaces the default; an endpoint without `->` drains its response."]
pub struct CustomApi {
    __s: ::std::sync::Arc<__ConcordCustomApiState>,
}
#[automatically_derived]
impl ::core::clone::Clone for CustomApi {
    #[inline]
    fn clone(&self) -> Self {
        CustomApi { __s: ::core::clone::Clone::clone(&self.__s) }
    }
}
#[doc(hidden)]
#[allow(non_camel_case_types)]
struct __ConcordCustomApiState {
    __http: ::concord::reqwest::Client,
    __base: ::concord::__private::Base,
}
#[automatically_derived]
#[allow(non_camel_case_types)]
impl ::core::clone::Clone for __ConcordCustomApiState {
    #[inline]
    fn clone(&self) -> Self {
        __ConcordCustomApiState {
            __http: ::core::clone::Clone::clone(&self.__http),
            __base: ::core::clone::Clone::clone(&self.__base),
        }
    }
}
impl CustomApi {
    #[doc =
    r" Creates the client. `http` is used for every request; configure timeouts,"]
    #[doc =
    r" redirects, proxies, TLS and default headers on it. Builds no connection."]
    #[doc = r" Initializers of the client's values are evaluated here, once."]
    #[inline]
    pub fn new(http: ::concord::reqwest::Client) -> Self {
        Self {
            __s: ::std::sync::Arc::new(__ConcordCustomApiState {
                    __http: http,
                    __base: ::concord::__private::Base::from_static("http://127.0.0.1"),
                }),
        }
    }
    #[doc =
    r" Replaces the declared base URL (for example with a staging server or a"]
    #[doc =
    r" local test server). It must be an `http`/`https` URL with a host, no port 0"]
    #[doc =
    r" and no user information, query or fragment; anything else is an error."]
    #[doc =
    r" Declared paths are appended to its path. With `subdomain` labels the host"]
    #[doc =
    r" must be a domain name: to reach a local server, keep a domain and map it"]
    #[doc =
    r" with `reqwest::ClientBuilder::resolve`. Returns a new handle: scopes and"]
    #[doc = r" requests created from the old one keep the old base."]
    #[inline]
    pub fn with_base_url(mut self, url: impl ::core::convert::AsRef<str>)
        -> ::core::result::Result<Self, ::concord::Error> {
        let base = ::concord::__private::Base::parse(url.as_ref())?;
        ::std::sync::Arc::make_mut(&mut self.__s).__base = base;
        ::core::result::Result::Ok(self)
    }
    #[doc = " `POST` endpoint `CustomApi.inherited`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`value`): `String`, encoded with the format `custom::Codec` (the `request` default of the client `CustomApi`). `Content-Type`: the format's `Encode::CONTENT_TYPE`, sent with the body."]
    #[doc =
    " Output: `String`, decoded from a 2xx response with the format `custom::Codec` (the `response` default of the client `CustomApi`)."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `value`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn inherited(&self, value: impl ::concord::IntoValue<String>)
        -> CustomApiInherited {
        CustomApiInherited {
            __parent: ::core::clone::Clone::clone(self),
            value: ::concord::IntoValue::into_value(value),
        }
    }
    #[doc = " `POST` endpoint `CustomApi.explicit`."]
    #[doc = ""]
    #[doc = " Request: `POST http://127.0.0.1`."]
    #[doc =
    " Body (`value`): `String`, encoded with the format `custom::Codec` (written in place). `Content-Type`: the format's `Encode::CONTENT_TYPE`, sent with the body."]
    #[doc =
    " Output: `String`, decoded from a 2xx response with the format `custom::Codec` (written in place)."]
    #[doc = ""]
    #[doc = " Arguments:"]
    #[doc = " - `value`: the request body."]
    #[inline]
    #[allow(clippy :: too_many_arguments)]
    pub fn explicit(&self, value: impl ::concord::IntoValue<String>)
        -> CustomApiExplicit {
        CustomApiExplicit {
            __parent: ::core::clone::Clone::clone(self),
            value: ::concord::IntoValue::into_value(value),
        }
    }
}
impl ::core::fmt::Debug for CustomApi {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let __s = &*self.__s;
        let mut __d = f.debug_struct("CustomApi");
        __d.field("base", &__s.__base);
        __d.finish()
    }
}
#[doc = " `POST` endpoint `CustomApi.inherited`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct CustomApiInherited {
    __parent: CustomApi,
    value: String,
}
impl ::core::clone::Clone for CustomApiInherited where
    for<'__concord> String: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            value: ::core::clone::Clone::clone(&self.value),
        }
    }
}
impl CustomApiInherited {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_value(mut self, value: impl ::concord::IntoValue<String>)
        -> Self {
        self.value = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<String>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "CustomApi.inherited",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<String> =
                    ::concord::__private::BodyFormat::new::<__ConcordCustomApiRequestFormat>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.value, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::decoder::<__ConcordCustomApiResponseFormat,
                    String>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self)
        -> ::core::result::Result<String, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for CustomApiInherited {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("CustomApiInherited");
        __d.field("value", &"..");
        __d.finish()
    }
}
#[doc = " `POST` endpoint `CustomApi.explicit`."]
#[must_use =
"an endpoint request does nothing until `send` or `prepare` is called"]
pub struct CustomApiExplicit {
    __parent: CustomApi,
    value: String,
}
impl ::core::clone::Clone for CustomApiExplicit where
    for<'__concord> String: ::core::clone::Clone {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            __parent: ::core::clone::Clone::clone(&self.__parent),
            value: ::core::clone::Clone::clone(&self.value),
        }
    }
}
impl CustomApiExplicit {
    #[doc = ""]
    #[doc =
    " Replaces the request body supplied when this request was created."]
    #[inline]
    pub fn set_value(mut self, value: impl ::concord::IntoValue<String>)
        -> Self {
        self.value = ::concord::IntoValue::into_value(value);
        self
    }
    #[doc = r" Builds the native request without sending it."]
    #[inline]
    pub fn prepare(self)
        ->
            ::core::result::Result<::concord::Prepared<String>,
            ::concord::Error> {
        static __META: ::concord::__private::EndpointMeta =
            ::concord::__private::EndpointMeta {
                name: "CustomApi.explicit",
                method: "POST",
                labels: 0usize,
                sensitive_labels: &[],
                segments: 0usize,
                sensitive_segments: &[],
                sensitive_query: &[],
                sensitive_content_type: false,
            };
        let __body =
            {
                static __FORMAT: ::concord::__private::BodyFormat<String> =
                    ::concord::__private::BodyFormat::new::<custom::Codec>();
                ::core::option::Option::Some(::concord::__private::encode_body(&__FORMAT,
                            self.value, &__META)?)
            };
        let __o0 = &*self.__parent.__s;
        let mut __p = ::concord::__private::Parts::new(&__META, &__o0.__base);
        __p.finish(::concord::reqwest::Method::POST, &__o0.__http, __body,
            ::concord::__private::decoder::<custom::Codec, String>(&__META))
    }
    #[doc =
    r" Sends the request once (`prepare` then one `reqwest::Client::execute`) and"]
    #[doc =
    r" decodes a 2xx response. A non-2xx status is `ErrorKind::Status` holding the"]
    #[doc =
    r" unread response (`Error::into_response`); Concord never retries."]
    #[inline]
    pub async fn send(self)
        -> ::core::result::Result<String, ::concord::Error> {
        self.prepare()?.send().await
    }
}
impl ::core::fmt::Debug for CustomApiExplicit {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        let mut __d = f.debug_struct("CustomApiExplicit");
        __d.field("value", &"..");
        __d.finish()
    }
}
extern crate test;
#[rustc_test_marker =
"downstream_codec_encodes_and_decodes_foreign_payload_by_path"]
#[doc(hidden)]
pub static downstream_codec_encodes_and_decodes_foreign_payload_by_path:
    test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("downstream_codec_encodes_and_decodes_foreign_payload_by_path"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 379usize,
            start_col: 10usize,
            end_line: 379usize,
            end_col: 70usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(downstream_codec_encodes_and_decodes_foreign_payload_by_path())),
    };
fn downstream_codec_encodes_and_decodes_foreign_payload_by_path() {
    let body =
        async {
            for inherited in [false, true] {
                let (url, s) = server(200, b"custom:response");
                let api =
                    CustomApi::new(client()).with_base_url(&url).unwrap();
                let value =
                    if inherited {
                            api.inherited("request").send().await
                        } else { api.explicit("request").send().await }.unwrap();
                {
                    match (&value, &"response") {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(kind, &*left_val,
                                    &*right_val, ::core::option::Option::None);
                            }
                        }
                    }
                };
                let raw = s.join().unwrap();
                if !raw.ends_with(b"custom:request") {
                    ::core::panicking::panic("assertion failed: raw.ends_with(b\"custom:request\")")
                };
                {
                    match (&headers(&raw, "content-type"),
                            &[b"application/x-independent".to_vec()]) {
                        (left_val, right_val) => {
                            if !(*left_val == *right_val) {
                                let kind = ::core::panicking::AssertKind::Eq;
                                ::core::panicking::assert_failed(kind, &*left_val,
                                    &*right_val, ::core::option::Option::None);
                            }
                        }
                    }
                };
            }
        };
    let mut body = body;
    #[allow(unused_mut)]
    let mut body =
        unsafe { ::tokio::macros::support::Pin::new_unchecked(&mut body) };
    let body: ::core::pin::Pin<&mut dyn ::core::future::Future<Output = ()>> =
        body;

    #[allow(clippy :: expect_used, clippy :: diverging_sub_expression, clippy
    :: needless_return, clippy :: unwrap_in_result)]
    {
        use tokio::runtime::Builder;
        return Builder::new_current_thread().enable_all().build().expect("Failed building the Runtime").block_on(body);
    }
}
extern crate test;
#[rustc_test_marker = "dual_use_prepared_must_withhold_native_flag"]
#[doc(hidden)]
pub static dual_use_prepared_must_withhold_native_flag: test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("dual_use_prepared_must_withhold_native_flag"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "tests/contract.rs",
            start_line: 391usize,
            start_col: 4usize,
            end_line: 391usize,
            end_col: 47usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(dual_use_prepared_must_withhold_native_flag())),
    };
fn dual_use_prepared_must_withhold_native_flag() {
    let request = DualUse::new(client()).go(Dual("NATIVE-SECRET"));
    no_secret(&request);
    let p = request.prepare().unwrap();
    if !p.request().headers()["x-dual"].is_sensitive() {
        ::core::panicking::panic("assertion failed: p.request().headers()[\"x-dual\"].is_sensitive()")
    };
    { ::std::io::_print(format_args!("dual-use Prepared: {0:?}\n", p)); };
    if !!p.request().headers()["x-copy"].is_sensitive() {
        {
            ::core::panicking::panic_fmt(format_args!("record the missing flag on the formatted copy"));
        }
    };
    no_secret(&p);
}
extern crate test;
#[rustc_main]
#[coverage(off)]
#[doc(hidden)]
pub fn main() -> test::ExitCode {
    test::test_main_env_args(&[&composed_content_type_and_sensitive_insertions_reach_wire,
                    &declared_sensitive_dual_use_is_protected,
                    &downstream_codec_encodes_and_decodes_foreign_payload_by_path,
                    &drain_typed_unit_string_alias_status_and_native_decoder,
                    &dual_use_prepared_must_withhold_native_flag,
                    &form_input_json_output_and_form_decode,
                    &format_presence_aliases_independent_defaults_lexical_and_media,
                    &native_owners_clone_snapshots_flags_bytes_and_wire])
}
