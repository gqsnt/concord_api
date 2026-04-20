#[derive(Debug)]
pub struct CacheProfilesBlock {
    pub profiles: Vec<CacheProfileDef>,
    pub default: Option<Ident>,
}

#[derive(Debug, Clone)]
pub struct CacheProfileDef {
    pub name: Ident,
    pub extends: Option<Ident>,
    pub patch: CachePatch,
}

#[derive(Debug, Clone)]
pub enum CacheSpec {
    Profile { only: bool, profile: Ident },
    Patch { only: bool, patch: CachePatch },
    Off,
}

#[derive(Debug, Clone, Default)]
pub struct CachePatch {
    pub http: Option<Span>,
    pub ttl: Option<CacheDurationSpec>,
    pub capacity: Option<CacheCapacitySpec>,
    pub max_body: Option<CacheSizeSpec>,
    pub revalidate: Option<LitBool>,
    pub shared: Option<LitBool>,
    pub on_error: Option<CacheOnErrorSpec>,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheOnErrorSpec {
    Ignore,
    ServeStale,
}

#[derive(Debug, Clone)]
pub struct CacheDurationSpec {
    pub amount: LitInt,
    pub unit: RateLimitDurationUnit,
}

#[derive(Debug, Clone)]
pub enum CacheCapacitySpec {
    Entries { amount: LitInt },
    Bytes(CacheSizeSpec),
}

#[derive(Debug, Clone)]
pub struct CacheSizeSpec {
    pub amount: LitInt,
    pub unit: CacheSizeUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheSizeUnit {
    Bytes,
    KiB,
    MiB,
    GiB,
}
