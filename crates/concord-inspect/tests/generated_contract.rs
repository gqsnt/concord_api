//! `concord.contract/3` export, check and diff over descriptions produced by
//! the real `concord::api!` macro (V3-O02, V3-O03).
//!
//! The crate's unit tests exercise a hand-built Catalog-like corpus; this
//! target proves the same guarantees hold for what generated
//! `Client::contract()` actually emits. Every variant below is one complete
//! `api!` declaration: the baseline plus exactly one edit, so each diff is
//! attributable to that edit alone.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::inspection::ContractDescription;
use concord::response::{Buffered, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord_inspect::{
    Axis, DiffReport, Verdict, check, diff_documents, export_string, export_value,
    to_canonical_string,
};
use serde::{Deserialize, Serialize};

// ----- shared Rust items the declarations reference ---------------------------

/// Default page size.
pub const PAGE: u32 = 25;
/// Alternative default page size (an opaque constant edit).
pub const PAGE_LARGE: u32 = 100;

#[derive(Deserialize)]
pub struct Stats {
    pub count: u64,
}

#[derive(Serialize)]
pub struct Update {
    pub title: String,
}

pub struct Label(pub String);

static CONVERTER_CALLS: AtomicUsize = AtomicUsize::new(0);

/// A vendor representation with static negotiation metadata. Its converter
/// functions count calls: export must never invoke them.
pub struct VendorJson;

impl ResponseRepresentation<Label> for VendorJson {
    type Driver = Buffered;
    const ACCEPT: Option<&'static str> = Some("application/vnd.shop+json; version=3");

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        CONVERTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body(
        body: concord::Bytes,
        _head: &ResponseHead<'_>,
    ) -> Result<Label, ResponseBodyError> {
        CONVERTER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Label(String::from_utf8_lossy(&body).into_owned()))
    }
}

// ----- the declaration template -------------------------------------------------

