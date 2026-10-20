//! The independent Data Dragon static-data API declaration.
//!
//! Required features: `json` and `rate-limit` through the Riot bundle.
//! Mode: deterministic planning plus optional manual live execution.
//! Documentation owner: `docs/spec.md`.
//! Proving test: `tests/riot_contract.rs`.

use concord::api;
use concord::prelude::*;

use crate::riot::domain::DDragonLocale;

api! {
    client DDragonApi {
        base "https://ddragon.leagueoflegends.com/";

        values {
            locale: DDragonLocale = DDragonLocale::EnUs;
        }

        headers { "User-Agent" = "ConcordRiotExample/1.0"; }
        timeout 20s;
        response Json;

    GET versions() -> Vec<String> {
        path "api/versions.json";
    }

    GET realm(region: String) -> serde_json::Value {
        path "realms/{region}.json";
    }

    scope version(version: String) {
        path "cdn/{version}";

        scope data() {
            path "data/{client.locale}";

            GET champions() -> serde_json::Value {
                path "champion.json";
            }

            GET items() -> serde_json::Value {
                path "item.json";
            }
        }

        scope assets() {
            path "img";
            response Binary;

            GET champion_image(filename: String) -> Bytes {
                path "champion/{filename}";
            }

            GET item_image(filename: String) -> Bytes {
                path "item/{filename}";
            }
        }
    }
}}
