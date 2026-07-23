//! The independent Data Dragon static-data API declaration.
//!
//! Required features: `json` and `rate-limit-governor` through the Riot bundle.
//! Mode: deterministic planning plus optional manual live execution.
//! Documentation owner: `docs/recipes/riot.md`.
//! Proving test: `tests/riot_contract.rs`.

use concord::api;
use concord::prelude::*;

api! {
    client DDragonApi {
        base "https://ddragon.leagueoflegends.com/";

        vars {
            locale: crate::riot::domain::DDragonLocale = crate::riot::domain::DDragonLocale::EnUs;
        }

        headers { "User-Agent" = "ConcordRiotExample/1.0"; }
        timeout 20s;
    }

    GET versions() -> Json<Vec<String>> {
        path ["api", "versions.json"];
    }

    GET realm(region: String) -> Json<serde_json::Value> {
        path ["realms", fmt[ep.region, ".json"]];
    }

    scope version(version: String) {
        path ["cdn", scope.version];

        scope data() {
            path ["data", vars.locale];

            GET champions() -> Json<serde_json::Value> {
                path ["champion.json"];
            }

            GET items() -> Json<serde_json::Value> {
                path ["item.json"];
            }
        }

        scope assets() {
            path ["img"];

            GET champion_image(filename: String) -> Bytes {
                path ["champion", ep.filename];
            }

            GET item_image(filename: String) -> Bytes {
                path ["item", ep.filename];
            }
        }
    }
}
