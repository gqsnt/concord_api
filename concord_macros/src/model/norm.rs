//! Canonical macro model produced by raw syntax normalization.

use crate::ast::{
    AuthCredentials, AuthUseKind, BehaviorProfilesBlock, BehaviorUseSpec, MapSpec, PaginateSpec,
    PolicyBlocks, RateLimitKeyBindingSpec, RateLimitProfilesBlock, RateLimitSpec, RawRequestIo,
    RawResponseIo, RetryProfilesBlock, RetrySpec, RouteExpr, VarDeclNoWire,
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
    pub default_behavior_uses: Vec<BehaviorUseSpec>,
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
pub(crate) struct NormEndpoint {
    pub span: Span,
    pub method: Ident,
    pub name: Ident,
    pub alias: Option<Ident>,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,
    pub policy: PolicyBlocks,
    pub behavior_uses: Vec<BehaviorUseSpec>,
    pub auth_uses: Vec<NormAuthUse>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    pub paginate: Option<PaginateSpec>,
    pub body: RawRequestIo,
    pub response: RawResponseIo,
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
            default_behavior_uses: Vec::new(),
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

    #[test]
    fn normalized_endpoint_identity_is_stored_once() {
        let endpoint = NormEndpoint {
            span: Span::call_site(),
            method: Ident::new("GET", Span::call_site()),
            name: Ident::new("Ping", Span::call_site()),
            alias: Some(Ident::new("ping", Span::call_site())),
            route: RouteExpr { atoms: Vec::new() },
            params: Vec::new(),
            policy: PolicyBlocks::default(),
            behavior_uses: Vec::new(),
            auth_uses: Vec::new(),
            retry: None,
            rate_limit: None,
            rate_limit_keys: Vec::new(),
            paginate: None,
            body: None,
            response: crate::ast::RawIoSpec {
                marker: syn::parse_quote!(Json<String>),
                enc: syn::parse_quote!(Json),
                ty: syn::parse_quote!(String),
                args: vec![syn::parse_quote!(String)],
                had_angle_args: true,
            },
            map: None,
        };

        assert_eq!(endpoint.method, "GET");
        assert_eq!(endpoint.name, "Ping");
        assert_eq!(
            endpoint.alias.as_ref().map(ToString::to_string).as_deref(),
            Some("ping")
        );
    }
}
