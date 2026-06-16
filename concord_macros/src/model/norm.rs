//! Canonical macro model produced by raw syntax normalization.

use crate::ast::{
    AuthCredentials, AuthUseKind, BehaviorProfilesBlock, BehaviorUseSpec, CacheProfilesBlock,
    CacheSpec, CodecSpec, MapSpec, PaginateSpec, PolicyBlocks, RateLimitKeyBindingSpec,
    RateLimitProfilesBlock, RateLimitSpec, RetryProfilesBlock, RetrySpec, RouteExpr, VarDeclNoWire,
};
use crate::model::Scheme;
use proc_macro2::Span;
use syn::{Ident, LitStr};

/// Normalized API tree consumed by semantic resolution.
///
/// Raw parser-only details such as keyword spans are stripped before this model
/// is produced.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NormApiTree {
    pub client: NormClient,
    pub items: Vec<NormNode>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NormClient {
    pub span: Span,
    pub name: Ident,
    pub scheme: Scheme,
    pub host: LitStr,
    pub policy: PolicyBlocks,
    pub vars: Option<NormVars>,
    pub auth_vars: Option<NormVars>,
    pub auth: Option<AuthCredentials>,
    pub auth_uses: Vec<NormAuthUse>,
    pub cache_profiles: Option<CacheProfilesBlock>,
    pub cache: Option<CacheSpec>,
    pub retry_profiles: Option<RetryProfilesBlock>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitProfilesBlock>,
    pub behavior_profiles: Option<BehaviorProfilesBlock>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NormVars {
    pub decls: Vec<VarDeclNoWire>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum NormNode {
    Layer(Box<NormScope>),
    Endpoint(Box<NormEndpoint>),
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NormScope {
    pub span: Span,
    pub scope_name: Option<Ident>,
    pub kind: RouteLayerKind,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,
    pub policy: PolicyBlocks,
    pub behavior_uses: Vec<BehaviorUseSpec>,
    pub auth_uses: Vec<NormAuthUse>,
    pub cache: Option<CacheSpec>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    pub items: Vec<NormNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteLayerKind {
    Prefix,
    Path,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NormEndpointLine {
    pub span: Span,
    pub method: Ident,
    pub name: Ident,
    pub alias: Option<Ident>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NormEndpoint {
    pub span: Span,
    pub line: NormEndpointLine,
    pub method: Ident,
    pub name: Ident,
    pub alias: Option<Ident>,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,
    pub policy: PolicyBlocks,
    pub behavior_uses: Vec<BehaviorUseSpec>,
    pub auth_uses: Vec<NormAuthUse>,
    pub cache: Option<CacheSpec>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    pub paginate: Option<PaginateSpec>,
    pub body: Option<CodecSpec>,
    pub response: CodecSpec,
    pub map: Option<MapSpec>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct NormAuthUse {
    pub kind: AuthUseKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_tree_boundary_has_client_and_items() {
        let client = NormClient {
            span: Span::call_site(),
            name: Ident::new("Api", Span::call_site()),
            scheme: Scheme::Https,
            host: LitStr::new("example.com", Span::call_site()),
            policy: PolicyBlocks::default(),
            vars: None,
            auth_vars: None,
            auth: None,
            auth_uses: Vec::new(),
            cache_profiles: None,
            cache: None,
            retry_profiles: None,
            retry: None,
            rate_limit: None,
            behavior_profiles: None,
        };

        let tree = NormApiTree {
            client,
            items: Vec::new(),
        };
        assert!(tree.items.is_empty());
    }
}
