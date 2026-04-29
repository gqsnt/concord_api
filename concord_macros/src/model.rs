//! Shared macro model primitives and normalized semantic tree.
//!
//! Raw parser structs stay in `ast`. `NormApiTree` is the first v5 semantic
//! boundary: removed syntax and unsupported auth forms are rejected before
//! semantic resolution consumes it. Generated code must depend only on resolved
//! sema output plus neutral primitives such as `Scheme` and `SetOp`.

use crate::ast::{
    AuthBlock, AuthUseKind, CacheProfilesBlock, CacheSpec, CodecSpec, MapSpec, PaginateSpec,
    PolicyBlocks, RateLimitKeyBindingSpec, RateLimitProfilesBlock, RateLimitSpec,
    RetryProfilesBlock, RetrySpec, RouteExpr, VarDeclNoWire,
};
use proc_macro2::Span;
use syn::{Ident, LitStr};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Scheme {
    Http,
    Https,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetOp {
    Set,
    Push,
}

/// Normalized API tree consumed by semantic resolution.
///
/// Raw parser-only details such as keyword spans and unsupported legacy auth
/// forms are stripped or rejected before this model is produced.
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
    pub auth: Option<AuthBlock>,
    pub auth_uses: Vec<NormAuthUse>,
    pub cache_profiles: Option<CacheProfilesBlock>,
    pub cache: Option<CacheSpec>,
    pub retry_profiles: Option<RetryProfilesBlock>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitProfilesBlock>,
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

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct NormAuthUse {
    pub kind: AuthUseKind,
}
