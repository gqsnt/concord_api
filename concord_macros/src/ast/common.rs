#[derive(Debug)]
pub struct ApiFile {
    pub client: ClientDef,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub struct ClientDef {
    pub name: Ident,
    pub scheme: SchemeLit,
    pub host: LitStr,
    pub policy: PolicyBlocks,
    pub vars: Option<VarsBlock>,
    pub auth_vars: Option<VarsBlock>,
    pub auth: Option<AuthBlock>,
    pub auth_uses: Vec<AuthUseDecl>,
    pub cache_profiles: Option<CacheProfilesBlock>,
    pub cache: Option<CacheSpec>,
    pub retry_profiles: Option<RetryProfilesBlock>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitProfilesBlock>,
}

#[derive(Debug)]
pub struct VarsBlock {
    pub decls: Vec<VarDeclNoWire>,
}

