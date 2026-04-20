#[derive(Debug)]
pub struct RateLimitProfilesBlock {
    pub profiles: Vec<RateLimitProfileDef>,
    pub default: Vec<Ident>,
    pub response_policy: Option<Path>,
}

#[derive(Debug, Clone)]
pub struct RateLimitProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub plan: RateLimitPlanSpec,
}

#[derive(Debug, Clone)]
pub enum RateLimitSpec {
    Profiles { only: bool, profiles: Vec<Ident> },
    Inline { only: bool, plan: RateLimitPlanSpec },
    Off,
}

#[derive(Debug, Clone, Default)]
pub struct RateLimitPlanSpec {
    pub buckets: Vec<RateLimitBucketSpec>,
}

#[derive(Debug, Clone)]
pub struct RateLimitBucketSpec {
    pub kind: Ident,
    pub key: Vec<RateLimitKeySpec>,
    pub cost: Option<LitInt>,
    pub windows: Vec<RateLimitWindowSpec>,
}

#[derive(Debug, Clone)]
pub enum RateLimitKeySpec {
    RouteHost,
    Endpoint,
    Method,
    Named(Ident),
    Static(LitStr),
}

#[derive(Debug, Clone)]
pub struct RateLimitWindowSpec {
    pub max: LitInt,
    pub every: LitInt,
    pub unit: RateLimitDurationUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum RateLimitDurationUnit {
    Seconds,
    Minutes,
}

#[derive(Debug, Clone)]
pub struct RateLimitKeyBindingSpec {
    pub name: Ident,
    pub value: Ident,
}