/// Expands one `Shop` declaration in module `$module`. Each slot has a
/// baseline default; a variant overrides whole slots by name
/// (`shop!(name, slot = { ... })`), so the edit is visible at the call site.
macro_rules! shop {
    ($module:ident $(, $slot:ident = { $($tokens:tt)* })* $(,)?) => {
        shop!(@munch $module
            [values = {
                header("X-Channel") channel: String;
                #[sensitive] header("X-Tenant") tenant: String;
                query page_size: u32 = crate::PAGE;
                region: String = "eu";
                market: String = "eu";
            }]
            [credentials = {
                secret primary_key;
                secret partner_secret;
                credential primary = api_key(secret.primary_key);
                credential partner = api_key(secret.partner_secret);
            }]
            [auth = { header "X-Key" = primary; }]
            [headers = {
                "X-Client" = "3";
                "X-Region" = region;
                "X-Market" = market;
            }]
            [rate_limits = {
                policy reads(tenant) {
                    id "shop.reads";
                    key [origin, "tenant" = tenant];
                    allow 10 per 1s;
                    allow 100 per 1m;
                }
            }]
            [admin = {
                rate_limit reads(tenant = client.tenant);

                GET stats() -> Stats {
                    id "shop.admin.stats";
                    path "stats";
                }

                GET audit() -> Stats {
                    id "shop.admin.audit";
                    path "audit";
                }
            }]
            [endpoints = {
                GET item(id: String) -> Label via VendorJson {
                    id "shop.item";
                    path "items/{id}";
                }

                PUT update(id: String, body input: Update) -> Stats via Json {
                    id "shop.update";
                    path "items/{id}";
                    auth replace { header "X-Key" = partner; }
                    effect idempotent;
                    rate_limit {
                        id "shop.write";
                        key [origin, endpoint, "tenant" = client.tenant];
                        cost 2;
                        allow 5 per 1s;
                    }
                }

                GET ping() -> Stats {
                    path "ping";
                }
            }]
            $($slot = { $($tokens)* })*
        );
    };
    (@munch $m:ident [values = $_:tt] $c:tt $a:tt $h:tt $r:tt $s:tt $e:tt values = $new:tt $($rest:tt)*) => {
        shop!(@munch $m [values = $new] $c $a $h $r $s $e $($rest)*);
    };
    (@munch $m:ident $v:tt [credentials = $_:tt] $a:tt $h:tt $r:tt $s:tt $e:tt credentials = $new:tt $($rest:tt)*) => {
        shop!(@munch $m $v [credentials = $new] $a $h $r $s $e $($rest)*);
    };
    (@munch $m:ident $v:tt $c:tt [auth = $_:tt] $h:tt $r:tt $s:tt $e:tt auth = $new:tt $($rest:tt)*) => {
        shop!(@munch $m $v $c [auth = $new] $h $r $s $e $($rest)*);
    };
    (@munch $m:ident $v:tt $c:tt $a:tt [headers = $_:tt] $r:tt $s:tt $e:tt headers = $new:tt $($rest:tt)*) => {
        shop!(@munch $m $v $c $a [headers = $new] $r $s $e $($rest)*);
    };
    (@munch $m:ident $v:tt $c:tt $a:tt $h:tt [rate_limits = $_:tt] $s:tt $e:tt rate_limits = $new:tt $($rest:tt)*) => {
        shop!(@munch $m $v $c $a $h [rate_limits = $new] $s $e $($rest)*);
    };
    (@munch $m:ident $v:tt $c:tt $a:tt $h:tt $r:tt [admin = $_:tt] $e:tt admin = $new:tt $($rest:tt)*) => {
        shop!(@munch $m $v $c $a $h $r [admin = $new] $e $($rest)*);
    };
    (@munch $m:ident $v:tt $c:tt $a:tt $h:tt $r:tt $s:tt [endpoints = $_:tt] endpoints = $new:tt $($rest:tt)*) => {
        shop!(@munch $m $v $c $a $h $r $s [endpoints = $new] $($rest)*);
    };
    (@munch $m:ident
        [values = { $($values:tt)* }]
        [credentials = { $($credentials:tt)* }]
        [auth = { $($auth:tt)* }]
        [headers = { $($headers:tt)* }]
        [rate_limits = { $($rate_limits:tt)* }]
        [admin = { $($admin:tt)* }]
        [endpoints = { $($endpoints:tt)* }]
    ) => {
        #[allow(dead_code)]
        pub mod $m {
            use super::*;
            use concord::Json;

            concord::api! {
                /// One `Shop` contract variant.
                client Shop {
                    base "https://shop.example.test/";
                    timeout 10s;
                    response Json;
                    request Json;

                    values { $($values)* }
                    credentials { $($credentials)* }
                    auth { $($auth)* }
                    headers { $($headers)* }
                    rate_limits { $($rate_limits)* }

                    scope admin(zone: String) {
                        path "admin/{zone}";
                        $($admin)*
                    }

                    $($endpoints)*
                }
            }
        }
    };
}

shop!(baseline);
// A second, independently expanded copy of the baseline.
shop!(baseline_copy);

// Credential selection: the client placement names another credential at the
// same header.
shop!(switch_credential, auth = { header "X-Key" = partner; });

// Credential source: the credentials read swapped secrets.
shop!(
    credential_source,
    credentials = {
        secret primary_key;
        secret partner_secret;
        credential primary = api_key(secret.partner_secret);
        credential partner = api_key(secret.primary_key);
    }
);

// Literal: one header literal changes; its key does not.
shop!(
    change_literal,
    headers = {
        "X-Client" = "4";
        "X-Region" = region;
        "X-Market" = market;
    }
);

// Value source: the same headers now read each other's value.
shop!(
    remap_value,
    headers = {
        "X-Client" = "3";
        "X-Region" = market;
        "X-Market" = region;
    }
);

// Default: the initializer names another (opaque) constant.
shop!(
    change_default,
    values = {
        header("X-Channel") channel: String;
        #[sensitive] header("X-Tenant") tenant: String;
        query page_size: u32 = crate::PAGE_LARGE;
        region: String = "eu";
        market: String = "eu";
    }
);

// Default: a required value becomes initialized.
shop!(
    add_default,
    values = {
        header("X-Channel") channel: String = "web";
        #[sensitive] header("X-Tenant") tenant: String;
        query page_size: u32 = crate::PAGE;
        region: String = "eu";
        market: String = "eu";
    }
);

