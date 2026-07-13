use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

api! {
    client DDragonClient {
        base "https://leagueoflegends.com"

        headers {
            "user-agent" = "ConcordDDragonExample/1.0"
        }

    }

    scope ddragon {
        host ["ddragon"]

        scope api {
            path ["api"]

            GET GetVersions
            as versions
            path ["versions.json"]
            -> Json<Vec<String>>
        }

        GET GetLanguages
        as languages
        path ["cdn", "languages.json"]
        -> Json<Vec<String>>

        GET GetRealmByRegion(region: String)
        as realm
        path ["realms", fmt[region, ".json"]]
        -> Json<models::RealmDto>

        scope cdn_versioned(version: String) {
            path ["cdn", version]

            scope data_localized(locale: String = "en_US".to_string()) {
                path ["data", locale]

                GET GetChampionList
                as champion_list
                path ["champion.json"]
                -> Json<models::ChampionListDto>

                GET GetChampionDetail(champion_id: String)
                as champion_detail
                path ["champion", fmt[champion_id, ".json"]]
                -> Json<models::ChampionDetailDto>

                GET GetItems
                as items
                path ["item.json"]
                -> Json<models::ItemListDto>

                GET GetSummonerSpells
                as summoner_spells
                path ["summoner.json"]
                -> Json<models::SummonerSpellListDto>

                GET GetProfileIcons
                as profile_icons
                path ["profileicon.json"]
                -> Json<models::ProfileIconListDto>

                GET GetRunesReforged
                as runes_reforged
                path ["runesReforged.json"]
                -> Json<serde_json::Value>

                GET GetMaps
                as maps
                path ["map.json"]
                -> Json<serde_json::Value>
            }
        }
    }
}

pub use self::d_dragon_client::{DDragonClient, endpoints as ddragon_endpoints};

pub mod models {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RealmDto {
        pub v: String,
        pub l: String,
        pub cdn: String,
        pub dd: String,
        pub lg: String,
        pub css: String,
        pub profileiconmax: u32,
        pub store: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChampionListDto {
        #[serde(rename = "type")]
        pub ty: String,
        pub format: String,
        pub version: String,
        pub data: HashMap<String, ChampionBriefDto>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChampionBriefDto {
        pub version: String,
        pub id: String,
        pub key: String,
        pub name: String,
        pub title: String,
        pub blurb: String,
        #[serde(default)]
        pub tags: Vec<String>,
        #[serde(default)]
        pub info: Option<Value>,
        #[serde(default)]
        pub image: Option<Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ChampionDetailDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ItemListDto {
        #[serde(flatten)]
        pub raw: Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SummonerSpellListDto {
        #[serde(rename = "type")]
        pub ty: String,
        pub version: String,
        pub data: HashMap<String, Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProfileIconListDto {
        #[serde(flatten)]
        pub raw: Value,
    }
}

pub async fn ddragon_test() -> Result<(), ApiClientError> {
    let ddragon = DDragonClient::new();

    let versions = ddragon.ddragon().api().versions().await?;

    let version = versions
        .first()
        .cloned()
        .unwrap_or_else(|| "latest".to_string());

    println!("Data Dragon latest version: {version}");

    let languages = ddragon.ddragon().languages().await?;

    println!("Data Dragon languages: {}", languages.len());

    let realm = ddragon.ddragon().realm("euw".to_string()).await?;

    println!(
        "Data Dragon EUW realm: version={} cdn={}",
        realm.v, realm.cdn
    );

    let champions = ddragon
        .ddragon()
        .cdn_versioned(version.clone())
        .data_localized()
        .champion_list()
        .await?;

    println!("Data Dragon champions: {}", champions.data.len());

    let champion = ddragon
        .ddragon()
        .cdn_versioned(version)
        .data_localized()
        .champion_detail("Aatrox".to_string())
        .await?;

    println!(
        "Data Dragon champion detail fields: {:?}",
        champion.raw.as_object().map(|object| object.len())
    );

    Ok(())
}
