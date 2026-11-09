//! Client construction and the three scoped domain operations.
use std::{net::IpAddr, num::NonZeroUsize, time::Duration};

use concord::reqwest::{self, Url, header::HeaderValue};
use serde::{Deserialize, Serialize};

use crate::{
    ConfigurationProblem, DeltaLink, DirectoryObject, GraphError, Group, GroupField, LinkProblem,
    Listing, MemberField, UserDelta, UserField,
    api::{GraphApi, GraphApiConfig},
    links::{Collection, LinkKind, StoredLink},
};

/// Caller-asserted authorization namespace. Supply a stable tenant identifier and a principal
/// plus permission-profile/version identifier. These labels do not authenticate the token.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(concord::Debug)]
#[sensitive]
pub struct AuthContext {
    tenant: String,
    principal: String,
}

impl AuthContext {
    /// Creates a namespace for saved links. Change the principal label when its permission
    /// profile changes. Token refresh within the same context can reuse saved links.
    pub fn new(
        tenant: impl Into<String>,
        principal: impl Into<String>,
    ) -> Result<Self, GraphError> {
        let result = Self {
            tenant: tenant.into(),
            principal: principal.into(),
        };
        if result.tenant.trim().is_empty() || result.principal.trim().is_empty() {
            return Err(GraphError::Configuration(
                ConfigurationProblem::EmptyContext,
            ));
        }
        Ok(result)
    }

    /// Caller-provided tenant identity.
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// Caller-provided principal and permission-profile identity.
    pub fn principal(&self) -> &str {
        &self.principal
    }
}

/// Sends `authorization: Bearer <token>` from the constructor's caller-obtained token.
///
/// A Graph v1.0 client with a fixed credential destination and redirects disabled.
///
/// It obtains no tokens, retries no requests and follows no HTTP redirects. Production uses
/// the global Graph origin only. The explicit loopback constructor is for synthetic tests.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct GraphClient {
    api: GraphApi,
    #[public]
    context: AuthContext,
    base: String,
    #[public]
    pub(crate) success_limit: usize,
    #[public]
    pub(crate) error_limit: usize,
}

impl GraphClient {
    /// Constructs a client for `https://graph.microsoft.com/v1.0` with a caller-obtained token.
    /// The transport uses 30s request/10s connect timeouts and refuses redirects.
    pub fn new(context: AuthContext, token: impl AsRef<str>) -> Result<Self, GraphError> {
        Self::build(
            context,
            token.as_ref(),
            "https://graph.microsoft.com/v1.0",
            false,
        )
    }

    /// Offline/test constructor. Only `http://<literal-loopback-IP>:<port>/v1.0` is accepted.
    /// No DNS names, user information, query or fragment are permitted. Use synthetic tokens.
    pub fn for_loopback(
        context: AuthContext,
        token: impl AsRef<str>,
        base: &str,
    ) -> Result<Self, GraphError> {
        let invalid = || GraphError::Configuration(ConfigurationProblem::InvalidLoopbackBase);
        let url = Url::parse(base).map_err(|_| invalid())?;
        let loopback = url
            .host_str()
            .and_then(|host| {
                host.trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<IpAddr>()
                    .ok()
            })
            .is_some_and(|ip| ip.is_loopback());
        if !loopback
            || url.scheme() != "http"
            || url.path() != "/v1.0"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.port() == Some(0)
            || url.as_str() != base
        {
            return Err(invalid());
        }
        Self::build(context, token.as_ref(), base, true)
    }