// Fence: the endpoint no longer re-selects the inherited representation.
shop!(
    remove_representation_fence,
    endpoints = {
        GET item(id: String) -> Label via VendorJson {
            id "shop.item";
            path "items/{id}";
        }

        PUT update(id: String, body input: Update) -> Stats {
            id "shop.update";
            path "items/{id}";
            auth replace { header "X-Key" = partner; }
            effect idempotent;
            rate_limit {
                id "shop.write";
                key [origin, endpoint, "tenant" = client.tenant];
                cost 2;
                allow 5 per 1s;
            }
        }

        GET ping() -> Stats {
            path "ping";
        }
    }
);

// Label: the named policy's key label is renamed (bytes enter rate state).
shop!(
    rename_label,
    rate_limits = {
        policy reads(tenant) {
            id "shop.reads";
            key [origin, "tenant_id" = tenant];
            allow 10 per 1s;
            allow 100 per 1m;
        }
    }
);

// Cost: only the inline write policy's cost changes.
shop!(
    change_cost,
    endpoints = {
        GET item(id: String) -> Label via VendorJson {
            id "shop.item";
            path "items/{id}";
        }

        PUT update(id: String, body input: Update) -> Stats via Json {
            id "shop.update";
            path "items/{id}";
            auth replace { header "X-Key" = partner; }
            effect idempotent;
            rate_limit {
                id "shop.write";
                key [origin, endpoint, "tenant" = client.tenant];
                cost 3;
                allow 5 per 1s;
            }
        }

        GET ping() -> Stats {
            path "ping";
        }
    }
);

// Sharing: `audit` leaves the scope-shared bucket for its own policy.
shop!(
    split_sharing,
    admin = {
        GET stats() -> Stats {
            id "shop.admin.stats";
            path "stats";
            rate_limit reads(tenant = client.tenant);
        }

        GET audit() -> Stats {
            id "shop.admin.audit";
            path "audit";
            rate_limit {
                id "shop.audit";
                key [origin, "tenant" = client.tenant];
                allow 10 per 1s;
                allow 100 per 1m;
            }
        }
    }
);

// Location: `ping` moves into the scope, so its derived ID changes.
shop!(
    move_endpoint,
    admin = {
        rate_limit reads(tenant = client.tenant);

        GET stats() -> Stats {
            id "shop.admin.stats";
            path "stats";
        }

        GET audit() -> Stats {
            id "shop.admin.audit";
            path "audit";
        }

        GET ping() -> Stats {
            path "ping";
        }
    },
    endpoints = {
        GET item(id: String) -> Label via VendorJson {
            id "shop.item";
            path "items/{id}";
        }

        PUT update(id: String, body input: Update) -> Stats via Json {
            id "shop.update";
            path "items/{id}";
            auth replace { header "X-Key" = partner; }
            effect idempotent;
            rate_limit {
                id "shop.write";
                key [origin, endpoint, "tenant" = client.tenant];
                cost 2;
                allow 5 per 1s;
            }
        }
    }
);

// Redacted literals: a sensitive initialized value with two hidden spellings.
macro_rules! sensitive_variant {
    ($module:ident, $hidden:literal) => {
        shop!(
            $module,
            values = {
                header("X-Channel") channel: String;
                #[sensitive] header("X-Tenant") tenant: String;
                query page_size: u32 = crate::PAGE;
                region: String = "eu";
                market: String = "eu";
                #[sensitive] header("X-Audit") audit: String = $hidden;
            }
        );
    };
}
sensitive_variant!(sensitive_a, "hidden-sentinel-a");
sensitive_variant!(sensitive_b, "hidden-sentinel-b");

// Window: the shared `reads` policy narrows 100/1m to 120/1m (RATE-02).
shop!(
    change_window,
    rate_limits = {
        policy reads(tenant) {
            id "shop.reads";
            key [origin, "tenant" = tenant];
            allow 10 per 1s;
            allow 120 per 1m;
        }
    }
);

