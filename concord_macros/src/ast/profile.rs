#[derive(Debug)]
pub struct ProfilesBlock {
    pub profiles: Vec<ProfileDef>,
}

#[derive(Debug, Clone)]
pub struct ProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub patch: ProfilePatch,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilePatch {
    pub auth_uses: Vec<AuthUseDecl>,
    pub rate_limit: Option<RateLimitSpec>,
}

#[derive(Debug, Clone)]
pub struct ProfileUseSpec {
    #[allow(dead_code)]
    pub span: proc_macro2::Span,
    pub names: Vec<Ident>,
}