    fn build(
        context: AuthContext,
        token: &str,
        base: &str,
        loopback: bool,
    ) -> Result<Self, GraphError> {
        if context.tenant.trim().is_empty() || context.principal.trim().is_empty() {
            return Err(GraphError::Configuration(
                ConfigurationProblem::EmptyContext,
            ));
        }
        if token.is_empty()
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
        {
            return Err(GraphError::Configuration(
                ConfigurationProblem::InvalidToken,
            ));
        }
        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| GraphError::Configuration(ConfigurationProblem::InvalidToken))?;
        authorization.set_sensitive(true);
        let builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .https_only(!loopback);
        let builder = if loopback {
            builder.no_proxy()
        } else {
            builder
        };
        let http = builder
            .build()
            .map_err(|_| GraphError::Configuration(ConfigurationProblem::Transport))?;
        let api = GraphApi::new(http, GraphApiConfig { authorization }).with_base_url(base)?;
        Ok(Self {
            api,
            context,
            base: base.into(),
            success_limit: 4 * 1024 * 1024,
            error_limit: 64 * 1024,
        })
    }

    /// Overrides finite response body caps before creating listings (default 4 MiB / 64 KiB).
    /// These limits bound each decoded page/error, not the total size of collected results.
    pub fn with_body_limits(mut self, success: NonZeroUsize, error: NonZeroUsize) -> Self {
        self.success_limit = success.get();
        self.error_limit = error.get();
        self
    }

    /// Initial users-delta round. Empty selection means Graph's default tracking scope.
    /// A nonempty selection also includes `id`; only selected properties are tracked.
    pub fn users_delta(&self, fields: &[UserField]) -> Result<Listing<UserDelta>, GraphError> {
        self.user_listing(selection(fields.iter().map(|f| f.wire())), None)
    }

    fn user_listing(
        &self,
        select: String,
        start: Option<DeltaLink>,
    ) -> Result<Listing<UserDelta>, GraphError> {
        let api = self.api.clone();
        let query = optional(&select);
        Listing::new(
            self,
            Collection::UsersDelta,
            select,
            start,
            move || api.users_delta().maybe_select(query.clone()).prepare(),
            |user| !user.id.is_empty(),
        )
    }

    /// Starts a later incremental round from a completed checkpoint; reuses its original
    /// selection unchanged. Context, destination and collection are verified before any I/O.
    pub fn resume_users(&self, checkpoint: &DeltaLink) -> Result<Listing<UserDelta>, GraphError> {
        let listing = self.user_listing(
            checkpoint.0.binding.select.clone(),
            Some(checkpoint.clone()),
        )?;
        checkpoint.0.check(&listing.binding, LinkKind::Delta)?;
        Ok(listing)
    }

    /// Restores a users-delta checkpoint for this exact authorization context and base URL.
    /// Stored JSON must come from `DeltaLink::to_json`, under trusted application storage.
    pub fn restore_delta(&self, json: &str) -> Result<DeltaLink, GraphError> {
        let stored = StoredLink::parse(json)?;
        let fields = &stored.binding.select;
        let allowed = [
            "id",
            "displayName",
            "givenName",
            "surname",
            "mail",
            "userPrincipalName",
            "accountEnabled",
        ];
        if !fields.is_empty()
            && (fields.split(',').any(|f| !allowed.contains(&f))
                || selection(fields.split(',')) != *fields)
        {
            return Err(GraphError::link(LinkProblem::BindingMismatch));
        }
        let checkpoint = DeltaLink(stored);
        self.resume_users(&checkpoint)?;
        Ok(checkpoint)
    }

    /// Lists group summaries; empty selection uses Graph defaults. No group delta is exposed.
    pub fn groups(&self, fields: &[GroupField]) -> Result<Listing<Group>, GraphError> {
        let select = selection(fields.iter().map(|f| f.wire()));
        let query = optional(&select);
        let api = self.api.clone();
        Listing::new(
            self,
            Collection::Groups,
            select,
            None,
            move || api.groups().maybe_select(query.clone()).prepare(),
            |group| !group.id.is_empty(),
        )
    }

    /// Lists one group's direct directory objects, without casts or transitive traversal.
    /// With a selection, sends `$count=true` and `ConsistencyLevel: eventual` to satisfy the
    /// documented advanced-query requirements; the header also accompanies continuation calls.
    pub fn direct_members(
        &self,
        group: &str,
        fields: &[MemberField],
    ) -> Result<Listing<DirectoryObject>, GraphError> {
        let select = selection(fields.iter().map(|f| f.wire()));
        let query = optional(&select);
        let advanced = !select.is_empty();
        let api = self.api.clone();
        let id = group.to_owned();
        Listing::new(
            self,
            Collection::Members(id.clone()),
            select,
            None,
            move || {
                api.direct_members(id.clone())
                    .maybe_select(query.clone())
                    .maybe_count(advanced.then_some(true))
                    .maybe_consistency(advanced.then(|| "eventual".into()))
                    .prepare()
            },
            |member| !member.id.is_empty() && !member.object_type.is_empty(),
        )
    }

    pub(crate) fn binding(
        &self,
        collection: Collection,
        select: String,
        path: String,
    ) -> crate::links::Binding {
        crate::links::Binding {
            context: self.context.clone(),
            base: self.base.clone(),
            collection,
            select,
            path,
        }
    }
}

fn selection<'a>(fields: impl Iterator<Item = &'a str>) -> String {
    let mut names: Vec<_> = fields.collect();
    if !names.is_empty() {
        names.push("id");
    }
    names.sort_unstable();
    names.dedup();
    names.join(",")
}
fn optional(select: &str) -> Option<String> {
    (!select.is_empty()).then(|| select.to_owned())
}
