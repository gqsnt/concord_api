#[derive(Debug)]
pub struct RetryProfilesBlock {
    pub profiles: Vec<RetryProfileDef>,
    pub default: Option<Ident>,
}

#[derive(Debug)]
pub struct RetryProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub patch: RetryPatch,
}

#[derive(Debug, Clone)]
pub enum RetrySpec {
    Profile(Ident),
    Patch(RetryPatch),
    Off,
}

#[derive(Debug, Clone, Default)]
pub struct RetryPatch {
    pub attempts: Option<LitInt>,
    pub methods: Option<Vec<Ident>>,
    pub statuses: Option<Vec<LitInt>>,
    pub transport_errors: Option<Vec<Ident>>,
    pub respect_retry_after: Option<bool>,
    pub idempotency: Option<RetryIdempotencySpec>,
}

#[derive(Debug, Clone)]
pub enum RetryIdempotencySpec {
    Header(LitStr),
}

