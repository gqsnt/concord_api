use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

#[derive(Clone, Debug)]
enum Region {
    EUW,
    NA,
    BadDot,
    BadDashStart,
    BadUnderscore,
}

impl core::fmt::Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Region::EUW => f.write_str("euw1"),
            Region::NA => f.write_str("na1"),
            Region::BadDot => f.write_str("bad.name"),
            Region::BadDashStart => f.write_str("-bad"),
            Region::BadUnderscore => f.write_str("bad_name"),
        }
    }
}

#[tokio::test]
async fn prefix_default_and_override_and_order() {
    api! {
        client ApiPrefixDefault {
            scheme: https,
            host: "example.com",
        }

        prefix {region:Region=Region::EUW} . "api" {
            GET Ping "" -> Json<()>;
        }
    }

    use api_prefix_default::*;

    // default region
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiPrefixDefault::new_with_transport(transport);
        let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("euw1.api.example.com");

        h.finish();
    }

    // override region
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiPrefixDefault::new_with_transport(transport);
        let _ = api
            .request(endpoints::Ping::new().region(Region::NA))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("na1.api.example.com");

        h.finish();
    }
}

#[tokio::test]
async fn prefix_optional_label_omitted_without_double_dot() {
    api! {
        client ApiPrefixOpt {
            scheme: https,
            host: "example.com",
        }

        prefix {sub?:String} . "api" {
            GET Ping "" -> Json<()>;
        }
    }

    use api_prefix_opt::*;

    // sub=None => "api.example.com"
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiPrefixOpt::new_with_transport(transport);
        let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("api.example.com");

        h.finish();
    }

    // sub=Some("x") => "x.api.example.com"
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiPrefixOpt::new_with_transport(transport);
        let _ = api
            .request(endpoints::Ping::new().sub("x".to_string()))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).host("x.api.example.com");

        h.finish();
    }
}

#[tokio::test]
async fn prefix_host_label_validation_errors() {
    api! {
        client ApiPrefixInvalid {
            scheme: https,
            host: "example.com",
        }

        prefix {region:Region=Region::EUW} . "api" {
            GET Ping "" -> Json<()>;
        }
    }

    use api_prefix_invalid::*;

    // BadDot => ContainsDot
    {
        let (transport, h) = mock().build();

        let api = ApiPrefixInvalid::new_with_transport(transport);
        let err = api
            .request(endpoints::Ping::new().region(Region::BadDot))
            .execute()
            .await
            .unwrap_err();

        h.assert_recorded_len(0);

        match err {
            ApiClientError::InvalidHostLabel { reason, .. } => {
                assert!(matches!(
                    reason,
                    concord_core::error::HostLabelInvalidReason::ContainsDot
                ));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }

    // BadDashStart => StartsOrEndsDash
    {
        let (transport, h) = mock().build();

        let api = ApiPrefixInvalid::new_with_transport(transport);
        let err = api
            .request(endpoints::Ping::new().region(Region::BadDashStart))
            .execute()
            .await
            .unwrap_err();

        h.assert_recorded_len(0);

        match err {
            ApiClientError::InvalidHostLabel { reason, .. } => {
                assert!(matches!(
                    reason,
                    concord_core::error::HostLabelInvalidReason::StartsOrEndsDash
                ));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }

    // BadUnderscore => InvalidByte
    {
        let (transport, h) = mock().build();

        let api = ApiPrefixInvalid::new_with_transport(transport);
        let err = api
            .request(endpoints::Ping::new().region(Region::BadUnderscore))
            .execute()
            .await
            .unwrap_err();

        h.assert_recorded_len(0);

        match err {
            ApiClientError::InvalidHostLabel { reason, .. } => {
                assert!(matches!(
                    reason,
                    concord_core::error::HostLabelInvalidReason::InvalidByte(_)
                ));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }
}