// Author docs only: `item` gains a `///` description (EXPLAIN-02).
shop!(
    document_endpoint,
    endpoints = {
        /// Returns one item label in the vendor representation.
        GET item(id: String) -> Label via VendorJson {
            id "shop.item";
            path "items/{id}";
        }

        PUT update(id: String, body input: Update) -> Stats via Json {
            id "shop.update";
            path "items/{id}";
            auth replace { header "X-Key" = partner; }
            effect idempotent;
            rate_limit {
                id "shop.write";
                key [origin, endpoint, "tenant" = client.tenant];
                cost 2;
                allow 5 per 1s;
            }
        }

        GET ping() -> Stats {
            path "ping";
        }
    }
);

// Deprecation only: `ping` gains a deprecation note (EXPLAIN-02).
shop!(
    deprecate_endpoint,
    endpoints = {
        GET item(id: String) -> Label via VendorJson {
            id "shop.item";
            path "items/{id}";
        }

        PUT update(id: String, body input: Update) -> Stats via Json {
            id "shop.update";
            path "items/{id}";
            auth replace { header "X-Key" = partner; }
            effect idempotent;
            rate_limit {
                id "shop.write";
                key [origin, endpoint, "tenant" = client.tenant];
                cost 2;
                allow 5 per 1s;
            }
        }

        #[deprecated(note = "poll the stats endpoint instead")]
        GET ping() -> Stats {
            path "ping";
        }
    }
);

// Type path spelling only: `ping` names its result `crate::Stats` (the same
// Rust type), which a structural export must not normalize away (EXPLAIN-02).
shop!(
    qualify_result_path,
    endpoints = {
        GET item(id: String) -> Label via VendorJson {
            id "shop.item";
            path "items/{id}";
        }

        PUT update(id: String, body input: Update) -> Stats via Json {
            id "shop.update";
            path "items/{id}";
            auth replace { header "X-Key" = partner; }
            effect idempotent;
            rate_limit {
                id "shop.write";
                key [origin, endpoint, "tenant" = client.tenant];
                cost 2;
                allow 5 per 1s;
            }
        }

        GET ping() -> crate::Stats {
            path "ping";
        }
    }
);

// Sharing merge: `audit` rejoins `stats` on the named `reads` policy with
// endpoint-local attachments; against `split_sharing` (whose `audit` had an
// equal inline definition) this is a pure merge of equal definitions.
shop!(
    merge_sharing,
    admin = {
        GET stats() -> Stats {
            id "shop.admin.stats";
            path "stats";
            rate_limit reads(tenant = client.tenant);
        }

        GET audit() -> Stats {
            id "shop.admin.audit";
            path "audit";
            rate_limit reads(tenant = client.tenant);
        }
    }
);

// Liveness (VALUE-06): an optional value that is sent, and the same value as
// a `#[deprecated]` no-op migration bridge that is no longer sent.
shop!(
    legacy_live,
    values = {
        header("X-Channel") channel: String;
        #[sensitive] header("X-Tenant") tenant: String;
        query page_size: u32 = crate::PAGE;
        region: String = "eu";
        market: String = "eu";
        header("X-Legacy") legacy?: String;
    }
);
shop!(
    legacy_no_op,
    values = {
        header("X-Channel") channel: String;
        #[sensitive] header("X-Tenant") tenant: String;
        query page_size: u32 = crate::PAGE;
        region: String = "eu";
        market: String = "eu";
        #[deprecated(note = "the provider ignores this header")]
        legacy?: String;
    }
);

// ----- helpers ------------------------------------------------------------------

fn diff(before: &ContractDescription, after: &ContractDescription) -> DiffReport {
    diff_documents(&export_value(before), &export_value(after)).expect("valid documents")
}

fn endpoint_ids(report: &DiffReport) -> BTreeSet<String> {
    report
        .changed_endpoints()
        .iter()
        .map(|endpoint| endpoint.id().to_owned())
        .collect()
}

fn located(report: &DiffReport, axis: Axis) -> Vec<String> {
    report
        .changes_on(axis)
        .iter()
        .map(|change| {
            format!(
                "{}:{}:{}",
                change.collection(),
                change.record(),
                change.field()
            )
        })
        .collect()
}

// ----- tests --------------------------------------------------------------------

