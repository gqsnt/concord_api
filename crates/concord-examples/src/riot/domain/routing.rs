//! Riot platform and regional routing values plus the bounded page default.
//!
//! Required features: the Riot bundle. Mode: deterministic planning and
//! manual live use. Documentation owner: `docs/recipes/riot.md`.
//! Proving test: `tests/riot_contract.rs`.

use std::fmt;
use std::num::NonZeroU32;

/// Riot's documented starting application limit for a production API key.
///
/// Riot applies the application limit per routing host. The unified client
/// declares separate lexical rules for its platform and regional route trees.
pub const DEFAULT_MATCH_PAGE_SIZE: NonZeroU32 = match NonZeroU32::new(20) {
    Some(value) => value,
    None => unreachable!(),
};

// ANCHOR: riot_routes
#[derive(Clone, Copy, Debug)]
pub enum PlatformRoute {
    Br1,
    Eun1,
    Euw1,
    Jp1,
    Kr,
    La1,
    La2,
    Me1,
    Na1,
    Oc1,
    Ru,
    Sg2,
    Tr1,
    Tw2,
    Vn2,
}

impl PlatformRoute {
    #[must_use]
    pub const fn regional(self) -> RegionalRoute {
        match self {
            Self::Br1 | Self::La1 | Self::La2 | Self::Na1 => RegionalRoute::Americas,
            Self::Jp1 | Self::Kr => RegionalRoute::Asia,
            Self::Eun1 | Self::Euw1 | Self::Me1 | Self::Ru | Self::Tr1 => RegionalRoute::Europe,
            Self::Oc1 | Self::Sg2 | Self::Tw2 | Self::Vn2 => RegionalRoute::Sea,
        }
    }
}

impl fmt::Display for PlatformRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Br1 => "br1",
            Self::Eun1 => "eun1",
            Self::Euw1 => "euw1",
            Self::Jp1 => "jp1",
            Self::Kr => "kr",
            Self::La1 => "la1",
            Self::La2 => "la2",
            Self::Me1 => "me1",
            Self::Na1 => "na1",
            Self::Oc1 => "oc1",
            Self::Ru => "ru",
            Self::Sg2 => "sg2",
            Self::Tr1 => "tr1",
            Self::Tw2 => "tw2",
            Self::Vn2 => "vn2",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum RegionalRoute {
    Americas,
    Asia,
    Europe,
    Sea,
}

impl fmt::Display for RegionalRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Americas => "americas",
            Self::Asia => "asia",
            Self::Europe => "europe",
            Self::Sea => "sea",
        })
    }
}
// ANCHOR_END: riot_routes
