#![forbid(unsafe_code)]

use concord_core::Request;
use concord_macros::api;

api! {
    client ReferenceApi {
        base "https://reference.example.test/root/";
        var tenant: String;
        var locale: Option<String>;
        var page_size: u32 = 25;
        credentials {
            secret api_key: String;
            credential application_key = api_key(secret.api_key);
        }
        retry off;
        host ["api"];
        path ["v1"];
        query {
            "tenant" = vars.tenant;
            "locale" = vars.locale;
        }
        headers { "X-Client" = "canonical-reference"; }
        timeout 15s;
        rate_limit {
            100 / 1s;
            1000 / 1m;
        }

        GET Status(verbose: Option<bool>) -> Bytes {
            path ["status"];
            query { "verbose" = ep.verbose; }
            auth off;
            timeout off;
        }

        scope Regions(region: String, language: Option<String>) {
            host [scope.region, "api"];
            path ["regions", scope.region];
            query { "language" = scope.language; }
            headers { "X-Region" = scope.region; }
            auth header "X-Api-Key" = application_key challenge never;
            timeout 5s;

            METHOD("PROPFIND") Resource(id: String, tags: Vec<String>) as fetch -> Text {
                path [fmt["resource-", ep.id]];
                query { "tag" = ep.tags; }
                headers { "Accept-Language" = scope.language; }
            }
        }
    }

    client ReqwestRetryReference {
        base "https://retry-reference.example.test/";
        retry reqwest_default;
        GET Health() -> NoContent;
    }
}

fn main() -> Result<(), concord_core::prelude::ClientBuildError> {
    use reference_api::prelude::*;

    let client = ReferenceApi::new("tenant-a".to_owned(), "example-api-key")?;
    let _: Request<_> = client.status().verbose(true);
    let _: Request<_> = client
        .regions("eu-west".to_owned())
        .language("fr".to_owned())
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    let endpoint = reference_api::endpoints::regions::Resource::new(
        "eu-west".to_owned(),
        "42".to_owned(),
        vec!["stable".to_owned()],
    );
    let _: Request<_> = client.request(endpoint.language("fr".to_owned()));
    let retry_client = ReqwestRetryReference::builder().build()?;
    let _: Request<_> = retry_client.health();
    Ok(())
}
