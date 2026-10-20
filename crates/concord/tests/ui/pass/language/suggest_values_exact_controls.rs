// Exact local references and explicitly qualified ancestors retain their
// meaning in every atom context, even when owners declare the same leaf name.
use concord::{Binary, Bytes, api};

api! {
    client SuggestVisibleValuesApi {
        base "https://values.example.test/";
        values { query("root_tenant") tenant: String; }
        subdomain [concat ["tenant-", tenant]];
        rate_limits {
            policy per_tenant(tenant) {
                id "suggest.values.exact";
                key [origin, "tenant" = tenant];
                allow 10 per 1s;
            }
        }

        scope regional(query region: String) {
            subdomain [region];
            path "regions/{region}";

            GET read(query("local_tenant") tenant: String, item_id: String) -> Bytes via Binary {
                path "{client.tenant}/{regional.region}/{tenant}/{item_id}";
                query { "label" = concat ["item-", item_id]; }
                headers { "X-Tenant" = concat [tenant, "/", client.tenant]; }
                rate_limit per_tenant(tenant = tenant);
            }
        }
    }
}

api! {
    client SuggestEscapedNamesApi {
        base "https://values.example.test/";
        values { query r#type: String; }
        rate_limits {
            policy off() {
                id "suggest.values.escaped";
                key [origin];
                allow 1 per 1s;
            }
        }
        scope secret(query tenant: String) {
            GET read(query concat: String) -> Bytes via Binary {
                headers {
                    "X-Type" = client.r#type;
                    "X-Tenant" = r#secret.tenant;
                    "X-Concat" = r#concat;
                }
                rate_limit r#off();
            }
        }
    }
}

fn main() {
    let _ = SuggestVisibleValuesApi::builder();
}
