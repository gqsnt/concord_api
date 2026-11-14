use serde::{Deserialize, Serialize};

/// One server of LoL and TFT, and its host: `euw1` in `https://euw1.api.riotgames.com`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Br1, Eun1, Euw1, Jp1, Kr, La1, La2, Me1, Na1, Oc1, Ru, Sg2, Tr1, Tw2, Vn2,
}

/// A region, and its host: it holds the accounts and the matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    Americas, Asia, Europe, Sea,
}

impl Platform {
    /// The region that holds this platform's accounts and matches.
    pub fn region(self) -> Region {
        use Platform::*;
        match self {
            Br1 | La1 | La2 | Na1 => Region::Americas,
            Jp1 | Kr => Region::Asia,
            Eun1 | Euw1 | Me1 | Ru | Tr1 => Region::Europe,
            Oc1 | Sg2 | Tw2 | Vn2 => Region::Sea,
        }
    }
}

/// How Riot says no: `{"status": {"message": "Data not found", "status_code": 404}}`.
#[derive(Clone, Debug, Deserialize)]
pub struct RiotProblem {
    pub status: RiotStatus,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RiotStatus {
    pub message: String,
    pub status_code: u16,
}

/// Riot's policy: a `429` was turned away before any work, so even a `POST` may go again.
pub fn retry() -> riot::Retry {
    riot::Retry::standard().not_applied_on(429)
}
