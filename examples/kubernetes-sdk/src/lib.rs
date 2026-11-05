//! A bounded, read-only slice of the Kubernetes API, declared with `concord::api!`.
//!
//! Pinned target: Kubernetes **v1.37.2** (OpenAPI v3 documents of that release tag; see
//! `PROVIDER.md` for digests, retrieval date and the supported/unsupported operation list). This is
//! not a complete Kubernetes SDK.
//!
//! # Layout
//!
//! - [`Kube`] — the generated client: `core_v1()`, `apps_v1()`, `batch_v1()`, `networking_v1()`
//!   group/version scopes, each with a `namespaced(namespace)` scope. Every operation is an
//!   ordinary generated request with its setters, `prepare()` and `send()`.
//! - [`KubeContext`] / [`NamespaceContext`] — one configured client (base URL, bearer token) and a
//!   namespace bound across the four group/version scopes (`core()`, `apps()`, `batch()`,
//!   `networking()`).
//! - [`model`] — typed objects (`Pod`, `Deployment`, `Ingress`, ...), list envelopes
//!   ([`ObjectList`]) with `items` extraction, the `Status` error document.
//! - [`LabelSelector`] / [`FieldSelector`] — typed selectors rendered in Kubernetes syntax.
//! - [`list`] — the chunked-list adapter for `concord-pagination` (opaque `continue`, consistent
//!   snapshot check, typed `410 Expired`).
//! - [`watch`] — a typed watch stream over `concord::jsonl::JsonValuesReader` (one connection;
//!   reconnection and resync are the caller's workflow).
//!
//! # Example
//!
//! ```
//! use concord::reqwest;
//! use concord_example_kubernetes_sdk::{
//!     BearerToken, KubeContext, LabelSelector, NamespaceName,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let kube = KubeContext::new(reqwest::Client::new())
//!     .with_base_url("https://127.0.0.1:6443")?
//!     .with_bearer(&BearerToken::new("synthetic-token"))?;
//! let team_a = kube.namespace(NamespaceName::new("team-a")?);
//!
//! let pods = team_a
//!     .core()
//!     .pods()
//!     .set_label_selector(LabelSelector::equals("app", "web")?)
//!     .set_limit(50)
//!     .prepare()?;
//! assert_eq!(
//!     pods.request().url().as_str(),
//!     "https://127.0.0.1:6443/api/v1/namespaces/team-a/pods?labelSelector=app%3Dweb&limit=50"
//! );
//! assert!(pods.request().headers()["authorization"].is_sensitive());
//! assert!(!format!("{pods:?}").contains("synthetic-token"));
//! # Ok(()) }
//! ```

mod api;
mod context;
mod error;
mod ids;
pub mod list;
pub mod model;
mod selector;
pub mod watch;

pub use api::*;
pub use context::{KubeContext, NamespaceContext};
pub use error::{ApiError, KubeError, STATUS_BODY_LIMIT};
pub use ids::{
    BearerToken, ContinueToken, InvalidToken, NameError, NamespaceName, ObjectName,
    ResourceVersion, Uid,
};
pub use model::{ObjectKey, ObjectList, Resource, Status};
pub use selector::{
    FieldSelector, FieldSelectorBuilder, LabelSelector, LabelSelectorBuilder, SelectorError,
};

/// The Kubernetes release whose OpenAPI v3 documents this SDK's paths, parameters and models were
/// checked against.
pub const PINNED_SERVER_VERSION: &str = "v1.37.2";
