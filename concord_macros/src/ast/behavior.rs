#[derive(Debug)]
pub struct BehaviorProfilesBlock {
    pub profiles: Vec<BehaviorProfileDef>,
}

#[derive(Debug, Clone)]
pub struct BehaviorProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub patch: BehaviorPatch,
}

#[derive(Debug, Clone, Default)]
pub struct BehaviorPatch {
    pub auth_uses: Vec<AuthUseDecl>,
    pub retry: Option<RetrySpec>,
    pub rate_limit: Option<RateLimitSpec>,
}

#[derive(Debug, Clone)]
pub struct BehaviorUseSpec {
    #[allow(dead_code)]
    pub span: proc_macro2::Span,
    pub names: Vec<Ident>,
}
