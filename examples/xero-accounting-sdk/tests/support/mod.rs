#![allow(dead_code)]
use concord_sdk_xero_accounting::*;
use concord_testserver::TestServer;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};
pub const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
pub const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
pub const CONTACT: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
pub const INVOICE: &str = "11111111-1111-4111-8111-111111111111";
pub const DRAFT_ID: &str = "33333333-3333-4333-8333-333333333333";
pub const MODERN1: &str = include_str!("../../fixtures/page-modern-1.json");
pub const MODERN2: &str = include_str!("../../fixtures/page-modern-2.json");
pub const ITEM: &str = include_str!("../../fixtures/invoice.json");
pub const DRAFT: &str = include_str!("../../fixtures/draft.json");
pub const LEGACY: &str = include_str!("../../fixtures/page-legacy.json");
pub const EMPTY: &str = include_str!("../../fixtures/empty.json");
pub const ROTATED: &str = include_str!("../../fixtures/rotated-token.json");
pub const VALIDATION: &str = include_str!("../../fixtures/validation.json");
pub const BAD200: &str = include_str!("../../fixtures/validation-200.json");
pub const OAUTH_ERROR: &str = include_str!("../../fixtures/oauth-error.json");
pub const MALFORMED: &str = include_str!("../../fixtures/malformed.json");
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Temp(pub PathBuf);
impl Temp {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "concord-xero-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub fn path(&self) -> PathBuf {
        self.0.join("tokens")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
pub fn id(value: &str) -> TenantId {
    TenantId::new(value).unwrap()
}
pub fn grant(tenant: &str, expired: bool) -> Grant {
    grant_scopes(tenant, expired, "offline_access accounting.invoices")
}
pub fn grant_scopes(tenant: &str, expired: bool, scopes: &str) -> Grant {
    let expiry = SystemTime::now()
        + if expired {
            Duration::ZERO
        } else {
            Duration::from_secs(1800)
        };
    Grant::new(
        id(tenant),
        "synthetic-client",
        "synthetic-access-old",
        "synthetic-refresh-old",
        scopes,
        expiry,
    )
    .unwrap()
}
pub fn client(temp: &Temp, server: &TestServer, tenant: &str, expired: bool) -> (Xero, FileStore) {
    let store = FileStore::create(temp.path(), grant(tenant, expired)).unwrap();
    let api = Xero::with_origins(
        store.clone(),
        "synthetic-client-secret",
        &server.url(),
        &server.url(),
    )
    .unwrap();
    (api, store)
}
pub fn draft(tenant: &Tenant) -> DraftInvoice {
    tenant
        .draft(
            tenant.contact(CONTACT).unwrap(),
            vec![
                LineItem::new(
                    "Synthetic consulting",
                    1,
                    Amount::from_minor_units(1234).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap()
}
pub fn no_secrets(value: impl std::fmt::Debug) {
    let shown = format!("{value:?}");
    for secret in [
        "synthetic-access",
        "synthetic-refresh",
        "synthetic-client-secret",
        "synthetic-secret-in-body",
        A,
        B,
        INVOICE,
        CONTACT,
    ] {
        assert!(!shown.contains(secret), "leaked {secret} in {shown}");
    }
}
pub fn concord_kind(e: &Error) -> concord::ErrorKind {
    e.concord_error().unwrap().kind()
}
