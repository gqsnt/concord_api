//! Construction facts: a required client or scope value is fixed unless declared `mut`; `base x;`
//! reads the base URL from a client value; getters read declared values. The forbidden moves are
//! compile-fail cases (`tests/ui/construction_fixed.rs`); these are the valid ones.

mod support;

use concord::{BaseUrl, reqwest};

/// The caller's instance: checked once, then the base of every request.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance(BaseUrl);

impl AsRef<BaseUrl> for Instance {
    fn as_ref(&self) -> &BaseUrl {
        &self.0
    }
}

concord::api! {
    pub client Org(
        instance: Instance,
        #[sensitive] header("authorization") mut token: String,
        version: u32,
        query("lang") lang?: String,
    ) {
        base instance;
        path "v{version}";
        scope account(id: u32, #[sensitive] header("x-account-key") mut key: String) {
            path "accounts/{id}";
            GET show();
        }
        GET ping();
    }
}

fn org(base: &str, token: &str) -> Org {
    Org::new(
        reqwest::Client::new(),
        OrgConfig {
            instance: Instance(BaseUrl::parse(base).unwrap()),
            token: token.into(),
            version: 60,
        },
    )
}

/// A fixture is an instance the caller builds; the base path is kept and declared paths follow.
#[tokio::test]
async fn the_base_is_the_instance_the_caller_constructs() {
    let server = support::Server::reply(support::Reply::new(204)).await;
    let api = org(&format!("{}/prefix", server.url()), "t-1");
    assert_eq!(api.base_url().path(), "/prefix");
    api.ping().send().await.unwrap();
    api.account(7, "k-1").show().send().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests[0].target, "/prefix/v60");
    assert_eq!(requests[1].target, "/prefix/v60/accounts/7");
    assert_eq!(requests[1].header("authorization"), Some("t-1"));
    assert_eq!(requests[1].header("x-account-key"), Some("k-1"));
}

/// `mut` values rotate on a new handle; earlier handles, scopes and requests keep their snapshot.
#[test]
fn mut_values_rotate_and_snapshots_stay() {
    let api = org("https://org.example.test", "old");
    let scope = api.account(1, "old-key");
    let earlier = scope.show();
    let rotated_api = api.clone().set_token("new".to_owned());
    let rotated_scope = scope.clone().set_key("new-key".to_owned());
    let header = |p: concord::Prepared<()>, name: &str| {
        p.request().headers()[name].to_str().unwrap().to_owned()
    };
    assert_eq!(header(earlier.prepare().unwrap(), "authorization"), "old");
    assert_eq!(
        header(api.ping().prepare().unwrap(), "authorization"),
        "old"
    );
    assert_eq!(
        header(rotated_api.ping().prepare().unwrap(), "authorization"),
        "new"
    );
    assert_eq!(
        header(rotated_scope.show().prepare().unwrap(), "x-account-key"),
        "new-key"
    );
    assert_eq!(
        header(scope.show().prepare().unwrap(), "x-account-key"),
        "old-key"
    );
    // Optional values keep `set_x`/`maybe_x`.
    let localized = api.set_lang("fr".to_owned()).ping().prepare().unwrap();
    assert_eq!(localized.request().url().query(), Some("lang=fr"));
}

/// Getters read the snapshot; a scope reaches its client; sensitive getters are crate-only (here,
/// the test crate is the declaring crate) and never printed.
#[test]
fn getters_read_declared_values() {
    let api = org("https://org.example.test", "secret-token");
    assert_eq!(
        api.instance(),
        &Instance(BaseUrl::parse("https://org.example.test").unwrap())
    );
    assert_eq!(*api.version(), 60);
    assert_eq!(api.lang(), None);
    assert_eq!(api.token(), "secret-token");
    let _: &reqwest::Client = api.http();
    let scope = api.account(9, "secret-key");
    assert_eq!(*scope.id(), 9);
    assert_eq!(scope.key(), "secret-key");
    assert_eq!(*scope.client().version(), 60);
    let shown = format!("{api:?} {scope:?}");
    assert!(shown.contains("org.example.test"), "{shown}");
    assert!(!shown.contains("secret"), "{shown}");
}

concord::api! {
    pub client SecretOrigin(#[sensitive] origin: BaseUrl) {
        base origin;
        scope nested { path "nested"; GET ping(); }
        GET ping();
    }
}

/// Sensitive base bytes reach the wire and stay withheld from every Concord rendering.
#[tokio::test]
async fn sensitive_base_canary() {
    const CANARY: &str = "CONSTRUCTION_BASE_CANARY_7f31";
    let server = support::Server::reply(support::Reply::new(400)).await;
    let config = SecretOriginConfig {
        origin: BaseUrl::parse(&format!("{}/{CANARY}", server.url())).unwrap(),
    };
    assert!(!format!("{config:?}").contains(CANARY));
    let api = SecretOrigin::new(reqwest::Client::new(), config);
    assert!(api.base_url().as_str().contains(CANARY));
    assert!(api.origin().url().as_str().contains(CANARY));
    let scope = api.nested();
    let endpoint = scope.ping();
    let prepared = endpoint.clone().prepare().unwrap();
    for shown in [
        format!("{api:?}"),
        format!("{scope:?}"),
        format!("{endpoint:?}"),
        format!("{prepared:?}"),
    ] {
        assert!(!shown.contains(CANARY), "{shown}");
    }
    assert!(format!("{api:?}").contains("<redacted>"));
    assert!(format!("{prepared:?}").contains("<redacted>"));
    let received = prepared.receive().await.unwrap();
    assert!(!format!("{received:?} {:?} {:?}", received.head(), received.meta()).contains(CANARY));
    let error = received.decode().await.unwrap_err();
    assert!(!format!("{error:?} {error}").contains(CANARY));
    assert_eq!(server.requests()[0].target, format!("/{CANARY}/nested"));
    let failed = SecretOrigin::new(
        reqwest::Client::new(),
        SecretOriginConfig {
            origin: BaseUrl::parse(&format!("http://{}/{CANARY}", support::refused_addr()))
                .unwrap(),
        },
    )
    .ping()
    .prepare()
    .unwrap()
    .send()
    .await
    .unwrap_err();
    assert!(!format!("{failed:?} {failed}").contains(CANARY));
    use std::error::Error as _;
    if let Some(source) = failed.source() {
        assert!(!format!("{source:?} {source}").contains(CANARY));
    }
}

/// Native flags and native-key conditions withhold getters across the crate boundary (UI case).
#[test]
fn native_sensitive_values_keep_their_wire_flags() {
    use concord::reqwest::header::HeaderValue;
    use concord_construction_fixture::{Native, NativeConfig};
    let mut owned = HeaderValue::from_static("NATIVE_GETTER_CANARY_81ab");
    owned.set_sensitive(true);
    let config = NativeConfig {
        owned,
        conditional: "CONDITIONAL_CANARY_21fe".into(),
    };
    let api = Native::new(reqwest::Client::new(), config);
    let prepared = api.ping().prepare().unwrap();
    for name in ["x-owned", "authorization"] {
        assert!(prepared.request().headers()[name].is_sensitive());
    }
    let shown = format!("{api:?} {prepared:?}");
    assert!(!shown.contains("CANARY"), "{shown}");
}
