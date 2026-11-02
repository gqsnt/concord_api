//! Routing values, one type per published route list (see `SOURCES.md`).
//!
//! The official reference lists the routes per operation, and the lists differ: account-v1 has
//! three regional clusters, match-v5 four (with SEA), and platform-routed operations their own
//! platform list. Each type is the host label it renders, so `{route}.api.riotgames.com` is the
//! subdomain template. There is deliberately no mapping from a platform to a cluster: an
//! account's cluster does not tell which platform a player plays on.

use std::fmt;

macro_rules! route {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $(
                #[doc = concat!("`", $label, "`")]
                $variant
            ),+
        }

        impl $name {
            /// Every value of the published list, in reference order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The host label (`americas`, `euw1`, ...).
            pub const fn label(self) -> &'static str {
                match self {
                    $($name::$variant => $label),+
                }
            }

            /// The value whose label is `label` (case-insensitive), if the list has one.
            pub fn from_label(label: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|r| r.label().eq_ignore_ascii_case(label))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.label())
            }
        }
    };
}

route!(
    /// account-v1 ("americas, asia, and europe. You can query for any account in any region").
    /// Also used for tournament-stub-v5, whose routing the reference does not state.
    AccountRoute {
        Americas => "americas",
        Asia => "asia",
        Europe => "europe",
    }
);

route!(
    /// match-v5 (AMERICAS, ASIA, EUROPE, SEA).
    MatchRoute {
        Americas => "americas",
        Asia => "asia",
        Europe => "europe",
        Sea => "sea",
    }
);

route!(
    /// Platform routes of summoner-v4, champion-mastery-v4 and champion-v3 as listed on their
    /// reference pages (2026-11-02). The general routing table in `docs/lol` differs (PH2, TH2,
    /// no ME1); the per-operation lists are used.
    Platform {
        Br1 => "br1",
        Eun1 => "eun1",
        Euw1 => "euw1",
        Jp1 => "jp1",
        Kr => "kr",
        La1 => "la1",
        La2 => "la2",
        Me1 => "me1",
        Na1 => "na1",
        Oc1 => "oc1",
        Ru => "ru",
        Sg2 => "sg2",
        Tr1 => "tr1",
        Tw2 => "tw2",
        Vn2 => "vn2",
    }
);
