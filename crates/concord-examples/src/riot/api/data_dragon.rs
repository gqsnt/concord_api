//! The independent Data Dragon static-data API declaration.
//!
//! Required features: `json` and `rate-limit` through the Riot bundle.
//! Mode: deterministic planning plus optional manual live execution.
//! Documentation owner: `docs/spec.md`.
//! Proving test: `tests/riot_contract.rs`.

use concord::api;
use concord::prelude::*;

api! {
    client DDragonApi {
        base "https://ddragon.leagueoflegends.com/";

        values {
            locale: crate::riot::domain::DDragonLocale = crate::riot::domain::DDragonLocale::EnUs;
        }

        headers { "User-Agent" = "ConcordRiotExample/1.0"; }
        timeout 20s;


    GET versions() -> Vec<String> via Json {
        path ["api", "versions.json"];
    }

    GET realm(region: String) -> serde_json::Value via Json {
        path ["realms", concat [endpoint.region, ".json"]];
    }

    scope version(version: String) {
        path ["cdn", version.version];

        scope data() {
            path ["data", client.locale];

            GET champions() -> serde_json::Value via Json {
                path ["champion.json"];
            }

            GET items() -> serde_json::Value via Json {
                path ["item.json"];
            }
        }

        scope assets() {
            path ["img"];

            GET champion_image(filename: String) -> Bytes via Binary {
                path ["champion", endpoint.filename];
            }

            GET item_image(filename: String) -> Bytes via Binary {
                path ["item", endpoint.filename];
            }
        }
    }
}}
