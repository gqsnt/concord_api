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
    pub revalidate: Option<LitBool>,
    pub on_error: Option<CacheOnErrorSpec>,
    pub capacity: Option<CacheCapacitySpec>,
    pub max_body: Option<CacheSizeSpec>,
    pub shared: Option<Span>,
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
pub struct CacheCapacitySpec {
    pub amount: LitInt,
    pub unit: CacheCapacityUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheCapacityUnit {
    Entries,
}

#[derive(Debug, Clone)]
pub struct CacheSizeSpec {
    pub amount: LitInt,
    pub unit: CacheSizeUnit,
}

#[derive(Debug, Clone, Copy)]
pub enum CacheSizeUnit {
    Bytes,
    Kb,
    Kib,
    Mb,
    Mib,
    Gb,
    Gib,
}
