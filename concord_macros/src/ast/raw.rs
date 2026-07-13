/// Complete raw parser output. This is syntax-shaped input for semantic
/// resolution, not a codegen model.
#[derive(Debug)]
#[allow(dead_code)]
pub struct RawApi {
    pub span: Span,
    pub client: RawClient,
    pub items: Vec<RawItem>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct RawClient {
    pub span: Span,
    pub client_span: Span,
    pub body_span: Span,
    pub name: Ident,
    pub scheme: Scheme,
    pub host: LitStr,
    pub policy: PolicyBlocks,
    pub vars: Option<VarsBlock>,
    pub auth_vars: Option<VarsBlock>,
    pub auth: Option<AuthCredentials>,
    pub auth_uses: Vec<AuthUseDecl>,
    pub default_behavior_uses: Vec<BehaviorUseSpec>,
    pub rate_limit: Option<RateLimitProfilesBlock>,
    pub behavior_profiles: Option<BehaviorProfilesBlock>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum RawItem {
    Layer(Box<RawScope>),
    Endpoint(Box<RawEndpoint>),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct RawScope {
    pub span: Span,
    pub scope_span: Span,
    pub body_span: Span,
    pub scope_name: Option<Ident>,
    pub host_route: Option<RouteExpr>,
    pub path_route: Option<RouteExpr>,
    pub params: Vec<VarDeclNoWire>,
    pub policy: PolicyBlocks,
    pub behavior_uses: Vec<BehaviorUseSpec>,
    pub auth_uses: Vec<AuthUseDecl>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    pub items: Vec<RawItem>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct RawEndpointLine {
    pub span: Span,
    pub method: Ident,
    pub name: Ident,
    pub alias: Option<Ident>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct RawEndpoint {
    pub span: Span,
    pub line: RawEndpointLine,
    pub method: Ident, // "GET", "POST", ...
    pub name: Ident,
    pub alias: Option<Ident>,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,

    pub policy: PolicyBlocks,
    pub behavior_uses: Vec<BehaviorUseSpec>,
    pub auth_uses: Vec<AuthUseDecl>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,

    pub paginate: Option<PaginateSpec>,
    pub body: RawRequestIo,

    pub response: RawResponseIo,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct RawAst {
    pub api: RawApi,
    pub diagnostics: RawDiagnostics,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct RawDiagnostics {
    pub source_name: Option<String>,
    pub parser_notes: Vec<String>,
}