/// The xtask registry stores providers as `fn() -> &'static
/// ContractDescription`; generated `contract` has exactly that shape.
#[test]
fn generated_contract_is_a_registry_provider() {
    let provider: fn() -> &'static ContractDescription = baseline::Shop::contract;
    assert!(std::ptr::eq(provider(), baseline::Shop::contract()));
    assert_eq!(baseline::Shop::contract().endpoints().len(), 5);
}

/// V3-O03: export is deterministic (byte-identical across calls and across
/// two independent expansions), canonical (sorted keys) and versioned.
#[test]
fn generated_export_is_deterministic_and_versioned() {
    let first = export_string(baseline::Shop::contract());
    assert_eq!(first, export_string(baseline::Shop::contract()));
    assert_eq!(first, export_string(baseline_copy::Shop::contract()));
    let document = export_value(baseline::Shop::contract());
    assert_eq!(document["format"], concord_inspect::CONTRACT_FORMAT);
    assert_eq!(to_canonical_string(&document), first);
    let ids = document["endpoints"]
        .as_array()
        .expect("endpoints")
        .iter()
        .map(|endpoint| endpoint["id"]["value"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    // Declaration order is preserved and every endpoint is exported.
    let declared = baseline::Shop::contract()
        .endpoints()
        .iter()
        .map(|endpoint| endpoint.id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids, declared);
    assert_eq!(
        ids.iter().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(
            [
                "shop.item",
                "shop.update",
                "shop.ping",
                "shop.admin.stats",
                "shop.admin.audit"
            ]
            .map(str::to_owned)
        )
    );
}

/// V3-O02: the static ACCEPT of a user representation is exported from its
/// associated constant; no converter function runs during export.
#[test]
fn generated_static_accept_is_exported_without_converter_calls() {
    let document = export_value(baseline::Shop::contract());
    let item = document["endpoints"]
        .as_array()
        .expect("endpoints")
        .iter()
        .find(|endpoint| endpoint["id"]["value"] == "shop.item")
        .expect("item endpoint");
    assert_eq!(
        item["result"]["accept"],
        "application/vnd.shop+json; version=3"
    );
    assert_eq!(item["result"]["driver"], "buffered");
    assert_eq!(CONVERTER_CALLS.load(Ordering::SeqCst), 0);
}

/// V3-O03: sensitive literals are redacted in the export (never the hidden
/// text), two hidden spellings export identically, and equal-looking
/// redacted literals are unresolved rather than equal. Ordinary literals
/// stay visible.
#[test]
fn generated_redacted_literals_never_compare_equal() {
    let a = export_string(sensitive_a::Shop::contract());
    let b = export_string(sensitive_b::Shop::contract());
    assert!(!a.contains("hidden-sentinel"), "export is value-free");
    assert_eq!(a, b, "exports never carry hidden values");
    assert!(a.contains("\"redacted\": true"), "{a}");
    assert!(a.contains("\"text\": \"3\""), "ordinary literals remain");

    for (before, after) in [
        (sensitive_a::Shop::contract(), sensitive_a::Shop::contract()),
        (sensitive_a::Shop::contract(), sensitive_b::Shop::contract()),
    ] {
        let report = diff(before, after);
        assert_eq!(report.verdict(), Verdict::StructurallyEqualWithUnresolved);
        assert!(
            report.changes().is_empty(),
            "no represented structural change"
        );
        assert!(
            !report.unresolved().is_empty(),
            "equality is never asserted"
        );
        assert!(
            report
                .unresolved()
                .iter()
                .any(|unresolved| unresolved.location() == "Shop#values.audit.initializer"),
            "{:?}",
            report
                .unresolved()
                .iter()
                .map(|u| u.location())
                .collect::<Vec<_>>()
        );
        assert!(
            report
                .unresolved()
                .iter()
                .all(|u| u.axis() == Axis::OpaqueDependency)
        );
        let json = report.to_json_string();
        assert!(json.contains("structurally-equal-with-unresolved"));
        assert!(!json.contains("hidden-sentinel"));
    }
}

/// V3-O02 / V3-O03: `check` against a snapshot of the generated baseline is
/// equal for the same declaration and reports a difference, with the
/// affected endpoint and axis, for an edited one.
#[test]
fn generated_check_detects_a_changed_declaration() {
    let snapshot = export_string(baseline::Shop::contract());
    let same = check(baseline_copy::Shop::contract(), &snapshot).expect("check");
    assert_eq!(same.verdict(), Verdict::StructurallyEqual);
    assert!(same.changes().is_empty() && same.unresolved().is_empty());

    let changed = check(change_cost::Shop::contract(), &snapshot).expect("check");
    assert_eq!(changed.verdict(), Verdict::Different);
    assert!(!changed.is_structurally_equal());
    assert_eq!(
        endpoint_ids(&changed),
        BTreeSet::from(["shop.update".to_owned()])
    );
    let text = changed.to_text();
    assert!(text.contains("shop.update"), "{text}");
    assert!(text.contains("RuntimePolicy"), "{text}");
}

/// V3-O02: every single-edit variant is detected, classified on the
/// required axes and not on the forbidden ones, never as `Unknown`, and
/// names the affected endpoints.
#[test]
fn generated_mutation_corpus_detects_and_classifies_every_edit() {
    use Axis::*;
    /// (variant, contract, required axes, forbidden axes, endpoints that must be named)
    type Case = (
        &'static str,
        &'static ContractDescription,
        &'static [Axis],
        &'static [Axis],
        &'static [&'static str],
    );
    let every_client_endpoint: &[&str] = &[
        "shop.item",
        "shop.update",
        "shop.ping",
        "shop.admin.stats",
        "shop.admin.audit",
    ];
    let cases: [Case; 15] = [
        (
            "switch_credential",
            switch_credential::Shop::contract(),
            &[AuthAuthority],
            &[Wire],
            // `update` replaces client auth and is unaffected.
            &[
                "shop.item",
                "shop.ping",
                "shop.admin.stats",
                "shop.admin.audit",
            ],
        ),
        (
            "credential_source",
            credential_source::Shop::contract(),
            &[AuthAuthority],
            &[Wire],
            &[],
        ),
        (
            "change_literal",
            change_literal::Shop::contract(),
            &[Wire],
            &[IdentityAndRateSchema, AuthAuthority],
            every_client_endpoint,
        ),
        (
            "remap_value",
            remap_value::Shop::contract(),
            &[Wire],
            &[AuthAuthority, IdentityAndRateSchema],
            every_client_endpoint,
        ),
        (
            "change_default",
            change_default::Shop::contract(),
            &[PresenceAndDefaults, OpaqueDependency],
            &[Wire],
            &[],
        ),
        (
            "add_default",
            add_default::Shop::contract(),
            &[PresenceAndDefaults, CallerApi],
            &[Wire],
            &[],
        ),
        (
            "remove_representation_fence",
            remove_representation_fence::Shop::contract(),
            &[Provenance],
            &[Wire, CallerApi],
            &["shop.update"],
        ),
        (
            "rename_label",
            rename_label::Shop::contract(),
            &[IdentityAndRateSchema],
            &[Wire],
            &["shop.admin.stats", "shop.admin.audit"],
        ),
        (
            "change_cost",
            change_cost::Shop::contract(),
            &[RuntimePolicy],
            &[IdentityAndRateSchema, SharingCoverage, Wire],
            &["shop.update"],
        ),
        (
            "split_sharing",
            split_sharing::Shop::contract(),
            &[SharingCoverage, IdentityAndRateSchema],
            &[Wire],
            &["shop.admin.audit", "shop.admin.stats"],
        ),
        (
            "move_endpoint",
            move_endpoint::Shop::contract(),
            &[CallerApi, IdentityAndRateSchema, Provenance],
            &[],
            &["shop.ping"],
        ),
        // RATE-02: a window edit is runtime policy and rate schema, never a
        // sharing/coverage or wire change.
        (
            "change_window",
            change_window::Shop::contract(),
            &[RuntimePolicy, IdentityAndRateSchema],
            &[SharingCoverage, Wire],
            &["shop.admin.stats", "shop.admin.audit"],
        ),
        // EXPLAIN-02: author docs, deprecation notes and type-path spelling
        // are never normalized away.
        (
            "document_endpoint",
            document_endpoint::Shop::contract(),
            &[Documentation],
            &[
                Wire,
                CallerApi,
                IdentityAndRateSchema,
                SharingCoverage,
                RuntimePolicy,
            ],
            &["shop.item"],
        ),
        (
            "deprecate_endpoint",
            deprecate_endpoint::Shop::contract(),
            &[Documentation, CallerApi],
            &[Wire, IdentityAndRateSchema, SharingCoverage, RuntimePolicy],
            &["shop.ping"],
        ),
        (
            "qualify_result_path",
            qualify_result_path::Shop::contract(),
            &[CallerApi, OpaqueDependency],
            &[Wire, IdentityAndRateSchema, SharingCoverage],
            &["shop.ping"],
        ),
    ];
    let base = baseline::Shop::contract();
    for (name, contract, required, forbidden, endpoints) in cases {
        let report = diff(base, contract);
        assert_eq!(
            report.verdict(),
            Verdict::Different,
            "{name} was not detected"
        );
        let axes = report.axes();
        for axis in required {
            assert!(
                axes.contains(axis),
                "{name} missing axis {axis:?}; got {axes:?}\n{}",
                report.to_text()
            );
        }
        for axis in forbidden {
            let wrong = located(&report, *axis);
            assert!(
                wrong.is_empty(),
                "{name} wrongly classified {axis:?}: {wrong:?}"
            );
        }
        assert!(
            !axes.contains(&Unknown),
            "{name} produced an unclassified change\n{}",
            report.to_text()
        );
        let ids = endpoint_ids(&report);
        for endpoint in endpoints {
            assert!(
                ids.contains(*endpoint),
                "{name} did not name {endpoint}; got {ids:?}"
            );
        }
        for change in report.changes() {
            assert!(!change.axes().is_empty(), "{name}: change without an axis");
        }
        // Export stays value-free for every variant.
        assert!(!export_string(contract).contains("hidden-sentinel"));
    }
}

/// EXPLAIN-02 (§18.2): an edit of author docs or of a deprecation note alone
/// is a represented change on the Documentation axis (plus CallerApi for the
/// deprecation that reaches callers), never "structurally equal", and the
/// exported contract carries the exact text.
#[test]
fn generated_doc_only_edits_are_never_normalized_away() {
    let documented = diff(
        baseline::Shop::contract(),
        document_endpoint::Shop::contract(),
    );
    assert_eq!(documented.verdict(), Verdict::Different);
    assert_eq!(
        documented.axes().into_iter().collect::<Vec<_>>(),
        [Axis::Documentation]
    );
    assert_eq!(
        located(&documented, Axis::Documentation),
        ["owners:Shop::item:docs"]
    );
    let change = &documented.changes()[0];
    assert_eq!(change.before(), Some(&serde_json::json!([])));
    assert_eq!(
        change.after(),
        Some(&serde_json::json!([
            "Returns one item label in the vendor representation."
        ]))
    );
    assert!(
        export_string(document_endpoint::Shop::contract())
            .contains("Returns one item label in the vendor representation.")
    );
    assert!(
        check(
            document_endpoint::Shop::contract(),
            &export_string(baseline::Shop::contract())
        )
        .expect("check")
        .verdict()
            == Verdict::Different
    );

    let deprecated = diff(
        baseline::Shop::contract(),
        deprecate_endpoint::Shop::contract(),
    );
    assert_eq!(deprecated.verdict(), Verdict::Different);
    assert_eq!(
        deprecated.axes().into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from([Axis::Documentation, Axis::CallerApi])
    );
    assert_eq!(
        located(&deprecated, Axis::Documentation),
        ["owners:Shop::ping:deprecation"]
    );
    assert_eq!(
        deprecated.changes()[0].after(),
        Some(&serde_json::json!({
            "since": null,
            "note": "poll the stats endpoint instead"
        }))
    );
    assert_eq!(
        endpoint_ids(&deprecated),
        BTreeSet::from(["shop.ping".to_owned()])
    );
}

/// EXPLAIN-02 / RATE-02 (generated merge): `audit` leaves its equal inline
/// definition and joins `stats` on the named policy. The diff removes the
/// `shop.audit` policy, adds `audit` to `shop.reads` coverage and to its
/// sharing relation, and changes no wire fact.
#[test]
fn generated_merge_of_equal_definitions_joins_the_shared_bucket() {
    let report = diff(
        split_sharing::Shop::contract(),
        merge_sharing::Shop::contract(),
    );
    assert_eq!(report.verdict(), Verdict::Different);
    assert!(report.has_axis(Axis::SharingCoverage));
    assert!(report.has_axis(Axis::IdentityAndRateSchema));
    assert!(
        located(&report, Axis::Wire).is_empty(),
        "a merge is not a wire change"
    );
    assert!(!report.has_axis(Axis::Unknown));
    let removed = report
        .changes()
        .iter()
        .find(|change| change.collection() == "policies" && change.record() == "shop.audit")
        .expect("the inline definition disappears");
    assert!(removed.field().is_empty() && removed.after().is_none());
    assert!(removed.axes().contains(&Axis::SharingCoverage));
    assert!(report.changes().iter().any(|change| {
        change.record() == "shop.reads" && change.field() == "coverage[shop.admin.audit]"
    }));
    let sharing = report
        .changes()
        .iter()
        .find(|change| change.record() == "shop.reads" && change.field() == "sharing[0].endpoints")
        .expect("the sharing relation grows");
    assert_eq!(sharing.axes(), [Axis::SharingCoverage]);
    assert_eq!(
        sharing.after(),
        Some(&serde_json::json!([
            { "id": "shop.admin.stats", "path": "Shop::admin::stats" },
            { "id": "shop.admin.audit", "path": "Shop::admin::audit" }
        ]))
    );
    assert_eq!(
        endpoint_ids(&report),
        BTreeSet::from(["shop.admin.audit".to_owned(), "shop.admin.stats".to_owned()])
    );
}

/// VALUE-06 (§4.6): the semantic diff discloses a value that stops being sent
/// and becomes a `#[deprecated]` no-op migration bridge: the export marks its
/// liveness `deprecated_no_op` (and `live` before), and the diff reports the
/// liveness and deprecation changes on the owner's value next to the wire
/// removal and the retained-bridge diagnostic.
#[test]
fn generated_live_to_deprecated_no_op_value_is_disclosed() {
    fn legacy(document: &serde_json::Value) -> serde_json::Value {
        document["owners"]
            .as_array()
            .expect("owners")
            .iter()
            .find(|owner| owner["path"] == "Shop")
            .expect("client owner")["values"]
            .as_array()
            .expect("values")
            .iter()
            .find(|value| value["name"] == "legacy")
            .expect("legacy value")
            .clone()
    }
    let live = legacy(&export_value(legacy_live::Shop::contract()));
    let no_op = legacy(&export_value(legacy_no_op::Shop::contract()));
    assert_eq!(live["liveness"], "live");
    assert_eq!(live["deprecation"], serde_json::Value::Null);
    assert_eq!(no_op["liveness"], "deprecated_no_op");
    assert_eq!(
        no_op["deprecation"]["note"],
        "the provider ignores this header"
    );
    assert_eq!(no_op["presence"], "optional");

    let report = diff(
        legacy_live::Shop::contract(),
        legacy_no_op::Shop::contract(),
    );
    assert_eq!(report.verdict(), Verdict::Different);
    let liveness = report
        .changes()
        .iter()
        .find(|change| change.record() == "Shop" && change.field() == "values[legacy].liveness")
        .expect("liveness change");
    assert_eq!(liveness.collection(), "owners");
    assert_eq!(liveness.before(), Some(&serde_json::json!("live")));
    assert_eq!(
        liveness.after(),
        Some(&serde_json::json!("deprecated_no_op"))
    );
    assert_eq!(
        liveness.axes(),
        [Axis::PresenceAndDefaults, Axis::Documentation]
    );
    assert!(report.changes().iter().any(|change| {
        change.field() == "values[legacy].deprecation"
            && change.axes().contains(&Axis::Documentation)
    }));
    assert!(report.has_axis(Axis::Wire), "the header is no longer sent");
    assert!(report.changes().iter().any(|change| {
        change.collection() == "diagnostics" && change.record() == "C3-PLACE:Shop:legacy"
    }));
    assert!(report.to_text().contains("deprecated_no_op"));
    assert!(!report.has_axis(Axis::Unknown));
}
