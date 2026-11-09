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
