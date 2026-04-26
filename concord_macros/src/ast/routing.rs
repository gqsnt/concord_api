#[derive(Debug, Clone, Copy)]
pub enum SchemeLit {
    Http,
    Https,
}

#[derive(Debug)]
pub enum Item {
    Layer(Box<LayerDef>),
    Endpoint(Box<EndpointDef>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Prefix,
    Path,
}

#[derive(Debug, Clone)]
pub struct RouteExpr {
    pub atoms: Vec<RouteAtom>,
}

#[derive(Debug, Clone)]
pub enum RouteAtom {
    Static(LitStr),
    Ref(ScopedRef),
    Fmt(FmtSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefScope {
    Cx,
    Ep,
    Auth,
}
#[derive(Debug, Clone)]
pub struct ScopedRef {
    pub scope: RefScope,
    pub ident: Ident,
}

#[derive(Debug)]
pub struct LayerDef {
    pub scope_name: Option<Ident>,
    pub kind: LayerKind,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,
    pub policy: PolicyBlocks,
    pub auth_uses: Vec<AuthUseDecl>,
    pub cache: Option<CacheSpec>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct EndpointDef {
    pub method: Ident, // "GET", "POST", ...
    pub name: Ident,
    pub alias: Option<Ident>,
    pub route: RouteExpr,
    pub params: Vec<VarDeclNoWire>,

    pub policy: PolicyBlocks,
    pub auth_uses: Vec<AuthUseDecl>,
    pub cache: Option<CacheSpec>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,
    pub rate_limit_keys: Vec<RateLimitKeyBindingSpec>,

    pub paginate: Option<PaginateSpec>,
    pub body: Option<CodecSpec>,

    pub response: CodecSpec,
    pub map: Option<MapSpec>,
}

