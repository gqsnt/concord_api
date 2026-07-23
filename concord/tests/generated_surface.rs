use concord::{Bytes, NoContent, api};

api! {
    client FacadeApi {
        base "http://facade.example.test/";
    }

    GET Fetch() -> Bytes;
    GET Delete() -> NoContent;
}

#[test]
fn one_facade_dependency_builds_a_client_and_inert_requests() {
    let client = FacadeApi::new().expect("zero-input façade client");
    let _: concord::Request<_> = client.fetch();
    let _: concord::Request<_> = client.delete();
    let _: concord::Request<_> = client.request(facade_api::endpoints::Fetch::new());
}
