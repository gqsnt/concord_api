#![allow(dead_code)]
use wire::{ToHeaderValue, reqwest::header::HeaderValue};
#[derive(Clone)]
pub struct Dual(pub &'static str);
impl ToHeaderValue for Dual {
    fn to_header_value(&self) -> Option<HeaderValue> {
        let mut h = HeaderValue::from_static(self.0);
        h.set_sensitive(true);
        Some(h)
    }
}
impl std::fmt::Display for Dual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.0) }
}
wire::api! {
    pub client Marked(#[sensitive] header("x-dual") v: Dual) {
        base "http://example.test";
        headers { "x-copy" = "{v}"; }
        scope child {
            subdomain "{client.v}";
            GET go() { path "{client.v}"; query { "q" = client.v; } }
        }
    }
}
wire::api! {
    pub client ByHeader(header("authorization") v: Dual) {
        base "http://example.test";
        headers { "x-copy" = "{v}"; }
        GET go() { path "{client.v}"; query { "q" = client.v; } }
    }
}
wire::api! {
    pub client ByQuery(v: Dual) {
        base "http://example.test";
        headers { "x-dual" = v; "x-copy" = "{v}"; }
        query { "token" = v; }
        GET go() { path "{client.v}"; }
    }
}
wire::api! {
    pub client Repeated(#[sensitive] header("x-dual") each v: Vec<Dual>) {
        base "http://example.test";
        GET go() { query { "q" = each client.v; } }
    }
}
wire::api! {
    pub client Composition {
        base "http://example.test";
        headers { "x-dual" = "base"; }
        GET replace(#[sensitive] v?: Dual) {
            query { "q" = v; }
            headers { override "x-dual" = v else inherit; "x-copy" = "{v}"; }
        }
        GET append(#[sensitive] v: Dual) {
            path "{v}";
            headers { append "x-dual" = v; "x-copy" = "{v}"; }
        }
        GET each_option(#[sensitive] v: Option<Dual>) {
            query { "q" = each v; }
            headers { append "x-dual" = each v; }
        }
    }
}
wire::api! {
    pub client Ordinary {
        base "http://example.test";
        GET go(s: String, n: u64, b: bool) {
            path "{s}/{n}/{b}";
            query { "s" = s; "n" = n; "b" = b; }
            headers { "x-s" = s; "x-n" = n; "x-b" = b; "x-copy" = "{s}-{n}-{b}"; }
        }
        GET wrappers(a: Box<String>, b: std::sync::Arc<String>, c: std::borrow::Cow<'static,str>, d: &'static str) {
            path "{a}/{b}/{c}/{d}";
            headers { "x-a" = a; "x-b" = b; "x-c" = c; "x-d" = d; }
        }
    }
}
wire::api! {
    pub client HeaderOnly {
        base "http://127.0.0.1";
        GET go(header("x-dual") v: Dual);
    }
}
wire::api! {
    pub client AcrossEndpoints(#[sensitive] v: Dual) {
        base "http://example.test";
        GET header() { headers { "x-dual" = client.v; } }
        GET text() { path "{client.v}"; query { "q" = client.v; } }
    }
}
wire::api! {
    pub client CfgClassified(v: Dual) {
        base "http://example.test";
        #[cfg(any())]
        GET disabled() { headers { "authorization" = client.v; } }
        GET enabled() { path "{client.v}"; headers { "x-dual" = client.v; "x-copy" = "{client.v}"; } }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn http() -> wire::reqwest::Client { wire::reqwest::Client::builder().no_proxy().build().unwrap() }
    fn hidden(v: &impl std::fmt::Debug) { let s = format!("{v:?}"); assert!(!s.to_ascii_lowercase().contains("s3cr3t"), "{s}"); }
    fn flagged(p:&wire::Prepared<()>, name:&str) { assert!(p.request().headers()[name].is_sensitive()); }
    #[test] fn declared_marker_redacts_config_client_scope_request_prepared() {
        let c=MarkedConfig { v:Dual("S3CR3T") }; hidden(&c);
        let c=Marked::new(http(),c); hidden(&c); let s=c.child(); hidden(&s); let r=s.go(); hidden(&r);
        let p=r.prepare().unwrap(); hidden(&p); flagged(&p,"x-dual"); flagged(&p,"x-copy");
        assert_eq!(p.request().url().as_str(),"http://s3cr3t.example.test/S3CR3T?q=S3CR3T");
        assert_eq!(p.request().headers()["x-copy"].as_bytes(),b"S3CR3T");
    }
    #[test] fn names_classify_all_uses() {
        let cfg=ByHeaderConfig {v:Dual("S3CR3T")}; hidden(&cfg);
        let c=ByHeader::new(http(),cfg); hidden(&c); let r=c.go(); hidden(&r); let p=r.prepare().unwrap(); hidden(&p); flagged(&p,"authorization"); flagged(&p,"x-copy");
        assert_eq!(p.request().url().query(),Some("q=S3CR3T"));
        let cfg=ByQueryConfig {v:Dual("S3CR3T")}; hidden(&cfg); let c=ByQuery::new(http(),cfg); hidden(&c);
        let r=c.go(); hidden(&r); let p=r.prepare().unwrap(); hidden(&p); flagged(&p,"x-dual"); flagged(&p,"x-copy");
        assert_eq!(p.request().url().query(),Some("token=S3CR3T"));
    }
    #[test] fn repeated_and_optional_composition() {
        let cfg=RepeatedConfig {v:vec![Dual("S3CR3T-a"),Dual("S3CR3T-b")]}; hidden(&cfg);
        let c=Repeated::new(http(),cfg); hidden(&c); let r=c.go(); hidden(&r); let p=r.prepare().unwrap(); hidden(&p);
        assert_eq!(p.request().headers().get_all("x-dual").iter().map(|h|h.as_bytes()).collect::<Vec<_>>(),vec![b"S3CR3T-a".as_slice(),b"S3CR3T-b".as_slice()]);
        assert!(p.request().headers().get_all("x-dual").iter().all(|h|h.is_sensitive()));
        assert_eq!(p.request().url().query(),Some("q=S3CR3T-a&q=S3CR3T-b"));
        let c=Composition::new(http());
        let r=c.replace().set_v(Dual("S3CR3T")); hidden(&r); let p=r.prepare().unwrap(); hidden(&p); flagged(&p,"x-dual"); flagged(&p,"x-copy");
        assert_eq!(p.request().headers()["x-dual"].as_bytes(),b"S3CR3T");
        let p=c.replace().prepare().unwrap(); hidden(&p); assert_eq!(p.request().headers()["x-dual"],"base"); assert!(!p.request().headers().contains_key("x-copy"));
        let r=c.append(Dual("S3CR3T")); hidden(&r); let p=r.prepare().unwrap(); hidden(&p);
        assert_eq!(p.request().headers().get_all("x-dual").iter().map(|h|h.as_bytes()).collect::<Vec<_>>(),vec![b"base".as_slice(),b"S3CR3T".as_slice()]);
        let r=c.each_option(Some(Dual("S3CR3T"))); hidden(&r); let p=r.prepare().unwrap(); hidden(&p); assert_eq!(p.request().url().query(),Some("q=S3CR3T"));
    }
    #[test] fn text_controls_keep_contents_and_flags() {
        let c=Ordinary::new(http()); let r=c.go("hello",7,true); assert!(format!("{r:?}").contains("hello"));
        let p=r.prepare().unwrap(); assert_eq!(p.request().url().as_str(),"http://example.test/hello/7/true?s=hello&n=7&b=true");
        for (k,v) in [("x-s","hello"),("x-n","7"),("x-b","true"),("x-copy","hello-7-true")] { assert_eq!(p.request().headers()[k],v); assert!(!p.request().headers()[k].is_sensitive()); }
        let s=format!("{p:?}"); for v in ["hello/7/true","x-copy: hello-7-true"] {assert!(s.contains(v),"{s}");}
        let p=c.wrappers(Box::new("a".into()),std::sync::Arc::new("b".into()),std::borrow::Cow::Borrowed("c"),"d").prepare().unwrap();
        assert_eq!(p.request().url().path(),"/a/b/c/d");
    }
    #[test] fn across_endpoints_and_cfg_classification() {
        let c=AcrossEndpoints::new(http(),AcrossEndpointsConfig{v:Dual("S3CR3T")}); hidden(&c);
        let h=c.header().prepare().unwrap(); hidden(&h); flagged(&h,"x-dual"); let t=c.text().prepare().unwrap(); hidden(&t); assert_eq!(t.request().url().path(),"/S3CR3T");
        let c=CfgClassified::new(http(),CfgClassifiedConfig{v:Dual("S3CR3T")}); hidden(&c); let p=c.enabled().prepare().unwrap(); hidden(&p); flagged(&p,"x-copy");
    }
    #[test] fn build_errors_omit_sensitive_value() {
        let c=Marked::new(http(),MarkedConfig{v:Dual("S3CR3T!")});
        let e=c.child().go().prepare().unwrap_err(); hidden(&e); assert!(!e.to_string().to_ascii_lowercase().contains("s3cr3t")); assert_eq!(e.kind(),wire::ErrorKind::Build);
    }
    // Each server accepts one request, closes, and is joined before returning. No background server remains.
    fn server()->(String,std::thread::JoinHandle<Vec<u8>>) {
        let l=std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.set_nonblocking(true).unwrap(); let url=format!("http://{}",l.local_addr().unwrap());
        let t=std::thread::spawn(move||{
            let start=std::time::Instant::now();
            let mut s=loop {match l.accept() {Ok((s,_))=>break s,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{assert!(start.elapsed()<std::time::Duration::from_secs(10));std::thread::sleep(std::time::Duration::from_millis(5));},Err(e)=>panic!("{e}")}};
            s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap(); let mut raw=Vec::new(); let mut buf=[0;1024];
            while !raw.windows(4).any(|w|w==b"\r\n\r\n") {let n=s.read(&mut buf).unwrap();assert!(n>0);raw.extend_from_slice(&buf[..n]);}
            s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 6\r\nConnection: close\r\n\r\ndenied").unwrap();raw
        });(url,t)
    }
    #[tokio::test] async fn native_sensitive_header_and_status_error() {
        let (url,t)=server(); let c=HeaderOnly::new(http()).with_base_url(&url).unwrap();
        let r=c.go(Dual("S3CR3T")); hidden(&r); let p=r.prepare().unwrap(); hidden(&p); flagged(&p,"x-dual");
        let result=p.send().await; let raw=t.join().unwrap(); assert!(raw.windows(6).any(|w|w==b"S3CR3T"));
        let e=result.unwrap_err(); hidden(&e); assert!(!e.to_string().to_ascii_lowercase().contains("s3cr3t")); assert_eq!(e.kind(),wire::ErrorKind::Status);
        assert_eq!(e.into_response().unwrap().text().await.unwrap(),"denied");
    }
    #[tokio::test] async fn dual_use_status_error_and_wire() {
        let (url,t)=server(); let c=ByHeader::new(http(),ByHeaderConfig{v:Dual("S3CR3T")}).with_base_url(&url).unwrap();
        hidden(&c); let r=c.go(); hidden(&r); let p=r.prepare().unwrap(); hidden(&p); flagged(&p,"authorization"); flagged(&p,"x-copy");
        let result=p.send().await; let raw=t.join().unwrap(); let text=String::from_utf8(raw).unwrap();
        for value in ["GET /S3CR3T?q=S3CR3T HTTP/1.1", "authorization: S3CR3T", "x-copy: S3CR3T"] {assert!(text.contains(value),"{text}");}
        let e=result.unwrap_err(); hidden(&e); assert!(!e.to_string().to_ascii_lowercase().contains("s3cr3t"));assert_eq!(e.kind(),wire::ErrorKind::Status);
    }
    #[tokio::test] async fn dual_use_transport_error() {
        let l=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let url=format!("http://{}",l.local_addr().unwrap());drop(l);
        let c=ByHeader::new(http(),ByHeaderConfig{v:Dual("S3CR3T")}).with_base_url(&url).unwrap();
        let p=c.go().prepare().unwrap(); hidden(&p); let e=p.send().await.unwrap_err(); hidden(&e); assert!(!e.to_string().to_ascii_lowercase().contains("s3cr3t"));assert_eq!(e.kind(),wire::ErrorKind::Transport);
    }
}
