//! First-contact v3 declaration (§2.1) compiled against the facade.

use concord::{ApiError, ClientBuildError, Json, api};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: String,
    pub title: String,
}

api! {
    client Catalog {
        base "https://catalog.example.test/v1/";
        response Json;
        values {
            query tenant: String;
        }

        scope regional(region: String) {
            path "regions/{region}";

            /// Returns one page of items. Ordinary await does not paginate.
            GET items(
                query category?: String,
                query count: u32 = 25,
                #[default] query("tag") each tags: Vec<String>,
            ) -> Vec<Item> {
                path "items";
            }

            /// Returns None only for the declared missing-resource status.
            GET find_item(id: String) -> Option<Item> {
                path "items/{id}";
                absent on 404;
            }
        }
    }
}

pub fn catalog() -> Result<Catalog, ClientBuildError> {
    Catalog::builder().set_tenant("acme").build()
}

pub async fn hardware(client: &Catalog) -> Result<Vec<Item>, ApiError> {
    let europe = client.regional("eu");
    europe
        .items()
        .set_category("hardware")
        .set_count(50)
        .set_tags(vec!["repairable".to_owned()])
        .await
}

#[test]
fn plan_renders_the_first_contact_request() {
    let client = catalog().expect("client");
    let plan = client
        .regional("eu")
        .items()
        .set_category("hardware")
        .plan()
        .expect("plan");
    let url = plan.observed_url().to_owned();
    assert!(
        url.starts_with("https://catalog.example.test/v1/regions/eu/items?"),
        "{url}"
    );
    assert!(url.contains("tenant=acme"), "{url}");
    assert!(url.contains("category=hardware"), "{url}");
    assert!(url.contains("count=25"), "{url}");
    assert!(!url.contains("tag="), "{url}");
}
