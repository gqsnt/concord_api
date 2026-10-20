//! The explicit, maintained registry of compiled contract-description
//! providers that `contract export|check` can resolve by name (EXPLAIN-03).
//!
//! A proc macro cannot discover every `api!` invocation, so targets are
//! listed here by hand. Each provider returns a `&'static ContractDescription`
//! exactly like generated `Client::contract()`; resolving a target reads a
//! static description and performs no network work.
//!
//! The repository example clients are compiled behind the `examples` feature
//! (which adds the `concord-examples` dependency): run
//! `cargo run -p concord-xtask --features examples -- contract ...`. Their
//! providers are the generated `Client::contract()` functions, exported and
//! checked by `tests/contract_cli.rs` under
//! `cargo test --locked -p concord-xtask --features examples` (the `tools`
//! recipe). The always-on `fixture-*` targets are hand-built Catalog-like
//! descriptions from `concord-inspect`'s fixture corpus.

use concord_core::inspection::ContractDescription;

/// One named contract-description provider.
pub(crate) struct ContractTarget {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) contract: fn() -> &'static ContractDescription,
}

/// Returns every registered target in listing order.
pub(crate) fn targets() -> Vec<ContractTarget> {
    #[allow(
        unused_mut,
        reason = "feature-gated example targets are appended below"
    )]
    let mut targets = vec![ContractTarget {
        name: "fixture-catalog",
        description: "hand-built Catalog-like contract fixture (concord-inspect corpus baseline)",
        contract: concord_inspect::fixtures::catalog_static,
    }];
    #[cfg(feature = "examples")]
    {
        targets.push(ContractTarget {
            name: "catalog",
            description: "concord-examples contracts::catalog_v3::CatalogApi (canonical Catalog)",
            contract: concord_examples::contracts::catalog_v3::CatalogApi::contract,
        });
        targets.push(ContractTarget {
            name: "riot",
            description: "concord-examples riot::api::RiotApi (46 endpoints)",
            contract: concord_examples::riot::api::RiotApi::contract,
        });
        targets.push(ContractTarget {
            name: "data-dragon",
            description: "concord-examples riot::api::DDragonApi",
            contract: concord_examples::riot::api::DDragonApi::contract,
        });
    }
    targets
}

/// Resolves one target by exact name.
pub(crate) fn resolve(name: &str) -> Option<ContractTarget> {
    targets().into_iter().find(|target| target.name == name)
}
